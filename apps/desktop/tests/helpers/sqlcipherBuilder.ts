import { createCipheriv, randomBytes } from 'node:crypto';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import {
  PAGE_SIZE,
  RESERVED_SIZE,
  SALT_SIZE,
  type SqlCipherKeys,
  deriveKeys,
  pageHmac,
} from '../../src/main/adapters/rekordbox/sqlcipher';

const RESERVED_SIZE_OFFSET = 20;
/** Where page 1's b-tree header records the start of its (still empty) cell content area. */
const FIRST_PAGE_CONTENT_START_OFFSET = 105;
const WAL_MAGIC = 0x377f0682;
const WAL_VERSION = 3007000;

/**
 * Builds a plain SQLite database that leaves room for SQLCipher's IV and HMAC
 * at the end of every page, as a SQLCipher database does once decrypted.
 */
export async function plainDatabase(setup: (database: DatabaseSync) => void, from?: Buffer): Promise<Buffer> {
  const directory = await mkdtemp(path.join(os.tmpdir(), 'sqlcipher-'));
  try {
    const file = path.join(directory, 'plain.db');
    if (from) {
      await writeFile(file, from);
    } else {
      const empty = new DatabaseSync(file);
      empty.exec(`PRAGMA page_size = ${PAGE_SIZE}; PRAGMA user_version = 1;`);
      empty.close();
      // Setting the reserved space is only safe while the database is empty.
      const header = await readFile(file);
      header[RESERVED_SIZE_OFFSET] = RESERVED_SIZE;
      header.writeUInt16BE(PAGE_SIZE - RESERVED_SIZE, FIRST_PAGE_CONTENT_START_OFFSET);
      await writeFile(file, header);
    }
    const database = new DatabaseSync(file);
    try {
      setup(database);
    } finally {
      database.close();
    }
    return await readFile(file);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

/** Source of the salt and IVs. Fixtures pass a seeded one so regenerating them gives the same bytes. */
export type RandomBytes = (size: number) => Buffer;

export async function encryptSqlCipher(
  plain: Buffer,
  passphrase: string,
  random: RandomBytes = randomBytes,
): Promise<{ database: Buffer; keys: SqlCipherKeys }> {
  const salt = random(SALT_SIZE);
  const keys = await deriveKeys(passphrase, salt);
  const pages: Buffer[] = [];
  for (let pageNumber = 1; pageNumber <= plain.length / PAGE_SIZE; pageNumber++) {
    const page = encryptPage(
      plain.subarray((pageNumber - 1) * PAGE_SIZE, pageNumber * PAGE_SIZE),
      pageNumber,
      keys,
      random,
    );
    if (pageNumber === 1) salt.copy(page);
    pages.push(page);
  }
  return { database: Buffer.concat(pages), keys };
}

export function encryptPage(
  plain: Buffer,
  pageNumber: number,
  keys: SqlCipherKeys,
  random: RandomBytes = randomBytes,
): Buffer {
  const start = pageNumber === 1 ? SALT_SIZE : 0;
  const iv = random(16);
  const cipher = createCipheriv('aes-256-cbc', keys.encryption, iv);
  cipher.setAutoPadding(false);
  const encrypted = Buffer.concat([cipher.update(plain.subarray(start, PAGE_SIZE - RESERVED_SIZE)), cipher.final()]);
  const contentAndIv = Buffer.concat([encrypted, iv]);
  const page = Buffer.alloc(PAGE_SIZE);
  contentAndIv.copy(page, start);
  pageHmac(keys, contentAndIv, pageNumber).copy(page, start + contentAndIv.length);
  return page;
}

export interface WalFrame {
  pageNumber: number;
  /** Encrypted page. */
  page: Buffer;
  /** Database size in pages after the transaction, for the frame that commits it. */
  commitPageCount?: number;
  salts?: Buffer;
}

/**
 * SQLite's WAL checksum. Magic `0x377f0682` selects the little-endian word
 * order, and the two running sums are stored big-endian. SQLCipher checks
 * these before it will replay a frame, so a log without them is ignored.
 */
function walChecksum(chunks: Buffer[], s1 = 0, s2 = 0): [number, number] {
  for (const chunk of chunks) {
    for (let offset = 0; offset < chunk.length; offset += 8) {
      s1 = (s1 + chunk.readUInt32LE(offset) + s2) >>> 0;
      s2 = (s2 + chunk.readUInt32LE(offset + 4) + s1) >>> 0;
    }
  }
  return [s1, s2];
}

export function walFile(salts: Buffer, frames: WalFrame[]): Buffer {
  const header = Buffer.alloc(32);
  header.writeUInt32BE(WAL_MAGIC, 0);
  header.writeUInt32BE(WAL_VERSION, 4);
  header.writeUInt32BE(PAGE_SIZE, 8);
  salts.copy(header, 16);
  let [s1, s2] = walChecksum([header.subarray(0, 24)]);
  header.writeUInt32BE(s1, 24);
  header.writeUInt32BE(s2, 28);
  return Buffer.concat([
    header,
    ...frames.flatMap((frame) => {
      const frameHeader = Buffer.alloc(24);
      frameHeader.writeUInt32BE(frame.pageNumber, 0);
      frameHeader.writeUInt32BE(frame.commitPageCount ?? 0, 4);
      (frame.salts ?? salts).copy(frameHeader, 8);
      [s1, s2] = walChecksum([frameHeader.subarray(0, 8), frame.page], s1, s2);
      frameHeader.writeUInt32BE(s1, 16);
      frameHeader.writeUInt32BE(s2, 20);
      return [frameHeader, frame.page];
    }),
  ]);
}

export function pageOf(database: Buffer, pageNumber: number): Buffer {
  return database.subarray((pageNumber - 1) * PAGE_SIZE, pageNumber * PAGE_SIZE);
}

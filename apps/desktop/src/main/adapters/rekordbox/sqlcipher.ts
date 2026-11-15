/**
 * Decrypts SQLCipher 4 databases that use SQLCipher's default settings, which
 * is what rekordbox's Device Library Plus (`exportLibrary.db`) does:
 * PBKDF2-HMAC-SHA512 key derivation, AES-256-CBC per page and an HMAC-SHA512
 * per page. See https://www.zetetic.net/sqlcipher/design/.
 *
 * The result is a plain SQLite database image, so no SQLCipher build of SQLite
 * is needed to read it.
 */
import { createDecipheriv, createHmac, pbkdf2, timingSafeEqual } from 'node:crypto';
import { promisify } from 'node:util';

const pbkdf2Async = promisify(pbkdf2);

export const PAGE_SIZE = 4096;
export const SALT_SIZE = 16;
const KDF_ITERATIONS = 256_000;
const HMAC_KDF_ITERATIONS = 2;
const HMAC_SALT_MASK = 0x3a;
const KEY_SIZE = 32;
const IV_SIZE = 16;
const HMAC_SIZE = 64;
/** The IV and HMAC at the end of every page. */
export const RESERVED_SIZE = IV_SIZE + HMAC_SIZE;
const SQLITE_MAGIC = Buffer.from('SQLite format 3\0', 'latin1');
const WAL_HEADER_SIZE = 32;
const WAL_FRAME_HEADER_SIZE = 24;
const ROLLBACK_JOURNAL = 1;

export interface SqlCipherKeys {
  encryption: Buffer;
  hmac: Buffer;
}

/**
 * Returns the decrypted database with the transactions committed in `wal` (the
 * `-wal` file next to it, if any) applied.
 */
export async function decryptSqlCipher(database: Buffer, passphrase: string, wal?: Buffer): Promise<Buffer> {
  if (database.length < PAGE_SIZE || database.length % PAGE_SIZE !== 0) {
    throw new Error('The database file has an unexpected size.');
  }
  const keys = await deriveKeys(passphrase, database.subarray(0, SALT_SIZE));

  const pages: Buffer[] = [];
  for (let pageNumber = 1; pageNumber <= database.length / PAGE_SIZE; pageNumber++) {
    const page = decryptPage(database.subarray((pageNumber - 1) * PAGE_SIZE, pageNumber * PAGE_SIZE), pageNumber, keys);
    if (!page) {
      throw new Error(
        pageNumber === 1 ? 'The database could not be decrypted.' : `Page ${pageNumber} of the database is damaged.`,
      );
    }
    pages.push(page);
  }
  if (wal) applyWal(pages, wal, keys);

  const plain = Buffer.concat(pages);
  // The WAL is already applied, so SQLite must not look for one.
  plain[18] = ROLLBACK_JOURNAL;
  plain[19] = ROLLBACK_JOURNAL;
  return plain;
}

export async function deriveKeys(passphrase: string, salt: Buffer): Promise<SqlCipherKeys> {
  const encryption = await pbkdf2Async(passphrase, salt, KDF_ITERATIONS, KEY_SIZE, 'sha512');
  const hmacSalt = Buffer.from(salt.map((byte) => byte ^ HMAC_SALT_MASK));
  const hmac = await pbkdf2Async(encryption, hmacSalt, HMAC_KDF_ITERATIONS, KEY_SIZE, 'sha512');
  return { encryption, hmac };
}

/** Returns null if the page fails its HMAC check (wrong key, wrong page number or damage). */
function decryptPage(page: Buffer, pageNumber: number, keys: SqlCipherKeys): Buffer | null {
  // Page 1 starts with the key salt instead of SQLite's magic string.
  const start = pageNumber === 1 ? SALT_SIZE : 0;
  const ivStart = PAGE_SIZE - RESERVED_SIZE;
  const hmacStart = ivStart + IV_SIZE;

  const expected = pageHmac(keys, page.subarray(start, hmacStart), pageNumber);
  if (!timingSafeEqual(expected, page.subarray(hmacStart, PAGE_SIZE))) return null;

  const decipher = createDecipheriv('aes-256-cbc', keys.encryption, page.subarray(ivStart, hmacStart));
  decipher.setAutoPadding(false);
  const plain = Buffer.alloc(PAGE_SIZE);
  if (pageNumber === 1) SQLITE_MAGIC.copy(plain);
  Buffer.concat([decipher.update(page.subarray(start, ivStart)), decipher.final()]).copy(plain, start);
  return plain;
}

/** HMAC over the encrypted page content and IV, followed by the page number. */
export function pageHmac(keys: SqlCipherKeys, contentAndIv: Buffer, pageNumber: number): Buffer {
  const number = Buffer.alloc(4);
  number.writeUInt32LE(pageNumber);
  return createHmac('sha512', keys.hmac).update(contentAndIv).update(number).digest();
}

/**
 * Applies the committed transactions of a WAL file to `pages`. The log ends at
 * the first frame from an older WAL generation (different salts) or one that
 * fails its HMAC check, such as a frame that was only partly written.
 */
function applyWal(pages: Buffer[], wal: Buffer, keys: SqlCipherKeys): void {
  if (wal.length < WAL_HEADER_SIZE || wal.readUInt32BE(8) !== PAGE_SIZE) return;
  const salts = wal.subarray(16, 24);
  const frameSize = WAL_FRAME_HEADER_SIZE + PAGE_SIZE;

  let uncommitted = new Map<number, Buffer>();
  for (let at = WAL_HEADER_SIZE; at + frameSize <= wal.length; at += frameSize) {
    if (!wal.subarray(at + 8, at + 16).equals(salts)) return;
    const pageNumber = wal.readUInt32BE(at);
    if (pageNumber === 0) return;
    const page = decryptPage(wal.subarray(at + WAL_FRAME_HEADER_SIZE, at + frameSize), pageNumber, keys);
    if (!page) return;
    uncommitted.set(pageNumber, page);

    const pageCountAfterCommit = wal.readUInt32BE(at + 4);
    if (pageCountAfterCommit === 0) continue;
    const committed = [...pages];
    for (const [number, content] of uncommitted) committed[number - 1] = content;
    committed.length = pageCountAfterCommit;
    for (let index = 0; index < committed.length; index++) {
      // A page past the old end of the database that the transaction did not write.
      if (committed[index] === undefined) return;
    }
    pages.splice(0, pages.length, ...committed);
    uncommitted = new Map();
  }
}

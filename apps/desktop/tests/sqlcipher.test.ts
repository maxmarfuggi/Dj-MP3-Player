import type { DatabaseSync } from 'node:sqlite';
import { describe, expect, it } from 'vitest';
import { PAGE_SIZE, decryptSqlCipher } from '../src/main/adapters/rekordbox/sqlcipher';
import { encryptPage, encryptSqlCipher, pageOf, plainDatabase, walFile } from './helpers/sqlcipherBuilder';

const KEY = 'test-key';

const withSongs = (count: number) => (database: DatabaseSync) => {
  database.exec('CREATE TABLE IF NOT EXISTS song (id INTEGER PRIMARY KEY, name TEXT)');
  const insert = database.prepare('INSERT INTO song (name) VALUES (?)');
  // Long names spread the rows over several pages.
  for (let i = 0; i < count; i++) insert.run(`${'song '.repeat(40)}${i}`);
};

describe('decryptSqlCipher', () => {
  it('restores the plain database', async () => {
    const plain = await plainDatabase(withSongs(60));
    const { database } = await encryptSqlCipher(plain, KEY);

    expect(plain.length / PAGE_SIZE).toBeGreaterThan(2);
    expect((await decryptSqlCipher(database, KEY)).equals(plain)).toBe(true);
  });

  it('rejects the wrong key', async () => {
    const { database } = await encryptSqlCipher(await plainDatabase(withSongs(1)), KEY);

    await expect(decryptSqlCipher(database, 'other-key')).rejects.toThrow(/could not be decrypted/);
  });

  it('rejects a damaged page', async () => {
    const { database } = await encryptSqlCipher(await plainDatabase(withSongs(60)), KEY);
    database[PAGE_SIZE + 100]! ^= 0xff;

    await expect(decryptSqlCipher(database, KEY)).rejects.toThrow(/Page 2/);
  });

  describe('with a WAL file', () => {
    async function setup() {
      const before = await plainDatabase(withSongs(20));
      const after = await plainDatabase(withSongs(40), before);
      const { database, keys } = await encryptSqlCipher(before, KEY);
      const pageCount = after.length / PAGE_SIZE;
      const changedPages = Array.from({ length: pageCount }, (_, index) => index + 1).filter(
        (pageNumber) => !pageOf(after, pageNumber).equals(pageOf(before, pageNumber)),
      );
      const frame = (pageNumber: number, commit = false) => ({
        pageNumber,
        page: encryptPage(pageOf(after, pageNumber), pageNumber, keys),
        commitPageCount: commit ? pageCount : undefined,
      });
      return { before, after, database, keys, changedPages, frame };
    }

    const salts = Buffer.from('0123456789abcdef'.slice(0, 8));

    it('applies committed transactions', async () => {
      const { after, database, changedPages, frame } = await setup();
      const wal = walFile(
        salts,
        changedPages.map((pageNumber, index) => frame(pageNumber, index === changedPages.length - 1)),
      );

      expect((await decryptSqlCipher(database, KEY, wal)).equals(after)).toBe(true);
    });

    it('ignores a transaction that was not committed', async () => {
      const { before, database, changedPages, frame } = await setup();
      const wal = walFile(salts, changedPages.map((pageNumber) => frame(pageNumber)));

      expect((await decryptSqlCipher(database, KEY, wal)).equals(before)).toBe(true);
    });

    it('ignores frames left over from an older WAL', async () => {
      const { before, database, changedPages, frame } = await setup();
      const wal = walFile(
        salts,
        changedPages.map((pageNumber, index) => ({
          ...frame(pageNumber, index === changedPages.length - 1),
          salts: Buffer.from('stale!!!'),
        })),
      );

      expect((await decryptSqlCipher(database, KEY, wal)).equals(before)).toBe(true);
    });

    it('stops at a frame that was only partly written', async () => {
      const { before, database, changedPages, frame } = await setup();
      const frames = changedPages.map((pageNumber, index) => frame(pageNumber, index === changedPages.length - 1));
      frames[0]!.page[200]! ^= 0xff;

      expect((await decryptSqlCipher(database, KEY, walFile(salts, frames))).equals(before)).toBe(true);
    });
  });
});

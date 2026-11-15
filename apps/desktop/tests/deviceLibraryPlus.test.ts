import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import type { DatabaseSync } from 'node:sqlite';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { rekordboxAdapter } from '../src/main/adapters/rekordbox';
import {
  DEVICE_LIBRARY_PLUS_KEY,
  deviceLibraryPlusAdapter,
} from '../src/main/adapters/rekordbox/deviceLibraryPlus';
import { encryptSqlCipher, plainDatabase } from './helpers/sqlcipherBuilder';

/** The tables and columns the adapter reads, as rekordbox writes them. */
const SCHEMA = `
  CREATE TABLE content (
    content_id INTEGER PRIMARY KEY, title VARCHAR, bpmx100 INTEGER, length INTEGER,
    artist_id_artist INTEGER, album_id INTEGER, genre_id INTEGER, label_id INTEGER,
    key_id INTEGER, color_id INTEGER, image_id INTEGER, path VARCHAR
  );
  CREATE TABLE artist (artist_id INTEGER PRIMARY KEY, name VARCHAR);
  CREATE TABLE album (album_id INTEGER PRIMARY KEY, name VARCHAR);
  CREATE TABLE genre (genre_id INTEGER PRIMARY KEY, name VARCHAR);
  CREATE TABLE label (label_id INTEGER PRIMARY KEY, name VARCHAR);
  CREATE TABLE "key" (key_id INTEGER PRIMARY KEY, name VARCHAR);
  CREATE TABLE color (color_id INTEGER PRIMARY KEY, name VARCHAR);
  CREATE TABLE image (image_id INTEGER PRIMARY KEY, path VARCHAR);
  CREATE TABLE playlist (
    playlist_id INTEGER PRIMARY KEY, sequenceNo INTEGER, name VARCHAR, image_id INTEGER,
    attribute INTEGER, playlist_id_parent INTEGER
  );
  CREATE TABLE playlist_content (playlist_id INTEGER, content_id INTEGER, sequenceNo INTEGER);
`;

function fillLibrary(database: DatabaseSync): void {
  database.exec(SCHEMA);
  database.exec(`
    INSERT INTO artist VALUES (1, 'DJ Example');
    INSERT INTO album VALUES (1, 'The Album');
    INSERT INTO genre VALUES (1, 'House');
    INSERT INTO label VALUES (1, 'Big Records');
    INSERT INTO "key" VALUES (1, '8A');
    INSERT INTO color VALUES (1, 'Pink'), (5, 'Warm up');
    INSERT INTO image VALUES
      (1, '/PIONEER/Artwork/00001/b1.jpg'),
      (2, '/PIONEER/Artwork/00001/b2.jpg'),
      (3, '/PIONEER/Artwork/00002/b3.jpg');
    INSERT INTO content VALUES
      (1, 'Opener', 12850, 301, 1, 1, 1, 1, 1, 5, 1, '/Contents/opener.mp3'),
      (2, '', 0, 0, 0, 0, 0, 0, 0, 0, 2, '/Contents/Some Artist/Untitled Jam.wav'),
      (3, 'Closer', 12000, 240, 1, 0, 0, 0, 0, 1, 3, '/Contents/closer.mp3');
    INSERT INTO playlist VALUES
      (1, 0, 'Friday', 0, 1, 0),
      (2, 1, 'Peak time', 0, 0, 1),
      (3, 0, 'Warm up', 0, 0, 1),
      (4, 1, 'Loose ends', 0, 0, 0);
    INSERT INTO playlist_content VALUES (3, 3, 2), (3, 1, 1), (2, 2, 1), (4, 99, 1);
  `);
}

describe('deviceLibraryPlusAdapter', () => {
  let root: string;
  let databaseDirectory: string;

  beforeEach(async () => {
    root = await mkdtemp(path.join(os.tmpdir(), 'device-library-plus-'));
    databaseDirectory = path.join(root, 'PIONEER', 'rekordbox');
    await mkdir(databaseDirectory, { recursive: true });
  });

  afterEach(async () => {
    await rm(root, { recursive: true, force: true });
  });

  async function writeExport(): Promise<void> {
    const { database } = await encryptSqlCipher(await plainDatabase(fillLibrary), DEVICE_LIBRARY_PLUS_KEY);
    await writeFile(path.join(databaseDirectory, 'exportLibrary.db'), database);
  }

  async function writeArtwork(...relativePaths: string[]): Promise<void> {
    for (const relativePath of relativePaths) {
      const file = path.join(root, ...relativePath.split('/'));
      await mkdir(path.dirname(file), { recursive: true });
      await writeFile(file, 'jpeg');
    }
  }

  it('ignores volumes without a Device Library Plus export', async () => {
    expect(await deviceLibraryPlusAdapter.detect(root)).toBe(false);
  });

  it('reads tracks with their details', async () => {
    await writeExport();

    expect(await deviceLibraryPlusAdapter.detect(root)).toBe(true);
    const library = await deviceLibraryPlusAdapter.load({ path: root, name: 'GIGS' });

    expect(library).toMatchObject({ source: 'deviceLibraryPlus', volumeName: 'GIGS', volumePath: root });
    expect(library.tracks).toEqual([
      {
        id: '1',
        title: 'Opener',
        artist: 'DJ Example',
        album: 'The Album',
        bpm: 128.5,
        key: '8A',
        durationSec: 301,
        relativePath: 'Contents/opener.mp3',
        genre: 'House',
        label: 'Big Records',
        // Colours keep their fixed look under the name the user gave them.
        color: { name: 'Warm up', value: '#3fcf5c' },
      },
      {
        id: '2',
        title: 'Untitled Jam',
        artist: '',
        album: '',
        bpm: null,
        key: null,
        durationSec: null,
        relativePath: 'Contents/Some Artist/Untitled Jam.wav',
        genre: '',
        label: '',
      },
      expect.objectContaining({ id: '3', color: { name: 'Pink', value: '#f870c8' } }),
    ]);
  });

  it('builds the playlist tree in rekordbox order', async () => {
    await writeExport();

    const library = await deviceLibraryPlusAdapter.load({ path: root, name: 'GIGS' });

    expect(library.playlists).toEqual([
      {
        id: '1',
        name: 'Friday',
        kind: 'folder',
        trackIds: [],
        children: [
          { id: '3', name: 'Warm up', kind: 'playlist', trackIds: ['1', '3'], children: [] },
          { id: '2', name: 'Peak time', kind: 'playlist', trackIds: ['2'], children: [] },
        ],
      },
      // Its only entry points at a track that is not in the library.
      { id: '4', name: 'Loose ends', kind: 'playlist', trackIds: [], children: [] },
    ]);
  });

  it('finds artwork, preferring the larger variant and falling back from b to a files', async () => {
    await writeArtwork(
      'PIONEER/Artwork/00001/a1.jpg',
      'PIONEER/Artwork/00001/a1_m.jpg',
      'PIONEER/Artwork/00001/B2.JPG',
    );
    await writeExport();

    const library = await deviceLibraryPlusAdapter.load({ path: root, name: 'GIGS' });

    expect(library.tracks.map((track) => track.artworkPath)).toEqual([
      'PIONEER/Artwork/00001/a1_m.jpg',
      'PIONEER/Artwork/00001/B2.JPG',
      undefined,
    ]);
  });

  it('is shown alongside the Device Library export on the same USB', async () => {
    await writeExport();
    await writeFile(path.join(databaseDirectory, 'export.pdb'), '');

    expect(await rekordboxAdapter.detect(root)).toBe(true);
    expect(await deviceLibraryPlusAdapter.detect(root)).toBe(true);
  });
});

import type { DatabaseSync } from 'node:sqlite';

/** The tables and columns the Device Library Plus adapter reads, as rekordbox writes them. */
export const DEVICE_LIBRARY_PLUS_SCHEMA = `
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

/** A small library covering colours, artwork, folders, ordering and dangling entries. */
export function fillBasicLibrary(database: DatabaseSync): void {
  database.exec(DEVICE_LIBRARY_PLUS_SCHEMA);
  database.exec(`
    INSERT INTO artist VALUES (1, 'DJ Example'), (2, 'Сергей');
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
      (3, 'Closer', 12000, 240, 1, 0, 0, 0, 0, 1, 3, '/Contents/closer.mp3'),
      (5, 'Café del Mar', 9950, 412, 2, 0, 0, 0, 0, 9, 0, '/Contents/Сергей/Café del Mar.aiff');
    INSERT INTO playlist VALUES
      (1, 0, 'Friday', 0, 1, 0),
      (2, 1, 'Peak time', 0, 0, 1),
      (3, 0, 'Warm up', 0, 0, 1),
      (4, 1, 'Loose ends', 0, 0, 0);
    INSERT INTO playlist_content VALUES (3, 3, 2), (3, 1, 1), (2, 2, 1), (4, 99, 1), (4, 5, 2);
  `);
}

/** Changes made on top of `fillBasicLibrary`, committed through a WAL file. */
export function applyLateChanges(database: DatabaseSync): void {
  database.exec(`
    INSERT INTO content VALUES (4, 'Late addition', 12400, 280, 1, 0, 0, 0, 0, 0, 0, '/Contents/late.mp3');
    INSERT INTO playlist_content VALUES (3, 4, 3);
    UPDATE playlist SET name = 'Peak hour' WHERE playlist_id = 2;
  `);
}

import { mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { DatabaseSync } from 'node:sqlite';
import type { Library, Track, TrackColor } from '../../../shared/types';
import { exists, fileNameWithoutExtension, resolveInsideVolume, toRelativePath } from '../paths';
import { type LibraryAdapter, type Volume, libraryId } from '../types';
import { buildPlaylistTree } from './index';
import type { PdbPlaylistEntry, PdbPlaylistNode } from './pdb';
import { decryptSqlCipher } from './sqlcipher';

/** "Device Library Plus" (also called OneLibrary), the SQLCipher database newer rekordbox versions export. */
const DATABASE_PATH = path.join('PIONEER', 'rekordbox', 'exportLibrary.db');
/** Every Device Library Plus export uses this key; players have it built in. */
export const DEVICE_LIBRARY_PLUS_KEY = 'r8gddnr4k847830ar6cqzbkk0el6qytmb3trbbx805jm74vez64i5o8fnrqryqls';

const FOLDER_ATTRIBUTE = 1;

/** rekordbox's eight track colours, by `color_id`. */
const TRACK_COLORS: Record<number, TrackColor> = {
  1: { name: 'Pink', value: '#f870c8' },
  2: { name: 'Red', value: '#f5423c' },
  3: { name: 'Orange', value: '#ff9a2e' },
  4: { name: 'Yellow', value: '#f2d93a' },
  5: { name: 'Green', value: '#3fcf5c' },
  6: { name: 'Aqua', value: '#2bd0d6' },
  7: { name: 'Blue', value: '#3d6cff' },
  8: { name: 'Purple', value: '#a65cff' },
};

export interface DeviceLibraryPlusTrack {
  id: number;
  title: string;
  artist: string;
  album: string;
  genre: string;
  label: string;
  key: string;
  colorId: number;
  colorName: string;
  /** BPM multiplied by 100. */
  bpmx100: number;
  durationSec: number;
  /** Path relative to the volume root, starting with `/`. */
  path: string;
  /** Artwork path as the database stores it; the file may not exist. */
  imagePath: string;
}

export interface DeviceLibraryPlusData {
  tracks: DeviceLibraryPlusTrack[];
  playlistNodes: PdbPlaylistNode[];
  playlistEntries: PdbPlaylistEntry[];
}

export const deviceLibraryPlusAdapter: LibraryAdapter = {
  source: 'deviceLibraryPlus',

  async detect(volumePath) {
    return exists(path.join(volumePath, DATABASE_PATH));
  },

  async load(volume) {
    const databasePath = path.join(volume.path, DATABASE_PATH);
    const plain = await decryptSqlCipher(
      await readFile(databasePath),
      DEVICE_LIBRARY_PLUS_KEY,
      await readFileIfPresent(`${databasePath}-wal`),
    );
    const data = await withDatabase(plain, readDeviceLibraryPlus);
    const artwork = await findArtwork(
      volume.path,
      data.tracks.map((track) => track.imagePath),
    );
    return buildDeviceLibraryPlusLibrary(data, volume, artwork);
  },
};

export function readDeviceLibraryPlus(database: DatabaseSync): DeviceLibraryPlusData {
  const tracks = database
    .prepare(
      `SELECT c.content_id AS id,
              COALESCE(c.title, '') AS title,
              COALESCE(artist.name, '') AS artist,
              COALESCE(album.name, '') AS album,
              COALESCE(genre.name, '') AS genre,
              COALESCE(label.name, '') AS label,
              COALESCE(k.name, '') AS key,
              COALESCE(c.color_id, 0) AS colorId,
              COALESCE(color.name, '') AS colorName,
              COALESCE(c.bpmx100, 0) AS bpmx100,
              COALESCE(c.length, 0) AS durationSec,
              COALESCE(c.path, '') AS path,
              COALESCE(image.path, '') AS imagePath
         FROM content c
         LEFT JOIN artist ON artist.artist_id = c.artist_id_artist
         LEFT JOIN album ON album.album_id = c.album_id
         LEFT JOIN genre ON genre.genre_id = c.genre_id
         LEFT JOIN label ON label.label_id = c.label_id
         LEFT JOIN "key" k ON k.key_id = c.key_id
         LEFT JOIN color ON color.color_id = c.color_id
         LEFT JOIN image ON image.image_id = c.image_id`,
    )
    .all() as unknown as DeviceLibraryPlusTrack[];

  const playlistRows = database
    .prepare(
      `SELECT playlist_id AS id,
              COALESCE(playlist_id_parent, 0) AS parentId,
              COALESCE(sequenceNo, 0) AS sortOrder,
              attribute,
              COALESCE(name, '') AS name
         FROM playlist`,
    )
    .all() as unknown as (Omit<PdbPlaylistNode, 'isFolder'> & { attribute: number })[];

  const playlistEntries = database
    .prepare(
      `SELECT playlist_id AS playlistId, content_id AS trackId, COALESCE(sequenceNo, 0) AS entryIndex
         FROM playlist_content`,
    )
    .all() as unknown as PdbPlaylistEntry[];

  return {
    tracks,
    playlistNodes: playlistRows.map(({ attribute, ...row }) => ({ ...row, isFolder: attribute === FOLDER_ATTRIBUTE })),
    playlistEntries,
  };
}

/**
 * @param artwork Maps artwork paths as the database stores them to the
 *   volume-relative path of the file to show, for those that were found.
 */
export function buildDeviceLibraryPlusLibrary(
  data: DeviceLibraryPlusData,
  volume: Volume,
  artwork: ReadonlyMap<string, string>,
): Library {
  const tracks = new Map<number, Track>();
  for (const row of data.tracks) {
    const relativePath = toRelativePath(row.path);
    const color = TRACK_COLORS[row.colorId];
    tracks.set(row.id, {
      id: String(row.id),
      title: row.title || fileNameWithoutExtension(relativePath),
      artist: row.artist,
      album: row.album,
      bpm: row.bpmx100 > 0 ? row.bpmx100 / 100 : null,
      key: row.key || null,
      durationSec: row.durationSec > 0 ? row.durationSec : null,
      relativePath,
      genre: row.genre,
      label: row.label,
      color: color && { name: row.colorName || color.name, value: color.value },
      artworkPath: row.imagePath ? artwork.get(toRelativePath(row.imagePath)) : undefined,
    });
  }

  return {
    id: libraryId('deviceLibraryPlus', volume.path),
    source: 'deviceLibraryPlus',
    volumeName: volume.name,
    volumePath: volume.path,
    tracks: [...tracks.values()],
    playlists: buildPlaylistTree(data.playlistNodes, data.playlistEntries, tracks),
  };
}

/**
 * Finds the artwork file to show for each stored artwork path. Exports often
 * reference `b<n>.jpg` files that were never written while the matching
 * `a<n>.jpg` ones exist, and each comes in a larger `_m` variant, which is
 * preferred.
 */
async function findArtwork(volumePath: string, imagePaths: string[]): Promise<Map<string, string>> {
  const byDirectory = new Map<string, Set<string>>();
  for (const imagePath of imagePaths) {
    if (!imagePath) continue;
    const relative = toRelativePath(imagePath);
    const directory = path.posix.dirname(relative);
    const paths = byDirectory.get(directory) ?? new Set();
    paths.add(relative);
    byDirectory.set(directory, paths);
  }

  const found = new Map<string, string>();
  for (const [directory, paths] of byDirectory) {
    const absolute = resolveInsideVolume(volumePath, directory);
    const names = absolute === null ? [] : await readdir(absolute).catch(() => []);
    // FAT volumes ignore case, so the database's spelling may not match the directory's.
    const byLowerCase = new Map(names.map((name) => [name.toLowerCase(), name]));
    for (const relative of paths) {
      const match = artworkCandidates(path.posix.basename(relative))
        .map((candidate) => byLowerCase.get(candidate.toLowerCase()))
        .find((name) => name !== undefined);
      if (match !== undefined) found.set(relative, `${directory}/${match}`);
    }
  }
  return found;
}

function artworkCandidates(fileName: string): string[] {
  const { name, ext } = path.posix.parse(fileName);
  const names = name.startsWith('b') ? [name, `a${name.slice(1)}`] : [name];
  return names.flatMap((candidate) => [`${candidate}_m${ext}`, `${candidate}${ext}`]);
}

async function readFileIfPresent(filePath: string): Promise<Buffer | undefined> {
  try {
    return await readFile(filePath);
  } catch {
    return undefined;
  }
}

/** node:sqlite can only open files, so the decrypted copy exists on disk only while it is read. */
async function withDatabase<T>(plain: Buffer, read: (database: DatabaseSync) => T): Promise<T> {
  const directory = await mkdtemp(path.join(os.tmpdir(), 'djusb-'));
  try {
    const databasePath = path.join(directory, 'exportLibrary.db');
    await writeFile(databasePath, plain);
    const database = new DatabaseSync(databasePath, { readOnly: true });
    try {
      return read(database);
    } finally {
      database.close();
    }
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

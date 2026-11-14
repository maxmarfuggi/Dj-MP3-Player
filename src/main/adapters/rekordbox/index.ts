import { readFile } from 'node:fs/promises';
import path from 'node:path';
import type { Library, PlaylistNode, Track } from '../../../shared/types';
import { exists, fileNameWithoutExtension, toRelativePath } from '../paths';
import { type LibraryAdapter, type Volume, libraryId } from '../types';
import { type PdbData, type PdbPlaylistNode, parsePdb } from './pdb';

const PDB_PATH = path.join('PIONEER', 'rekordbox', 'export.pdb');
/** "Device Library Plus", the encrypted SQLite database newer rekordbox versions export. */
const DEVICE_LIBRARY_PLUS_PATH = path.join('PIONEER', 'rekordbox', 'exportLibrary.db');

const ROOT_PARENT_ID = 0;

export const rekordboxAdapter: LibraryAdapter = {
  source: 'rekordbox',

  async detect(volumePath) {
    return (
      (await exists(path.join(volumePath, PDB_PATH))) ||
      (await exists(path.join(volumePath, DEVICE_LIBRARY_PLUS_PATH)))
    );
  },

  async load(volume) {
    const pdbPath = path.join(volume.path, PDB_PATH);
    if (!(await exists(pdbPath))) {
      throw new Error(
        'This USB only has a Device Library Plus export (exportLibrary.db), which is not supported yet.',
      );
    }
    const pdb = parsePdb(await readFile(pdbPath));
    if (pdb.skippedRows > 0) {
      console.warn(`${pdbPath}: skipped ${pdb.skippedRows} unreadable rows`);
    }
    return buildRekordboxLibrary(pdb, volume);
  },
};

export function buildRekordboxLibrary(pdb: PdbData, volume: Volume): Library {
  const artists = new Map(pdb.artists.map((row) => [row.id, row.name]));
  const albums = new Map(pdb.albums.map((row) => [row.id, row.name]));
  const keys = new Map(pdb.keys.map((row) => [row.id, row.name]));

  const tracks = new Map<number, Track>();
  for (const row of pdb.tracks) {
    const relativePath = toRelativePath(row.filePath);
    tracks.set(row.id, {
      id: String(row.id),
      title: row.title || fileNameWithoutExtension(relativePath),
      artist: artists.get(row.artistId) ?? '',
      album: albums.get(row.albumId) ?? '',
      bpm: row.tempo > 0 ? row.tempo / 100 : null,
      key: keys.get(row.keyId) || null,
      durationSec: row.durationSec > 0 ? row.durationSec : null,
      relativePath,
    });
  }

  return {
    id: libraryId('rekordbox', volume.path),
    source: 'rekordbox',
    volumeName: volume.name,
    volumePath: volume.path,
    tracks: [...tracks.values()],
    playlists: buildPlaylistTree(pdb, tracks),
  };
}

function buildPlaylistTree(pdb: PdbData, tracks: Map<number, Track>): PlaylistNode[] {
  const entriesByPlaylist = new Map<number, { entryIndex: number; trackId: number }[]>();
  for (const entry of pdb.playlistEntries) {
    // Entries can outlive the tracks they point to; skip those.
    if (!tracks.has(entry.trackId)) continue;
    const entries = entriesByPlaylist.get(entry.playlistId) ?? [];
    entries.push(entry);
    entriesByPlaylist.set(entry.playlistId, entries);
  }

  // The root's ID is reserved; a row using it would be its own ancestor.
  const rowsById = new Map(
    pdb.playlistNodes.filter((row) => row.id !== ROOT_PARENT_ID).map((row) => [row.id, row]),
  );
  const rowsByParent = new Map<number, PdbPlaylistNode[]>();
  for (const row of rowsById.values()) {
    // A node whose parent is missing would otherwise be unreachable.
    const parentId = rowsById.has(row.parentId) ? row.parentId : ROOT_PARENT_ID;
    const siblings = rowsByParent.get(parentId) ?? [];
    siblings.push(row);
    rowsByParent.set(parentId, siblings);
  }

  const build = (parentId: number): PlaylistNode[] =>
    (rowsByParent.get(parentId) ?? [])
      .sort((a, b) => a.sortOrder - b.sortOrder)
      .map((row) => {
        const entries = entriesByPlaylist.get(row.id) ?? [];
        return {
          id: String(row.id),
          name: row.name,
          kind: row.isFolder ? 'folder' : 'playlist',
          children: build(row.id),
          trackIds: entries
            .sort((a, b) => a.entryIndex - b.entryIndex)
            .map((entry) => String(entry.trackId)),
        };
      });

  return build(ROOT_PARENT_ID);
}

import { readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import type { Library, PlaylistNode, Track } from '../../../shared/types';
import { exists, fileNameWithoutExtension, toRelativePath } from '../paths';
import { type LibraryAdapter, type Volume, libraryId } from '../types';
import { decodeText, readChunks } from './chunks';

const DATABASE_PATH = path.join('_Serato_', 'database V2');
const SUBCRATES_PATH = path.join('_Serato_', 'Subcrates');
const CRATE_EXTENSION = '.crate';
/** `House%%Deep.crate` is the crate "Deep" nested inside the crate "House". */
const CRATE_NESTING_SEPARATOR = '%%';

export interface SeratoCrate {
  /** Full name including parents, e.g. `House%%Deep`. */
  name: string;
  trackPaths: string[];
}

export const seratoAdapter: LibraryAdapter = {
  source: 'serato',

  // Serato creates an empty `_Serato_` folder on every drive it sees, so the
  // folder alone does not mean there is a library.
  async detect(volumePath) {
    if (await exists(path.join(volumePath, DATABASE_PATH))) return true;
    return (await listCrateFiles(volumePath)).length > 0;
  },

  async load(volume) {
    const databasePath = path.join(volume.path, DATABASE_PATH);
    const tracks = (await exists(databasePath)) ? parseDatabase(await readFile(databasePath)) : [];

    const crates: SeratoCrate[] = [];
    for (const fileName of await listCrateFiles(volume.path)) {
      const file = await readFile(path.join(volume.path, SUBCRATES_PATH, fileName));
      crates.push({
        name: fileName.slice(0, -CRATE_EXTENSION.length),
        trackPaths: parseCrate(file),
      });
    }
    return buildSeratoLibrary(tracks, crates, volume);
  },
};

async function listCrateFiles(volumePath: string): Promise<string[]> {
  let fileNames: string[];
  try {
    fileNames = await readdir(path.join(volumePath, SUBCRATES_PATH));
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return [];
    throw error;
  }
  // macOS leaves `._*` metadata files on FAT/exFAT drives.
  return fileNames.filter((name) => name.endsWith(CRATE_EXTENSION) && !name.startsWith('._'));
}

/** Reads every track from a `database V2` file. */
export function parseDatabase(file: Buffer): Track[] {
  const tracks: Track[] = [];
  for (const chunk of readChunks(file)) {
    if (chunk.tag !== 'otrk') continue;
    const fields = new Map<string, string>();
    for (const field of readChunks(chunk.data)) {
      // Only text (`t…`) and path (`p…`) fields are needed; the rest are binary.
      if (field.tag.startsWith('t') || field.tag.startsWith('p')) {
        fields.set(field.tag, decodeText(field.data));
      }
    }
    const filePath = fields.get('pfil');
    if (!filePath) continue;
    const relativePath = toRelativePath(filePath);
    tracks.push({
      id: relativePath,
      title: fields.get('tsng') || fileNameWithoutExtension(relativePath),
      artist: fields.get('tart') ?? '',
      album: fields.get('talb') ?? '',
      bpm: parsePositiveNumber(fields.get('tbpm')),
      key: fields.get('tkey') || null,
      durationSec: parseDuration(fields.get('tlen')),
      relativePath,
    });
  }
  return tracks;
}

/** Reads the track paths listed in a `.crate` file, in crate order. */
export function parseCrate(file: Buffer): string[] {
  const trackPaths: string[] = [];
  for (const chunk of readChunks(file)) {
    if (chunk.tag !== 'otrk') continue;
    for (const field of readChunks(chunk.data)) {
      if (field.tag === 'ptrk') trackPaths.push(decodeText(field.data));
    }
  }
  return trackPaths;
}

export function buildSeratoLibrary(
  databaseTracks: Track[],
  crates: SeratoCrate[],
  volume: Volume,
): Library {
  const tracks = new Map<string, Track>();
  for (const track of databaseTracks) {
    if (!tracks.has(track.id)) tracks.set(track.id, track);
  }

  const nodes = new Map<string, PlaylistNode>();
  const roots: PlaylistNode[] = [];
  const nodeFor = (fullName: string): PlaylistNode => {
    const existing = nodes.get(fullName);
    if (existing) return existing;
    const separatorAt = fullName.lastIndexOf(CRATE_NESTING_SEPARATOR);
    const node: PlaylistNode = {
      id: fullName,
      name: fullName.slice(separatorAt === -1 ? 0 : separatorAt + CRATE_NESTING_SEPARATOR.length),
      kind: 'playlist',
      children: [],
      trackIds: [],
    };
    nodes.set(fullName, node);
    // A parent crate can exist only as a name prefix, without a file of its own.
    const siblings = separatorAt === -1 ? roots : nodeFor(fullName.slice(0, separatorAt)).children;
    siblings.push(node);
    return node;
  };

  for (const crate of crates) {
    const node = nodeFor(crate.name);
    for (const trackPath of crate.trackPaths) {
      const relativePath = toRelativePath(trackPath);
      if (!tracks.has(relativePath)) {
        // Crates can list files the database has no metadata for.
        tracks.set(relativePath, trackFromPath(relativePath));
      }
      node.trackIds.push(relativePath);
    }
  }
  sortByName(roots);

  return {
    id: libraryId('serato', volume.path),
    source: 'serato',
    volumeName: volume.name,
    volumePath: volume.path,
    tracks: [...tracks.values()],
    playlists: roots,
  };
}

function trackFromPath(relativePath: string): Track {
  return {
    id: relativePath,
    title: fileNameWithoutExtension(relativePath),
    artist: '',
    album: '',
    bpm: null,
    key: null,
    durationSec: null,
    relativePath,
  };
}

function sortByName(nodes: PlaylistNode[]): void {
  nodes.sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: 'base' }));
  for (const node of nodes) sortByName(node.children);
}

function parsePositiveNumber(text: string | undefined): number | null {
  const value = Number.parseFloat(text ?? '');
  return value > 0 ? value : null;
}

/** Serato stores lengths as text such as `06:22.93`. */
function parseDuration(text: string | undefined): number | null {
  if (!text) return null;
  let seconds = 0;
  for (const part of text.split(':')) {
    const value = Number.parseFloat(part);
    if (Number.isNaN(value)) return null;
    seconds = seconds * 60 + value;
  }
  return seconds > 0 ? seconds : null;
}

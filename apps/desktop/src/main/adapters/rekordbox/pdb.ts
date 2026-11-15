/**
 * Reader for rekordbox's `export.pdb` (the "DeviceSQL" database that CDJs read).
 *
 * The layout follows the reverse-engineering work documented by Deep Symmetry
 * (https://djl-analysis.deepsymmetry.org/rekordbox-export-analysis/exports.html)
 * and implemented in rekordcrate. Only the tables needed to browse and play
 * tracks are decoded.
 */

export interface PdbTrack {
  id: number;
  title: string;
  artistId: number;
  albumId: number;
  keyId: number;
  /** BPM multiplied by 100. */
  tempo: number;
  durationSec: number;
  /** Path relative to the volume root, starting with `/`. */
  filePath: string;
}

export interface PdbNamedRow {
  id: number;
  name: string;
}

export interface PdbPlaylistNode {
  id: number;
  /** 0 for nodes at the top level. */
  parentId: number;
  sortOrder: number;
  isFolder: boolean;
  name: string;
}

export interface PdbPlaylistEntry {
  playlistId: number;
  trackId: number;
  entryIndex: number;
}

export interface PdbData {
  tracks: PdbTrack[];
  artists: PdbNamedRow[];
  albums: PdbNamedRow[];
  keys: PdbNamedRow[];
  playlistNodes: PdbPlaylistNode[];
  playlistEntries: PdbPlaylistEntry[];
  /** Rows that were marked present but could not be decoded. */
  skippedRows: number;
}

const TABLE_TRACKS = 0;
const TABLE_ARTISTS = 2;
const TABLE_ALBUMS = 3;
const TABLE_KEYS = 5;
const TABLE_PLAYLIST_TREE = 7;
const TABLE_PLAYLIST_ENTRIES = 8;

const FILE_HEADER_SIZE = 28;
const TABLE_POINTER_SIZE = 16;
/** Row data ("heap") starts after the page header. */
const PAGE_HEAP_START = 40;
const PAGE_FLAG_INDEX = 0x40;
const ROWS_PER_GROUP = 16;
/** 16 two-byte row offsets, a two-byte presence bitmask and two unknown bytes. */
const ROW_GROUP_SIZE = 36;

/** Long strings start with one of these bytes; short ASCII strings have the low bit set. */
const STRING_LONG_ASCII = 0x40;
const STRING_LONG_UTF16LE = 0x90;

/** Row subtypes with this bit set store string offsets as 16-bit instead of 8-bit values. */
const SUBTYPE_WIDE_OFFSETS = 0x04;

const TRACK_STRING_OFFSETS_START = 94;
const TRACK_STRING_TITLE = 17;
const TRACK_STRING_FILE_PATH = 20;

interface TablePointer {
  type: number;
  firstPage: number;
  lastPage: number;
}

type RowDecoder<T> = (page: Buffer, row: number) => T;

export function parsePdb(file: Buffer): PdbData {
  if (file.length < FILE_HEADER_SIZE) {
    throw new Error('export.pdb is too small to be a rekordbox database');
  }
  const pageSize = file.readUInt32LE(4);
  const tableCount = file.readUInt32LE(8);
  const tablesEnd = FILE_HEADER_SIZE + tableCount * TABLE_POINTER_SIZE;
  if (pageSize < PAGE_HEAP_START + ROW_GROUP_SIZE || tablesEnd > pageSize || tablesEnd > file.length) {
    throw new Error('export.pdb has an invalid header');
  }

  const tables = new Map<number, TablePointer>();
  for (let i = 0; i < tableCount; i++) {
    const at = FILE_HEADER_SIZE + i * TABLE_POINTER_SIZE;
    const type = file.readUInt32LE(at);
    tables.set(type, {
      type,
      firstPage: file.readUInt32LE(at + 8),
      lastPage: file.readUInt32LE(at + 12),
    });
  }

  const stats = { skippedRows: 0 };
  const read = <T>(type: number, decode: RowDecoder<T>): T[] => {
    const table = tables.get(type);
    return table ? readTable(file, pageSize, table, decode, stats) : [];
  };

  return {
    tracks: read(TABLE_TRACKS, decodeTrack),
    artists: read(TABLE_ARTISTS, decodeArtist),
    albums: read(TABLE_ALBUMS, decodeAlbum),
    keys: read(TABLE_KEYS, decodeKey),
    playlistNodes: read(TABLE_PLAYLIST_TREE, decodePlaylistNode),
    playlistEntries: read(TABLE_PLAYLIST_ENTRIES, decodePlaylistEntry),
    skippedRows: stats.skippedRows,
  };
}

function readTable<T>(
  file: Buffer,
  pageSize: number,
  table: TablePointer,
  decode: RowDecoder<T>,
  stats: { skippedRows: number },
): T[] {
  const rows: T[] = [];
  const visited = new Set<number>();
  let pageIndex = table.firstPage;

  // Pages of a table form a linked list. The last page's "next" pointer leads
  // past the end of the file, so the table pointer says where to stop.
  while (!visited.has(pageIndex) && (pageIndex + 1) * pageSize <= file.length) {
    visited.add(pageIndex);
    const page = file.subarray(pageIndex * pageSize, (pageIndex + 1) * pageSize);
    if (page.readUInt32LE(8) !== table.type) break;

    const flags = page.readUInt8(27);
    if ((flags & PAGE_FLAG_INDEX) === 0) {
      // Three bytes pack the number of row offsets (13 bits) and the number of
      // rows that are still present (11 bits).
      const rowOffsetCount = page.readUIntLE(24, 3) & 0x1fff;
      for (let r = 0; r < rowOffsetCount; r++) {
        // Row groups are laid out backwards from the end of the page.
        const groupEnd = pageSize - Math.floor(r / ROWS_PER_GROUP) * ROW_GROUP_SIZE;
        const indexInGroup = r % ROWS_PER_GROUP;
        try {
          const presentMask = page.readUInt16LE(groupEnd - 4);
          if ((presentMask & (1 << indexInGroup)) === 0) continue;
          const rowStart = PAGE_HEAP_START + page.readUInt16LE(groupEnd - 6 - 2 * indexInGroup);
          rows.push(decode(page, rowStart));
        } catch (error) {
          // A row pointing outside its page is corrupt; keep the rest of the library usable.
          if (!(error instanceof RangeError)) throw error;
          stats.skippedRows++;
        }
      }
    }

    if (pageIndex === table.lastPage) break;
    pageIndex = page.readUInt32LE(12);
  }
  return rows;
}

function readString(page: Buffer, at: number): string {
  const kind = page.readUInt8(at);
  if (kind & 1) {
    // Short ASCII: the remaining bits hold the length including this byte.
    return page.toString('latin1', at + 1, at + (kind >> 1));
  }
  // Long strings: a 16-bit length that includes the 4-byte header.
  const end = at + page.readUInt16LE(at + 1);
  if (end > page.length) throw new RangeError('string runs past the end of its page');
  if (kind === STRING_LONG_ASCII) return page.toString('latin1', at + 4, end);
  if (kind === STRING_LONG_UTF16LE) return page.toString('utf16le', at + 4, end);
  return '';
}

function decodeTrack(page: Buffer, row: number): PdbTrack {
  const stringOffset = (index: number) =>
    page.readUInt16LE(row + TRACK_STRING_OFFSETS_START + 2 * index);
  return {
    keyId: page.readUInt32LE(row + 32),
    tempo: page.readUInt32LE(row + 56),
    albumId: page.readUInt32LE(row + 64),
    artistId: page.readUInt32LE(row + 68),
    id: page.readUInt32LE(row + 72),
    durationSec: page.readUInt16LE(row + 84),
    title: readString(page, row + stringOffset(TRACK_STRING_TITLE)),
    filePath: readString(page, row + stringOffset(TRACK_STRING_FILE_PATH)),
  };
}

function decodeArtist(page: Buffer, row: number): PdbNamedRow {
  const wide = (page.readUInt16LE(row) & SUBTYPE_WIDE_OFFSETS) !== 0;
  const nameOffset = wide ? page.readUInt16LE(row + 10) : page.readUInt8(row + 9);
  return {
    id: page.readUInt32LE(row + 4),
    name: readString(page, row + nameOffset),
  };
}

function decodeAlbum(page: Buffer, row: number): PdbNamedRow {
  const wide = (page.readUInt16LE(row) & SUBTYPE_WIDE_OFFSETS) !== 0;
  const nameOffset = wide ? page.readUInt16LE(row + 22) : page.readUInt8(row + 21);
  return {
    id: page.readUInt32LE(row + 12),
    name: readString(page, row + nameOffset),
  };
}

function decodeKey(page: Buffer, row: number): PdbNamedRow {
  return {
    id: page.readUInt32LE(row),
    name: readString(page, row + 8),
  };
}

function decodePlaylistNode(page: Buffer, row: number): PdbPlaylistNode {
  return {
    parentId: page.readUInt32LE(row),
    sortOrder: page.readUInt32LE(row + 8),
    id: page.readUInt32LE(row + 12),
    isFolder: page.readUInt32LE(row + 16) !== 0,
    name: readString(page, row + 20),
  };
}

function decodePlaylistEntry(page: Buffer, row: number): PdbPlaylistEntry {
  return {
    entryIndex: page.readUInt32LE(row),
    trackId: page.readUInt32LE(row + 4),
    playlistId: page.readUInt32LE(row + 8),
  };
}

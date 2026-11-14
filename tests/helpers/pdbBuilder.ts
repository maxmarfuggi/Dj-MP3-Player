/**
 * Builds minimal `export.pdb` files for tests, following the same layout the
 * parser reads: a header page with table pointers, then one or more pages per
 * table with rows at the front and the row index growing back from the end.
 */

export const PAGE_SIZE = 4096;
const HEAP_START = 40;
const PAGE_FLAGS_DATA = 0x24;
const PAGE_FLAGS_INDEX = 0x64;

export const TABLE = {
  tracks: 0,
  artists: 2,
  albums: 3,
  keys: 5,
  playlistTree: 7,
  playlistEntries: 8,
} as const;

export interface RowSpec {
  data: Buffer;
  /** Deleted rows keep their slot in the row index but lose their presence bit. */
  present?: boolean;
}

export interface PageSpec {
  rows: RowSpec[];
  /** Index pages hold no rows and must be skipped by readers. */
  indexPage?: boolean;
}

export interface TableSpec {
  type: number;
  pages: PageSpec[];
}

export function buildPdb(tables: TableSpec[]): Buffer {
  const pageCount = 1 + tables.reduce((count, table) => count + table.pages.length, 0);
  const file = Buffer.alloc(pageCount * PAGE_SIZE);
  file.writeUInt32LE(PAGE_SIZE, 4);
  file.writeUInt32LE(tables.length, 8);

  let nextFreePage = 1;
  tables.forEach((table, tableIndex) => {
    const firstPage = nextFreePage;
    const lastPage = firstPage + table.pages.length - 1;
    nextFreePage = lastPage + 1;

    const pointer = 28 + tableIndex * 16;
    file.writeUInt32LE(table.type, pointer);
    file.writeUInt32LE(firstPage, pointer + 8);
    file.writeUInt32LE(lastPage, pointer + 12);

    table.pages.forEach((page, offset) => {
      const pageIndex = firstPage + offset;
      // Like rekordbox, the last page points past the end of the file.
      const nextPage = pageIndex === lastPage ? pageCount + 100 : pageIndex + 1;
      buildPage(table.type, pageIndex, nextPage, page).copy(file, pageIndex * PAGE_SIZE);
    });
  });
  return file;
}

function buildPage(type: number, pageIndex: number, nextPage: number, spec: PageSpec): Buffer {
  const page = Buffer.alloc(PAGE_SIZE);
  page.writeUInt32LE(pageIndex, 4);
  page.writeUInt32LE(type, 8);
  page.writeUInt32LE(nextPage, 12);
  page.writeUInt8(spec.indexPage ? PAGE_FLAGS_INDEX : PAGE_FLAGS_DATA, 27);

  const presentCount = spec.rows.filter((row) => row.present !== false).length;
  page.writeUIntLE(spec.rows.length | (presentCount << 13), 24, 3);

  let heapOffset = 0;
  spec.rows.forEach((row, rowIndex) => {
    const groupEnd = PAGE_SIZE - Math.floor(rowIndex / 16) * 36;
    const indexInGroup = rowIndex % 16;
    const offsetAt = groupEnd - 6 - 2 * indexInGroup;
    if (HEAP_START + heapOffset + row.data.length > offsetAt) {
      throw new Error('Too many rows for one test page');
    }
    row.data.copy(page, HEAP_START + heapOffset);
    page.writeUInt16LE(heapOffset, offsetAt);
    if (row.present !== false) {
      page.writeUInt16LE(page.readUInt16LE(groupEnd - 4) | (1 << indexInGroup), groupEnd - 4);
    }
    heapOffset += row.data.length;
  });
  return page;
}

/** Encodes a string the way rekordbox does, picking the smallest form that fits. */
export function pdbString(text: string): Buffer {
  const isAscii = /^[\x00-\x7f]*$/.test(text);
  if (isAscii && text.length < 127) {
    return Buffer.concat([Buffer.from([((text.length + 1) << 1) | 1]), Buffer.from(text, 'latin1')]);
  }
  const body = Buffer.from(text, isAscii ? 'latin1' : 'utf16le');
  const header = Buffer.alloc(4);
  header.writeUInt8(isAscii ? 0x40 : 0x90, 0);
  header.writeUInt16LE(body.length + 4, 1);
  return Buffer.concat([header, body]);
}

export interface TrackRowFields {
  id: number;
  title: string;
  filePath: string;
  artistId?: number;
  albumId?: number;
  keyId?: number;
  tempo?: number;
  durationSec?: number;
}

export function trackRow(fields: TrackRowFields): RowSpec {
  const STRING_COUNT = 21;
  const fixed = Buffer.alloc(94 + STRING_COUNT * 2);
  fixed.writeUInt16LE(0x24, 0);
  fixed.writeUInt32LE(fields.keyId ?? 0, 32);
  fixed.writeUInt32LE(fields.tempo ?? 0, 56);
  fixed.writeUInt32LE(fields.albumId ?? 0, 64);
  fixed.writeUInt32LE(fields.artistId ?? 0, 68);
  fixed.writeUInt32LE(fields.id, 72);
  fixed.writeUInt16LE(fields.durationSec ?? 0, 84);

  const empty = pdbString('');
  const title = pdbString(fields.title);
  const filePath = pdbString(fields.filePath);
  const emptyAt = fixed.length;
  const titleAt = emptyAt + empty.length;
  const filePathAt = titleAt + title.length;
  for (let i = 0; i < STRING_COUNT; i++) {
    const at = i === 17 ? titleAt : i === 20 ? filePathAt : emptyAt;
    fixed.writeUInt16LE(at, 94 + i * 2);
  }
  return { data: Buffer.concat([fixed, empty, title, filePath]) };
}

/** `wideOffset` uses the row variant with a 16-bit name offset. */
export function artistRow(id: number, name: string, wideOffset = false): RowSpec {
  const fixed = Buffer.alloc(wideOffset ? 12 : 10);
  fixed.writeUInt16LE(wideOffset ? 0x64 : 0x60, 0);
  fixed.writeUInt32LE(id, 4);
  if (wideOffset) {
    fixed.writeUInt16LE(fixed.length, 10);
  } else {
    fixed.writeUInt8(fixed.length, 9);
  }
  return { data: Buffer.concat([fixed, pdbString(name)]) };
}

export function albumRow(id: number, name: string): RowSpec {
  const fixed = Buffer.alloc(22);
  fixed.writeUInt16LE(0x80, 0);
  fixed.writeUInt32LE(id, 12);
  fixed.writeUInt8(fixed.length, 21);
  return { data: Buffer.concat([fixed, pdbString(name)]) };
}

export function keyRow(id: number, name: string): RowSpec {
  const fixed = Buffer.alloc(8);
  fixed.writeUInt32LE(id, 0);
  fixed.writeUInt32LE(id, 4);
  return { data: Buffer.concat([fixed, pdbString(name)]) };
}

export function playlistNodeRow(fields: {
  id: number;
  parentId: number;
  sortOrder: number;
  isFolder: boolean;
  name: string;
}): RowSpec {
  const fixed = Buffer.alloc(20);
  fixed.writeUInt32LE(fields.parentId, 0);
  fixed.writeUInt32LE(fields.sortOrder, 8);
  fixed.writeUInt32LE(fields.id, 12);
  fixed.writeUInt32LE(fields.isFolder ? 1 : 0, 16);
  return { data: Buffer.concat([fixed, pdbString(fields.name)]) };
}

export function playlistEntryRow(playlistId: number, trackId: number, entryIndex: number): RowSpec {
  const data = Buffer.alloc(12);
  data.writeUInt32LE(entryIndex, 0);
  data.writeUInt32LE(trackId, 4);
  data.writeUInt32LE(playlistId, 8);
  return { data };
}

import { describe, expect, it } from 'vitest';
import { parsePdb } from '../src/main/adapters/rekordbox/pdb';
import {
  PAGE_SIZE,
  TABLE,
  albumRow,
  artistRow,
  buildPdb,
  keyRow,
  playlistEntryRow,
  playlistNodeRow,
  trackRow,
} from './helpers/pdbBuilder';

describe('parsePdb', () => {
  it('decodes tracks and the rows they reference', () => {
    const longTitle = 'A very long track title '.repeat(8);
    const pdb = parsePdb(
      buildPdb([
        {
          type: TABLE.tracks,
          pages: [
            {
              rows: [
                trackRow({
                  id: 7,
                  title: 'Mädchen',
                  filePath: '/Contents/Artist/Mädchen.mp3',
                  artistId: 2,
                  albumId: 3,
                  keyId: 4,
                  tempo: 12850,
                  durationSec: 301,
                }),
                trackRow({ id: 8, title: longTitle, filePath: '/Contents/long.wav' }),
              ],
            },
          ],
        },
        {
          type: TABLE.artists,
          pages: [{ rows: [artistRow(2, 'Short Offset'), artistRow(5, 'Wide Offset', true)] }],
        },
        { type: TABLE.albums, pages: [{ rows: [albumRow(3, 'The Album')] }] },
        { type: TABLE.keys, pages: [{ rows: [keyRow(4, 'Am')] }] },
      ]),
    );

    expect(pdb.tracks).toEqual([
      {
        id: 7,
        title: 'Mädchen',
        filePath: '/Contents/Artist/Mädchen.mp3',
        artistId: 2,
        albumId: 3,
        keyId: 4,
        tempo: 12850,
        durationSec: 301,
      },
      {
        id: 8,
        title: longTitle,
        filePath: '/Contents/long.wav',
        artistId: 0,
        albumId: 0,
        keyId: 0,
        tempo: 0,
        durationSec: 0,
      },
    ]);
    expect(pdb.artists).toEqual([
      { id: 2, name: 'Short Offset' },
      { id: 5, name: 'Wide Offset' },
    ]);
    expect(pdb.albums).toEqual([{ id: 3, name: 'The Album' }]);
    expect(pdb.keys).toEqual([{ id: 4, name: 'Am' }]);
    expect(pdb.skippedRows).toBe(0);
  });

  it('decodes the playlist tree', () => {
    const pdb = parsePdb(
      buildPdb([
        {
          type: TABLE.playlistTree,
          pages: [
            {
              rows: [
                playlistNodeRow({ id: 1, parentId: 0, sortOrder: 0, isFolder: true, name: 'Sets' }),
                playlistNodeRow({ id: 2, parentId: 1, sortOrder: 3, isFolder: false, name: 'Warm up' }),
              ],
            },
          ],
        },
      ]),
    );

    expect(pdb.playlistNodes).toEqual([
      { id: 1, parentId: 0, sortOrder: 0, isFolder: true, name: 'Sets' },
      { id: 2, parentId: 1, sortOrder: 3, isFolder: false, name: 'Warm up' },
    ]);
  });

  it('reads every row of a page that holds more than 255 rows', () => {
    // The row count does not fit one byte here; parsers that only read the
    // low byte silently lose playlist entries.
    const rows = Array.from({ length: 270 }, (_, i) => playlistEntryRow(1, i + 1, i + 1));
    const pdb = parsePdb(buildPdb([{ type: TABLE.playlistEntries, pages: [{ rows }] }]));

    expect(pdb.playlistEntries).toHaveLength(270);
    expect(pdb.playlistEntries.at(-1)).toEqual({ playlistId: 1, trackId: 270, entryIndex: 270 });
  });

  it('skips deleted rows', () => {
    const rows = [
      keyRow(1, 'Am'),
      { ...keyRow(2, 'Deleted'), present: false },
      keyRow(3, 'C'),
    ];
    const pdb = parsePdb(buildPdb([{ type: TABLE.keys, pages: [{ rows }] }]));

    expect(pdb.keys.map((key) => key.name)).toEqual(['Am', 'C']);
  });

  it('follows a table across pages and skips index pages', () => {
    const pdb = parsePdb(
      buildPdb([
        {
          type: TABLE.keys,
          pages: [
            { rows: [], indexPage: true },
            { rows: [keyRow(1, 'Am')] },
            { rows: [keyRow(2, 'C')] },
          ],
        },
        // A following table must not leak into the previous one.
        { type: TABLE.artists, pages: [{ rows: [artistRow(9, 'Artist')] }] },
      ]),
    );

    expect(pdb.keys.map((key) => key.name)).toEqual(['Am', 'C']);
    expect(pdb.artists).toEqual([{ id: 9, name: 'Artist' }]);
  });

  it('counts rows that point outside their page instead of failing', () => {
    const file = buildPdb([{ type: TABLE.keys, pages: [{ rows: [keyRow(1, 'Am'), keyRow(2, 'C')] }] }]);
    // Corrupt the heap offset of the second row (stored just before the first row's offset).
    file.writeUInt16LE(0xfff0, PAGE_SIZE + PAGE_SIZE - 8);
    const pdb = parsePdb(file);

    expect(pdb.keys).toEqual([{ id: 1, name: 'Am' }]);
    expect(pdb.skippedRows).toBe(1);
  });

  it('rejects files that are not a rekordbox database', () => {
    expect(() => parsePdb(Buffer.alloc(8))).toThrow(/too small/);
    expect(() => parsePdb(Buffer.alloc(PAGE_SIZE))).toThrow(/invalid header/);
  });
});

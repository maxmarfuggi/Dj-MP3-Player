import { describe, expect, it } from 'vitest';
import { buildSeratoLibrary, parseCrate, parseDatabase } from '../src/main/adapters/serato';
import { seratoCrate, seratoDatabase } from './helpers/seratoBuilder';

const volume = { path: '/Volumes/GIGS', name: 'GIGS' };

describe('parseDatabase', () => {
  it('reads track metadata', () => {
    const tracks = parseDatabase(
      seratoDatabase([
        {
          pfil: 'Music/Björk - Jóga.mp3',
          tsng: 'Jóga',
          tart: 'Björk',
          talb: 'Homogenic',
          tbpm: '126.00',
          tkey: 'Bb',
          tlen: '06:22.93',
        },
      ]),
    );

    expect(tracks).toEqual([
      {
        id: 'Music/Björk - Jóga.mp3',
        title: 'Jóga',
        artist: 'Björk',
        album: 'Homogenic',
        bpm: 126,
        key: 'Bb',
        durationSec: 382.93,
        relativePath: 'Music/Björk - Jóga.mp3',
      },
    ]);
  });

  it('falls back to the file name when a track has no tags', () => {
    const [track] = parseDatabase(seratoDatabase([{ pfil: 'Music/Untitled Jam.wav' }]));

    expect(track).toMatchObject({
      title: 'Untitled Jam',
      artist: '',
      bpm: null,
      key: null,
      durationSec: null,
    });
  });

  it('ignores a truncated record at the end of the file', () => {
    const file = seratoDatabase([{ pfil: 'a.mp3' }, { pfil: 'b.mp3' }]);

    expect(parseDatabase(file.subarray(0, file.length - 3)).map((track) => track.id)).toEqual(['a.mp3']);
  });
});

describe('parseCrate', () => {
  it('lists track paths in crate order', () => {
    expect(parseCrate(seratoCrate(['Music/b.mp3', 'Music/a.mp3']))).toEqual(['Music/b.mp3', 'Music/a.mp3']);
  });
});

describe('buildSeratoLibrary', () => {
  it('nests crates by their `%%` separated names', () => {
    const tracks = parseDatabase(seratoDatabase([{ pfil: 'a.mp3' }, { pfil: 'b.mp3' }]));
    const library = buildSeratoLibrary(
      tracks,
      [
        { name: 'House%%Deep', trackPaths: ['b.mp3'] },
        { name: 'House', trackPaths: ['a.mp3', 'b.mp3'] },
        // "Techno" has no crate file of its own.
        { name: 'Techno%%Hard', trackPaths: ['a.mp3'] },
      ],
      volume,
    );

    expect(library.playlists).toEqual([
      {
        id: 'House',
        name: 'House',
        kind: 'playlist',
        trackIds: ['a.mp3', 'b.mp3'],
        children: [{ id: 'House%%Deep', name: 'Deep', kind: 'playlist', trackIds: ['b.mp3'], children: [] }],
      },
      {
        id: 'Techno',
        name: 'Techno',
        kind: 'playlist',
        trackIds: [],
        children: [{ id: 'Techno%%Hard', name: 'Hard', kind: 'playlist', trackIds: ['a.mp3'], children: [] }],
      },
    ]);
  });

  it('adds tracks that only a crate knows about', () => {
    const library = buildSeratoLibrary([], [{ name: 'New', trackPaths: ['Music/Fresh Tune.flac'] }], volume);

    expect(library.tracks).toMatchObject([{ id: 'Music/Fresh Tune.flac', title: 'Fresh Tune' }]);
    expect(library.playlists[0]?.trackIds).toEqual(['Music/Fresh Tune.flac']);
  });
});

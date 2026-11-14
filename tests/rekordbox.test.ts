import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { buildRekordboxLibrary, rekordboxAdapter } from '../src/main/adapters/rekordbox';
import type { PdbData, PdbTrack } from '../src/main/adapters/rekordbox/pdb';
import { TABLE, buildPdb, playlistEntryRow, playlistNodeRow, trackRow } from './helpers/pdbBuilder';

const volume = { path: '/Volumes/GIGS', name: 'GIGS' };

function pdbTrack(id: number, overrides: Partial<PdbTrack> = {}): PdbTrack {
  return {
    id,
    title: `Track ${id}`,
    artistId: 0,
    albumId: 0,
    keyId: 0,
    tempo: 0,
    durationSec: 0,
    filePath: `/Contents/track-${id}.mp3`,
    ...overrides,
  };
}

function pdbData(overrides: Partial<PdbData>): PdbData {
  return {
    tracks: [],
    artists: [],
    albums: [],
    keys: [],
    playlistNodes: [],
    playlistEntries: [],
    skippedRows: 0,
    ...overrides,
  };
}

describe('buildRekordboxLibrary', () => {
  it('joins tracks with their artist, album and key', () => {
    const library = buildRekordboxLibrary(
      pdbData({
        tracks: [
          pdbTrack(1, { artistId: 10, albumId: 20, keyId: 30, tempo: 12850, durationSec: 301 }),
          pdbTrack(2, { title: '', filePath: '/Contents/Some Artist/Untitled Jam.wav' }),
        ],
        artists: [{ id: 10, name: 'DJ Example' }],
        albums: [{ id: 20, name: 'The Album' }],
        keys: [{ id: 30, name: 'Am' }],
      }),
      volume,
    );

    expect(library).toMatchObject({ source: 'rekordbox', volumeName: 'GIGS', volumePath: '/Volumes/GIGS' });
    expect(library.tracks).toEqual([
      {
        id: '1',
        title: 'Track 1',
        artist: 'DJ Example',
        album: 'The Album',
        bpm: 128.5,
        key: 'Am',
        durationSec: 301,
        relativePath: 'Contents/track-1.mp3',
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
      },
    ]);
  });

  it('builds the playlist tree in rekordbox order', () => {
    const library = buildRekordboxLibrary(
      pdbData({
        tracks: [pdbTrack(1), pdbTrack(2), pdbTrack(3)],
        playlistNodes: [
          { id: 3, parentId: 1, sortOrder: 1, isFolder: false, name: 'Peak time' },
          { id: 2, parentId: 1, sortOrder: 0, isFolder: false, name: 'Warm up' },
          { id: 1, parentId: 0, sortOrder: 0, isFolder: true, name: 'Friday' },
        ],
        playlistEntries: [
          { playlistId: 2, trackId: 3, entryIndex: 2 },
          { playlistId: 2, trackId: 1, entryIndex: 1 },
          { playlistId: 3, trackId: 2, entryIndex: 1 },
        ],
      }),
      volume,
    );

    expect(library.playlists).toEqual([
      {
        id: '1',
        name: 'Friday',
        kind: 'folder',
        trackIds: [],
        children: [
          { id: '2', name: 'Warm up', kind: 'playlist', trackIds: ['1', '3'], children: [] },
          { id: '3', name: 'Peak time', kind: 'playlist', trackIds: ['2'], children: [] },
        ],
      },
    ]);
  });

  it('drops playlist entries whose track no longer exists', () => {
    const library = buildRekordboxLibrary(
      pdbData({
        tracks: [pdbTrack(1)],
        playlistNodes: [{ id: 1, parentId: 0, sortOrder: 0, isFolder: false, name: 'Set' }],
        playlistEntries: [
          { playlistId: 1, trackId: 1, entryIndex: 1 },
          { playlistId: 1, trackId: 99, entryIndex: 2 },
        ],
      }),
      volume,
    );

    expect(library.playlists[0]?.trackIds).toEqual(['1']);
  });

  it('keeps playlists whose parent folder is missing', () => {
    const library = buildRekordboxLibrary(
      pdbData({
        playlistNodes: [{ id: 5, parentId: 42, sortOrder: 0, isFolder: false, name: 'Orphan' }],
      }),
      volume,
    );

    expect(library.playlists.map((node) => node.name)).toEqual(['Orphan']);
  });
});

describe('rekordboxAdapter', () => {
  let root: string;
  let databaseDirectory: string;

  beforeEach(async () => {
    root = await mkdtemp(path.join(os.tmpdir(), 'rekordbox-usb-'));
    databaseDirectory = path.join(root, 'PIONEER', 'rekordbox');
    await mkdir(databaseDirectory, { recursive: true });
  });

  afterEach(async () => {
    await rm(root, { recursive: true, force: true });
  });

  it('ignores volumes without a rekordbox export', async () => {
    expect(await rekordboxAdapter.detect(root)).toBe(false);
  });

  it('loads export.pdb from a volume', async () => {
    await writeFile(
      path.join(databaseDirectory, 'export.pdb'),
      buildPdb([
        { type: TABLE.tracks, pages: [{ rows: [trackRow({ id: 1, title: 'Opener', filePath: '/Contents/opener.mp3' })] }] },
        {
          type: TABLE.playlistTree,
          pages: [{ rows: [playlistNodeRow({ id: 1, parentId: 0, sortOrder: 0, isFolder: false, name: 'Set' })] }],
        },
        { type: TABLE.playlistEntries, pages: [{ rows: [playlistEntryRow(1, 1, 1)] }] },
      ]),
    );

    expect(await rekordboxAdapter.detect(root)).toBe(true);
    const library = await rekordboxAdapter.load({ path: root, name: 'GIGS' });
    expect(library.tracks.map((track) => track.title)).toEqual(['Opener']);
    expect(library.playlists.map((node) => [node.name, node.trackIds])).toEqual([['Set', ['1']]]);
  });

  it('leaves a Device Library Plus only export to its own adapter', async () => {
    await writeFile(path.join(databaseDirectory, 'exportLibrary.db'), 'encrypted');

    expect(await rekordboxAdapter.detect(root)).toBe(false);
  });
});

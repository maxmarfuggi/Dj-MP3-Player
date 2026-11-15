import { describe, expect, it } from 'vitest';
import {
  currentEntryIndex,
  findPlaylist,
  libraryWasRemoved,
  listEntries,
  listViewKey,
  queueFromEntries,
  visibleLibrary,
  visiblePlaylist,
} from '../src/logic/libraryView';
import type { Library, PlaylistNode, Track } from '../src/shared/types';

function track(id: string, title = id): Track {
  return {
    id,
    title,
    artist: '',
    album: '',
    bpm: null,
    key: null,
    durationSec: null,
    relativePath: `${id}.mp3`,
  };
}

function library(id: string, tracks: Track[], playlists: PlaylistNode[] = []): Library {
  return { id, source: 'rekordbox', volumeName: id, volumePath: `/${id}`, tracks, playlists };
}

const nested: PlaylistNode = {
  id: 'folder',
  name: 'Folder',
  kind: 'folder',
  trackIds: [],
  children: [{ id: 'set', name: 'Set', kind: 'playlist', trackIds: ['b', 'a', 'gone'], children: [] }],
};

describe('visibleLibrary', () => {
  const libraries = [library('first', []), library('second', [])];

  it('uses the selected library', () => {
    expect(visibleLibrary(libraries, 'second')?.id).toBe('second');
  });

  it('falls back to the first library when nothing is selected or the USB was removed', () => {
    expect(visibleLibrary(libraries, undefined)?.id).toBe('first');
    expect(visibleLibrary(libraries, 'unplugged')?.id).toBe('first');
    expect(visibleLibrary([], 'first')).toBeUndefined();
  });
});

describe('findPlaylist', () => {
  it('finds a playlist nested in a folder', () => {
    expect(findPlaylist([nested], 'set')?.name).toBe('Set');
    expect(findPlaylist([nested], 'missing')).toBeUndefined();
  });
});

describe('visiblePlaylist', () => {
  const shown = library('lib', [], [nested]);

  it('returns the selected playlist of the library on screen', () => {
    expect(visiblePlaylist(shown, { libraryId: 'lib', playlistId: 'set' })?.id).toBe('set');
  });

  it('shows all tracks when the selection is not a playlist of this library', () => {
    expect(visiblePlaylist(shown, { libraryId: 'lib', playlistId: null })).toBeUndefined();
    expect(visiblePlaylist(shown, { libraryId: 'other', playlistId: 'set' })).toBeUndefined();
    expect(visiblePlaylist(shown, { libraryId: 'lib', playlistId: 'missing' })).toBeUndefined();
    expect(visiblePlaylist(undefined, { libraryId: 'lib', playlistId: 'set' })).toBeUndefined();
  });
});

describe('listEntries', () => {
  const tracks = [track('b', 'Beta'), track('a', 'Alpha')];
  const shown = library('lib', tracks, [nested]);
  const playlist = nested.children[0];

  it('sorts all tracks by title', () => {
    expect(listEntries(shown, undefined, undefined, null).map((entry) => entry.track.id)).toEqual(['a', 'b']);
  });

  it('lists a playlist in its order and skips tracks the library no longer has', () => {
    expect(listEntries(shown, playlist, undefined, null).map((entry) => entry.track.id)).toEqual(['b', 'a']);
  });

  it('applies a saved custom order', () => {
    expect(listEntries(shown, playlist, ['a', 'b'], null).map((entry) => entry.track.id)).toEqual(['a', 'b']);
  });

  it('sorts the rows when a column sort is active', () => {
    const titles = listEntries(shown, playlist, undefined, { column: 'title', direction: 'asc' }).map(
      (entry) => entry.track.title,
    );
    expect(titles).toEqual(['Alpha', 'Beta']);
  });

  it('returns nothing when no library is mounted', () => {
    expect(listEntries(undefined, undefined, undefined, null)).toEqual([]);
  });
});

describe('queueFromEntries', () => {
  it('builds a queue from the rows on screen', () => {
    const entries = listEntries(library('lib', [track('a')]), undefined, undefined, null);
    expect(queueFromEntries('lib', entries)).toEqual([
      { libraryId: 'lib', track: entries[0]!.track, entryKey: 'a#0' },
    ]);
  });

  it('is empty until a library is showing', () => {
    expect(queueFromEntries(undefined, [])).toEqual([]);
  });
});

describe('listViewKey', () => {
  it('names a playlist list and an all-tracks list differently', () => {
    const shown = library('lib', []);
    expect(listViewKey(shown, nested.children[0])).toBe('lib/set');
    expect(listViewKey(shown, undefined)).toBe('lib:all');
  });
});

describe('libraryWasRemoved', () => {
  const libraries = [library('still', [])];

  it('is true only when the playing library is gone', () => {
    expect(libraryWasRemoved(libraries, 'still')).toBe(false);
    expect(libraryWasRemoved(libraries, 'unplugged')).toBe(true);
    expect(libraryWasRemoved(libraries, undefined)).toBe(false);
  });
});

describe('currentEntryIndex', () => {
  const entries = listEntries(library('lib', [track('a'), track('a')]), undefined, undefined, null);

  it('prefers the queue position, and otherwise matches the track', () => {
    expect(currentEntryIndex(entries, 'a#1', 'a')).toBe(1);
    expect(currentEntryIndex(entries, undefined, 'a')).toBe(0);
    expect(currentEntryIndex(entries, undefined, 'missing')).toBe(-1);
  });
});

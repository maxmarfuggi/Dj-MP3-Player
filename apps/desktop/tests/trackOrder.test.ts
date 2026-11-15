import { describe, expect, it } from 'vitest';
import {
  moveItem,
  nextSort,
  reconcileOrder,
  sortEntries,
  toEntries,
} from '../src/renderer/src/trackOrder';
import type { Track } from '../src/shared/types';

function track(id: string, fields: Partial<Track> = {}): Track {
  return {
    id,
    title: id,
    artist: '',
    album: '',
    bpm: null,
    key: null,
    durationSec: null,
    relativePath: `${id}.mp3`,
    ...fields,
  };
}

const titles = (entries: { track: Track }[]) => entries.map((entry) => entry.track.title);

describe('toEntries', () => {
  it('gives repeated tracks distinct keys', () => {
    const a = track('a');
    expect(toEntries([a, track('b'), a]).map((entry) => entry.key)).toEqual(['a#0', 'b#0', 'a#1']);
  });
});

describe('reconcileOrder', () => {
  it('uses the saved order when the playlist is unchanged', () => {
    expect(reconcileOrder(['c', 'a', 'b'], ['a', 'b', 'c'])).toEqual(['c', 'a', 'b']);
  });

  it('drops removed tracks and appends new ones in playlist order', () => {
    expect(reconcileOrder(['c', 'x', 'a'], ['a', 'd', 'c', 'e'])).toEqual(['c', 'a', 'd', 'e']);
  });

  it('matches repeated tracks by count', () => {
    expect(reconcileOrder(['b', 'a', 'a', 'a'], ['a', 'b', 'a'])).toEqual(['b', 'a', 'a']);
    expect(reconcileOrder(['a', 'b'], ['a', 'b', 'a'])).toEqual(['a', 'b', 'a']);
  });
});

describe('moveItem', () => {
  it('moves an item down or up', () => {
    expect(moveItem(['a', 'b', 'c', 'd'], 0, 2)).toEqual(['b', 'c', 'a', 'd']);
    expect(moveItem(['a', 'b', 'c', 'd'], 3, 0)).toEqual(['d', 'a', 'b', 'c']);
  });

  it('clamps the target to the list', () => {
    expect(moveItem(['a', 'b', 'c'], 0, 10)).toEqual(['b', 'c', 'a']);
  });
});

describe('sortEntries', () => {
  const entries = toEntries([
    track('Zeta', { artist: 'Bravo', bpm: 128 }),
    track('alpha', { artist: '', bpm: null }),
    track('Track 10', { artist: 'alpha', bpm: 124.5 }),
    track('Track 2', { artist: 'Alpha', bpm: 128 }),
  ]);

  it('sorts titles case-insensitively with natural number order', () => {
    expect(titles(sortEntries(entries, { column: 'title', direction: 'asc' }))).toEqual([
      'alpha',
      'Track 2',
      'Track 10',
      'Zeta',
    ]);
    expect(titles(sortEntries(entries, { column: 'title', direction: 'desc' }))).toEqual([
      'Zeta',
      'Track 10',
      'Track 2',
      'alpha',
    ]);
  });

  it('sorts by artist, then title, with blank artists last either way', () => {
    expect(titles(sortEntries(entries, { column: 'artist', direction: 'asc' }))).toEqual([
      'Track 2',
      'Track 10',
      'Zeta',
      'alpha',
    ]);
    expect(titles(sortEntries(entries, { column: 'artist', direction: 'desc' })).at(-1)).toBe('alpha');
  });

  it('sorts by genre or label, with tracks that have none last', () => {
    const tagged = toEntries([
      track('b', { genre: 'Techno', label: 'Kompakt' }),
      track('a', {}),
      track('d', { genre: 'house', label: '' }),
      track('c', { genre: 'House', label: 'Defected' }),
    ]);
    expect(titles(sortEntries(tagged, { column: 'genre', direction: 'asc' }))).toEqual(['c', 'd', 'b', 'a']);
    expect(titles(sortEntries(tagged, { column: 'genre', direction: 'desc' }))).toEqual(['b', 'c', 'd', 'a']);
    expect(titles(sortEntries(tagged, { column: 'label', direction: 'asc' }))).toEqual(['c', 'b', 'a', 'd']);
  });

  it('sorts by BPM with missing BPMs last and ties in list order', () => {
    expect(titles(sortEntries(entries, { column: 'bpm', direction: 'asc' }))).toEqual([
      'Track 10',
      'Zeta',
      'Track 2',
      'alpha',
    ]);
    expect(titles(sortEntries(entries, { column: 'bpm', direction: 'desc' }))).toEqual([
      'Zeta',
      'Track 2',
      'Track 10',
      'alpha',
    ]);
  });
});

describe('nextSort', () => {
  it('cycles ascending, descending, off', () => {
    const asc = nextSort(null, 'bpm');
    expect(asc).toEqual({ column: 'bpm', direction: 'asc' });
    const desc = nextSort(asc, 'bpm');
    expect(desc).toEqual({ column: 'bpm', direction: 'desc' });
    expect(nextSort(desc, 'bpm')).toBeNull();
    expect(nextSort(desc, 'title')).toEqual({ column: 'title', direction: 'asc' });
  });
});

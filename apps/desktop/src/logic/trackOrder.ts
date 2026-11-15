import type { Track } from '../shared/types';

export type SortColumn = 'title' | 'artist' | 'genre' | 'label' | 'bpm';

export interface SortState {
  column: SortColumn;
  direction: 'asc' | 'desc';
}

/** A track as it appears at one position of a list; playlists can repeat a track. */
export interface TrackEntry {
  /** Unique within its list: the track ID plus which repeat of it this is. */
  key: string;
  track: Track;
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' });

export function toEntries(tracks: Track[]): TrackEntry[] {
  const seen = new Map<string, number>();
  return tracks.map((track) => {
    const occurrence = seen.get(track.id) ?? 0;
    seen.set(track.id, occurrence + 1);
    return { key: `${track.id}#${occurrence}`, track };
  });
}

/**
 * Applies a saved custom order to a playlist's current track IDs. The playlist
 * may have changed since the order was saved (the USB was re-exported): saved
 * tracks that are gone are dropped and tracks the order does not know about are
 * appended in their playlist order. Repeated tracks are matched by count.
 */
export function reconcileOrder(saved: readonly string[], current: readonly string[]): string[] {
  const remaining = new Map<string, number>();
  for (const id of current) remaining.set(id, (remaining.get(id) ?? 0) + 1);

  const result: string[] = [];
  const take = (id: string) => {
    const count = remaining.get(id) ?? 0;
    if (count === 0) return;
    remaining.set(id, count - 1);
    result.push(id);
  };
  saved.forEach(take);
  current.forEach(take);
  return result;
}

/** Moves the item at `from` so that it ends up at index `to` of the result. */
export function moveItem<T>(items: readonly T[], from: number, to: number): T[] {
  const result = [...items];
  const [item] = result.splice(from, 1);
  if (item === undefined) return result;
  result.splice(Math.max(0, Math.min(to, result.length)), 0, item);
  return result;
}

/**
 * Sorts entries by a column. Tracks without a BPM, or with a blank artist,
 * genre or label, always go last; ties keep list order.
 */
export function sortEntries(entries: readonly TrackEntry[], sort: SortState): TrackEntry[] {
  const sign = sort.direction === 'asc' ? 1 : -1;
  const compareText = (x: string | undefined, y: string | undefined, a: Track, b: Track): number => {
    if (!x || !y) return Number(!x) - Number(!y);
    return sign * collator.compare(x, y) || collator.compare(a.title, b.title);
  };
  const compare = (a: Track, b: Track): number => {
    switch (sort.column) {
      case 'title':
        return sign * collator.compare(a.title, b.title);
      case 'artist':
        return compareText(a.artist, b.artist, a, b);
      case 'genre':
        return compareText(a.genre, b.genre, a, b);
      case 'label':
        return compareText(a.label, b.label, a, b);
      case 'bpm':
        if (a.bpm === null || b.bpm === null) return Number(a.bpm === null) - Number(b.bpm === null);
        return sign * (a.bpm - b.bpm);
    }
  };
  return entries
    .map((entry, index) => ({ entry, index }))
    .sort((a, b) => compare(a.entry.track, b.entry.track) || a.index - b.index)
    .map(({ entry }) => entry);
}

export function sortByTitle(tracks: readonly Track[]): Track[] {
  return [...tracks].sort((a, b) => collator.compare(a.title, b.title));
}

/** Clicking a column header sorts ascending, then descending, then turns sorting off. */
export function nextSort(sort: SortState | null, column: SortColumn): SortState | null {
  if (sort?.column !== column) return { column, direction: 'asc' };
  return sort.direction === 'asc' ? { column, direction: 'desc' } : null;
}

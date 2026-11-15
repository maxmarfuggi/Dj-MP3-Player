import type { Library, PlaylistNode, Track } from '../shared/types';
import {
  type SortState,
  type TrackEntry,
  reconcileOrder,
  sortByTitle,
  sortEntries,
  toEntries,
} from './trackOrder';

/** One row of the queue built from the list on screen. */
export interface QueueEntry {
  libraryId: string;
  track: Track;
  /** Identifies this position; a playlist can repeat a track. */
  entryKey: string;
}

/** `<libraryId>/<playlistId>`, the key a custom playlist order is saved under. */
export function orderKey(libraryId: string, playlistId: string): string {
  return `${libraryId}/${playlistId}`;
}

/**
 * The library to show. A selection that names a library still mounted wins;
 * otherwise the first library is used, which is what happens when a USB is removed.
 */
export function visibleLibrary(
  libraries: readonly Library[],
  selectedLibraryId: string | undefined,
): Library | undefined {
  return libraries.find((candidate) => candidate.id === selectedLibraryId) ?? libraries[0];
}

export function findPlaylist(nodes: readonly PlaylistNode[], id: string): PlaylistNode | undefined {
  for (const node of nodes) {
    const found = node.id === id ? node : findPlaylist(node.children, id);
    if (found) return found;
  }
  return undefined;
}

/**
 * The playlist named by the selection, when that selection belongs to `library`.
 * All tracks, a selection for another library, and a playlist that no longer
 * exists all mean there is no playlist to show.
 */
export function visiblePlaylist(
  library: Library | undefined,
  selection: { libraryId: string; playlistId: string | null } | null,
): PlaylistNode | undefined {
  if (!library || selection?.libraryId !== library.id || selection.playlistId === null) return undefined;
  return findPlaylist(library.playlists, selection.playlistId);
}

/** Identifies the list on screen. The player uses it to tell one queue from another. */
export function listViewKey(library: Library | undefined, playlist: PlaylistNode | undefined): string {
  return library && playlist ? orderKey(library.id, playlist.id) : `${library?.id}:all`;
}

/** The rows of the list on screen, in custom order when there is one, then sorted. */
export function listEntries(
  library: Library | undefined,
  playlist: PlaylistNode | undefined,
  customOrder: readonly string[] | undefined,
  sort: SortState | null,
): TrackEntry[] {
  if (!library) return [];
  const tracks = playlist ? tracksInPlaylist(library, playlist, customOrder) : sortByTitle(library.tracks);
  const listed = toEntries(tracks);
  return sort ? sortEntries(listed, sort) : listed;
}

function tracksInPlaylist(
  library: Library,
  playlist: PlaylistNode,
  customOrder: readonly string[] | undefined,
): Track[] {
  const tracksById = new Map(library.tracks.map((track) => [track.id, track]));
  const trackIds = customOrder ? reconcileOrder(customOrder, playlist.trackIds) : playlist.trackIds;
  return trackIds.flatMap((id) => tracksById.get(id) ?? []);
}

export function queueFromEntries(libraryId: string | undefined, entries: readonly TrackEntry[]): QueueEntry[] {
  if (libraryId === undefined) return [];
  return entries.map((entry) => ({ libraryId, track: entry.track, entryKey: entry.key }));
}

/** The track that is playing lives on a library that is no longer mounted. */
export function libraryWasRemoved(libraries: readonly Library[], playingLibraryId: string | undefined): boolean {
  return playingLibraryId !== undefined && !libraries.some((candidate) => candidate.id === playingLibraryId);
}

/**
 * Which row is the one playing. A list prefers its own queue position; a track
 * started from another list of the same library is matched by track id.
 */
export function currentEntryIndex(
  entries: readonly TrackEntry[],
  playingEntryKey: string | undefined,
  playingTrackId: string | undefined,
): number {
  return playingEntryKey !== undefined
    ? entries.findIndex((entry) => entry.key === playingEntryKey)
    : entries.findIndex((entry) => entry.track.id === playingTrackId);
}

export type LibrarySource = 'rekordbox' | 'serato';

export interface Track {
  id: string;
  title: string;
  artist: string;
  album: string;
  bpm: number | null;
  key: string | null;
  durationSec: number | null;
  /** Path of the audio file relative to the volume root, using `/` separators. */
  relativePath: string;
}

export interface PlaylistNode {
  id: string;
  name: string;
  /**
   * rekordbox folders only hold other nodes. Serato crates are always
   * `playlist`, but can hold both tracks and child crates.
   */
  kind: 'folder' | 'playlist';
  children: PlaylistNode[];
  trackIds: string[];
}

export interface Library {
  id: string;
  source: LibrarySource;
  /** Display name of the volume, e.g. the USB stick's label. */
  volumeName: string;
  volumePath: string;
  tracks: Track[];
  playlists: PlaylistNode[];
}

export interface LibraryLoadError {
  volumePath: string;
  volumeName: string;
  source: LibrarySource;
  message: string;
}

export interface LibraryState {
  libraries: Library[];
  errors: LibraryLoadError[];
}

export type OpenFolderResult =
  | { status: 'opened' }
  | { status: 'cancelled' }
  | { status: 'no-library'; folderPath: string };

/** The API the preload script exposes to the renderer as `window.djusb`. */
export interface DjUsbApi {
  getState(): Promise<LibraryState>;
  onStateChanged(listener: (state: LibraryState) => void): () => void;
  /** Lets the user pick a folder that contains a DJ library, e.g. a copy of a USB. */
  openFolder(): Promise<OpenFolderResult>;
}

export const MEDIA_SCHEME = 'media';

/** URL the renderer hands to `<audio>`; resolved to a file by the main process. */
export function trackUrl(libraryId: string, trackId: string): string {
  return `${MEDIA_SCHEME}://track/${encodeURIComponent(libraryId)}/${encodeURIComponent(trackId)}`;
}

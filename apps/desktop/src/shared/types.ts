// The data types in this file are checked against `crates/djusb-core`. A Rust
// test fails when the two disagree. `DjUsbApi` and `MEDIA_SCHEME` are desktop-only.

/** `rekordbox` is rekordbox's Device Library (`export.pdb`). */
export type LibrarySource = 'rekordbox' | 'deviceLibraryPlus' | 'serato';

export interface TrackColor {
  name: string;
  /** CSS colour. */
  value: string;
}

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
  // Only Device Library Plus provides the fields below.
  genre?: string;
  label?: string;
  color?: TrackColor;
  /** Path of an artwork image that exists on the volume, relative to its root. */
  artworkPath?: string;
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

/** Scheme the UI uses to ask the main process for a track or its artwork. */
export const MEDIA_SCHEME = 'media';

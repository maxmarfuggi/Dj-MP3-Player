import { MEDIA_SCHEME, type LibraryState, type OpenFolderResult } from '../../shared/types';

/** The libraries currently loaded, including exports that failed to read. */
export function getState(): Promise<LibraryState> {
  return window.djusb.getState();
}

/** Calls `listener` whenever a USB is added or removed. Returns an unsubscribe function. */
export function onStateChanged(listener: (state: LibraryState) => void): () => void {
  return window.djusb.onStateChanged(listener);
}

/** Asks the user for a folder that contains a DJ library, such as a copy of a USB. */
export function openFolder(): Promise<OpenFolderResult> {
  return window.djusb.openFolder();
}

/** URL the player hands to `<audio>`. The main process resolves it to a file. */
export function trackUrl(libraryId: string, trackId: string): string {
  return `${MEDIA_SCHEME}://track/${encodeURIComponent(libraryId)}/${encodeURIComponent(trackId)}`;
}

/** URL of a track's artwork, for tracks that have an `artworkPath`. */
export function artworkUrl(libraryId: string, trackId: string): string {
  return `${MEDIA_SCHEME}://artwork/${encodeURIComponent(libraryId)}/${encodeURIComponent(trackId)}`;
}

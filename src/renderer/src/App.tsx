import { useEffect, useMemo, useState } from 'react';
import type { LibraryState, PlaylistNode } from '../../shared/types';
import { PlayerBar } from './PlayerBar';
import { type Selection, Sidebar } from './Sidebar';
import { TrackList } from './TrackList';
import { usePlayer } from './usePlayer';

const titleOrder = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' });

function useLibraryState(): LibraryState {
  const [state, setState] = useState<LibraryState>({ libraries: [], errors: [] });
  useEffect(() => {
    const unsubscribe = window.djusb.onStateChanged(setState);
    void window.djusb.getState().then(setState);
    return unsubscribe;
  }, []);
  return state;
}

function findPlaylist(nodes: PlaylistNode[], id: string): PlaylistNode | undefined {
  for (const node of nodes) {
    const found = node.id === id ? node : findPlaylist(node.children, id);
    if (found) return found;
  }
  return undefined;
}

export function App() {
  const { libraries, errors } = useLibraryState();
  const player = usePlayer();
  const [selection, setSelection] = useState<Selection | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  // Fall back to the first library when nothing is selected or the selected USB was removed.
  const library = libraries.find((candidate) => candidate.id === selection?.libraryId) ?? libraries[0];
  const playlist =
    library && selection?.libraryId === library.id && selection.playlistId !== null
      ? findPlaylist(library.playlists, selection.playlistId)
      : undefined;

  const tracks = useMemo(() => {
    if (!library) return [];
    if (!playlist) {
      return [...library.tracks].sort((a, b) => titleOrder.compare(a.title, b.title));
    }
    const tracksById = new Map(library.tracks.map((track) => [track.id, track]));
    return playlist.trackIds.flatMap((id) => tracksById.get(id) ?? []);
  }, [library, playlist]);

  const { current, stop, togglePlay } = player;
  useEffect(() => {
    // The USB that the current track lives on was unplugged.
    if (current && !libraries.some((candidate) => candidate.id === current.libraryId)) stop();
  }, [libraries, current, stop]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      // Space already activates a focused button or slider.
      const onControl =
        event.target instanceof HTMLButtonElement || event.target instanceof HTMLInputElement;
      if (event.code !== 'Space' || onControl) return;
      event.preventDefault();
      togglePlay();
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [togglePlay]);

  const openFolder = async () => {
    const result = await window.djusb.openFolder();
    setNotice(
      result.status === 'no-library'
        ? `No rekordbox or Serato library found in ${result.folderPath}`
        : null,
    );
  };

  return (
    <div className="app">
      <Sidebar
        libraries={libraries}
        selection={library ? { libraryId: library.id, playlistId: playlist?.id ?? null } : null}
        onSelect={setSelection}
        onOpenFolder={openFolder}
      />

      <main className="content">
        {errors.map((error) => (
          <p key={`${error.volumePath}/${error.source}`} className="banner banner-error">
            Could not read the {error.source} library on “{error.volumeName}”: {error.message}
          </p>
        ))}
        {notice && (
          <p className="banner">
            {notice}
            <button className="banner-dismiss" aria-label="Dismiss" onClick={() => setNotice(null)}>
              ×
            </button>
          </p>
        )}

        {library ? (
          <>
            <header className="content-header">
              <h1>{playlist?.name ?? 'All tracks'}</h1>
              <span className="content-meta">
                {library.volumeName} · {tracks.length} {tracks.length === 1 ? 'track' : 'tracks'}
              </span>
            </header>
            <TrackList libraryId={library.id} tracks={tracks} player={player} />
          </>
        ) : (
          <div className="empty-state">
            <h1>Plug in a DJ USB</h1>
            <p>rekordbox and Serato libraries show up here automatically.</p>
          </div>
        )}
      </main>

      <PlayerBar player={player} />
    </div>
  );
}

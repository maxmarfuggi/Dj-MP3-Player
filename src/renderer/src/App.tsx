import { type CSSProperties, useCallback, useEffect, useMemo, useState } from 'react';
import type { LibraryState, PlaylistNode } from '../../shared/types';
import { coverHue } from './colors';
import { formatTotalTime } from './format';
import { CloseIcon, DiscIcon, PauseIcon, PlayIcon, PlaylistIcon, UsbIcon, WarningIcon } from './icons';
import { PlayerBar } from './PlayerBar';
import { SOURCE_LABELS, type Selection, Sidebar } from './Sidebar';
import { TrackList } from './TrackList';
import {
  type SortColumn,
  type SortState,
  moveItem,
  nextSort,
  reconcileOrder,
  sortByTitle,
  sortEntries,
  toEntries,
} from './trackOrder';
import { orderKey, useCustomOrders } from './useCustomOrders';
import { type QueueItem, usePlayer } from './usePlayer';
import { VisualizerOverlay } from './visualizers/VisualizerOverlay';

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
  const [sort, setSort] = useState<SortState | null>(null);
  const [visualizerOpen, setVisualizerOpen] = useState(false);
  const closeVisualizer = useCallback(() => setVisualizerOpen(false), []);
  const { orders, saveOrder, resetOrder } = useCustomOrders();

  // Fall back to the first library when nothing is selected or the selected USB was removed.
  const library = libraries.find((candidate) => candidate.id === selection?.libraryId) ?? libraries[0];
  const playlist =
    library && selection?.libraryId === library.id && selection.playlistId !== null
      ? findPlaylist(library.playlists, selection.playlistId)
      : undefined;

  const customOrderKey = library && playlist ? orderKey(library.id, playlist.id) : null;
  const customOrder = customOrderKey === null ? undefined : orders[customOrderKey];
  const viewKey = customOrderKey ?? `${library?.id}:all`;

  const entries = useMemo(() => {
    if (!library) return [];
    let tracks;
    if (playlist) {
      const tracksById = new Map(library.tracks.map((track) => [track.id, track]));
      const trackIds = customOrder ? reconcileOrder(customOrder, playlist.trackIds) : playlist.trackIds;
      tracks = trackIds.flatMap((id) => tracksById.get(id) ?? []);
    } else {
      tracks = sortByTitle(library.tracks);
    }
    const listed = toEntries(tracks);
    return sort ? sortEntries(listed, sort) : listed;
  }, [library, playlist, customOrder, sort]);

  const libraryId = library?.id;
  const queue = useMemo<QueueItem[]>(
    () =>
      libraryId === undefined
        ? []
        : entries.map((entry) => ({ libraryId, track: entry.track, entryKey: entry.key })),
    [libraryId, entries],
  );

  const { reorderQueue } = player;
  useEffect(() => {
    if (libraryId !== undefined) reorderQueue(queue, viewKey);
  }, [libraryId, queue, viewKey, reorderQueue]);

  const totalSeconds = useMemo(
    () => entries.reduce((total, entry) => total + (entry.track.durationSec ?? 0), 0),
    [entries],
  );

  // The big play button starts this list, or pauses and resumes it once it is playing.
  const playingThisList = player.current !== null && player.currentSource === viewKey;
  const playList = () => {
    if (playingThisList) player.togglePlay();
    else player.play(queue, 0, viewKey);
  };

  // Dragging only makes sense in a playlist shown in its own order, not a sorted view.
  const canReorder = customOrderKey !== null && sort === null;
  const moveTrack = useCallback(
    (from: number, to: number) => {
      if (customOrderKey === null) return;
      saveOrder(customOrderKey, moveItem(entries.map((entry) => entry.track.id), from, to));
    },
    [customOrderKey, entries, saveOrder],
  );
  const changeSort = useCallback(
    (column: SortColumn) => setSort((previous) => nextSort(previous, column)),
    [],
  );
  const keepSortedOrder = () => {
    if (customOrderKey === null) return;
    saveOrder(customOrderKey, entries.map((entry) => entry.track.id));
    setSort(null);
  };

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
    <div className={`app${visualizerOpen ? ' visualizer-open' : ''}`}>
      <Sidebar
        libraries={libraries}
        selection={library ? { libraryId: library.id, playlistId: playlist?.id ?? null } : null}
        onSelect={setSelection}
        onOpenFolder={openFolder}
      />

      <main className="content">
        {errors.map((error) => (
          <p key={`${error.volumePath}/${error.source}`} className="banner banner-error">
            <WarningIcon />
            <span className="banner-text">
              Could not read {SOURCE_LABELS[error.source]} on “{error.volumeName}”: {error.message}
            </span>
          </p>
        ))}
        {notice && (
          <p className="banner">
            <span className="banner-text">{notice}</span>
            <button className="banner-dismiss" aria-label="Dismiss" onClick={() => setNotice(null)}>
              <CloseIcon size={14} />
            </button>
          </p>
        )}

        {library ? (
          <>
            <header
              className="content-header"
              style={{ '--hue': coverHue(playlist?.name ?? library.volumeName) } as CSSProperties}
            >
              <div className="cover" aria-hidden="true">
                {playlist ? <PlaylistIcon size={30} /> : <DiscIcon size={30} />}
              </div>
              <div className="heading">
                <span className="eyebrow">
                  {SOURCE_LABELS[library.source]}{' '}
                  {playlist ? (library.source === 'serato' ? 'crate' : 'playlist') : 'library'}
                </span>
                <h1 title={playlist?.name}>{playlist?.name ?? 'All tracks'}</h1>
                <span className="content-meta">
                  {library.volumeName} · {entries.length} {entries.length === 1 ? 'track' : 'tracks'}
                  {totalSeconds > 0 && ` · ${formatTotalTime(totalSeconds)}`}
                  {customOrder && sort === null && ' · custom order'}
                </span>
              </div>
              <div className="content-actions">
                {customOrderKey !== null && sort !== null && (
                  <button
                    className="button"
                    title="Save the sorted order as this playlist's order"
                    onClick={keepSortedOrder}
                  >
                    Keep this order
                  </button>
                )}
                {customOrderKey !== null && customOrder && sort === null && (
                  <button
                    className="button"
                    title="Go back to the order on the USB"
                    onClick={() => resetOrder(customOrderKey)}
                  >
                    Reset order
                  </button>
                )}
                <button
                  className="header-play"
                  aria-label={playingThisList && player.isPlaying ? 'Pause' : 'Play this list'}
                  disabled={entries.length === 0}
                  onClick={playList}
                >
                  {playingThisList && player.isPlaying ? <PauseIcon size={20} /> : <PlayIcon size={20} />}
                </button>
              </div>
            </header>
            <TrackList
              libraryId={library.id}
              viewKey={viewKey}
              entries={entries}
              current={player.current}
              currentSource={player.currentSource}
              isPlaying={player.isPlaying}
              failed={player.failed}
              play={player.play}
              sort={sort}
              onSortClick={changeSort}
              onMove={canReorder ? moveTrack : undefined}
            />
          </>
        ) : (
          <div className="empty-state">
            <div className="empty-icon" aria-hidden="true">
              <UsbIcon size={34} />
            </div>
            <h1>Plug in a DJ USB</h1>
            <p>rekordbox and Serato libraries show up here automatically.</p>
            <button className="button" onClick={openFolder}>
              Open folder…
            </button>
          </div>
        )}
      </main>

      <PlayerBar player={player} onOpenVisualizer={() => setVisualizerOpen(true)} />
      {visualizerOpen && <VisualizerOverlay player={player} onClose={closeVisualizer} />}
    </div>
  );
}

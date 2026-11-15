import { type CSSProperties, useCallback, useEffect, useMemo, useState } from 'react';
import { formatTotalTime } from '../../logic/format';
import {
  currentEntryIndex,
  libraryWasRemoved,
  listEntries,
  listViewKey,
  orderKey,
  queueFromEntries,
  visibleLibrary,
  visiblePlaylist,
} from '../../logic/libraryView';
import { type SortColumn, type SortState, moveItem, nextSort } from '../../logic/trackOrder';
import type { LibraryState } from '../../shared/types';
import { coverHue } from './colors';
import { CloseIcon, DiscIcon, PauseIcon, PlayIcon, PlaylistIcon, UsbIcon, WarningIcon } from './icons';
import { EnergyFlowChart } from './EnergyFlowChart';
import { PlayerBar } from './PlayerBar';
import { SOURCE_LABELS, type Selection, Sidebar } from './Sidebar';
import { TrackList } from './TrackList';
import { getState, onStateChanged, openFolder as chooseFolder } from './backend';
import { useCustomOrders } from './useCustomOrders';
import { usePlayer } from './usePlayer';
import { VisualizerOverlay } from './visualizers/VisualizerOverlay';

const ENERGY_FLOW_STORAGE_KEY = 'energyFlowOpen';

function useLibraryState(): LibraryState {
  const [state, setState] = useState<LibraryState>({ libraries: [], errors: [] });
  useEffect(() => {
    const unsubscribe = onStateChanged(setState);
    void getState().then(setState);
    return unsubscribe;
  }, []);
  return state;
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
  const [energyFlowOpen, setEnergyFlowOpen] = useState(
    () => localStorage.getItem(ENERGY_FLOW_STORAGE_KEY) === 'true',
  );
  const toggleEnergyFlow = () => {
    localStorage.setItem(ENERGY_FLOW_STORAGE_KEY, String(!energyFlowOpen));
    setEnergyFlowOpen(!energyFlowOpen);
  };

  const library = visibleLibrary(libraries, selection?.libraryId);
  const playlist = visiblePlaylist(library, selection);

  const customOrderKey = library && playlist ? orderKey(library.id, playlist.id) : null;
  const customOrder = customOrderKey === null ? undefined : orders[customOrderKey];
  const viewKey = listViewKey(library, playlist);

  const entries = useMemo(
    () => listEntries(library, playlist, customOrder, sort),
    [library, playlist, customOrder, sort],
  );
  const queue = useMemo(() => queueFromEntries(library?.id, entries), [library?.id, entries]);

  const { reorderQueue } = player;
  useEffect(() => {
    if (library !== undefined) reorderQueue(queue, viewKey);
  }, [library, queue, viewKey, reorderQueue]);

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

  const { play } = player;
  const playFrom = useCallback((index: number) => play(queue, index, viewKey), [play, queue, viewKey]);
  const playingEntryKey = playingThisList ? player.current?.entryKey : undefined;
  // A track playing from another list of the same library is still marked.
  const playingTrackId = player.current?.libraryId === library?.id ? player.current?.track.id : undefined;
  const currentIndex = useMemo(
    () => currentEntryIndex(entries, playingEntryKey, playingTrackId),
    [entries, playingEntryKey, playingTrackId],
  );

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
    if (libraryWasRemoved(libraries, current?.libraryId)) stop();
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
    const result = await chooseFolder();
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
                {playlist && (
                  <button
                    className={`button${energyFlowOpen ? ' active' : ''}`}
                    aria-pressed={energyFlowOpen}
                    title="Chart the tempo and keys of this list in its current order"
                    onClick={toggleEnergyFlow}
                  >
                    {energyFlowOpen ? 'Hide energy flow' : 'Show energy flow'}
                  </button>
                )}
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
            {playlist && energyFlowOpen && (
              <EnergyFlowChart
                entries={entries}
                currentIndex={currentIndex}
                isPlaying={player.isPlaying}
                onPlay={playFrom}
              />
            )}
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

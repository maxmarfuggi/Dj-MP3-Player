import { type CSSProperties, type DragEvent, type KeyboardEvent, memo, useRef, useState } from 'react';
import { formatBpm, formatTime } from '../../logic/format';
import type { SortColumn, SortState, TrackEntry } from '../../logic/trackOrder';
import { PlayIcon, SortArrowIcon } from './icons';
import { KeyTag } from './KeyTag';
import { type Player, type QueueItem, queueItemKey } from './usePlayer';

interface TrackListProps extends Pick<Player, 'current' | 'currentSource' | 'isPlaying' | 'failed' | 'play'> {
  libraryId: string;
  /** Identifies the list being shown, so the player can follow reorders of it. */
  viewKey: string;
  entries: TrackEntry[];
  sort: SortState | null;
  onSortClick(column: SortColumn): void;
  /** Present when rows can be dragged into a new order. */
  onMove?(from: number, to: number): void;
}

interface DropTarget {
  index: number;
  position: 'before' | 'after';
}

interface SortableColumn {
  column: SortColumn;
  label: string;
  className?: string;
}

const TITLE_COLUMN: SortableColumn = { column: 'title', label: 'Title' };
const ARTIST_COLUMN: SortableColumn = { column: 'artist', label: 'Artist' };
const GENRE_COLUMN: SortableColumn = { column: 'genre', label: 'Genre' };
const LABEL_COLUMN: SortableColumn = { column: 'label', label: 'Label' };
const BPM_COLUMN: SortableColumn = { column: 'bpm', label: 'BPM', className: 'track-numeric' };

// Memoized so that the playback position ticking along does not re-render every row.
export const TrackList = memo(function TrackList({
  libraryId,
  viewKey,
  entries,
  current,
  currentSource,
  isPlaying,
  failed,
  play,
  sort,
  onSortClick,
  onMove,
}: TrackListProps) {
  const dragFrom = useRef<number | null>(null);
  const [dropTarget, setDropTarget] = useState<DropTarget | null>(null);

  if (entries.length === 0) {
    return <p className="empty-list">No tracks here.</p>;
  }

  // Only some libraries have genres and labels, so their columns appear when the list uses them.
  const showGenre = entries.some((entry) => entry.track.genre);
  const showLabel = entries.some((entry) => entry.track.label);
  const sortableColumns = [
    TITLE_COLUMN,
    ARTIST_COLUMN,
    ...(showGenre ? [GENRE_COLUMN] : []),
    ...(showLabel ? [LABEL_COLUMN] : []),
    BPM_COLUMN,
  ];
  const columns = [
    '32px',
    'minmax(0, 2.2fr)',
    'minmax(0, 1.5fr)',
    ...(showGenre ? ['minmax(0, 1fr)'] : []),
    ...(showLabel ? ['minmax(0, 1fr)'] : []),
    '52px 46px 50px',
  ].join(' ');

  const playingThisList = current !== null && currentSource === viewKey;
  // A track that is playing from another list is still marked here.
  const isCurrent = (entry: TrackEntry) =>
    playingThisList
      ? current.entryKey === entry.key
      : current !== null && queueItemKey(current) === queueItemKey({ libraryId, track: entry.track });

  const playFrom = (index: number) => {
    const queue: QueueItem[] = entries.map((entry) => ({ libraryId, track: entry.track, entryKey: entry.key }));
    play(queue, index, viewKey);
  };

  const onDragOver = (event: DragEvent<HTMLElement>, index: number) => {
    if (dragFrom.current === null) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = 'move';
    const { top, height } = event.currentTarget.getBoundingClientRect();
    const position = event.clientY < top + height / 2 ? 'before' : 'after';
    if (dropTarget?.index !== index || dropTarget.position !== position) setDropTarget({ index, position });
  };

  const onDrop = (event: DragEvent<HTMLElement>) => {
    event.preventDefault();
    const from = dragFrom.current;
    if (from !== null && dropTarget && onMove) {
      const insertAt = dropTarget.position === 'before' ? dropTarget.index : dropTarget.index + 1;
      // Removing the dragged row first shifts everything after it up by one.
      const to = insertAt > from ? insertAt - 1 : insertAt;
      if (to !== from) onMove(from, to);
    }
    dragFrom.current = null;
    setDropTarget(null);
  };

  const onRowKeyDown = (event: KeyboardEvent<HTMLElement>, index: number) => {
    if (!onMove || !event.altKey) return;
    const to = event.key === 'ArrowUp' ? index - 1 : event.key === 'ArrowDown' ? index + 1 : null;
    if (to === null || to < 0 || to >= entries.length) return;
    event.preventDefault();
    onMove(index, to);
    // Keep focus on the moved track so it can be moved again.
    const list = event.currentTarget.parentElement;
    requestAnimationFrame(() => (list?.children[to + 1] as HTMLElement | undefined)?.focus());
  };

  return (
    <div
      className={`track-list${isPlaying ? '' : ' paused'}`}
      style={{ '--track-columns': columns } as CSSProperties}
    >
      <div className="track-row track-header">
        <span className="track-number">#</span>
        {sortableColumns.map(({ column, label, className }) => {
          const active = sort?.column === column;
          return (
            <button
              key={column}
              className={`sort-header${active ? ' active' : ''}${className ? ` ${className}` : ''}`}
              aria-sort={active ? (sort.direction === 'asc' ? 'ascending' : 'descending') : 'none'}
              title={`Sort by ${label}`}
              onClick={() => onSortClick(column)}
            >
              {label}
              {active && <SortArrowIcon size={11} direction={sort.direction} />}
            </button>
          );
        })}
        <span className="track-key">Key</span>
        <span className="track-numeric">Time</span>
      </div>
      {entries.map((entry, index) => {
        const { track } = entry;
        const hasFailed = failed.has(queueItemKey({ libraryId, track }));
        const playing = isCurrent(entry);
        const drop = dropTarget?.index === index ? ` drop-${dropTarget.position}` : '';
        return (
          <button
            key={entry.key}
            className={`track-row${playing ? ' current' : ''}${hasFailed ? ' failed' : ''}${drop}`}
            title={hasFailed ? 'This file could not be played (missing or unsupported format)' : undefined}
            onClick={() => playFrom(index)}
            onKeyDown={(event) => onRowKeyDown(event, index)}
            draggable={onMove !== undefined}
            onDragStart={(event) => {
              dragFrom.current = index;
              event.dataTransfer.effectAllowed = 'move';
              event.dataTransfer.setData('text/plain', track.title);
            }}
            onDragOver={(event) => onDragOver(event, index)}
            onDrop={onDrop}
            onDragEnd={() => {
              dragFrom.current = null;
              setDropTarget(null);
            }}
          >
            <span className="track-number">
              {playing ? (
                <span className="playing-bars">
                  <i />
                  <i />
                  <i />
                </span>
              ) : (
                <>
                  <span className="track-index">{index + 1}</span>
                  <span className="track-play">
                    <PlayIcon size={12} />
                  </span>
                </>
              )}
            </span>
            <span className="track-text track-title">
              {track.color && (
                <span
                  className="track-color"
                  style={{ background: track.color.value }}
                  title={track.color.name}
                  aria-label={track.color.name}
                />
              )}
              {track.title}
            </span>
            <span className="track-text track-secondary">{track.artist}</span>
            {showGenre && <span className="track-text track-secondary">{track.genre}</span>}
            {showLabel && <span className="track-text track-secondary">{track.label}</span>}
            <span className="track-numeric track-secondary">
              {track.bpm === null ? '' : formatBpm(track.bpm)}
            </span>
            <span className="track-key">{track.key && <KeyTag musicalKey={track.key} />}</span>
            <span className="track-numeric track-secondary">
              {track.durationSec === null ? '' : formatTime(track.durationSec)}
            </span>
          </button>
        );
      })}
    </div>
  );
});

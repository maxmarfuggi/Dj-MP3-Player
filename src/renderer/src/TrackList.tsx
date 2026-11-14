import type { Track } from '../../shared/types';
import { formatBpm, formatTime } from './format';
import { PlayIcon } from './icons';
import { type Player, type QueueItem, queueItemKey } from './usePlayer';

interface TrackListProps {
  libraryId: string;
  tracks: Track[];
  player: Player;
}

export function TrackList({ libraryId, tracks, player }: TrackListProps) {
  if (tracks.length === 0) {
    return <p className="empty-list">No tracks here.</p>;
  }

  const currentKey = player.current ? queueItemKey(player.current) : null;
  const playFrom = (index: number) => {
    const queue: QueueItem[] = tracks.map((track) => ({ libraryId, track }));
    player.play(queue, index);
  };

  return (
    <div className="track-list">
      <div className="track-row track-header">
        <span className="track-number">#</span>
        <span>Title</span>
        <span>Artist</span>
        <span className="track-numeric">BPM</span>
        <span>Key</span>
        <span className="track-numeric">Time</span>
      </div>
      {tracks.map((track, index) => {
        const key = queueItemKey({ libraryId, track });
        const isCurrent = key === currentKey;
        const hasFailed = player.failed.has(key);
        return (
          // A playlist can list the same track twice, so rows are keyed by position.
          <button
            key={index}
            className={`track-row${isCurrent ? ' current' : ''}${hasFailed ? ' failed' : ''}`}
            title={hasFailed ? 'This file could not be played (missing or unsupported format)' : undefined}
            onClick={() => playFrom(index)}
          >
            <span className="track-number">{isCurrent ? <PlayIcon size={11} /> : index + 1}</span>
            <span className="track-text">{track.title}</span>
            <span className="track-text track-secondary">{track.artist}</span>
            <span className="track-numeric track-secondary">
              {track.bpm === null ? '' : formatBpm(track.bpm)}
            </span>
            <span className="track-secondary">{track.key ?? ''}</span>
            <span className="track-numeric track-secondary">
              {track.durationSec === null ? '' : formatTime(track.durationSec)}
            </span>
          </button>
        );
      })}
    </div>
  );
}

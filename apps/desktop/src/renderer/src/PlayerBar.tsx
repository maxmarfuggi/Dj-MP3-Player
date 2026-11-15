import { type CSSProperties, useState } from 'react';
import { artworkUrl } from '../../shared/types';
import { labelColor } from './colors';
import { formatBpm, formatTime } from './format';
import { NextIcon, PauseIcon, PlayIcon, PreviousIcon, VisualizerIcon, VolumeIcon } from './icons';
import { KeyTag } from './KeyTag';
import { Slider } from './Slider';
import type { Player } from './usePlayer';

export function PlayerBar({ player, onOpenVisualizer }: { player: Player; onOpenVisualizer(): void }) {
  const { current, isPlaying, currentTime, duration, volume } = player;
  const disabled = current === null;
  const track = current?.track ?? null;
  const artwork = current?.track.artworkPath ? artworkUrl(current.libraryId, current.track.id) : null;
  const [brokenArtwork, setBrokenArtwork] = useState<string | null>(null);

  return (
    <footer className="player-bar">
      <div className="now-playing">
        {artwork && artwork !== brokenArtwork ? (
          <img className="now-artwork" src={artwork} alt="" onError={() => setBrokenArtwork(artwork)} />
        ) : (
          // The same label colour as the record on the turntable visualizer.
          <div
            className={`mini-record${isPlaying ? ' spinning' : ''}`}
            style={{ '--label': labelColor(track) } as CSSProperties}
            aria-hidden="true"
          >
            <div className="mini-record-label" />
          </div>
        )}
        <div className="now-text">
          {track ? (
            <>
              <span className="now-title">{track.title}</span>
              <span className="now-artist">{track.artist}</span>
            </>
          ) : (
            <span className="now-artist">Nothing playing</span>
          )}
        </div>
        {track && (track.bpm !== null || track.key) && (
          <div className="now-meta">
            {track.bpm !== null && (
              <span className="now-bpm">
                {formatBpm(track.bpm)} <small>BPM</small>
              </span>
            )}
            {track.key && <KeyTag musicalKey={track.key} />}
          </div>
        )}
      </div>

      <div className="transport">
        <div className="transport-buttons">
          <button aria-label="Previous" disabled={disabled} onClick={player.previous}>
            <PreviousIcon />
          </button>
          <button
            className="play-button"
            aria-label={isPlaying ? 'Pause' : 'Play'}
            disabled={disabled}
            onClick={player.togglePlay}
          >
            {isPlaying ? <PauseIcon size={18} /> : <PlayIcon size={18} />}
          </button>
          <button aria-label="Next" disabled={disabled} onClick={player.next}>
            <NextIcon />
          </button>
        </div>
        <div className="seek">
          <span className="time">{formatTime(currentTime)}</span>
          <Slider
            label="Seek"
            max={duration || 1}
            step={0.1}
            value={Math.min(currentTime, duration || 1)}
            disabled={disabled || duration === 0}
            onChange={player.seek}
          />
          <span className="time">{formatTime(duration)}</span>
        </div>
      </div>

      <div className="player-extras">
        <button
          className={`visualizer-button${isPlaying ? ' playing' : ''}`}
          title="Open the full-window visualizer"
          onClick={onOpenVisualizer}
        >
          <VisualizerIcon />
          Visualizer
        </button>
        <div className="volume">
          <VolumeIcon volume={volume} />
          <Slider label="Volume" max={1} step={0.01} value={volume} onChange={player.setVolume} />
        </div>
      </div>
    </footer>
  );
}

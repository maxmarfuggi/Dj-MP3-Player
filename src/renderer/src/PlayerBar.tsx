import { formatTime } from './format';
import { NextIcon, PauseIcon, PlayIcon, PreviousIcon, VolumeIcon } from './icons';
import type { Player } from './usePlayer';

export function PlayerBar({ player }: { player: Player }) {
  const { current, isPlaying, currentTime, duration, volume } = player;
  const disabled = current === null;

  return (
    <footer className="player-bar">
      <div className="now-playing">
        {current ? (
          <>
            <span className="now-title">{current.track.title}</span>
            <span className="now-artist">{current.track.artist}</span>
          </>
        ) : (
          <span className="now-artist">Nothing playing</span>
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
          <input
            type="range"
            aria-label="Seek"
            min={0}
            max={duration || 1}
            step={0.1}
            value={Math.min(currentTime, duration || 1)}
            disabled={disabled || duration === 0}
            onChange={(event) => player.seek(Number(event.target.value))}
          />
          <span className="time">{formatTime(duration)}</span>
        </div>
      </div>

      <div className="volume">
        <VolumeIcon />
        <input
          type="range"
          aria-label="Volume"
          min={0}
          max={1}
          step={0.01}
          value={volume}
          onChange={(event) => player.setVolume(Number(event.target.value))}
        />
      </div>
    </footer>
  );
}

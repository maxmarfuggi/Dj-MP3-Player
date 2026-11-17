import { useEffect, useRef, useState } from 'react';
import { formatTime } from '../../../logic/format';
import { trackUrl } from '../backend';
import { NextIcon, PauseIcon, PlayIcon, PreviousIcon } from '../icons';
import { Slider } from '../Slider';
import type { Player } from '../usePlayer';
import { Deck } from './Deck';
import { Equalizer } from './Equalizer';
import { MilkDrop } from './MilkDrop';
import { Turntable } from './Turntable';
import { Waveform } from './Waveform';

export type VisualizerMode = 'waveform' | 'equalizer' | 'turntable' | 'deck' | 'milkdrop';

const MODES: { mode: VisualizerMode; label: string }[] = [
  { mode: 'waveform', label: 'Waveform' },
  { mode: 'equalizer', label: 'Equalizer' },
  { mode: 'turntable', label: 'Turntable' },
  { mode: 'deck', label: 'Deck' },
  { mode: 'milkdrop', label: 'MilkDrop' },
];

const HIDE_CONTROLS_AFTER_MS = 2500;

interface VisualizerOverlayProps {
  player: Player;
  onClose(): void;
}

export function VisualizerOverlay({ player, onClose }: VisualizerOverlayProps) {
  // Always opens on the waveform.
  const [mode, setMode] = useState<VisualizerMode>('waveform');
  const [controlsVisible, setControlsVisible] = useState(true);
  const hideTimer = useRef<number | undefined>(undefined);
  const overlayRef = useRef<HTMLDivElement>(null);
  // Where the current visualizer can put its own controls, in the top bar.
  const [topSlot, setTopSlot] = useState<HTMLDivElement | null>(null);
  const { current, isPlaying } = player;

  const showControls = () => {
    setControlsVisible(true);
    window.clearTimeout(hideTimer.current);
    hideTimer.current = window.setTimeout(() => setControlsVisible(false), HIDE_CONTROLS_AFTER_MS);
  };

  useEffect(() => {
    showControls();
    return () => window.clearTimeout(hideTimer.current);
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      // Electron does not leave HTML full screen on Esc by itself.
      if (document.fullscreenElement) {
        void document.exitFullscreen();
      } else {
        onClose();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
      if (document.fullscreenElement) void document.exitFullscreen();
    };
  }, [onClose]);

  const toggleFullScreen = () => {
    if (document.fullscreenElement) {
      void document.exitFullscreen();
    } else {
      void overlayRef.current?.requestFullscreen();
    }
  };

  return (
    <div
      ref={overlayRef}
      className={`visualizer${controlsVisible ? '' : ' controls-hidden'}`}
      onMouseMove={showControls}
      onClick={showControls}
    >
      {mode === 'waveform' && <Waveform analyser={player.analyser} />}
      {mode === 'equalizer' && <Equalizer analyser={player.analyser} topSlot={topSlot} />}
      {mode === 'turntable' && (
        <Turntable
          track={current?.track ?? null}
          isPlaying={isPlaying}
          currentTime={player.currentTime}
          duration={player.duration}
          onStartStop={player.togglePlay}
        />
      )}
      {mode === 'deck' && (
        <Deck
          track={current?.track ?? null}
          url={current ? trackUrl(current.libraryId, current.track.id) : null}
          currentTime={player.currentTime}
          duration={player.duration}
          paused={current !== null && !isPlaying}
        />
      )}
      {mode === 'milkdrop' && <MilkDrop analyser={player.analyser} topSlot={topSlot} />}

      <div className="visualizer-top">
        <div className="visualizer-modes" role="tablist" aria-label="Visualizer">
          {MODES.map(({ mode: option, label }) => (
            <button
              key={option}
              role="tab"
              aria-selected={mode === option}
              className={mode === option ? 'active' : undefined}
              onClick={() => setMode(option)}
            >
              {label}
            </button>
          ))}
        </div>
        <div ref={setTopSlot} className="visualizer-top-slot" />
        <div className="visualizer-window-buttons">
          <button onClick={toggleFullScreen}>Full screen</button>
          <button aria-label="Close visualizer" onClick={onClose}>
            ×
          </button>
        </div>
      </div>

      <div className="visualizer-bottom">
        <div className="visualizer-track">
          <span className="now-title">{current?.track.title ?? 'Nothing playing'}</span>
          <span className="now-artist">{current?.track.artist}</span>
        </div>
        <div className="seek visualizer-seek">
          <span className="time">{formatTime(player.currentTime)}</span>
          <Slider
            label="Seek"
            max={player.duration || 1}
            step={0.1}
            value={Math.min(player.currentTime, player.duration || 1)}
            disabled={!current || player.duration === 0}
            onChange={player.seek}
          />
          <span className="time">{formatTime(player.duration)}</span>
        </div>
        <div className="transport-buttons">
          <button aria-label="Previous" disabled={!current} onClick={player.previous}>
            <PreviousIcon />
          </button>
          <button
            className="play-button"
            aria-label={isPlaying ? 'Pause' : 'Play'}
            disabled={!current}
            onClick={player.togglePlay}
          >
            {isPlaying ? <PauseIcon size={18} /> : <PlayIcon size={18} />}
          </button>
          <button aria-label="Next" disabled={!current} onClick={player.next}>
            <NextIcon />
          </button>
        </div>
      </div>
    </div>
  );
}

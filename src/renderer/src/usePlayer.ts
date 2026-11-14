import { useCallback, useEffect, useRef, useState } from 'react';
import { type Track, trackUrl } from '../../shared/types';

export interface QueueItem {
  libraryId: string;
  track: Track;
}

export interface Player {
  current: QueueItem | null;
  isPlaying: boolean;
  currentTime: number;
  duration: number;
  volume: number;
  /** Keys (see `queueItemKey`) of tracks that could not be played. */
  failed: ReadonlySet<string>;
  /** Replaces the queue and starts playing the item at `index`. */
  play(queue: QueueItem[], index: number): void;
  togglePlay(): void;
  next(): void;
  previous(): void;
  seek(seconds: number): void;
  setVolume(volume: number): void;
  stop(): void;
}

/** Going "previous" this far into a track restarts it instead. */
const RESTART_THRESHOLD_SEC = 3;

export function queueItemKey(item: QueueItem): string {
  return `${item.libraryId}/${item.track.id}`;
}

export function usePlayer(): Player {
  const [audio] = useState(() => new Audio());
  // Audio events fire outside React's render cycle, so the queue also lives in a ref.
  const playback = useRef<{ queue: QueueItem[]; index: number }>({ queue: [], index: -1 });
  const [current, setCurrent] = useState<QueueItem | null>(null);
  const [isPlaying, setIsPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolumeState] = useState(1);
  const [failed, setFailed] = useState<ReadonlySet<string>>(new Set());

  const stop = useCallback(() => {
    playback.current = { queue: [], index: -1 };
    audio.pause();
    audio.removeAttribute('src');
    audio.load();
    setCurrent(null);
    setIsPlaying(false);
    setCurrentTime(0);
    setDuration(0);
  }, [audio]);

  const play = useCallback(
    (queue: QueueItem[], index: number) => {
      const item = queue[index];
      if (!item) return;
      playback.current = { queue, index };
      setCurrent(item);
      setCurrentTime(0);
      setDuration(item.track.durationSec ?? 0);
      audio.src = trackUrl(item.libraryId, item.track.id);
      // A rejection here means the load failed (reported by the `error` event)
      // or another track was started before this one began.
      audio.play().catch(() => {});
    },
    [audio],
  );

  /** Plays the next item, or reports false at the end of the queue. */
  const advance = useCallback((): boolean => {
    const { queue, index } = playback.current;
    if (index + 1 >= queue.length) return false;
    play(queue, index + 1);
    return true;
  }, [play]);

  const next = useCallback(() => {
    advance();
  }, [advance]);

  const previous = useCallback(() => {
    const { queue, index } = playback.current;
    if (index > 0 && audio.currentTime < RESTART_THRESHOLD_SEC) {
      play(queue, index - 1);
    } else {
      audio.currentTime = 0;
    }
  }, [audio, play]);

  const togglePlay = useCallback(() => {
    if (playback.current.index === -1) return;
    if (audio.paused) {
      audio.play().catch(() => {});
    } else {
      audio.pause();
    }
  }, [audio]);

  const seek = useCallback(
    (seconds: number) => {
      audio.currentTime = seconds;
      setCurrentTime(seconds);
    },
    [audio],
  );

  const setVolume = useCallback(
    (value: number) => {
      audio.volume = value;
      setVolumeState(value);
    },
    [audio],
  );

  useEffect(() => {
    const onTimeUpdate = () => setCurrentTime(audio.currentTime);
    const onDurationChange = () => {
      if (Number.isFinite(audio.duration)) setDuration(audio.duration);
    };
    const onPlay = () => setIsPlaying(true);
    const onPause = () => setIsPlaying(false);
    const onEnded = () => {
      if (!advance()) setIsPlaying(false);
    };
    const onError = () => {
      // Unsupported format or missing file: remember it and move on.
      const { queue, index } = playback.current;
      const item = queue[index];
      if (!item) return;
      setFailed((previous) => new Set(previous).add(queueItemKey(item)));
      if (!advance()) setIsPlaying(false);
    };

    audio.addEventListener('timeupdate', onTimeUpdate);
    audio.addEventListener('durationchange', onDurationChange);
    audio.addEventListener('play', onPlay);
    audio.addEventListener('pause', onPause);
    audio.addEventListener('ended', onEnded);
    audio.addEventListener('error', onError);
    return () => {
      audio.removeEventListener('timeupdate', onTimeUpdate);
      audio.removeEventListener('durationchange', onDurationChange);
      audio.removeEventListener('play', onPlay);
      audio.removeEventListener('pause', onPause);
      audio.removeEventListener('ended', onEnded);
      audio.removeEventListener('error', onError);
    };
  }, [audio, advance]);

  return {
    current,
    isPlaying,
    currentTime,
    duration,
    volume,
    failed,
    play,
    togglePlay,
    next,
    previous,
    seek,
    setVolume,
    stop,
  };
}

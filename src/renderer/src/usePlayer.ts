import { useCallback, useEffect, useRef, useState } from 'react';
import { type Track, trackUrl } from '../../shared/types';

export interface QueueItem {
  libraryId: string;
  track: Track;
  /** Identifies this position in the queue; the same track can appear twice. */
  entryKey: string;
}

export interface Player {
  current: QueueItem | null;
  /** The view (see `play`) the current queue was started from. */
  currentSource: string | null;
  isPlaying: boolean;
  currentTime: number;
  duration: number;
  volume: number;
  /**
   * Taps the audio as it goes to the speakers, for visualizers. Created on the
   * first play, because browsers only start audio processing after a click.
   */
  analyser: AnalyserNode | null;
  /** Keys (see `queueItemKey`) of tracks that could not be played. */
  failed: ReadonlySet<string>;
  /**
   * Replaces the queue and starts playing the item at `index`. `source`
   * identifies the view the queue came from, for `reorderQueue`.
   */
  play(queue: QueueItem[], index: number, source: string): void;
  /**
   * Swaps in a reordered queue if it came from `source`, without interrupting
   * the current track. Ignored if the current track is not in the new queue.
   */
  reorderQueue(queue: QueueItem[], source: string): void;
  togglePlay(): void;
  next(): void;
  previous(): void;
  seek(seconds: number): void;
  setVolume(volume: number): void;
  stop(): void;
}

/** Going "previous" this far into a track restarts it instead. */
const RESTART_THRESHOLD_SEC = 3;

export function queueItemKey(item: Pick<QueueItem, 'libraryId' | 'track'>): string {
  return `${item.libraryId}/${item.track.id}`;
}

export function usePlayer(): Player {
  const [audio] = useState(() => {
    const element = new Audio();
    // Required for Web Audio to read the samples; see `registerMediaScheme`.
    element.crossOrigin = 'anonymous';
    return element;
  });
  const audioGraph = useRef<{ context: AudioContext; analyser: AnalyserNode } | null>(null);
  const [analyser, setAnalyser] = useState<AnalyserNode | null>(null);
  // Audio events fire outside React's render cycle, so the queue also lives in a ref.
  const playback = useRef<{ queue: QueueItem[]; index: number; source: string | null }>({
    queue: [],
    index: -1,
    source: null,
  });
  const [current, setCurrent] = useState<QueueItem | null>(null);
  const [currentSource, setCurrentSource] = useState<string | null>(null);
  const [isPlaying, setIsPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolumeState] = useState(1);
  const [failed, setFailed] = useState<ReadonlySet<string>>(new Set());

  /** Routes the audio element through an analyser on its way to the speakers. */
  const startAudioGraph = useCallback(() => {
    if (audioGraph.current) {
      void audioGraph.current.context.resume();
      return;
    }
    const context = new AudioContext();
    const node = context.createAnalyser();
    node.fftSize = 2048;
    // Once connected, the element is only audible through the context.
    context.createMediaElementSource(audio).connect(node);
    node.connect(context.destination);
    audioGraph.current = { context, analyser: node };
    setAnalyser(node);
  }, [audio]);

  const stop = useCallback(() => {
    playback.current = { queue: [], index: -1, source: null };
    audio.pause();
    audio.removeAttribute('src');
    audio.load();
    setCurrent(null);
    setCurrentSource(null);
    setIsPlaying(false);
    setCurrentTime(0);
    setDuration(0);
  }, [audio]);

  const play = useCallback(
    (queue: QueueItem[], index: number, source: string) => {
      const item = queue[index];
      if (!item) return;
      playback.current = { queue, index, source };
      setCurrent(item);
      setCurrentSource(source);
      setCurrentTime(0);
      setDuration(item.track.durationSec ?? 0);
      startAudioGraph();
      audio.src = trackUrl(item.libraryId, item.track.id);
      // A rejection here means the load failed (reported by the `error` event)
      // or another track was started before this one began.
      audio.play().catch(() => {});
    },
    [audio, startAudioGraph],
  );

  const reorderQueue = useCallback((queue: QueueItem[], source: string) => {
    const { queue: oldQueue, index: oldIndex, source: oldSource } = playback.current;
    const playing = oldQueue[oldIndex];
    if (!playing || oldSource !== source) return;
    const index = queue.findIndex((item) => item.entryKey === playing.entryKey);
    if (index !== -1) playback.current = { queue, index, source };
  }, []);

  /** Plays the next item, or reports false at the end of the queue. */
  const advance = useCallback((): boolean => {
    const { queue, index, source } = playback.current;
    if (index + 1 >= queue.length || source === null) return false;
    play(queue, index + 1, source);
    return true;
  }, [play]);

  const next = useCallback(() => {
    advance();
  }, [advance]);

  const previous = useCallback(() => {
    const { queue, index, source } = playback.current;
    if (index > 0 && source !== null && audio.currentTime < RESTART_THRESHOLD_SEC) {
      play(queue, index - 1, source);
    } else {
      audio.currentTime = 0;
    }
  }, [audio, play]);

  const togglePlay = useCallback(() => {
    if (playback.current.index === -1) return;
    if (audio.paused) {
      startAudioGraph();
      audio.play().catch(() => {});
    } else {
      audio.pause();
    }
  }, [audio, startAudioGraph]);

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
    currentSource,
    isPlaying,
    currentTime,
    duration,
    volume,
    analyser,
    failed,
    play,
    reorderQueue,
    togglePlay,
    next,
    previous,
    seek,
    setVolume,
    stop,
  };
}

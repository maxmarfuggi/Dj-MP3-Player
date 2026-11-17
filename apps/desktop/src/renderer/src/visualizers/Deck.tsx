import { useEffect, useRef } from 'react';
import type { Track } from '../../../shared/types';
import {
  barsText,
  beats,
  columns,
  detailSpan,
  peaksFromChannel,
  remainText,
  tempoText,
  titleLine,
  windowStart,
  type Peaks,
} from './deckLayout';

const SCREEN = '#000610';
const WAVE = '#7ecbff';
const WAVE_DIM = '#2c5c82';
const BEAT = 'rgba(255, 59, 78, 0.75)';
const BAR = 'rgba(255, 106, 50, 0.9)';
const NEEDLE = '#fff4e8';
const TEXT = '#f4faff';
const CYAN = '#7ec8ff';
const FONT = "ui-monospace, 'Cascadia Mono', Consolas, monospace";

const cache = new Map<string, Peaks>();

interface DeckProps {
  track: Track | null;
  /** `media://` URL of the file, or null when nothing is loaded. */
  url: string | null;
  currentTime: number;
  duration: number;
  paused: boolean;
}

/** CDJ performance screen. The turntable stays a separate visualizer. */
export function Deck({ track, url, currentTime, duration, paused }: DeckProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const timeRef = useRef(currentTime);
  const durationRef = useRef(duration);
  const trackRef = useRef(track);
  const pausedRef = useRef(paused);
  const peaksRef = useRef<Peaks | null>(null);
  const statusRef = useRef<'idle' | 'reading' | 'ready' | 'failed'>('idle');
  timeRef.current = currentTime;
  durationRef.current = duration;
  trackRef.current = track;
  pausedRef.current = paused;

  useEffect(() => {
    if (!url) {
      peaksRef.current = null;
      statusRef.current = 'idle';
      return;
    }
    const cached = cache.get(url);
    if (cached) {
      peaksRef.current = cached;
      statusRef.current = 'ready';
      return;
    }
    let cancelled = false;
    peaksRef.current = null;
    statusRef.current = 'reading';
    void loadPeaks(url).then(
      (peaks) => {
        if (cancelled) return;
        if (cache.size >= 8) {
          const oldest = cache.keys().next().value;
          if (oldest) cache.delete(oldest);
        }
        cache.set(url, peaks);
        peaksRef.current = peaks;
        statusRef.current = 'ready';
      },
      () => {
        if (!cancelled) statusRef.current = 'failed';
      },
    );
    return () => {
      cancelled = true;
    };
  }, [url]);

  useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext('2d');
    if (!canvas || !context) return;

    let frame = 0;
    const draw = () => {
      frame = requestAnimationFrame(draw);
      const scale = window.devicePixelRatio || 1;
      const width = canvas.clientWidth;
      const height = canvas.clientHeight;
      if (canvas.width !== Math.round(width * scale) || canvas.height !== Math.round(height * scale)) {
        canvas.width = Math.round(width * scale);
        canvas.height = Math.round(height * scale);
      }
      context.setTransform(scale, 0, 0, scale, 0, 0);
      paint(context, width, height, {
        track: trackRef.current,
        peaks: peaksRef.current,
        status: statusRef.current,
        position: timeRef.current,
        duration: durationRef.current,
        paused: pausedRef.current,
      });
    };
    draw();
    return () => cancelAnimationFrame(frame);
  }, []);

  return <canvas ref={canvasRef} className="visualizer-canvas" />;
}

async function loadPeaks(url: string): Promise<Peaks> {
  const response = await fetch(url);
  if (!response.ok) throw new Error('The track could not be read.');
  const bytes = await response.arrayBuffer();
  const context = new AudioContext();
  try {
    const audio = await context.decodeAudioData(bytes);
    const mixed = new Float32Array(audio.length);
    const channels = audio.numberOfChannels;
    for (let channel = 0; channel < channels; channel += 1) {
      const data = audio.getChannelData(channel);
      for (let index = 0; index < audio.length; index += 1) {
        mixed[index] = (mixed[index] ?? 0) + (data[index] ?? 0) / channels;
      }
    }
    return peaksFromChannel(mixed, audio.sampleRate);
  } finally {
    await context.close();
  }
}

interface PaintModel {
  track: Track | null;
  peaks: Peaks | null;
  status: 'idle' | 'reading' | 'ready' | 'failed';
  position: number;
  duration: number;
  paused: boolean;
}

function paint(context: CanvasRenderingContext2D, width: number, height: number, model: PaintModel) {
  context.fillStyle = SCREEN;
  context.fillRect(0, 0, width, height);
  const topInset = 72;
  const side = 28;
  const innerWidth = Math.max(1, width - side * 2);
  const innerHeight = Math.max(1, height - topInset - 112);
  context.save();
  context.translate(side, topInset);

  const track = model.track;
  if (!track) {
    context.fillStyle = TEXT;
    context.font = '32px ' + FONT;
    context.textAlign = 'center';
    context.textBaseline = 'middle';
    context.fillText('Nothing playing', innerWidth / 2, innerHeight / 2);
    context.restore();
    return;
  }

  const scale = innerHeight < 420 ? Math.min(1, Math.max(0.55, innerHeight / 420)) : 1;
  const titleH = 48 * scale;
  const readoutH = 112 * scale;
  const overviewH = 72 * scale;
  const gap = 8 * scale;
  const detailH = Math.max(36, innerHeight - titleH - readoutH - overviewH - gap * 2);
  const titleSize = 32 * scale;
  const remainSize = 56 * scale;
  const tempoSize = 48 * scale;
  const labelSize = 18 * scale;

  const length = model.duration > 0 ? model.duration : null;
  const position = Math.max(0, model.position);
  const span = detailSpan(track.bpm);
  const start = windowStart(position, span, length ?? Number.POSITIVE_INFINITY);

  context.textBaseline = 'middle';
  context.font = `${titleSize}px ${FONT}`;
  context.fillStyle = TEXT;
  context.textAlign = 'left';
  context.fillText(fit(titleLine(track.artist, track.title), titleSize, innerWidth * 0.62), 0, titleH * 0.5);
  const bars = barsText(position, track.bpm);
  if (bars) {
    context.fillStyle = CYAN;
    context.font = `${labelSize}px ${FONT}`;
    context.textAlign = 'center';
    context.fillText(bars, innerWidth * 0.5, titleH * 0.5);
  }
  if (track.key) {
    context.fillStyle = CYAN;
    context.font = `${titleSize}px ${FONT}`;
    context.textAlign = 'right';
    context.fillText(track.key, innerWidth, titleH * 0.5);
  }

  const detailTop = titleH;
  if (model.peaks) {
    paintWave(context, model.peaks, start, start + span, detailTop, innerWidth, detailH, null);
  } else if (model.status !== 'idle') {
    context.fillStyle = CYAN;
    context.font = `${labelSize}px ${FONT}`;
    context.textAlign = 'center';
    context.fillText(
      model.status === 'failed' ? 'The waveform could not be read.' : 'Reading the waveform…',
      innerWidth / 2,
      detailTop + detailH / 2,
    );
  }
  if (track.bpm) {
    for (const mark of beats(start, span, track.bpm)) {
      const x = ((mark.at - start) / span) * innerWidth;
      stroke(context, x, detailTop, detailTop + detailH, mark.bar ? BAR : BEAT, mark.bar ? 2 : 1);
    }
  }
  const needle = Math.min(innerWidth, Math.max(0, ((position - start) / span) * innerWidth));
  stroke(context, needle, detailTop, detailTop + detailH, NEEDLE, 2);

  const readoutTop = detailTop + detailH + gap;
  context.textBaseline = 'top';
  context.textAlign = 'left';
  context.fillStyle = CYAN;
  context.font = `${labelSize}px ${FONT}`;
  context.fillText(model.paused ? 'REMAIN  PAUSED' : 'REMAIN', 0, readoutTop);
  context.fillStyle = TEXT;
  context.font = `${remainSize}px ${FONT}`;
  context.fillText(remainText(position, length), 0, readoutTop + labelSize + 2);
  context.textAlign = 'right';
  context.fillStyle = CYAN;
  context.font = `${labelSize}px ${FONT}`;
  context.fillText('TEMPO', innerWidth, readoutTop);
  context.fillStyle = TEXT;
  context.font = `${tempoSize}px ${FONT}`;
  context.fillText(tempoText(track.bpm), innerWidth, readoutTop + labelSize + 2);
  context.fillStyle = CYAN;
  context.font = `${labelSize}px ${FONT}`;
  context.fillText('BPM', innerWidth, readoutTop + labelSize + tempoSize);

  if (model.peaks) {
    const overviewTop = readoutTop + readoutH + gap;
    const end = Math.max(length ?? 0, model.peaks.seconds);
    if (end > 0) {
      paintWave(context, model.peaks, 0, end, overviewTop, innerWidth, overviewH, position);
      stroke(context, Math.min(innerWidth, Math.max(0, (position / end) * innerWidth)), overviewTop, overviewTop + overviewH, NEEDLE, 2);
    }
  }
  context.restore();
}

function paintWave(
  context: CanvasRenderingContext2D,
  peaks: Peaks,
  from: number,
  to: number,
  top: number,
  width: number,
  height: number,
  dimAfter: number | null,
) {
  const count = Math.min(1600, Math.max(1, Math.round(width)));
  const shape = columns(peaks, from, to, count);
  const slot = width / count;
  const mid = top + height * 0.5;
  const half = height * 0.46;
  const span = Math.max(1e-6, to - from);
  for (let index = 0; index < shape.length; index += 1) {
    const column = shape[index]!;
    const columnTime = from + ((index + 0.5) / count) * span;
    context.fillStyle = dimAfter !== null && columnTime >= dimAfter ? WAVE_DIM : WAVE;
    const y = mid - column.max * half;
    const bottom = mid - column.min * half;
    context.fillRect(index * slot, y, Math.max(1, slot), Math.max(1, bottom - y));
  }
}

function stroke(
  context: CanvasRenderingContext2D,
  x: number,
  top: number,
  bottom: number,
  color: string,
  width: number,
) {
  context.strokeStyle = color;
  context.lineWidth = width;
  context.beginPath();
  context.moveTo(x, top);
  context.lineTo(x, bottom);
  context.stroke();
}

function fit(text: string, size: number, width: number): string {
  const limit = Math.floor(width / Math.max(1, size * 0.6));
  if (limit <= 0 || text.length <= limit) return text;
  return `${text.slice(0, Math.max(0, limit - 1))}…`;
}

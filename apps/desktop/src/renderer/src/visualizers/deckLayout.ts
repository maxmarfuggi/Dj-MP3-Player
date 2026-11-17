/**
 * Timing and waveform bins for the CDJ deck.
 *
 * The same rules are in the wall player's `deck_math.rs` and `peaks.rs`:
 * eight beats in the scrolling window, beat zero at the start of the file,
 * and eighty peaks per second.
 */

const DETAIL_BEATS = 8;
const FALLBACK_SPAN = 4;
const BPM_MIN = 20;
const BPM_MAX = 300;

export const PEAKS_PER_SECOND = 80;

export interface Remain {
  minutes: number;
  seconds: number;
  hundredths: number;
}

export interface Beat {
  at: number;
  bar: boolean;
}

export interface Peaks {
  mins: number[];
  maxs: number[];
  seconds: number;
}

/** Time left in the track. A position past the end reads as zero. */
export function remain(position: number, length: number): Remain {
  const left =
    Number.isFinite(position) && Number.isFinite(length) ? Math.max(0, length - position) : 0;
  // Round to a millisecond first, so a fraction that displays as the next
  // tenth carries into the seconds instead of reading 100.0.
  const millis = Math.round(left * 1000);
  return {
    minutes: Math.floor(millis / 60_000),
    seconds: Math.floor(millis / 1000) % 60,
    hundredths: (millis % 1000) / 10,
  };
}

/** `06M:31S 66.0F`, or `--M:--S` when the file does not state a length. */
export function remainText(position: number, length: number | null): string {
  if (length === null || !Number.isFinite(length) || length <= 0) return '--M:--S';
  const left = remain(position, length);
  const hundredths = left.hundredths.toFixed(1).padStart(4, '0');
  return `${String(left.minutes).padStart(2, '0')}M:${String(left.seconds).padStart(2, '0')}S ${hundredths}F`;
}

/** One decimal, as on a CDJ. Missing tempo is an em dash. */
export function tempoText(bpm: number | null): string {
  if (bpm === null || !Number.isFinite(bpm) || bpm <= 0) return '—';
  return bpm.toFixed(1);
}

/** Bars from the start of the file, counting four beats to a bar. */
export function barsText(position: number, bpm: number | null): string | null {
  if (bpm === null || !Number.isFinite(bpm) || bpm <= BPM_MIN || bpm >= BPM_MAX) return null;
  const bars = Math.max(0, position) * bpm / 240;
  return `${bars.toFixed(1).padStart(4, '0')} BARS`;
}

/** Seconds across the scrolling waveform: eight beats, or four seconds. */
export function detailSpan(bpm: number | null): number {
  if (bpm !== null && Number.isFinite(bpm) && bpm > BPM_MIN && bpm < BPM_MAX) {
    return (DETAIL_BEATS * 60) / bpm;
  }
  return FALLBACK_SPAN;
}

/**
 * Puts the playhead in the middle of the window, and pins the window to the
 * ends of the track so it never shows time outside the file.
 */
export function windowStart(position: number, span: number, length: number): number {
  if (!Number.isFinite(position) || !Number.isFinite(span) || span <= 0) return 0;
  const at = Math.max(0, position);
  if (!Number.isFinite(length)) return Math.max(0, at - span / 2);
  if (length <= span) return 0;
  return Math.min(Math.max(0, at - span / 2), length - span);
}

/** Beat marks inside `[start, start + span)`. Every fourth beat is a bar. */
export function beats(start: number, span: number, bpm: number): Beat[] {
  if (!Number.isFinite(bpm) || bpm <= BPM_MIN || bpm >= BPM_MAX || !Number.isFinite(span) || span <= 0) {
    return [];
  }
  const period = 60 / bpm;
  const end = start + span;
  let index = Math.max(0, Math.ceil(start / period));
  const marks: Beat[] = [];
  while (marks.length < 64 && index < 1_000_000_000) {
    const at = index * period;
    if (at >= end - 1e-6) break;
    if (at + 1e-6 >= start) marks.push({ at, bar: index % 4 === 0 });
    index += 1;
  }
  return marks;
}

/**
 * Folds samples into `peaksPerSecond` bins. The last, shorter bin is kept.
 * A bin's minimum starts at zero, so a loud peak still reaches the centre.
 */
export function peaksFromChannel(samples: ArrayLike<number>, rate: number, peaksPerSecond = PEAKS_PER_SECOND): Peaks {
  const safeRate = Math.max(1, Math.floor(rate) || 1);
  const framesPerBin = Math.max(1, Math.floor(safeRate / peaksPerSecond));
  const mins: number[] = [];
  const maxs: number[] = [];
  let inBin = 0;
  let min = 0;
  let max = 0;
  for (let index = 0; index < samples.length; index += 1) {
    const sample = samples[index] ?? 0;
    const finite = Number.isFinite(sample) ? sample : 0;
    min = Math.min(min, finite);
    max = Math.max(max, finite);
    inBin += 1;
    if (inBin === framesPerBin) {
      mins.push(min);
      maxs.push(max);
      min = 0;
      max = 0;
      inBin = 0;
    }
  }
  if (inBin > 0) {
    mins.push(min);
    maxs.push(max);
  }
  return { mins, maxs, seconds: samples.length / safeRate };
}

/**
 * Min and max for each column between `from` and `to`. Each peak covers its
 * whole slice of time, so a column between two peak starts still has a shape.
 */
export function columns(peaks: Peaks, from: number, to: number, count: number): { min: number; max: number }[] {
  const out = Array.from({ length: count }, () => ({ min: 0, max: 0 }));
  if (count === 0 || peaks.mins.length === 0 || !(to > from) || peaks.seconds <= 0) return out;
  const span = to - from;
  const total = peaks.mins.length;
  for (let index = 0; index < total; index += 1) {
    const binStart = (index / total) * peaks.seconds;
    const binEnd = ((index + 1) / total) * peaks.seconds;
    if (binEnd <= from || binStart >= to) continue;
    const first = Math.min(count - 1, Math.max(0, Math.floor(((binStart - from) / span) * count)));
    const last = Math.min(count, Math.max(first + 1, Math.ceil(((binEnd - from) / span) * count)));
    const min = peaks.mins[index] ?? 0;
    const max = peaks.maxs[index] ?? 0;
    for (let column = first; column < last; column += 1) {
      const slot = out[column]!;
      slot.min = Math.min(slot.min, min);
      slot.max = Math.max(slot.max, max);
    }
  }
  return out;
}

/** "Artist - Title", matching a CDJ performance screen. */
export function titleLine(artist: string, title: string): string {
  if (!artist && !title) return '';
  if (!artist) return title;
  if (!title) return artist;
  return `${artist} - ${title}`;
}

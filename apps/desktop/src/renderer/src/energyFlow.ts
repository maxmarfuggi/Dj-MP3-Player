import type { Track } from '../../shared/types';
import { type CamelotKey, camelotKey } from './keyColor';

export type HarmonicKind = 'smooth' | 'shift' | 'clash';

export interface HarmonicMix {
  kind: HarmonicKind;
  /** What the move is called, for tooltips. */
  label: string;
}

/** More of a tempo change than this between two tracks is hard to beatmatch. */
export const TEMPO_JUMP = 0.06;

export interface FlowPoint {
  key: CamelotKey | null;
  bpm: number | null;
  /**
   * Height on the chart in BPM: usually the track's BPM, but doubled or halved
   * when that is how it would be mixed with the track before it.
   */
  level: number | null;
}

export interface FlowMix {
  /** Null when either key is unknown. */
  harmonic: HarmonicMix | null;
  /** Relative tempo change, allowing for half and double time; null when either BPM is unknown. */
  tempoChange: number | null;
  tempoJump: boolean;
  /** The tempos only line up at half or double time. */
  halfTime: boolean;
}

export interface EnergyFlow {
  /** One per track. */
  points: FlowPoint[];
  /** Between each track and the next. */
  mixes: FlowMix[];
  counts: Record<HarmonicKind, number>;
  tempoJumps: number;
}

/** Steps from one wheel number to another the short way round, from -5 to 6. */
function wheelSteps(from: number, to: number): number {
  const steps = (((to - from) % 12) + 12) % 12;
  return steps > 6 ? steps - 12 : steps;
}

/** Rates a mix from one key into another by the usual Camelot wheel rules. */
export function classifyHarmonic(from: CamelotKey, to: CamelotKey): HarmonicMix {
  const steps = wheelSteps(from.number, to.number);
  if (from.letter === to.letter) {
    if (steps === 0) return { kind: 'smooth', label: 'Same key' };
    if (steps === 1) return { kind: 'smooth', label: 'One step up' };
    if (steps === -1) return { kind: 'smooth', label: 'One step down' };
    if (steps === 2) return { kind: 'shift', label: 'Energy boost' };
    if (steps === -2) return { kind: 'shift', label: 'Energy drop' };
    // Seven steps round the wheel is a semitone up; five is a semitone down.
    if (steps === -5) return { kind: 'shift', label: 'Semitone up' };
    if (steps === 5) return { kind: 'shift', label: 'Semitone down' };
  } else {
    if (steps === 0) return { kind: 'smooth', label: to.letter === 'B' ? 'Relative major' : 'Relative minor' };
    // Minor up one into major, or major down one into minor, keeps most notes in common.
    if ((from.letter === 'A' && steps === 1) || (from.letter === 'B' && steps === -1)) {
      return { kind: 'shift', label: 'Diagonal mix' };
    }
  }
  return { kind: 'clash', label: 'Key clash' };
}

/**
 * The tempo `to` would be played at after `from`: as it is, or doubled or
 * halved when only that brings it within beatmatching range.
 */
export function matchTempo(from: number, to: number): { bpm: number; halfTime: boolean } {
  const change = (bpm: number) => Math.abs(bpm - from) / from;
  if (change(to) <= TEMPO_JUMP) return { bpm: to, halfTime: false };
  for (const bpm of [to * 2, to / 2]) {
    if (change(bpm) <= TEMPO_JUMP) return { bpm, halfTime: true };
  }
  return { bpm: to, halfTime: false };
}

export function energyFlow(tracks: readonly Pick<Track, 'bpm' | 'key'>[]): EnergyFlow {
  const counts: Record<HarmonicKind, number> = { smooth: 0, shift: 0, clash: 0 };
  let tempoJumps = 0;
  let previousLevel: number | null = null;
  const points = tracks.map((track): FlowPoint => {
    const key = track.key ? camelotKey(track.key) : null;
    const bpm = track.bpm !== null && track.bpm > 0 ? track.bpm : null;
    let level = bpm;
    if (bpm !== null && previousLevel !== null) level = matchTempo(previousLevel, bpm).bpm;
    if (level !== null) previousLevel = level;
    return { key, bpm, level };
  });

  const mixes = points.slice(1).map((point, index): FlowMix => {
    const previous = points[index]!;
    const harmonic = previous.key && point.key ? classifyHarmonic(previous.key, point.key) : null;
    if (harmonic) counts[harmonic.kind] += 1;
    if (previous.bpm === null || point.bpm === null) {
      return { harmonic, tempoChange: null, tempoJump: false, halfTime: false };
    }
    const matched = matchTempo(previous.bpm, point.bpm);
    const tempoChange = (matched.bpm - previous.bpm) / previous.bpm;
    const tempoJump = Math.abs(tempoChange) > TEMPO_JUMP;
    if (tempoJump) tempoJumps += 1;
    return { harmonic, tempoChange, tempoJump, halfTime: matched.halfTime };
  });

  return { points, mixes, counts, tempoJumps };
}

/** Evenly spaced BPM gridlines covering a range, about four of them. */
export function bpmGrid(low: number, high: number): number[] {
  const step = [1, 2, 5, 10, 20, 50].find((candidate) => (high - low) / candidate <= 5) ?? 100;
  const lines: number[] = [];
  for (let bpm = Math.ceil(low / step) * step; bpm <= high; bpm += step) lines.push(bpm);
  return lines;
}

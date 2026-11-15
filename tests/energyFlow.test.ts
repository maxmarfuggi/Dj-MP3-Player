import { describe, expect, it } from 'vitest';
import { bpmGrid, classifyHarmonic, energyFlow, matchTempo } from '../src/renderer/src/energyFlow';
import { camelotKey } from '../src/renderer/src/keyColor';

const kind = (from: string, to: string) => classifyHarmonic(camelotKey(from)!, camelotKey(to)!).kind;

describe('camelotKey', () => {
  it('reads Camelot codes and note names', () => {
    expect(camelotKey('8a')).toEqual({ number: 8, letter: 'A' });
    expect(camelotKey('Am')).toEqual({ number: 8, letter: 'A' });
    expect(camelotKey('C')).toEqual({ number: 8, letter: 'B' });
    expect(camelotKey('F#min')).toEqual({ number: 11, letter: 'A' });
    expect(camelotKey('13A')).toBeNull();
    expect(camelotKey('H')).toBeNull();
  });
});

describe('classifyHarmonic', () => {
  it('rates neighbouring and relative keys as smooth', () => {
    expect(kind('8A', '8A')).toBe('smooth');
    expect(kind('8A', '9A')).toBe('smooth');
    expect(kind('1B', '12B')).toBe('smooth');
    expect(kind('8A', '8B')).toBe('smooth');
  });

  it('rates energy boosts, semitone changes and diagonal mixes as shifts', () => {
    expect(kind('8A', '10A')).toBe('shift');
    expect(kind('12A', '2A')).toBe('shift');
    expect(kind('8A', '3A')).toBe('shift');
    expect(kind('8A', '9B')).toBe('shift');
    expect(kind('8B', '7A')).toBe('shift');
  });

  it('rates anything else as a clash', () => {
    expect(kind('8A', '11A')).toBe('clash');
    expect(kind('8A', '7B')).toBe('clash');
    expect(kind('8B', '2A')).toBe('clash');
  });
});

describe('matchTempo', () => {
  it('keeps tempos that are close enough to beatmatch', () => {
    expect(matchTempo(124, 128)).toEqual({ bpm: 128, halfTime: false });
  });

  it('doubles or halves a tempo when only that lines it up', () => {
    expect(matchTempo(172, 87)).toEqual({ bpm: 174, halfTime: true });
    expect(matchTempo(70, 140)).toEqual({ bpm: 70, halfTime: true });
  });

  it('leaves tempos that do not line up either way', () => {
    expect(matchTempo(100, 128)).toEqual({ bpm: 128, halfTime: false });
  });
});

describe('energyFlow', () => {
  it('rates each mix by key and flags tempo jumps', () => {
    const flow = energyFlow([
      { bpm: 124, key: '8A' },
      { bpm: 126, key: '9A' },
      { bpm: 140, key: '12A' },
    ]);
    expect(flow.mixes.map((mix) => mix.harmonic?.kind)).toEqual(['smooth', 'clash']);
    expect(flow.mixes.map((mix) => mix.tempoJump)).toEqual([false, true]);
    expect(flow.mixes[0]!.tempoChange).toBeCloseTo(2 / 124);
    expect(flow.counts).toEqual({ smooth: 1, shift: 0, clash: 1 });
    expect(flow.tempoJumps).toBe(1);
  });

  it('plots a half-time track at the tempo it would be mixed at', () => {
    const flow = energyFlow([
      { bpm: 172, key: null },
      { bpm: 87, key: null },
    ]);
    expect(flow.points.map((point) => point.level)).toEqual([172, 174]);
    expect(flow.points[1]!.bpm).toBe(87);
    expect(flow.mixes[0]).toMatchObject({ tempoJump: false, halfTime: true });
  });

  it('leaves mixes next to a track without a BPM or key unrated', () => {
    const flow = energyFlow([
      { bpm: 124, key: '8A' },
      { bpm: null, key: 'nonsense' },
      { bpm: 125, key: '8A' },
    ]);
    expect(flow.points.map((point) => point.level)).toEqual([124, null, 125]);
    expect(flow.mixes).toEqual([
      { harmonic: null, tempoChange: null, tempoJump: false, halfTime: false },
      { harmonic: null, tempoChange: null, tempoJump: false, halfTime: false },
    ]);
  });
});

describe('bpmGrid', () => {
  it('picks a round step for the range', () => {
    expect(bpmGrid(121.5, 130.5)).toEqual([122, 124, 126, 128, 130]);
    expect(bpmGrid(85, 175)).toEqual([100, 120, 140, 160]);
  });
});

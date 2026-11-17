import { describe, expect, it } from 'vitest';
import {
  barsText,
  beats,
  columns,
  detailSpan,
  peaksFromChannel,
  remainText,
  tempoText,
  windowStart,
} from '../src/renderer/src/visualizers/deckLayout';

describe('the CDJ readout', () => {
  it('writes the time remaining like a deck', () => {
    expect(remainText(0, 6 * 60 + 31.66)).toBe('06M:31S 66.0F');
    expect(remainText(10, 6 * 60 + 31.66)).toBe('06M:21S 66.0F');
    expect(remainText(0, null)).toBe('--M:--S');
    expect(remainText(5, 0)).toBe('--M:--S');
  });

  it('carries a rounded fraction into the next second', () => {
    expect(remainText(0, 0.9995)).toBe('00M:01S 00.0F');
    expect(remainText(0, 59.9999)).toBe('01M:00S 00.0F');
  });

  it('prints tempo and bars from the file bpm', () => {
    expect(tempoText(128.36)).toBe('128.4');
    expect(tempoText(null)).toBe('—');
    expect(barsText(0, 120)).toBe('00.0 BARS');
    expect(barsText(8, 120)).toBe('04.0 BARS');
    expect(barsText(1, null)).toBeNull();
    expect(barsText(1, 10)).toBeNull();
  });
});

describe('the scrolling window', () => {
  it('shows eight beats and stays inside the track', () => {
    expect(detailSpan(120)).toBe(4);
    expect(detailSpan(null)).toBe(4);
    expect(detailSpan(10)).toBe(4);
    expect(detailSpan(400)).toBe(4);
    expect(windowStart(10, 4, 100)).toBe(8);
    expect(windowStart(1, 4, 100)).toBe(0);
    expect(windowStart(99, 4, 100)).toBe(96);
    expect(windowStart(2, 4, 3)).toBe(0);
  });

  it('counts beats from zero and marks every fourth as a bar', () => {
    const marks = beats(0, 2, 120);
    expect(marks).toHaveLength(4);
    expect(marks[0]!.bar).toBe(true);
    expect(marks[1]!.at).toBeCloseTo(0.5);
    expect(marks[1]!.bar).toBe(false);
    expect(marks[3]!.bar).toBe(false);

    const later = beats(1, 2, 120);
    expect(later.some((mark) => mark.bar && Math.abs(mark.at - 2) < 1e-9)).toBe(true);
    expect(beats(0, 4, 10)).toEqual([]);
  });
});

describe('waveform peaks', () => {
  it('folds samples into eightieths of a second and keeps a short tail', () => {
    const peaks = peaksFromChannel([0.2, -0.5, 0.1, 0.4], 160);
    expect(peaks.mins).toEqual([-0.5, 0]);
    expect(peaks.maxs).toEqual([0.2, 0.4]);
    expect(peaks.seconds).toBeCloseTo(4 / 160);

    const tail = peaksFromChannel([0.5, -0.25, 0.1], 160);
    expect(tail.maxs).toEqual([0.5, 0.1]);
    expect(tail.mins).toHaveLength(2);
  });

  it('lets one peak fill every column it covers', () => {
    const peaks = { mins: [-0.2, -0.4], maxs: [0.5, 0.1], seconds: 1 };
    expect(columns(peaks, 0, 1, 2)).toEqual([
      { min: -0.2, max: 0.5 },
      { min: -0.4, max: 0.1 },
    ]);
    expect(columns(peaks, 0.5, 1, 1)).toEqual([{ min: -0.4, max: 0.1 }]);
    expect(columns({ mins: [-0.3], maxs: [0.8], seconds: 4 }, 0, 4, 4)).toEqual([
      { min: -0.3, max: 0.8 },
      { min: -0.3, max: 0.8 },
      { min: -0.3, max: 0.8 },
      { min: -0.3, max: 0.8 },
    ]);
  });
});

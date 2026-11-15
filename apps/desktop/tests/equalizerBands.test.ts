import { describe, expect, it } from 'vitest';
import { bandLevel, logBandRanges } from '../src/renderer/src/visualizers/equalizerBands';

describe('logBandRanges', () => {
  const sampleRate = 48000;
  const fftSize = 16384;
  const binWidth = sampleRate / fftSize;
  const ranges = logBandRanges(sampleRate, fftSize, 72, 35, 16000);

  it('gives every band at least one bin, in order and inside the FFT', () => {
    expect(ranges).toHaveLength(72);
    for (const [index, { start, end }] of ranges.entries()) {
      expect(end).toBeGreaterThan(start);
      expect(start).toBeGreaterThanOrEqual(1);
      expect(end).toBeLessThanOrEqual(fftSize / 2);
      if (index > 0) expect(start).toBeGreaterThanOrEqual(ranges[index - 1]!.start);
    }
  });

  it('covers the requested frequencies', () => {
    expect(ranges[0]!.start * binWidth).toBeCloseTo(35, -1);
    expect(ranges.at(-1)!.end * binWidth).toBeCloseTo(16000, -1);
  });

  it('makes bands wider in hertz as they go up', () => {
    const width = ({ start, end }: { start: number; end: number }) => end - start;
    expect(width(ranges.at(-1)!)).toBeGreaterThan(width(ranges[36]!));
    expect(width(ranges[36]!)).toBeGreaterThan(width(ranges[0]!));
  });
});

describe('bandLevel', () => {
  it('adds up the power of the bins in the band', () => {
    const decibels = new Float32Array([-100, -20, -20, -100]);
    expect(bandLevel(decibels, { start: 1, end: 3 })).toBeCloseTo(-20 + 10 * Math.log10(2), 5);
  });

  it('is silent when every bin is', () => {
    expect(bandLevel(new Float32Array([-Infinity, -Infinity]), { start: 0, end: 2 })).toBe(-Infinity);
  });
});

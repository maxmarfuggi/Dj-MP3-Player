import { describe, expect, it } from 'vitest';
import { ledMetrics } from '../src/renderer/src/visualizers/Equalizer';

describe('LED matrix', () => {
  it('leaves a dark gap between neighbouring lamps', () => {
    const windows = [
      [1280, 720, 1],
      [1920, 1080, 1],
      [1920, 1080, 1.25],
      [2560, 1440, 1.5],
      [800, 500, 2],
    ];
    for (const [width, height, px] of windows) {
      const { slot, rowPitch, radius } = ledMetrics(width!, height!, px!);
      const pitch = Math.min(slot, rowPitch);
      expect(radius).toBeGreaterThan(0);
      expect(radius * 2).toBeLessThan(pitch);
    }
  });
});

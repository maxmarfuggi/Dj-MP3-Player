/** A band's share of an FFT's bins: `start` inclusive, `end` exclusive. */
export interface BinRange {
  start: number;
  end: number;
}

/**
 * Splits `minHz`–`maxHz` into `count` bands of equal width in octaves, as the ear
 * hears pitch, and says which FFT bins fall in each. Bands narrower than a bin
 * still get one.
 */
export function logBandRanges(
  sampleRate: number,
  fftSize: number,
  count: number,
  minHz: number,
  maxHz: number,
): BinRange[] {
  const binCount = fftSize / 2;
  const binWidth = sampleRate / fftSize;
  const edge = (index: number) => minHz * (maxHz / minHz) ** (index / count);
  return Array.from({ length: count }, (_, index) => {
    const start = Math.min(Math.max(1, Math.round(edge(index) / binWidth)), binCount - 1);
    const end = Math.min(Math.max(start + 1, Math.round(edge(index + 1) / binWidth)), binCount);
    return { start, end };
  });
}

/**
 * The band's total level in dB, adding up the power of its bins. Higher bands
 * span more bins, which offsets music naturally being quieter up there.
 */
export function bandLevel(decibels: Float32Array, { start, end }: BinRange): number {
  let power = 0;
  for (let bin = start; bin < end; bin++) power += 10 ** (decibels[bin]! / 10);
  return power > 0 ? 10 * Math.log10(power) : -Infinity;
}

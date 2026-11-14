import { describe, expect, it } from 'vitest';
import { parseRangeHeader } from '../src/main/range';

describe('parseRangeHeader', () => {
  it('returns null when there is no usable range', () => {
    expect(parseRangeHeader(null, 1000)).toBeNull();
    expect(parseRangeHeader('bytes=-', 1000)).toBeNull();
    expect(parseRangeHeader('bytes=0-10,20-30', 1000)).toBeNull();
  });

  it('parses open and closed ranges', () => {
    expect(parseRangeHeader('bytes=0-', 1000)).toEqual({ start: 0, end: 999 });
    expect(parseRangeHeader('bytes=200-299', 1000)).toEqual({ start: 200, end: 299 });
  });

  it('clamps the end to the file size', () => {
    expect(parseRangeHeader('bytes=900-5000', 1000)).toEqual({ start: 900, end: 999 });
  });

  it('parses suffix ranges', () => {
    expect(parseRangeHeader('bytes=-100', 1000)).toEqual({ start: 900, end: 999 });
    expect(parseRangeHeader('bytes=-5000', 1000)).toEqual({ start: 0, end: 999 });
  });

  it('reports ranges outside the file', () => {
    expect(parseRangeHeader('bytes=1000-', 1000)).toBe('unsatisfiable');
    expect(parseRangeHeader('bytes=500-400', 1000)).toBe('unsatisfiable');
    expect(parseRangeHeader('bytes=-0', 1000)).toBe('unsatisfiable');
    expect(parseRangeHeader('bytes=0-', 0)).toBe('unsatisfiable');
  });
});

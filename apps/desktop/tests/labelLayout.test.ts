import { describe, expect, it } from 'vitest';
import { layoutLabelText, type LabelTextOptions } from '../src/renderer/src/visualizers/labelLayout';

const measure = (text: string, fontSize: number) => text.length * fontSize * 0.6;

const title: LabelTextOptions = {
  measure,
  radius: 86,
  fontSizes: [15, 13, 11.5],
  maxLines: 3,
  anchorY: -24,
  direction: 'up',
};

function fitsCircle(result: ReturnType<typeof layoutLabelText>, options: LabelTextOptions): boolean {
  return result.lines.every(({ text, y }) => {
    const edge = options.direction === 'up' ? y - result.fontSize * 0.8 : y + result.fontSize * 0.25;
    const width = 2 * Math.sqrt(options.radius ** 2 - edge ** 2);
    return measure(text, result.fontSize) <= width + 1e-9;
  });
}

describe('layoutLabelText', () => {
  it('keeps short text on one line at full size, nearest the centre', () => {
    expect(layoutLabelText('Low Tone', title)).toEqual({ fontSize: 15, lines: [{ text: 'Low Tone', y: -24 }] });
  });

  it('wraps long titles upwards, keeping every word and staying inside the circle', () => {
    const text = 'Strings of Life (Derrick May Original Extended Mix)';
    const result = layoutLabelText(text, title);
    expect(result.lines.length).toBeGreaterThan(1);
    expect(result.lines.map((line) => line.text).join(' ')).toBe(text);
    expect(result.lines.at(-1)!.y).toBe(-24);
    const ys = result.lines.map((line) => line.y);
    expect(ys).toEqual([...ys].sort((a, b) => a - b));
    expect(fitsCircle(result, title)).toBe(true);
  });

  it('stacks downwards for text below the spindle', () => {
    const options: LabelTextOptions = { ...title, fontSizes: [13, 11], maxLines: 2, anchorY: 36, direction: 'down' };
    const result = layoutLabelText('Charlotte de Witte & Enrico Sangiuliano', options);
    expect(result.lines[0]!.y).toBe(36);
    expect(result.lines.length).toBe(2);
    expect(fitsCircle(result, options)).toBe(true);
  });

  it('breaks a single word too long for any line', () => {
    const result = layoutLabelText('Supercalifragilisticexpialidocious', title);
    expect(result.lines.map((line) => line.text).join('')).toBe('Supercalifragilisticexpialidocious');
    expect(fitsCircle(result, title)).toBe(true);
  });

  it('shrinks and then ends with an ellipsis when nothing fits', () => {
    const result = layoutLabelText('word '.repeat(60), title);
    expect(result.fontSize).toBe(11.5);
    expect(result.lines.length).toBe(3);
    expect(result.lines.at(-1)!.text.endsWith('…')).toBe(true);
    expect(fitsCircle(result, title)).toBe(true);
  });

  it('returns no lines for blank text', () => {
    expect(layoutLabelText('  ', title).lines).toEqual([]);
  });
});

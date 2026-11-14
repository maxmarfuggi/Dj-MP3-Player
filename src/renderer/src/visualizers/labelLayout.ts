export type MeasureText = (text: string, fontSize: number) => number;

export interface LabelTextOptions {
  measure: MeasureText;
  /** Radius, from the label's centre, that text must stay inside. */
  radius: number;
  /** Font sizes to try, largest first. */
  fontSizes: number[];
  maxLines: number;
  /** Baseline of the line nearest the centre (negative is above it). */
  anchorY: number;
  /** Whether extra lines stack away from the centre upwards or downwards. */
  direction: 'up' | 'down';
}

export interface LabelText {
  fontSize: number;
  lines: { text: string; y: number }[];
}

const LINE_HEIGHT = 1.15;
// Rough glyph extents, as fractions of the font size.
const ASCENT = 0.8;
const DESCENT = 0.25;

/**
 * Wraps text into lines that fit a circular label, where lines further from
 * the centre have less room. Shrinks the font before resorting to an ellipsis.
 */
export function layoutLabelText(text: string, options: LabelTextOptions): LabelText {
  const { measure, fontSizes, maxLines } = options;
  const trimmed = text.trim();
  if (!trimmed) return { fontSize: fontSizes[0]!, lines: [] };

  for (const fontSize of fontSizes) {
    for (let count = 1; count <= maxLines; count++) {
      const slots = lineSlots(count, fontSize, options);
      const lines = wrap(trimmed, slots, fontSize, measure);
      if (lines.length <= count) return place(lines, slots, fontSize);
    }
  }

  const fontSize = fontSizes[fontSizes.length - 1]!;
  const slots = lineSlots(maxLines, fontSize, options);
  const lines = wrap(trimmed, slots, fontSize, measure);
  const kept = lines.slice(0, maxLines);
  const lastIndex = kept.length - 1;
  kept[lastIndex] = ellipsize(kept[lastIndex]!, slots[lastIndex]!.width, fontSize, measure);
  return place(kept, slots, fontSize);
}

interface Slot {
  y: number;
  width: number;
}

/** Baselines and available widths for `count` lines, in reading order. */
function lineSlots(count: number, fontSize: number, options: LabelTextOptions): Slot[] {
  const { radius, anchorY, direction } = options;
  const step = fontSize * LINE_HEIGHT;
  return Array.from({ length: count }, (_, index) => {
    const y = direction === 'up' ? anchorY - (count - 1 - index) * step : anchorY + index * step;
    const farEdge = direction === 'up' ? y - fontSize * ASCENT : y + fontSize * DESCENT;
    const halfChord = Math.sqrt(Math.max(0, radius * radius - farEdge * farEdge));
    return { y, width: 2 * halfChord };
  });
}

/** Greedy word wrap; anything beyond the last slot ends up on one extra line. */
function wrap(text: string, slots: Slot[], fontSize: number, measure: MeasureText): string[] {
  const lines: string[] = [];
  const fits = (candidate: string) =>
    lines.length >= slots.length || measure(candidate, fontSize) <= slots[lines.length]!.width;

  let line = '';
  for (const word of text.split(/\s+/)) {
    const candidate = line ? `${line} ${word}` : word;
    if (fits(candidate)) {
      line = candidate;
      continue;
    }
    if (line) lines.push(line);
    let rest = word;
    // A word too long for a whole line is broken mid-word.
    while (!fits(rest)) {
      let cut = rest.length - 1;
      while (cut > 1 && !fits(rest.slice(0, cut))) cut--;
      lines.push(rest.slice(0, cut));
      rest = rest.slice(cut);
    }
    line = rest;
  }
  if (line) lines.push(line);
  return lines;
}

function ellipsize(line: string, width: number, fontSize: number, measure: MeasureText): string {
  let text = line;
  while (text && measure(`${text}…`, fontSize) > width) text = text.slice(0, -1);
  return `${text.trimEnd()}…`;
}

function place(lines: string[], slots: Slot[], fontSize: number): LabelText {
  return { fontSize, lines: lines.map((text, index) => ({ text, y: slots[index]!.y })) };
}

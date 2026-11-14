import type { Track } from '../../shared/types';

function hash(text: string): number {
  let value = 0;
  for (const char of text) value = (value * 31 + char.charCodeAt(0)) | 0;
  return Math.abs(value);
}

const LABEL_COLORS = ['#d9463e', '#e9a23b', '#3f8fd9', '#4bb07b', '#9b5de5', '#e0dcd0'];
const BLANK_LABEL_COLOR = '#3a3d48';

/** Colour of a track's record label. The same track always gets the same one. */
export function labelColor(track: Track | null): string {
  return track ? LABEL_COLORS[hash(track.id) % LABEL_COLORS.length]! : BLANK_LABEL_COLOR;
}

// Hues that make a good-looking gradient; arbitrary hues include muddy ones.
const COVER_HUES = [212, 232, 258, 284, 322, 346, 18, 168, 192];

/** A stable hue (in degrees) for generated cover art, picked from the name. */
export function coverHue(name: string): number {
  return COVER_HUES[hash(name) % COVER_HUES.length]!;
}

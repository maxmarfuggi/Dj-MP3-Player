const PITCH_CLASSES: Record<string, number> = { C: 0, D: 2, E: 4, F: 5, G: 7, A: 9, B: 11 };

export interface CamelotKey {
  /** Position on the wheel, 1–12. */
  number: number;
  /** A for minor keys, B for major keys. */
  letter: 'A' | 'B';
}

/**
 * A key's Camelot code, or null if the text is not a key we recognise.
 * Accepts Camelot codes (`8A`) and note names (`Am`, `F#`, `Bbmin`).
 */
export function camelotKey(key: string): CamelotKey | null {
  const text = key.trim();

  const camelot = /^(\d{1,2})([AB])$/i.exec(text);
  if (camelot) {
    const number = Number(camelot[1]);
    const letter = camelot[2]!.toUpperCase() as 'A' | 'B';
    return number >= 1 && number <= 12 ? { number, letter } : null;
  }

  const note = /^([A-G])([#♯b♭]?)\s*(m|min|minor|maj|major)?$/i.exec(text);
  if (!note) return null;
  const accidental = note[2] === '' ? 0 : note[2] === '#' || note[2] === '♯' ? 1 : -1;
  const pitchClass = (PITCH_CLASSES[note[1]!.toUpperCase()]! + accidental + 12) % 12;
  // A lone capital M means major; every other spelling starting with m but not "maj" is minor.
  const mode = note[3] ?? '';
  const isMinor = mode === 'm' || /^min/i.test(mode);
  // A minor key shares its number with the major key three semitones up.
  const majorPitchClass = isMinor ? (pitchClass + 3) % 12 : pitchClass;
  // Each step around the wheel is a fifth (7 semitones); C major is 8.
  return { number: (((majorPitchClass * 7) % 12) + 7) % 12 + 1, letter: isMinor ? 'A' : 'B' };
}

/**
 * Position of a key on the Camelot wheel (1–12), or null if unrecognised.
 * Neighbouring numbers mix well, which is why DJs colour keys by it.
 */
export function camelotNumber(key: string): number | null {
  return camelotKey(key)?.number ?? null;
}

/** Hue (in degrees) of a key in the usual Camelot wheel colours, or null if unrecognised. */
export function keyHue(key: string): number | null {
  const number = camelotNumber(key);
  return number === null ? null : camelotHue(number);
}

/** Hue (in degrees) of a Camelot wheel number. */
export function camelotHue(number: number): number {
  // 1 is teal-green; the colours then run backwards through the rainbow.
  return (((165 - (number - 1) * 30) % 360) + 360) % 360;
}

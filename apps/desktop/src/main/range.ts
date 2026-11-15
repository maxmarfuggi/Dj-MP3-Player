export interface ByteRange {
  /** First byte, inclusive. */
  start: number;
  /** Last byte, inclusive. */
  end: number;
}

/**
 * Parses an HTTP `Range` header for a resource of `size` bytes.
 *
 * Returns null when the whole resource should be sent (no header, or a form
 * this parser does not handle, such as multiple ranges) and `'unsatisfiable'`
 * when the range lies outside the resource.
 */
export function parseRangeHeader(header: string | null, size: number): ByteRange | null | 'unsatisfiable' {
  const match = header?.match(/^bytes=(\d*)-(\d*)$/);
  if (!match) return null;
  const [, first, last] = match;

  if (first === '') {
    if (last === '') return null;
    // `bytes=-N` asks for the final N bytes.
    const suffixLength = Number(last);
    if (suffixLength === 0 || size === 0) return 'unsatisfiable';
    return { start: Math.max(size - suffixLength, 0), end: size - 1 };
  }

  const start = Number(first);
  const end = last === '' ? size - 1 : Math.min(Number(last), size - 1);
  if (start >= size || start > end) return 'unsatisfiable';
  return { start, end };
}

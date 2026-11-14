/**
 * Serato's `database V2` and `.crate` files are sequences of chunks: a 4-byte
 * ASCII tag, a 4-byte big-endian length, then the data. Tags starting with `o`
 * hold nested chunks; `t` and `p` tags hold UTF-16BE text.
 */

export interface Chunk {
  tag: string;
  data: Buffer;
}

const CHUNK_HEADER_SIZE = 8;

/** Yields the chunks in `buffer`, stopping at the first truncated one. */
export function* readChunks(buffer: Buffer): Generator<Chunk> {
  let at = 0;
  while (at + CHUNK_HEADER_SIZE <= buffer.length) {
    const tag = buffer.toString('latin1', at, at + 4);
    const end = at + CHUNK_HEADER_SIZE + buffer.readUInt32BE(at + 4);
    if (end > buffer.length) return;
    yield { tag, data: buffer.subarray(at + CHUNK_HEADER_SIZE, end) };
    at = end;
  }
}

export function decodeText(data: Buffer): string {
  // Node has no UTF-16BE decoder, so swap to little-endian on a copy.
  const evenLength = data.length - (data.length % 2);
  return Buffer.from(data.subarray(0, evenLength)).swap16().toString('utf16le');
}

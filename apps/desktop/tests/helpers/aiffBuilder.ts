/** Builds AIFF and AIFF-C files for tests. */

function chunk(id: string, data: Buffer): Buffer {
  const header = Buffer.alloc(8);
  header.write(id, 0, 'latin1');
  header.writeUInt32BE(data.length, 4);
  // Chunks are padded to an even length; the pad byte is not part of the size.
  return Buffer.concat([header, data, Buffer.alloc(data.length % 2)]);
}

/** Encodes a positive integer as an 80-bit extended float. */
function extendedFloat(value: number): Buffer {
  const exponent = Math.floor(Math.log2(value));
  const buffer = Buffer.alloc(10);
  buffer.writeUInt16BE(16383 + exponent, 0);
  buffer.writeBigUInt64BE(BigInt(value) << BigInt(63 - exponent), 2);
  return buffer;
}

export interface AiffSpec {
  channels: number;
  bitsPerSample: number;
  sampleRate: number;
  samples: Buffer;
  /** Set for AIFF-C files, which name their sample encoding. */
  compression?: string;
  /** Puts the sound data before the format description. */
  soundFirst?: boolean;
}

export function buildAiff(spec: AiffSpec): Buffer {
  const frameSize = spec.channels * Math.ceil(spec.bitsPerSample / 8);
  const comm = Buffer.alloc(8);
  comm.writeInt16BE(spec.channels, 0);
  comm.writeUInt32BE(spec.samples.length / frameSize, 2);
  comm.writeInt16BE(spec.bitsPerSample, 6);
  const commChunk = chunk(
    'COMM',
    Buffer.concat([
      comm,
      extendedFloat(spec.sampleRate),
      // AIFF-C adds the compression ID and its name as a Pascal string.
      ...(spec.compression ? [Buffer.from(spec.compression, 'latin1'), Buffer.from([0, 0])] : []),
    ]),
  );
  const soundChunk = chunk('SSND', Buffer.concat([Buffer.alloc(8), spec.samples]));
  // An odd-sized chunk in front checks that padding is skipped.
  const nameChunk = chunk('NAME', Buffer.from('abc', 'latin1'));
  const chunks = spec.soundFirst ? [nameChunk, soundChunk, commChunk] : [nameChunk, commChunk, soundChunk];

  const body = Buffer.concat([Buffer.from(spec.compression ? 'AIFC' : 'AIFF', 'latin1'), ...chunks]);
  const form = Buffer.alloc(8);
  form.write('FORM', 0, 'latin1');
  form.writeUInt32BE(body.length, 4);
  return Buffer.concat([form, body]);
}

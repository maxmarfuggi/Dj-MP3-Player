/**
 * Chromium cannot play AIFF, which is common on DJ USBs. Uncompressed AIFF is
 * the same PCM audio as WAV with a different header and byte order, so it is
 * presented to the player as a WAV file, converted on the fly.
 */
import { createReadStream } from 'node:fs';
import { open } from 'node:fs/promises';
import { Readable } from 'node:stream';

export interface AiffPcm {
  channels: number;
  sampleRate: number;
  bytesPerSample: number;
  /** File offset of the first sample. */
  dataOffset: number;
  dataLength: number;
  /** Plain AIFF is big-endian; AIFF-C `sowt` is already little-endian like WAV. */
  bigEndian: boolean;
}

const WAV_HEADER_SIZE = 44;

const CHUNK_HEADER_SIZE = 8;
const COMM_SIZE = 18;
const COMM_SIZE_WITH_COMPRESSION = 22;
const SSND_HEADER_SIZE = 8;

/** Describes the audio in an AIFF file, or returns null if it is not uncompressed PCM. */
export async function readAiffPcm(filePath: string): Promise<AiffPcm | null> {
  const file = await open(filePath, 'r');
  try {
    const { size: fileSize } = await file.stat();
    const read = async (position: number, length: number): Promise<Buffer> => {
      const buffer = Buffer.alloc(length);
      const { bytesRead } = await file.read(buffer, 0, length, position);
      return buffer.subarray(0, bytesRead);
    };

    const form = await read(0, 12);
    const formType = form.toString('latin1', 8, 12);
    if (form.length < 12 || form.toString('latin1', 0, 4) !== 'FORM') return null;
    if (formType !== 'AIFF' && formType !== 'AIFC') return null;

    let format: Pick<AiffPcm, 'channels' | 'sampleRate' | 'bytesPerSample' | 'bigEndian'> | undefined;
    let frameCount = 0;
    let sound: { offset: number; length: number } | undefined;

    for (let at = 12; at + CHUNK_HEADER_SIZE <= fileSize && !(format && sound); ) {
      const header = await read(at, CHUNK_HEADER_SIZE);
      if (header.length < CHUNK_HEADER_SIZE) break;
      const id = header.toString('latin1', 0, 4);
      const size = header.readUInt32BE(4);
      const dataAt = at + CHUNK_HEADER_SIZE;

      if (id === 'COMM') {
        const comm = await read(dataAt, COMM_SIZE_WITH_COMPRESSION);
        if (comm.length < COMM_SIZE) return null;
        const compression =
          comm.length >= COMM_SIZE_WITH_COMPRESSION && size >= COMM_SIZE_WITH_COMPRESSION
            ? comm.toString('latin1', 18, 22)
            : 'NONE';
        if (compression !== 'NONE' && compression !== 'sowt') return null;
        frameCount = comm.readUInt32BE(2);
        format = {
          channels: comm.readInt16BE(0),
          bytesPerSample: Math.ceil(comm.readInt16BE(6) / 8),
          sampleRate: Math.round(readExtendedFloat(comm, 8)),
          bigEndian: compression === 'NONE',
        };
      } else if (id === 'SSND') {
        const ssnd = await read(dataAt, SSND_HEADER_SIZE);
        if (ssnd.length < SSND_HEADER_SIZE) return null;
        const offset = dataAt + SSND_HEADER_SIZE + ssnd.readUInt32BE(0);
        sound = { offset, length: Math.min(dataAt + size, fileSize) - offset };
      }
      // Chunks are padded to an even length.
      at = dataAt + size + (size % 2);
    }

    if (!format || !sound) return null;
    const { channels, bytesPerSample, sampleRate } = format;
    if (channels < 1 || bytesPerSample < 1 || bytesPerSample > 4 || sampleRate < 1) return null;

    const frameSize = channels * bytesPerSample;
    const availableFrames = Math.floor(Math.max(sound.length, 0) / frameSize);
    const dataLength = Math.min(frameCount, availableFrames) * frameSize;
    return dataLength > 0 ? { ...format, dataOffset: sound.offset, dataLength } : null;
  } finally {
    await file.close();
  }
}

/** AIFF stores the sample rate as an 80-bit IEEE 754 extended float. */
function readExtendedFloat(buffer: Buffer, at: number): number {
  const exponent = buffer.readUInt16BE(at) & 0x7fff;
  const mantissa = buffer.readUInt32BE(at + 2) * 2 ** 32 + buffer.readUInt32BE(at + 6);
  return mantissa * 2 ** (exponent - 16383 - 63);
}

export function wavSize(pcm: AiffPcm): number {
  return WAV_HEADER_SIZE + pcm.dataLength;
}

function wavHeader(pcm: AiffPcm): Buffer {
  const frameSize = pcm.channels * pcm.bytesPerSample;
  const header = Buffer.alloc(WAV_HEADER_SIZE);
  header.write('RIFF', 0, 'latin1');
  header.writeUInt32LE(WAV_HEADER_SIZE - 8 + pcm.dataLength, 4);
  header.write('WAVEfmt ', 8, 'latin1');
  header.writeUInt32LE(16, 16);
  header.writeUInt16LE(1, 20); // PCM
  header.writeUInt16LE(pcm.channels, 22);
  header.writeUInt32LE(pcm.sampleRate, 24);
  header.writeUInt32LE(pcm.sampleRate * frameSize, 28);
  header.writeUInt16LE(frameSize, 32);
  header.writeUInt16LE(pcm.bytesPerSample * 8, 34);
  header.write('data', 36, 'latin1');
  header.writeUInt32LE(pcm.dataLength, 40);
  return header;
}

/** Converts whole samples in place from AIFF's to WAV's representation. */
function toWavSamples(samples: Buffer, pcm: AiffPcm): void {
  if (pcm.bytesPerSample === 1) {
    // 8-bit audio is signed in AIFF but unsigned in WAV.
    for (let i = 0; i < samples.length; i++) samples[i]! ^= 0x80;
  } else if (!pcm.bigEndian) {
    return;
  } else if (pcm.bytesPerSample === 2) {
    samples.swap16();
  } else if (pcm.bytesPerSample === 4) {
    samples.swap32();
  } else {
    for (let i = 0; i + 2 < samples.length; i += 3) {
      const first = samples[i]!;
      samples[i] = samples[i + 2]!;
      samples[i + 2] = first;
    }
  }
}

/**
 * Streams bytes `start` to `end` (inclusive) of the WAV file equivalent to the
 * AIFF file at `filePath`.
 */
export function readAiffAsWav(filePath: string, pcm: AiffPcm, start: number, end: number): Readable {
  return Readable.from(wavBytes(filePath, pcm, start, end));
}

async function* wavBytes(
  filePath: string,
  pcm: AiffPcm,
  start: number,
  end: number,
): AsyncGenerator<Buffer> {
  if (start < WAV_HEADER_SIZE) {
    yield wavHeader(pcm).subarray(start, Math.min(end + 1, WAV_HEADER_SIZE));
  }
  const dataStart = Math.max(start, WAV_HEADER_SIZE) - WAV_HEADER_SIZE;
  const dataEnd = Math.min(end - WAV_HEADER_SIZE, pcm.dataLength - 1);
  if (dataEnd < dataStart) return;

  // Samples can only be converted whole, so read from the sample boundaries
  // around the requested range and trim the excess afterwards.
  const size = pcm.bytesPerSample;
  const alignedStart = dataStart - (dataStart % size);
  const alignedEnd = dataEnd - (dataEnd % size) + size - 1;
  let leadingExcess = dataStart - alignedStart;
  let remaining = dataEnd - dataStart + 1;
  let carry: Buffer = Buffer.alloc(0);

  const source = createReadStream(filePath, {
    start: pcm.dataOffset + alignedStart,
    end: pcm.dataOffset + alignedEnd,
  });
  for await (const chunk of source as AsyncIterable<Buffer>) {
    const joined = carry.length > 0 ? Buffer.concat([carry, chunk]) : chunk;
    const wholeLength = joined.length - (joined.length % size);
    carry = joined.subarray(wholeLength);
    if (wholeLength === 0) continue;
    const samples = joined.subarray(0, wholeLength);
    toWavSamples(samples, pcm);

    const wanted = samples.subarray(leadingExcess, leadingExcess + remaining);
    leadingExcess = 0;
    remaining -= wanted.length;
    yield wanted;
  }
}

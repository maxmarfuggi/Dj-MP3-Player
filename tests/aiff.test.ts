import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { type AiffPcm, readAiffAsWav, readAiffPcm, wavSize } from '../src/main/aiff';

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

interface AiffSpec {
  channels: number;
  bitsPerSample: number;
  sampleRate: number;
  samples: Buffer;
  /** Set for AIFF-C files, which name their sample encoding. */
  compression?: string;
  /** Puts the sound data before the format description. */
  soundFirst?: boolean;
}

function buildAiff(spec: AiffSpec): Buffer {
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

describe('AIFF to WAV conversion', () => {
  let directory: string;
  let fileCount = 0;

  beforeAll(async () => {
    directory = await mkdtemp(path.join(os.tmpdir(), 'aiff-'));
  });

  afterAll(async () => {
    await rm(directory, { recursive: true, force: true });
  });

  async function writeAiff(spec: AiffSpec): Promise<{ filePath: string; pcm: AiffPcm }> {
    const filePath = path.join(directory, `${fileCount++}.aiff`);
    await writeFile(filePath, buildAiff(spec));
    const pcm = await readAiffPcm(filePath);
    if (!pcm) throw new Error('expected a readable AIFF file');
    return { filePath, pcm };
  }

  async function readWav(filePath: string, pcm: AiffPcm, start = 0, end = wavSize(pcm) - 1) {
    return Buffer.concat(await readAiffAsWav(filePath, pcm, start, end).toArray());
  }

  it('produces a WAV file with little-endian 16-bit samples', async () => {
    const { filePath, pcm } = await writeAiff({
      channels: 2,
      bitsPerSample: 16,
      sampleRate: 44100,
      samples: Buffer.from([0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]),
    });

    const wav = await readWav(filePath, pcm);

    expect(wav.length).toBe(wavSize(pcm));
    expect(wav.toString('latin1', 0, 4)).toBe('RIFF');
    expect(wav.readUInt32LE(4)).toBe(wav.length - 8);
    expect(wav.toString('latin1', 8, 16)).toBe('WAVEfmt ');
    expect(wav.readUInt16LE(20)).toBe(1);
    expect(wav.readUInt16LE(22)).toBe(2);
    expect(wav.readUInt32LE(24)).toBe(44100);
    expect(wav.readUInt32LE(28)).toBe(44100 * 4);
    expect(wav.readUInt16LE(32)).toBe(4);
    expect(wav.readUInt16LE(34)).toBe(16);
    expect(wav.readUInt32LE(40)).toBe(8);
    expect([...wav.subarray(44)]).toEqual([0x02, 0x01, 0x04, 0x03, 0x06, 0x05, 0x08, 0x07]);
  });

  it('reverses the bytes of 24-bit samples', async () => {
    const { filePath, pcm } = await writeAiff({
      channels: 1,
      bitsPerSample: 24,
      sampleRate: 48000,
      samples: Buffer.from([0x01, 0x02, 0x03, 0x04, 0x05, 0x06]),
    });

    const wav = await readWav(filePath, pcm);

    expect(wav.readUInt32LE(24)).toBe(48000);
    expect(wav.readUInt16LE(34)).toBe(24);
    expect([...wav.subarray(44)]).toEqual([0x03, 0x02, 0x01, 0x06, 0x05, 0x04]);
  });

  it('keeps AIFF-C `sowt` samples, which are already little-endian', async () => {
    const samples = Buffer.from([0x01, 0x02, 0x03, 0x04]);
    const { filePath, pcm } = await writeAiff({
      channels: 1,
      bitsPerSample: 16,
      sampleRate: 44100,
      samples,
      compression: 'sowt',
    });

    expect((await readWav(filePath, pcm)).subarray(44)).toEqual(samples);
  });

  it('makes signed 8-bit samples unsigned', async () => {
    const { filePath, pcm } = await writeAiff({
      channels: 1,
      bitsPerSample: 8,
      sampleRate: 22050,
      samples: Buffer.from([0x00, 0x7f, 0x80, 0xff]),
    });

    expect([...(await readWav(filePath, pcm)).subarray(44)]).toEqual([0x80, 0xff, 0x00, 0x7f]);
  });

  it('finds the audio when the sound chunk comes before the format chunk', async () => {
    const { filePath, pcm } = await writeAiff({
      channels: 1,
      bitsPerSample: 16,
      sampleRate: 44100,
      samples: Buffer.from([0x01, 0x02]),
      soundFirst: true,
    });

    expect([...(await readWav(filePath, pcm)).subarray(44)]).toEqual([0x02, 0x01]);
  });

  it('serves any byte range of the WAV file, including ones that split samples', async () => {
    // Larger than one 64 KiB read, so samples also straddle read boundaries.
    const samples = Buffer.from(Array.from({ length: 3 * 2 * 30_000 }, (_, i) => i % 251));
    const { filePath, pcm } = await writeAiff({
      channels: 2,
      bitsPerSample: 24,
      sampleRate: 44100,
      samples,
    });
    const whole = await readWav(filePath, pcm);
    const last = whole.length - 1;
    expect([...whole.subarray(44, 50)]).toEqual([2, 1, 0, 5, 4, 3]);
    expect([...whole.subarray(last - 2)]).toEqual([...samples.subarray(-3)].reverse());

    const ranges = [
      [0, 0],
      [0, 43],
      [10, 50],
      [43, 44],
      [44, last],
      [45, 46],
      [46, 100],
      [101, 101],
      [44 + 65_530, 44 + 65_545],
      [100, 150_000],
      [last - 1, last],
      [last, last],
    ] as const;
    for (const [start, end] of ranges) {
      expect(await readWav(filePath, pcm, start, end), `bytes ${start}-${end}`).toEqual(
        whole.subarray(start, end + 1),
      );
    }
  });

  it('declines compressed AIFF-C and files that are not AIFF', async () => {
    const compressed = path.join(directory, 'compressed.aiff');
    await writeFile(
      compressed,
      buildAiff({
        channels: 1,
        bitsPerSample: 16,
        sampleRate: 44100,
        samples: Buffer.alloc(4),
        compression: 'ima4',
      }),
    );
    const notAiff = path.join(directory, 'not.aiff');
    await writeFile(notAiff, 'ID3 this is really an mp3');

    expect(await readAiffPcm(compressed)).toBeNull();
    expect(await readAiffPcm(notAiff)).toBeNull();
  });
});

import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { type AiffPcm, readAiffAsWav, readAiffPcm, wavSize } from '../src/main/aiff';
import { type AiffSpec, buildAiff } from './helpers/aiffBuilder';

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

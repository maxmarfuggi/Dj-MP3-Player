import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { type MediaLibrary, serveMedia } from '../src/main/mediaService';

function chunk(id: string, data: Buffer): Buffer {
  const header = Buffer.alloc(8);
  header.write(id, 0, 'latin1');
  header.writeUInt32BE(data.length, 4);
  return Buffer.concat([header, data, Buffer.alloc(data.length % 2)]);
}

/** A tiny uncompressed AIFF: two big-endian 16-bit samples, 0x0102 then 0x0304. */
function stereoFrame(): Buffer {
  const samples = Buffer.from([0x01, 0x02, 0x03, 0x04]);
  const comm = Buffer.alloc(18);
  comm.writeInt16BE(1, 0);
  comm.writeUInt32BE(2, 2);
  comm.writeInt16BE(16, 6);
  comm.writeUInt16BE(0x400e, 8);
  comm.writeBigUInt64BE(1n << 63n, 10);
  const body = Buffer.concat([
    Buffer.from('AIFF', 'latin1'),
    chunk('COMM', comm),
    chunk('SSND', Buffer.concat([Buffer.alloc(8), samples])),
  ]);
  const form = Buffer.alloc(8);
  form.write('FORM', 0, 'latin1');
  form.writeUInt32BE(body.length, 4);
  return Buffer.concat([form, body]);
}

describe('serveMedia', () => {
  let directory: string;
  let libraries: MediaLibrary;

  beforeEach(async () => {
    directory = await mkdtemp(path.join(os.tmpdir(), 'media-service-'));
    libraries = {
      resolveTrackPath: (_libraryId, trackId) => path.join(directory, trackId),
      resolveArtworkPath: () => null,
    };
  });

  afterEach(async () => {
    await rm(directory, { recursive: true, force: true });
  });

  async function bodyOf(url: string, range: string | null = null): Promise<Buffer> {
    const result = await serveMedia(url, range, libraries);
    expect(result.body).not.toBeNull();
    return Buffer.concat(await result.body!.toArray());
  }

  it('rejects a request that does not name a listed file', async () => {
    const missing = await serveMedia('media://track/lib/nope.mp3', null, libraries);
    const artwork = await serveMedia('media://artwork/lib/nope.mp3', null, libraries);
    const other = await serveMedia('media://other/lib/nope.mp3', null, libraries);

    expect(missing).toEqual({ status: 404, headers: {}, body: null });
    expect(artwork).toEqual({ status: 404, headers: {}, body: null });
    expect(other).toEqual({ status: 404, headers: {}, body: null });
  });

  it('streams a byte range of the file', async () => {
    await writeFile(path.join(directory, 'song.mp3'), Buffer.from('abcdefghij'));

    const result = await serveMedia('media://track/lib/song.mp3', 'bytes=2-5', libraries);

    expect(result.status).toBe(206);
    expect(result.headers['Content-Type']).toBe('audio/mpeg');
    expect(result.headers['Content-Range']).toBe('bytes 2-5/10');
    expect(result.headers['Content-Length']).toBe('4');
    expect(result.headers['Access-Control-Allow-Origin']).toBe('*');
    expect(Buffer.concat(await result.body!.toArray())).toEqual(Buffer.from('cdef'));
  });

  it('reports a range outside the file', async () => {
    await writeFile(path.join(directory, 'song.mp3'), Buffer.from('abcdefghij'));

    const result = await serveMedia('media://track/lib/song.mp3', 'bytes=20-30', libraries);

    expect(result.status).toBe(416);
    expect(result.headers).toEqual({ 'Content-Range': 'bytes */10' });
    expect(result.body).toBeNull();
  });

  it('sends no body for an empty file', async () => {
    await writeFile(path.join(directory, 'song.mp3'), Buffer.alloc(0));

    const result = await serveMedia('media://track/lib/song.mp3', null, libraries);

    expect(result.status).toBe(200);
    expect(result.headers['Content-Length']).toBeUndefined();
    expect(result.body).toBeNull();
  });

  it('serves uncompressed AIFF as WAV', async () => {
    await writeFile(path.join(directory, 'song.aiff'), stereoFrame());

    const result = await serveMedia('media://track/lib/song.aiff', null, libraries);
    const wav = await bodyOf('media://track/lib/song.aiff');

    expect(result.status).toBe(200);
    expect(result.headers['Content-Type']).toBe('audio/wav');
    expect(wav.subarray(0, 4).toString('latin1')).toBe('RIFF');
    expect([...wav.subarray(44)]).toEqual([0x02, 0x01, 0x04, 0x03]);
  });

  it('finds a file stored in the other Unicode normalization', async () => {
    const composed = 'café.mp3'.normalize('NFC');
    const decomposed = composed.normalize('NFD');
    await writeFile(path.join(directory, decomposed), Buffer.from('accented'));
    libraries = {
      resolveTrackPath: () => path.join(directory, composed),
      resolveArtworkPath: () => null,
    };

    const wav = await bodyOf(`media://track/lib/${encodeURIComponent(composed)}`);

    expect(wav.toString('latin1')).toBe('accented');
  });
});

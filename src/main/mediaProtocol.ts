import { createReadStream } from 'node:fs';
import { stat } from 'node:fs/promises';
import path from 'node:path';
import { Readable } from 'node:stream';
import { protocol } from 'electron';
import { MEDIA_SCHEME } from '../shared/types';
import { readAiffAsWav, readAiffPcm, wavSize } from './aiff';
import type { LibraryManager } from './libraryManager';
import { parseRangeHeader } from './range';

const MIME_TYPES: Record<string, string> = {
  '.mp3': 'audio/mpeg',
  '.m4a': 'audio/mp4',
  '.mp4': 'audio/mp4',
  '.aac': 'audio/aac',
  '.wav': 'audio/wav',
  '.flac': 'audio/flac',
  '.ogg': 'audio/ogg',
  '.opus': 'audio/ogg',
  '.aif': 'audio/aiff',
  '.aiff': 'audio/aiff',
};

const AIFF_EXTENSIONS = new Set(['.aif', '.aiff']);

/** Must run before the app is ready. `stream` is what lets `<audio>` seek. */
export function registerMediaScheme(): void {
  protocol.registerSchemesAsPrivileged([
    { scheme: MEDIA_SCHEME, privileges: { standard: true, secure: true, stream: true } },
  ]);
}

/**
 * Serves `media://track/<libraryId>/<trackId>`. The renderer never names a
 * file path, so it can only reach files that a loaded library lists.
 */
export function handleMediaProtocol(libraries: LibraryManager): void {
  protocol.handle(MEDIA_SCHEME, async (request) => {
    const url = new URL(request.url);
    const [libraryId, trackId] = url.pathname.split('/').filter(Boolean).map(decodeURIComponent);
    if (url.hostname !== 'track' || libraryId === undefined || trackId === undefined) {
      return new Response(null, { status: 404 });
    }
    const resolved = libraries.resolveTrackPath(libraryId, trackId);
    const file = resolved === null ? null : await findFile(resolved);
    if (file === null) return new Response(null, { status: 404 });
    const source = await openSource(file);

    const range = parseRangeHeader(request.headers.get('range'), source.size);
    if (range === 'unsatisfiable') {
      return new Response(null, {
        status: 416,
        headers: { 'Content-Range': `bytes */${source.size}` },
      });
    }

    const headers = new Headers({ 'Content-Type': source.contentType, 'Accept-Ranges': 'bytes' });
    if (source.size === 0) return new Response(null, { status: 200, headers });

    const start = range?.start ?? 0;
    const end = range?.end ?? source.size - 1;
    headers.set('Content-Length', String(end - start + 1));
    if (range) headers.set('Content-Range', `bytes ${start}-${end}/${source.size}`);

    const body = Readable.toWeb(source.read(start, end)) as ReadableStream<Uint8Array>;
    return new Response(body, { status: range ? 206 : 200, headers });
  });
}

interface FoundFile {
  path: string;
  size: number;
}

/** What the player receives for a track; not always the file's bytes as stored. */
interface MediaSource {
  size: number;
  contentType: string;
  /** Streams bytes `start` to `end`, both inclusive. */
  read(start: number, end: number): Readable;
}

async function openSource(file: FoundFile): Promise<MediaSource> {
  const extension = path.extname(file.path).toLowerCase();
  if (AIFF_EXTENSIONS.has(extension)) {
    const pcm = await readAiffPcm(file.path);
    if (pcm) {
      return {
        size: wavSize(pcm),
        contentType: 'audio/wav',
        read: (start, end) => readAiffAsWav(file.path, pcm, start, end),
      };
    }
  }
  return {
    size: file.size,
    contentType: MIME_TYPES[extension] ?? 'application/octet-stream',
    read: (start, end) => createReadStream(file.path, { start, end }),
  };
}

/**
 * Libraries and file systems do not always agree on how accented characters
 * are composed, so a path that fails as written is retried in both Unicode
 * normalization forms.
 */
async function findFile(filePath: string): Promise<FoundFile | null> {
  const candidates = new Set([filePath, filePath.normalize('NFC'), filePath.normalize('NFD')]);
  for (const candidate of candidates) {
    try {
      const stats = await stat(candidate);
      if (stats.isFile()) return { path: candidate, size: stats.size };
    } catch {
      // Try the next spelling.
    }
  }
  return null;
}

import { createReadStream } from 'node:fs';
import { stat } from 'node:fs/promises';
import path from 'node:path';
import type { Readable } from 'node:stream';
import { readAiffAsWav, readAiffPcm, wavSize } from './aiff';
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
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.png': 'image/png',
};

const AIFF_EXTENSIONS = new Set(['.aif', '.aiff']);

/** Looks up a file the loaded libraries list. The renderer never names a path. */
export interface MediaLibrary {
  resolveTrackPath(libraryId: string, trackId: string): string | null;
  resolveArtworkPath(libraryId: string, trackId: string): string | null;
}

export interface MediaResponse {
  status: number;
  headers: Record<string, string>;
  /** Bytes to send, or null when the response has no body. */
  body: Readable | null;
}

/**
 * Serves one `media://track/<libraryId>/<trackId>` or
 * `media://artwork/<libraryId>/<trackId>` request. File types, ranges, the
 * AIFF to WAV conversion, and the NFC/NFD retry all live here, with no
 * dependency on Electron.
 */
export async function serveMedia(
  url: string,
  rangeHeader: string | null,
  libraries: MediaLibrary,
): Promise<MediaResponse> {
  const parsed = new URL(url);
  const [libraryId, trackId] = parsed.pathname.split('/').filter(Boolean).map(decodeURIComponent);
  if (libraryId === undefined || trackId === undefined) return { status: 404, headers: {}, body: null };
  const resolved =
    parsed.hostname === 'track'
      ? libraries.resolveTrackPath(libraryId, trackId)
      : parsed.hostname === 'artwork'
        ? libraries.resolveArtworkPath(libraryId, trackId)
        : null;
  const file = resolved === null ? null : await findFile(resolved);
  if (file === null) return { status: 404, headers: {}, body: null };
  const source = await openSource(file);

  const range = parseRangeHeader(rangeHeader, source.size);
  if (range === 'unsatisfiable') {
    return { status: 416, headers: { 'Content-Range': `bytes */${source.size}` }, body: null };
  }

  const headers: Record<string, string> = {
    'Content-Type': source.contentType,
    'Accept-Ranges': 'bytes',
    'Access-Control-Allow-Origin': '*',
  };
  if (source.size === 0) return { status: 200, headers, body: null };

  const start = range?.start ?? 0;
  const end = range?.end ?? source.size - 1;
  headers['Content-Length'] = String(end - start + 1);
  if (range) headers['Content-Range'] = `bytes ${start}-${end}/${source.size}`;
  return { status: range ? 206 : 200, headers, body: source.read(start, end) };
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

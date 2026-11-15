import { Readable } from 'node:stream';
import { protocol } from 'electron';
import { MEDIA_SCHEME } from '../shared/types';
import type { LibraryManager } from './libraryManager';
import { serveMedia } from './mediaService';

/**
 * Must run before the app is ready. `stream` is what lets `<audio>` seek.
 * `corsEnabled` plus the CORS header on responses let the visualizers read the
 * audio; without them Web Audio treats it as cross-origin and only sees silence.
 */
export function registerMediaScheme(): void {
  protocol.registerSchemesAsPrivileged([
    {
      scheme: MEDIA_SCHEME,
      privileges: { standard: true, secure: true, stream: true, corsEnabled: true },
    },
  ]);
}

/** Electron protocol handler. Resolving and streaming live in `serveMedia`. */
export function handleMediaProtocol(libraries: LibraryManager): void {
  protocol.handle(MEDIA_SCHEME, async (request) => {
    const result = await serveMedia(request.url, request.headers.get('range'), libraries);
    const body = result.body === null ? null : (Readable.toWeb(result.body) as ReadableStream<Uint8Array>);
    return new Response(body, { status: result.status, headers: result.headers });
  });
}

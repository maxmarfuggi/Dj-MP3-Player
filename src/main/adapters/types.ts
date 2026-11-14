import { createHash } from 'node:crypto';
import type { Library, LibrarySource } from '../../shared/types';

export interface Volume {
  /** Absolute path of the volume root (or of a folder the user picked). */
  path: string;
  name: string;
}

export interface LibraryAdapter {
  source: LibrarySource;
  /** Whether the volume holds a library in this adapter's format. */
  detect(volumePath: string): Promise<boolean>;
  /** Reads the library. Never writes to the volume. */
  load(volume: Volume): Promise<Library>;
}

export function libraryId(source: LibrarySource, volumePath: string): string {
  const hash = createHash('sha1').update(volumePath).digest('hex').slice(0, 12);
  return `${source}-${hash}`;
}

import { access } from 'node:fs/promises';
import path from 'node:path';

export async function exists(filePath: string): Promise<boolean> {
  try {
    await access(filePath);
    return true;
  } catch {
    return false;
  }
}

/** Library paths are volume-relative; rekordbox adds a leading `/`, Serato does not. */
export function toRelativePath(libraryPath: string): string {
  return libraryPath.replace(/^\/+/, '');
}

/**
 * Resolves a volume-relative path to an absolute one, or returns null if it
 * would land outside the volume (a library is untrusted input).
 */
export function resolveInsideVolume(volumePath: string, relativePath: string): string | null {
  const root = path.resolve(volumePath);
  const resolved = path.resolve(root, ...relativePath.split('/'));
  const rootPrefix = root.endsWith(path.sep) ? root : root + path.sep;
  return resolved.startsWith(rootPrefix) ? resolved : null;
}

export function fileNameWithoutExtension(relativePath: string): string {
  return path.posix.parse(relativePath).name;
}

import { readdir, realpath } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { exists } from './adapters/paths';
import type { Volume } from './adapters/types';

/** Lists mounted volumes that could be a DJ USB, excluding the system volume. */
export async function listVolumes(): Promise<Volume[]> {
  switch (process.platform) {
    case 'darwin':
      return listMacVolumes();
    case 'win32':
      return listWindowsDrives();
    default:
      return listLinuxMounts();
  }
}

async function listMacVolumes(): Promise<Volume[]> {
  const volumes: Volume[] = [];
  for (const name of await listDirectory('/Volumes')) {
    const volumePath = path.join('/Volumes', name);
    try {
      // The boot volume shows up in /Volumes as a symlink to `/`.
      if ((await realpath(volumePath)) === '/') continue;
    } catch {
      // Unmounted between listing and inspecting it.
      continue;
    }
    volumes.push({ path: volumePath, name });
  }
  return volumes;
}

async function listWindowsDrives(): Promise<Volume[]> {
  const systemDrive = (process.env.SystemDrive ?? 'C:').toUpperCase();
  const volumes: Volume[] = [];
  for (let code = 'A'.charCodeAt(0); code <= 'Z'.charCodeAt(0); code++) {
    const drive = `${String.fromCharCode(code)}:`;
    if (drive === systemDrive) continue;
    if (await exists(`${drive}\\`)) volumes.push({ path: `${drive}\\`, name: drive });
  }
  return volumes;
}

async function listLinuxMounts(): Promise<Volume[]> {
  const user = os.userInfo().username;
  const volumes: Volume[] = [];
  for (const parent of [`/media/${user}`, `/run/media/${user}`]) {
    for (const name of await listDirectory(parent)) {
      volumes.push({ path: path.join(parent, name), name });
    }
  }
  return volumes;
}

async function listDirectory(directory: string): Promise<string[]> {
  try {
    return await readdir(directory);
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') return [];
    throw error;
  }
}

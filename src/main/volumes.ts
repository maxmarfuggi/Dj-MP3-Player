import { execFile } from 'node:child_process';
import { readdir, realpath } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { promisify } from 'node:util';
import { exists } from './adapters/paths';
import type { Volume } from './adapters/types';

const execFileAsync = promisify(execFile);

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

/** Volume labels by drive, kept while the drive stays mounted. Blank if it has none. */
const driveLabels = new Map<string, string>();

async function listWindowsDrives(): Promise<Volume[]> {
  const systemDrive = (process.env.SystemDrive ?? 'C:').toUpperCase();
  const drives: string[] = [];
  for (let code = 'A'.charCodeAt(0); code <= 'Z'.charCodeAt(0); code++) {
    const drive = `${String.fromCharCode(code)}:`;
    if (drive === systemDrive) continue;
    if (await exists(`${drive}\\`)) drives.push(drive);
  }

  for (const drive of driveLabels.keys()) {
    if (!drives.includes(drive)) driveLabels.delete(drive);
  }
  const unlabelled = drives.filter((drive) => !driveLabels.has(drive));
  if (unlabelled.length > 0) {
    const labels = await readDriveLabels(unlabelled);
    unlabelled.forEach((drive, index) => driveLabels.set(drive, labels[index] ?? ''));
  }

  return drives.map((drive) => ({ path: `${drive}\\`, name: driveLabels.get(drive) || drive }));
}

/**
 * Node has no API for volume labels, so this asks PowerShell. It takes a few
 * hundred milliseconds, which is why labels are cached. Returns blanks if
 * PowerShell is unavailable.
 */
async function readDriveLabels(drives: string[]): Promise<string[]> {
  const script = [
    '[Console]::OutputEncoding = [Text.Encoding]::UTF8',
    `foreach ($drive in @(${drives.map((drive) => `'${drive}\\'`).join(',')})) {`,
    '  try { (New-Object System.IO.DriveInfo $drive).VolumeLabel } catch { "" }',
    '}',
  ].join('\n');
  try {
    const { stdout } = await execFileAsync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script], {
      timeout: 10_000,
      windowsHide: true,
    });
    return stdout.split(/\r?\n/).map((label) => label.trim());
  } catch (error) {
    console.warn('Reading drive labels failed:', error);
    return [];
  }
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

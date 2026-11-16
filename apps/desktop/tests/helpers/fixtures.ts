import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import type { Library, LibrarySource, LibraryState } from '../../src/shared/types';

/** Stands in for the absolute path of a fixture's `volume` folder in `expected.json`. */
export const VOLUME_PATH_PLACEHOLDER = '{volume}';
/** Stands in for the volume-path hash in library ids, e.g. `rekordbox-{volumeHash}`. */
export const VOLUME_HASH_PLACEHOLDER = '{volumeHash}';

export const LIBRARY_FORMATS = ['rekordbox', 'deviceLibraryPlus', 'serato'] as const;

/** What `expected.json` holds: every library the app finds on the volume. */
export interface ExpectedVolume {
  volumeName: string;
  /** In the order the core reads them: rekordbox, Device Library Plus, Serato. */
  libraries: Library[];
  /** Sources whose export is present but could not be read. */
  errors: LibrarySource[];
}

export interface FixtureCase {
  /** `<format>/<case>`, e.g. `rekordbox/basic`. */
  name: string;
  directory: string;
  volumePath: string;
}

export function repoRoot(): string {
  for (let directory = process.cwd(); ; directory = path.dirname(directory)) {
    const manifest = path.join(directory, 'package.json');
    if (existsSync(manifest) && 'workspaces' in JSON.parse(readFileSync(manifest, 'utf8'))) return directory;
    if (path.dirname(directory) === directory) throw new Error('Could not find the repository root');
  }
}

export function fixturesDirectory(): string {
  return path.join(repoRoot(), 'fixtures');
}

/** Fixture cases under `fixtures/<group>`, sorted by name. Missing groups have none. */
export function listCases(group: string): FixtureCase[] {
  const groupDirectory = path.join(fixturesDirectory(), group);
  if (!existsSync(groupDirectory)) return [];
  return readdirSync(groupDirectory, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort()
    .map((name) => {
      const directory = path.join(groupDirectory, name);
      return { name: `${group}/${name}`, directory, volumePath: path.join(directory, 'volume') };
    });
}

/**
 * Reads a volume with the Rust core, the way the app does, and replaces the
 * machine-specific path and id so the result can be written to `expected.json`.
 */
export async function readVolume(volumePath: string, volumeName: string): Promise<ExpectedVolume> {
  const state = readWithCore(volumePath, volumeName);
  return {
    volumeName,
    libraries: state.libraries.map((library) => withPlaceholders(library, volumePath)),
    errors: state.errors.map((error) => error.source),
  };
}

function readWithCore(volumePath: string, volumeName: string): LibraryState {
  const require = createRequire(import.meta.url);
  const binding = require('djusb-node') as { readVolume?: (path: string, name: string) => LibraryState };
  if (typeof binding.readVolume !== 'function') {
    throw new Error('The library core is not built. Run npm run build:node from the repository root.');
  }
  return binding.readVolume(volumePath, volumeName);
}

function libraryId(source: LibrarySource, volumePath: string): string {
  const hash = createHash('sha1').update(volumePath).digest('hex').slice(0, 12);
  return `${source}-${hash}`;
}

function withPlaceholders(library: Library, volumePath: string): Library {
  if (library.id !== libraryId(library.source, volumePath)) {
    throw new Error(`Unexpected library id ${library.id} for ${volumePath}`);
  }
  return {
    ...library,
    id: `${library.source}-${VOLUME_HASH_PLACEHOLDER}`,
    volumePath: VOLUME_PATH_PLACEHOLDER,
  };
}

export function readExpected(fixture: FixtureCase): ExpectedVolume {
  return JSON.parse(readFileSync(path.join(fixture.directory, 'expected.json'), 'utf8')) as ExpectedVolume;
}

/** The volume name a fixture is read with: its case folder's name. */
export function volumeNameOf(fixture: FixtureCase): string {
  return path.basename(fixture.directory);
}

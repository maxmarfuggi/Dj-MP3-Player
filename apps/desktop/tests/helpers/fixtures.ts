import { existsSync, readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { adapters } from '../../src/main/adapters';
import { libraryId } from '../../src/main/adapters/types';
import type { Library, LibrarySource } from '../../src/shared/types';

/** Stands in for the absolute path of a fixture's `volume` folder in `expected.json`. */
export const VOLUME_PATH_PLACEHOLDER = '{volume}';
/** Stands in for the volume-path hash in library ids, e.g. `rekordbox-{volumeHash}`. */
export const VOLUME_HASH_PLACEHOLDER = '{volumeHash}';

export const LIBRARY_FORMATS = ['rekordbox', 'deviceLibraryPlus', 'serato'] as const;

/** What `expected.json` holds: every library the app finds on the volume. */
export interface ExpectedVolume {
  volumeName: string;
  /** In the order the adapters run: rekordbox, Device Library Plus, Serato. */
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

/** Runs every adapter on a volume the way the app does, with machine-specific values replaced. */
export async function readVolume(volumePath: string, volumeName: string): Promise<ExpectedVolume> {
  const result: ExpectedVolume = { volumeName, libraries: [], errors: [] };
  for (const adapter of adapters) {
    try {
      if (!(await adapter.detect(volumePath))) continue;
      const library = await adapter.load({ path: volumePath, name: volumeName });
      result.libraries.push(withPlaceholders(library, volumePath));
    } catch {
      result.errors.push(adapter.source);
    }
  }
  return JSON.parse(JSON.stringify(result)) as ExpectedVolume;
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

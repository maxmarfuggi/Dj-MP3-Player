import path from 'node:path';
import type { Library, LibrarySource, LibraryState } from '../shared/types';
import { adapters } from './adapters';
import { exists, resolveInsideVolume } from './adapters/paths';
import type { Volume } from './adapters/types';
import { listVolumes } from './volumes';

const SCAN_INTERVAL_MS = 2000;

type LoadResult =
  | { library: Library; trackPaths: Map<string, string> }
  | { error: string };

interface VolumeEntry {
  volume: Volume;
  /** Folders the user picked are kept until they disappear from disk. */
  manual: boolean;
  /** One result per format found on the volume; formats not found are retried on the next scan. */
  results: Map<LibrarySource, LoadResult>;
}

/** Tracks which DJ libraries are currently available and keeps them loaded. */
export class LibraryManager {
  private readonly entries = new Map<string, VolumeEntry>();
  private timer: NodeJS.Timeout | undefined;
  private scanning = false;

  constructor(private readonly onChange: (state: LibraryState) => void) {}

  start(): void {
    void this.scan();
    this.timer = setInterval(() => void this.scan(), SCAN_INTERVAL_MS);
  }

  stop(): void {
    clearInterval(this.timer);
  }

  getState(): LibraryState {
    const state: LibraryState = { libraries: [], errors: [] };
    for (const { volume, results } of this.entries.values()) {
      for (const [source, result] of results) {
        if ('library' in result) {
          state.libraries.push(result.library);
        } else {
          state.errors.push({
            volumePath: volume.path,
            volumeName: volume.name,
            source,
            message: result.error,
          });
        }
      }
    }
    return state;
  }

  /** Adds a folder the user picked. Returns false if it holds no DJ library. */
  async addFolder(folderPath: string): Promise<boolean> {
    const entry = this.entries.get(folderPath) ?? {
      volume: { path: folderPath, name: path.basename(folderPath) },
      manual: true,
      results: new Map(),
    };
    const changed = await this.loadNewLibraries(entry);
    if (entry.results.size === 0) return false;
    this.entries.set(folderPath, entry);
    if (changed) this.onChange(this.getState());
    return true;
  }

  /** Absolute path of a track's audio file, or null if the track is unknown. */
  resolveTrackPath(libraryId: string, trackId: string): string | null {
    for (const { volume, results } of this.entries.values()) {
      for (const result of results.values()) {
        if ('library' in result && result.library.id === libraryId) {
          const relativePath = result.trackPaths.get(trackId);
          return relativePath === undefined ? null : resolveInsideVolume(volume.path, relativePath);
        }
      }
    }
    return null;
  }

  private async scan(): Promise<void> {
    if (this.scanning) return;
    this.scanning = true;
    try {
      const volumes = await listVolumes();
      const mounted = new Set(volumes.map((volume) => volume.path));
      let changed = false;

      for (const [entryPath, entry] of this.entries) {
        const gone = entry.manual ? !(await exists(entryPath)) : !mounted.has(entryPath);
        if (gone) {
          this.entries.delete(entryPath);
          changed = true;
        }
      }

      for (const volume of volumes) {
        const entry = this.entries.get(volume.path) ?? { volume, manual: false, results: new Map() };
        if (await this.loadNewLibraries(entry)) {
          this.entries.set(volume.path, entry);
          changed = true;
        }
      }

      if (changed) this.onChange(this.getState());
    } catch (error) {
      console.error('Scanning for volumes failed:', error);
    } finally {
      this.scanning = false;
    }
  }

  /** Runs every adapter that has no result for this volume yet. Returns whether any produced one. */
  private async loadNewLibraries(entry: VolumeEntry): Promise<boolean> {
    let changed = false;
    for (const adapter of adapters) {
      if (entry.results.has(adapter.source)) continue;
      try {
        if (!(await adapter.detect(entry.volume.path))) continue;
        const library = await adapter.load(entry.volume);
        entry.results.set(adapter.source, {
          library,
          trackPaths: new Map(library.tracks.map((track) => [track.id, track.relativePath])),
        });
      } catch (error) {
        console.error(`Loading the ${adapter.source} library on ${entry.volume.path} failed:`, error);
        entry.results.set(adapter.source, {
          error: error instanceof Error ? error.message : String(error),
        });
      }
      changed = true;
    }
    return changed;
  }
}

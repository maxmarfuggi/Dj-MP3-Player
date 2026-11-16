import { createRequire } from 'node:module';
import path from 'node:path';
import { app } from 'electron';
import type { LibraryState } from '../shared/types';
import type { MediaLibrary } from './mediaService';

/** The scan loop and path lookup the Rust core exposes to the main process. */
export interface NativeLibraryManager extends MediaLibrary {
  start(): void;
  stop(): void;
  getState(): LibraryState;
  /** Returns false when the folder holds no DJ library. */
  addFolder(folderPath: string): boolean;
}

interface NativeBinding {
  LibraryManager: new (onChange: (state: LibraryState) => void) => NativeLibraryManager;
}

/**
 * Loads the Rust core. A packaged app uses the copy in its resources folder,
 * because a native addon cannot be loaded from inside the app archive.
 */
export function loadCore(): NativeBinding {
  const require = createRequire(import.meta.url);
  const id = app.isPackaged ? path.join(process.resourcesPath, 'djusb-node', 'index.js') : 'djusb-node';
  let binding: Partial<NativeBinding>;
  try {
    binding = require(id) as Partial<NativeBinding>;
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    throw new Error(`The library core could not be loaded. Build it with npm run build:node. ${detail}`);
  }
  if (typeof binding.LibraryManager !== 'function') {
    throw new Error('The library core is missing LibraryManager. Build it with npm run build:node.');
  }
  return binding as NativeBinding;
}

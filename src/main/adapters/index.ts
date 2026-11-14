import { rekordboxAdapter } from './rekordbox';
import { deviceLibraryPlusAdapter } from './rekordbox/deviceLibraryPlus';
import { seratoAdapter } from './serato';
import type { LibraryAdapter } from './types';

/**
 * A single USB can carry several formats, so every adapter is tried on every
 * volume. A rekordbox USB often has both a Device Library and a Device Library
 * Plus export, and each is shown as its own library.
 */
export const adapters: LibraryAdapter[] = [rekordboxAdapter, deviceLibraryPlusAdapter, seratoAdapter];

import { rekordboxAdapter } from './rekordbox';
import { seratoAdapter } from './serato';
import type { LibraryAdapter } from './types';

/** A single USB can carry both formats, so every adapter is tried on every volume. */
export const adapters: LibraryAdapter[] = [rekordboxAdapter, seratoAdapter];

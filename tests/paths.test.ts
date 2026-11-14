import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { resolveInsideVolume, toRelativePath } from '../src/main/adapters/paths';

describe('toRelativePath', () => {
  it('strips the leading slash rekordbox adds', () => {
    expect(toRelativePath('/Contents/Artist/track.mp3')).toBe('Contents/Artist/track.mp3');
    expect(toRelativePath('Music/track.mp3')).toBe('Music/track.mp3');
  });
});

describe('resolveInsideVolume', () => {
  const volume = path.resolve('/Volumes/GIGS');

  it('resolves paths below the volume root', () => {
    expect(resolveInsideVolume(volume, 'Contents/Artist/track.mp3')).toBe(
      path.join(volume, 'Contents', 'Artist', 'track.mp3'),
    );
  });

  it('rejects paths that escape the volume', () => {
    expect(resolveInsideVolume(volume, '../../etc/passwd')).toBeNull();
    expect(resolveInsideVolume(volume, 'Contents/../../other/track.mp3')).toBeNull();
    expect(resolveInsideVolume(volume, '')).toBeNull();
  });
});

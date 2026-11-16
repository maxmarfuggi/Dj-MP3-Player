import { existsSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { readAiffAsWav, readAiffPcm, wavSize } from '../src/main/aiff';
import { LIBRARY_FORMATS, listCases, readExpected, readVolume, volumeNameOf } from './helpers/fixtures';

describe('library fixtures', () => {
  const cases = LIBRARY_FORMATS.flatMap((format) => listCases(format));

  it('has fixtures for every format', () => {
    for (const format of LIBRARY_FORMATS) {
      expect(cases.some((fixture) => fixture.name.startsWith(`${format}/`)), format).toBe(true);
    }
  });

  it.each(cases.map((fixture) => [fixture.name, fixture] as const))('%s', async (_name, fixture) => {
    expect(await readVolume(fixture.volumePath, volumeNameOf(fixture))).toEqual(readExpected(fixture));
  });
});

// Copies of real USBs, made with `npm run fixtures -- capture`. They are not committed.
const localCases = listCases('local');
describe.skipIf(localCases.length === 0)('local USB fixtures', () => {
  it.each(localCases.map((fixture) => [fixture.name, fixture] as const))('%s', async (_name, fixture) => {
    expect(await readVolume(fixture.volumePath, volumeNameOf(fixture))).toEqual(readExpected(fixture));
  });
});

describe('AIFF fixtures', () => {
  const cases = listCases('aiff');

  it.each(cases.map((fixture) => [fixture.name, fixture] as const))('%s', async (_name, fixture) => {
    const pcm = await readAiffPcm(path.join(fixture.directory, 'input.aiff'));
    const expectedWav = path.join(fixture.directory, 'expected.wav');
    if (!existsSync(expectedWav)) {
      expect(pcm).toBeNull();
      return;
    }
    expect(pcm).not.toBeNull();
    const wav = Buffer.concat(
      await readAiffAsWav(path.join(fixture.directory, 'input.aiff'), pcm!, 0, wavSize(pcm!) - 1).toArray(),
    );
    expect(wav).toEqual(await readFile(expectedWav));
  });
});

/**
 * Writes the shared fixtures in `fixtures/`.
 *
 *   npm run fixtures                          Rebuilds the synthetic fixtures from the test builders.
 *   npm run fixtures -- capture <volume> <name>
 *                                             Copies the library files of a real USB into
 *                                             `fixtures/local/<name>` (gitignored). The USB is only read.
 *
 * Both write `expected.json` from what the Rust core reads, so review its diff.
 */
import { createHash } from 'node:crypto';
import { cp, mkdir, rm, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import path from 'node:path';
import { readAiffAsWav, readAiffPcm, wavSize } from '../src/main/aiff';
import { type AiffSpec, buildAiff } from '../tests/helpers/aiffBuilder';
import { applyLateChanges, fillBasicLibrary } from '../tests/helpers/deviceLibraryPlusBuilder';
import { fixturesDirectory, readVolume } from '../tests/helpers/fixtures';
import {
  type RowSpec,
  TABLE,
  albumRow,
  artistRow,
  buildPdb,
  keyRow,
  playlistEntryRow,
  playlistNodeRow,
  trackRow,
} from '../tests/helpers/pdbBuilder';
import { seratoCrate, seratoDatabase } from '../tests/helpers/seratoBuilder';
import {
  type RandomBytes,
  encryptPage,
  encryptSqlCipher,
  pageOf,
  plainDatabase,
  walFile,
  PAGE_SIZE,
} from '../tests/helpers/sqlcipherBuilder';

/** The Device Library Plus passphrase. Every export uses it. See docs/contract.md. */
const DEVICE_LIBRARY_PLUS_KEY = 'r8gddnr4k847830ar6cqzbkk0el6qytmb3trbbx805jm74vez64i5o8fnrqryqls';

/** Files a volume holds, by path relative to its root. */
type VolumeFiles = Record<string, Buffer | string>;

interface LibraryCase {
  format: 'rekordbox' | 'deviceLibraryPlus' | 'serato';
  name: string;
  files: () => Promise<VolumeFiles>;
}

const PDB_PATH = 'PIONEER/rekordbox/export.pdb';
const DLP_PATH = 'PIONEER/rekordbox/exportLibrary.db';

function seededRandom(seed: string): RandomBytes {
  let counter = 0;
  return (size) => {
    const blocks: Buffer[] = [];
    for (let length = 0; length < size; length += 32) {
      blocks.push(createHash('sha256').update(`${seed}:${counter++}`).digest());
    }
    return Buffer.concat(blocks).subarray(0, size);
  };
}

function basicPdb(): Buffer {
  const deleted = (row: RowSpec): RowSpec => ({ ...row, present: false });
  return buildPdb([
    {
      type: TABLE.tracks,
      pages: [
        {
          rows: [
            trackRow({
              id: 7,
              title: 'Mädchen',
              filePath: '/Contents/Artist/Mädchen.mp3',
              artistId: 2,
              albumId: 3,
              keyId: 4,
              tempo: 12850,
              durationSec: 301,
            }),
            trackRow({ id: 8, title: 'A very long track title '.repeat(8).trim(), filePath: '/Contents/long.wav' }),
            trackRow({ id: 9, title: '', filePath: '/Contents/Some Artist/Untitled Jam.wav' }),
            deleted(trackRow({ id: 11, title: 'Deleted', filePath: '/Contents/deleted.mp3' })),
            trackRow({
              id: 10,
              title: '東京の夜',
              filePath: '/Contents/日本/東京の夜.mp3',
              artistId: 5,
              keyId: 6,
              tempo: 9000,
              durationSec: 200,
            }),
          ],
        },
      ],
    },
    { type: TABLE.artists, pages: [{ rows: [artistRow(2, 'Short Offset'), artistRow(5, 'Кино', true)] }] },
    { type: TABLE.albums, pages: [{ rows: [albumRow(3, 'The Album')] }] },
    {
      type: TABLE.keys,
      pages: [
        { rows: [], indexPage: true },
        { rows: [keyRow(4, 'Am')] },
        { rows: [keyRow(6, '8A'), deleted(keyRow(12, 'Gone'))] },
      ],
    },
    {
      type: TABLE.playlistTree,
      pages: [
        {
          rows: [
            playlistNodeRow({ id: 3, parentId: 1, sortOrder: 1, isFolder: false, name: 'Peak time' }),
            playlistNodeRow({ id: 2, parentId: 1, sortOrder: 0, isFolder: false, name: 'Warm up' }),
            playlistNodeRow({ id: 1, parentId: 0, sortOrder: 0, isFolder: true, name: 'Sets' }),
            playlistNodeRow({ id: 4, parentId: 42, sortOrder: 1, isFolder: false, name: 'Orphan' }),
            playlistNodeRow({ id: 5, parentId: 0, sortOrder: 2, isFolder: false, name: 'Ночь' }),
          ],
        },
      ],
    },
    {
      type: TABLE.playlistEntries,
      pages: [
        {
          rows: [
            playlistEntryRow(2, 9, 2),
            playlistEntryRow(2, 7, 1),
            playlistEntryRow(3, 8, 1),
            playlistEntryRow(2, 99, 3),
            playlistEntryRow(3, 7, 2),
            playlistEntryRow(5, 10, 1),
          ],
        },
      ],
    },
  ]);
}

/** One playlist with more entries than fit a one-byte row count, spread over many track pages. */
function manyEntriesPdb(): Buffer {
  const count = 270;
  const tracks = Array.from({ length: count }, (_, i) =>
    trackRow({ id: i + 1, title: `T${i + 1}`, filePath: `/C/${i + 1}.mp3` }),
  );
  const trackPages = [];
  for (let i = 0; i < tracks.length; i += 20) trackPages.push({ rows: tracks.slice(i, i + 20) });
  return buildPdb([
    { type: TABLE.tracks, pages: trackPages },
    {
      type: TABLE.playlistTree,
      pages: [{ rows: [playlistNodeRow({ id: 1, parentId: 0, sortOrder: 0, isFolder: false, name: 'Long set' })] }],
    },
    {
      type: TABLE.playlistEntries,
      // Stored backwards, so the entry index has to put them in order.
      pages: [{ rows: Array.from({ length: count }, (_, i) => playlistEntryRow(1, count - i, count - i)) }],
    },
  ]);
}

async function basicDeviceLibraryPlus(seed: string): Promise<Buffer> {
  const { database } = await encryptSqlCipher(
    await plainDatabase(fillBasicLibrary),
    DEVICE_LIBRARY_PLUS_KEY,
    seededRandom(seed),
  );
  return database;
}

const artwork: VolumeFiles = {
  'PIONEER/Artwork/00001/a1.jpg': 'jpeg',
  'PIONEER/Artwork/00001/a1_m.jpg': 'jpeg',
  'PIONEER/Artwork/00001/B2.JPG': 'jpeg',
};

const LIBRARY_CASES: LibraryCase[] = [
  { format: 'rekordbox', name: 'basic', files: async () => ({ [PDB_PATH]: basicPdb() }) },
  { format: 'rekordbox', name: 'many-entries', files: async () => ({ [PDB_PATH]: manyEntriesPdb() }) },
  {
    format: 'rekordbox',
    name: 'both-exports',
    files: async () => ({
      [PDB_PATH]: basicPdb(),
      [DLP_PATH]: await basicDeviceLibraryPlus('both-exports'),
      ...artwork,
    }),
  },
  {
    format: 'deviceLibraryPlus',
    name: 'basic',
    files: async () => ({ [DLP_PATH]: await basicDeviceLibraryPlus('dlp-basic'), ...artwork }),
  },
  {
    format: 'deviceLibraryPlus',
    name: 'with-wal',
    files: async () => {
      const random = seededRandom('dlp-wal');
      const before = await plainDatabase(fillBasicLibrary);
      const after = await plainDatabase(applyLateChanges, before);
      const { database, keys } = await encryptSqlCipher(before, DEVICE_LIBRARY_PLUS_KEY, random);
      const pageCount = after.length / PAGE_SIZE;
      const changed = Array.from({ length: pageCount }, (_, index) => index + 1).filter(
        (pageNumber) => !pageOf(after, pageNumber).equals(pageOf(before, pageNumber)),
      );
      const wal = walFile(
        Buffer.from('fixture!', 'latin1'),
        changed.map((pageNumber, index) => ({
          pageNumber,
          page: encryptPage(pageOf(after, pageNumber), pageNumber, keys, random),
          commitPageCount: index === changed.length - 1 ? pageCount : undefined,
        })),
      );
      return { [DLP_PATH]: database, [`${DLP_PATH}-wal`]: wal };
    },
  },
  {
    format: 'deviceLibraryPlus',
    name: 'damaged',
    files: async () => {
      const database = Buffer.from(await basicDeviceLibraryPlus('dlp-damaged'));
      database[PAGE_SIZE + 100]! ^= 0xff;
      return { [DLP_PATH]: database };
    },
  },
  {
    format: 'serato',
    name: 'basic',
    files: async () => ({
      '_Serato_/database V2': seratoDatabase([
        {
          pfil: 'Music/Björk - Jóga.mp3',
          tsng: 'Jóga',
          tart: 'Björk',
          talb: 'Homogenic',
          tbpm: '126.00',
          tkey: 'Bb',
          tlen: '06:22.93',
        },
        { pfil: 'Music/Untitled Jam.wav' },
        { pfil: 'Music/a.mp3', tsng: 'A', tbpm: '0', tlen: '00:00.00' },
        { pfil: 'Music/a.mp3', tsng: 'Duplicate of A' },
        { pfil: 'Музыка/Лето.flac', tsng: 'Лето', tart: 'Кино', tlen: '4:05' },
      ]),
      '_Serato_/Subcrates/House.crate': seratoCrate(['Music/a.mp3', 'Music/Björk - Jóga.mp3']),
      '_Serato_/Subcrates/House%%Deep.crate': seratoCrate(['Music/Untitled Jam.wav']),
      '_Serato_/Subcrates/Techno%%Hard.crate': seratoCrate(['Music/a.mp3', 'Music/Fresh Tune.flac']),
      '_Serato_/Subcrates/crate 10.crate': seratoCrate(['Музыка/Лето.flac']),
      '_Serato_/Subcrates/crate 9.crate': seratoCrate(['Music/New One.mp3', 'Music/a.mp3']),
      '_Serato_/Subcrates/夏.crate': seratoCrate(['Музыка/Лето.flac']),
      '_Serato_/Subcrates/._House.crate': 'resource fork',
    }),
  },
  {
    format: 'serato',
    name: 'crates-only',
    files: async () => ({
      '_Serato_/Subcrates/Openers.crate': seratoCrate(['Music/Fresh Tune.flac', 'Music/Second.mp3']),
    }),
  },
  {
    format: 'serato',
    name: 'not-a-library',
    // Serato makes `_Serato_` on every drive it sees; macOS adds `._` files next to real ones.
    files: async () => ({ '_Serato_/Subcrates/._Openers.crate': 'resource fork' }),
  },
];

interface AiffCase {
  name: string;
  spec: AiffSpec;
}

const AIFF_CASES: AiffCase[] = [
  {
    name: 'pcm16-stereo',
    spec: { channels: 2, bitsPerSample: 16, sampleRate: 44100, samples: Buffer.from([1, 2, 3, 4, 5, 6, 7, 8]) },
  },
  {
    name: 'pcm24-mono',
    spec: { channels: 1, bitsPerSample: 24, sampleRate: 48000, samples: Buffer.from([1, 2, 3, 4, 5, 6]) },
  },
  {
    name: 'pcm32-mono',
    spec: { channels: 1, bitsPerSample: 32, sampleRate: 96000, samples: Buffer.from([1, 2, 3, 4, 5, 6, 7, 8]) },
  },
  {
    name: 'pcm8-signed',
    spec: { channels: 1, bitsPerSample: 8, sampleRate: 22050, samples: Buffer.from([0x00, 0x7f, 0x80, 0xff]) },
  },
  {
    name: 'sowt',
    spec: { channels: 1, bitsPerSample: 16, sampleRate: 44100, samples: Buffer.from([1, 2, 3, 4]), compression: 'sowt' },
  },
  {
    name: 'sound-before-format',
    spec: { channels: 1, bitsPerSample: 16, sampleRate: 44100, samples: Buffer.from([1, 2]), soundFirst: true },
  },
  {
    name: 'compressed-declined',
    spec: { channels: 1, bitsPerSample: 16, sampleRate: 44100, samples: Buffer.alloc(4), compression: 'ima4' },
  },
];

async function writeVolume(volumePath: string, files: VolumeFiles): Promise<void> {
  await rm(volumePath, { recursive: true, force: true });
  for (const [relativePath, data] of Object.entries(files)) {
    const filePath = path.join(volumePath, ...relativePath.split('/'));
    await mkdir(path.dirname(filePath), { recursive: true });
    await writeFile(filePath, data);
  }
}

async function writeExpected(caseDirectory: string, volumeName: string): Promise<void> {
  const expected = await readVolume(path.join(caseDirectory, 'volume'), volumeName);
  await writeFile(path.join(caseDirectory, 'expected.json'), `${JSON.stringify(expected, null, 2)}\n`);
}

async function build(): Promise<void> {
  const root = fixturesDirectory();
  for (const fixture of LIBRARY_CASES) {
    const caseDirectory = path.join(root, fixture.format, fixture.name);
    await writeVolume(path.join(caseDirectory, 'volume'), await fixture.files());
    await writeExpected(caseDirectory, fixture.name);
    console.log(`${fixture.format}/${fixture.name}`);
  }

  for (const fixture of AIFF_CASES) {
    const caseDirectory = path.join(root, 'aiff', fixture.name);
    await rm(caseDirectory, { recursive: true, force: true });
    await mkdir(caseDirectory, { recursive: true });
    const input = path.join(caseDirectory, 'input.aiff');
    await writeFile(input, buildAiff(fixture.spec));
    const pcm = await readAiffPcm(input);
    if (pcm) {
      const wav = Buffer.concat(await readAiffAsWav(input, pcm, 0, wavSize(pcm) - 1).toArray());
      await writeFile(path.join(caseDirectory, 'expected.wav'), wav);
    }
    console.log(`aiff/${fixture.name}`);
  }
}

/** The files the readers use. Audio is left behind; artwork is needed to resolve `artworkPath`. */
const CAPTURED_PATHS = [
  'PIONEER/rekordbox/export.pdb',
  'PIONEER/rekordbox/exportLibrary.db',
  'PIONEER/rekordbox/exportLibrary.db-wal',
  'PIONEER/Artwork',
  '_Serato_/database V2',
  '_Serato_/Subcrates',
];

async function capture(sourceVolume: string | undefined, name: string | undefined): Promise<void> {
  if (!sourceVolume || !name || !/^[a-z0-9][a-z0-9-]*$/i.test(name)) {
    throw new Error('Usage: npm run fixtures -- capture <volume path> <name, letters, digits and dashes>');
  }
  const source = path.resolve(sourceVolume);
  const caseDirectory = path.join(fixturesDirectory(), 'local', name);
  const target = path.join(caseDirectory, 'volume');
  const fromSource = path.relative(source, target);
  if (fromSource === '' || (!fromSource.startsWith('..') && !path.isAbsolute(fromSource))) {
    throw new Error('The fixture folder must not be inside the volume being captured.');
  }

  await rm(target, { recursive: true, force: true });
  let copied = 0;
  for (const relativePath of CAPTURED_PATHS) {
    const from = path.join(source, ...relativePath.split('/'));
    if (!existsSync(from)) continue;
    // Only reads `from`; nothing on the volume is written.
    await cp(from, path.join(target, ...relativePath.split('/')), { recursive: true });
    copied++;
  }
  if (copied === 0) throw new Error(`No rekordbox or Serato library files found on ${source}`);
  await writeExpected(caseDirectory, name);
  console.log(`local/${name}: copied ${copied} item(s) from ${source}`);
}

const [command = 'build', ...args] = process.argv.slice(2);
const run = command === 'build' ? build() : command === 'capture' ? capture(args[0], args[1]) : null;
if (run === null) {
  console.error(`Unknown command "${command}". Use "build" or "capture".`);
  process.exitCode = 1;
} else {
  run.catch((error: unknown) => {
    console.error(error instanceof Error ? error.message : error);
    process.exitCode = 1;
  });
}

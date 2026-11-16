const assert = require('node:assert/strict');
const { createHash } = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');

const binding = path.join(__dirname, 'index.js');
if (!fs.existsSync(binding)) {
  throw new Error('Build the bridge first: npm run build -w djusb-node');
}

const { LibraryManager, listVolumes, readVolume, serveMedia } = require('./index.js');

const repoRoot = path.resolve(__dirname, '../..');
const fixturesRoot = path.join(repoRoot, 'fixtures');

function libraryId(source, volumePath) {
  const hash = createHash('sha1').update(volumePath).digest('hex').slice(0, 12);
  return `${source}-${hash}`;
}

function listFixtures() {
  const cases = [];
  for (const group of ['rekordbox', 'deviceLibraryPlus', 'serato', 'local']) {
    const directory = path.join(fixturesRoot, group);
    if (!fs.existsSync(directory)) continue;
    for (const name of fs.readdirSync(directory).sort()) {
      const caseDirectory = path.join(directory, name);
      const volumePath = path.join(caseDirectory, 'volume');
      if (fs.existsSync(volumePath) && fs.existsSync(path.join(caseDirectory, 'expected.json'))) {
        cases.push({ name: `${group}/${name}`, volumePath, caseDirectory });
      }
    }
  }
  return cases;
}

function mediaUrl(kind, libraryId, trackId) {
  return `media://${kind}/${encodeURIComponent(libraryId)}/${encodeURIComponent(trackId)}`;
}

function fileInVolume(volumePath, relativePath) {
  return path.join(volumePath, ...relativePath.split('/'));
}

function copyDir(from, to) {
  fs.mkdirSync(to, { recursive: true });
  for (const entry of fs.readdirSync(from, { withFileTypes: true })) {
    const source = path.join(from, entry.name);
    const target = path.join(to, entry.name);
    if (entry.isDirectory()) copyDir(source, target);
    else fs.copyFileSync(source, target);
  }
}

function flush() {
  return new Promise((resolve) => {
    setImmediate(resolve);
  });
}

test('readVolume matches a folder opened through the manager', () => {
  const volumePath = path.join(fixturesRoot, 'rekordbox', 'basic', 'volume');
  const manager = new LibraryManager(() => {});
  assert.equal(manager.addFolder(volumePath), true);
  assert.deepEqual(readVolume(volumePath, path.basename(volumePath)), manager.getState());
});

test('readVolume on an empty folder is an empty state', () => {
  const folder = fs.mkdtempSync(path.join(os.tmpdir(), 'djusb-node-read-'));
  try {
    assert.deepEqual(readVolume(folder, 'empty'), { libraries: [], errors: [] });
  } finally {
    fs.rmSync(folder, { recursive: true, force: true });
  }
});

test('listVolumes returns a path and a name for each volume', () => {
  const volumes = listVolumes();
  assert.ok(Array.isArray(volumes));
  for (const volume of volumes) {
    assert.equal(typeof volume.path, 'string');
    assert.notEqual(volume.path, '');
    assert.equal(typeof volume.name, 'string');
    assert.notEqual(volume.name, '');
  }
});

test('a new manager has nothing loaded', () => {
  const manager = new LibraryManager(() => {});
  assert.deepEqual(manager.getState(), { libraries: [], errors: [] });
});

test('an empty folder is not a library', () => {
  const manager = new LibraryManager(() => {});
  const folder = fs.mkdtempSync(path.join(os.tmpdir(), 'djusb-node-empty-'));
  try {
    assert.equal(manager.addFolder(folder), false);
    assert.deepEqual(manager.getState(), { libraries: [], errors: [] });
  } finally {
    fs.rmSync(folder, { recursive: true, force: true });
  }
});

test('loading a folder reports the new state', async () => {
  const volumePath = path.join(fixturesRoot, 'rekordbox', 'basic', 'volume');
  /** @type {unknown[]} */
  const seen = [];
  const manager = new LibraryManager((state) => {
    seen.push(state);
  });
  assert.equal(manager.addFolder(volumePath), true);
  await flush();
  assert.equal(seen.length, 1);
  assert.equal(seen[0].libraries.length, 1);
  assert.equal(seen[0].libraries[0].source, 'rekordbox');

  assert.equal(manager.addFolder(volumePath), true);
  await flush();
  assert.equal(seen.length, 1);
});

test('the scan loop can be started and stopped', () => {
  const manager = new LibraryManager(() => {});
  manager.start();
  manager.stop();
  manager.stop();
});

for (const fixture of listFixtures()) {
  test(`${fixture.name} matches expected.json`, () => {
    const expected = JSON.parse(fs.readFileSync(path.join(fixture.caseDirectory, 'expected.json'), 'utf8'));
    const manager = new LibraryManager(() => {});
    const found = manager.addFolder(fixture.volumePath);
    const state = manager.getState();

    if (!found) {
      assert.deepEqual(expected.libraries, []);
      assert.deepEqual(expected.errors, []);
      assert.deepEqual(state, { libraries: [], errors: [] });
      return;
    }

    assert.deepEqual(
      state.errors.map((error) => error.source),
      expected.errors,
    );
    for (const error of state.errors) {
      assert.equal(error.volumePath, fixture.volumePath);
      assert.equal(error.volumeName, path.basename(fixture.volumePath));
      assert.equal(typeof error.message, 'string');
      assert.notEqual(error.message, '');
    }

    assert.equal(state.libraries.length, expected.libraries.length);
    for (let index = 0; index < state.libraries.length; index += 1) {
      const library = state.libraries[index];
      const want = expected.libraries[index];
      assert.equal(library.id, libraryId(library.source, fixture.volumePath));
      assert.equal(library.volumePath, fixture.volumePath);
      assert.equal(library.volumeName, path.basename(fixture.volumePath));
      if (library.tracks.length > 0) {
        const resolved = manager.resolveTrackPath(library.id, library.tracks[0].id);
        assert.equal(typeof resolved, 'string');
        assert.equal(manager.resolveTrackPath(library.id, 'missing-track'), null);
      }
      library.id = `${library.source}-{volumeHash}`;
      library.volumePath = '{volume}';
      library.volumeName = want.volumeName;
      assert.deepEqual(library, want);
    }
  });
}

test('serves ranges, artwork, and AIFF through the bridge', () => {
  const source = path.join(fixturesRoot, 'deviceLibraryPlus', 'basic', 'volume');
  const volumePath = fs.mkdtempSync(path.join(os.tmpdir(), 'djusb-node-media-'));
  try {
    copyDir(source, volumePath);
    const manager = new LibraryManager(() => {});
    assert.equal(manager.addFolder(volumePath), true);
    const library = manager.getState().libraries[0];
    const mp3 = library.tracks.find((track) => track.relativePath.endsWith('.mp3'));
    const empty = library.tracks.find((track) => track.relativePath.endsWith('closer.mp3'));
    const aiff = library.tracks.find((track) => track.relativePath.endsWith('.aiff'));
    const artwork = library.tracks.find((track) => track.artworkPath);

    const mp3Path = fileInVolume(volumePath, mp3.relativePath);
    fs.mkdirSync(path.dirname(mp3Path), { recursive: true });
    fs.writeFileSync(mp3Path, Buffer.from('abcdefghij'));

    const ranged = serveMedia(mediaUrl('track', library.id, mp3.id), 'bytes=2-5', manager);
    assert.equal(ranged.status, 206);
    assert.equal(ranged.headers['Content-Type'], 'audio/mpeg');
    assert.equal(ranged.headers['Content-Range'], 'bytes 2-5/10');
    assert.equal(ranged.headers['Content-Length'], '4');
    assert.equal(ranged.headers['Accept-Ranges'], 'bytes');
    assert.equal(ranged.headers['Access-Control-Allow-Origin'], '*');
    assert.ok(ranged.body.equals(Buffer.from('cdef')));

    const outside = serveMedia(mediaUrl('track', library.id, mp3.id), 'bytes=20-30', manager);
    assert.equal(outside.status, 416);
    assert.deepEqual(outside.headers, { 'Content-Range': 'bytes */10' });
    assert.equal(outside.body, null);

    const missing = serveMedia(mediaUrl('track', library.id, 'missing'), null, manager);
    assert.equal(missing.status, 404);
    assert.deepEqual(missing.headers, {});
    assert.equal(missing.body, null);

    const other = serveMedia(mediaUrl('other', library.id, mp3.id), null, manager);
    assert.equal(other.status, 404);
    assert.equal(other.body, null);

    fs.writeFileSync(fileInVolume(volumePath, empty.relativePath), Buffer.alloc(0));
    const blank = serveMedia(mediaUrl('track', library.id, empty.id), null, manager);
    assert.equal(blank.status, 200);
    assert.equal(blank.headers['Content-Type'], 'audio/mpeg');
    assert.equal(blank.headers['Content-Length'], undefined);
    assert.equal(blank.body, null);

    const artPath = fileInVolume(volumePath, artwork.artworkPath);
    const art = serveMedia(mediaUrl('artwork', library.id, artwork.id), null, manager);
    assert.equal(art.status, 200);
    assert.equal(art.headers['Content-Type'], 'image/jpeg');
    assert.ok(art.body.equals(fs.readFileSync(artPath)));
    assert.equal(
      path.normalize(manager.resolveArtworkPath(library.id, artwork.id)),
      path.normalize(artPath),
    );

    const aiffPath = fileInVolume(volumePath, aiff.relativePath);
    fs.mkdirSync(path.dirname(aiffPath), { recursive: true });
    fs.copyFileSync(path.join(fixturesRoot, 'aiff', 'pcm16-stereo', 'input.aiff'), aiffPath);
    const wav = serveMedia(mediaUrl('track', library.id, aiff.id), null, manager);
    assert.equal(wav.status, 200);
    assert.equal(wav.headers['Content-Type'], 'audio/wav');
    assert.ok(wav.body.equals(fs.readFileSync(path.join(fixturesRoot, 'aiff', 'pcm16-stereo', 'expected.wav'))));
  } finally {
    fs.rmSync(volumePath, { recursive: true, force: true });
  }
});

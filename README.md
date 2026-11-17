# Dj-MP3-Player
An MP3 Player for DJ USBs

Plug in a USB stick prepared with rekordbox or Serato and browse its playlists
and play its tracks, like a regular music player. This is a proof of concept.

## Running it

Requires Node.js 22.13 or newer (the fixture script uses the built-in
`node:sqlite`).

Libraries are read by the Rust core. Build it once, and again after that code
changes:

```sh
npm install
npm run build:node
npm run dev         # run with hot reload
npm start           # build and run the production build
npm test            # desktop unit tests
npm run test:node   # library fixtures through the core
npm run typecheck
```

A USB with a library is picked up automatically within a couple of seconds of
mounting. **Open folder…** loads a library from any folder, which is useful for
a copy of a USB on disk.

## Wall player

`apps/device` is the wall player. It is an iced window, and it reads libraries
through the same Rust core as the desktop app. Run it on this computer with:

```sh
cargo run -p djusb-device
```

The window opens at the 7" panel size, turned landscape to 1280×720. It lists a rekordbox or
Serato USB when one is mounted. Pressing an export plays its tracks in order
through the default audio output, and pressing it again pauses. A Linux build
needs ALSA's development package (`libasound2-dev`); on the Pi, PipeWire
serves ALSA's default output, so wired and Bluetooth speakers both work.

The player's own tests include one that plays through this computer's output.
It is skipped by default:

```sh
cargo test -p djusb-device -- --ignored
```

A 64-bit Raspberry Pi build uses [`cross`](https://github.com/cross-rs/cross),
which supplies the Wayland and ALSA libraries the app links against:

```sh
cargo install cross --locked
cross build --release -p djusb-device --target aarch64-unknown-linux-gnu
```

The binary is `target/aarch64-unknown-linux-gnu/release/djusb-device`.

## How it works

The app is Electron with a React UI. Library files are read by the Rust core
in `crates/djusb-core`, which the main process loads through `crates/djusb-node`.

- The core watches mounted volumes, loads any DJ library it finds, and drops
  it when the volume goes away. Each format becomes the same `Library` model
  (`apps/desktop/src/shared/types.ts`), so the UI and the player do not know
  which software wrote the USB.
  - **rekordbox Device Library**: reads `PIONEER/rekordbox/export.pdb`.
  - **rekordbox Device Library Plus**: reads `PIONEER/rekordbox/exportLibrary.db`.
    It is a SQLCipher-encrypted SQLite database with a key shared by every
    export. The core decrypts it in memory (applying any `-wal` file), and the
    plain copy is written to a temporary folder only while it is queried.
    This export also provides genre, label, track colour and artwork. A USB
    usually carries both exports, and they can differ, so each appears as its
    own library.
  - **Serato**: reads `_Serato_/database V2` and the crates in
    `_Serato_/Subcrates`.
- `apps/desktop/src/main/mediaProtocol.ts` serves audio to the UI as
  `media://track/<libraryId>/<trackId>`, with range requests so seeking works,
  and artwork as `media://artwork/<libraryId>/<trackId>`.
  AIFF files are converted to WAV on the fly (`apps/desktop/src/main/aiff.ts`) because
  Chromium cannot play AIFF.
- `apps/desktop/src/renderer/` is the UI: playlist tree, track list and transport bar.
  Click the Title, Artist or BPM header to sort. In a playlist, drag tracks
  (or press Alt+↑/↓) to reorder them. Custom orders are saved on this computer
  only (`useCustomOrders.ts`); the USB is not changed.
- `apps/desktop/src/renderer/src/visualizers/` holds the full-window visualizers, opened
  from the button next to the volume slider: a live waveform of the audio
  output, a turntable whose record spins and whose tonearm follows the
  track's progress, and MilkDrop (via
  [butterchurn](https://github.com/jberg/butterchurn) and its preset pack,
  loaded only when that tab opens). The waveform and MilkDrop read the audio
  through a Web Audio `AnalyserNode` (set up in `usePlayer.ts`); that only
  works because the `media://` scheme sends CORS headers, see
  `registerMediaScheme`. Butterchurn compiles preset equations with
  `new Function` and loads textures from `data:` URLs, which is why the
  renderer's Content-Security-Policy allows `'unsafe-eval'` and `data:` images.

The USB is only ever read, never written to.

## Not supported yet

- Genre, label, colour and artwork from the Device Library (`export.pdb`);
  only Device Library Plus libraries show them.
- Apple Lossless (ALAC) files and compressed AIFF-C. They are skipped and
  greyed out.
- Serato smart crates and Serato's custom crate order (crates are sorted by
  name).
- Waveforms, cue points, beatgrids and search.
- Volume detection is tested on macOS and Windows. Linux has a code path but
  has not been run.

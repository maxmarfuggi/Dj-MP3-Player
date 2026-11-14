# Dj-MP3-Player
An MP3 Player for DJ USBs

Plug in a USB stick prepared with rekordbox or Serato and browse its playlists
and play its tracks, like a regular music player. This is a proof of concept.

## Running it

Requires Node.js 22.13 or newer (the tests use the built-in `node:sqlite`).

```sh
npm install
npm run dev      # run with hot reload
npm start        # build and run the production build
npm test         # unit tests
npm run typecheck
```

A USB with a library is picked up automatically within a couple of seconds of
mounting. **Open folder…** loads a library from any folder, which is useful for
a copy of a USB on disk.

## How it works

The app is Electron with a React UI.

- `src/main/volumes.ts` lists mounted volumes and `src/main/libraryManager.ts`
  polls them, loading any DJ library it finds and dropping it when the volume
  goes away.
- `src/main/adapters/` has one adapter per library format. Each turns its
  format into the same `Library` model (`src/shared/types.ts`), so the UI and
  the player do not know which software wrote the USB.
  - **rekordbox Device Library**: reads `PIONEER/rekordbox/export.pdb` with
    the parser in `adapters/rekordbox/pdb.ts`.
  - **rekordbox Device Library Plus**: reads `PIONEER/rekordbox/exportLibrary.db`
    (`adapters/rekordbox/deviceLibraryPlus.ts`). It is a SQLCipher-encrypted
    SQLite database with a key shared by every export.
    `adapters/rekordbox/sqlcipher.ts` decrypts it in memory (applying any
    `-wal` file), and the plain copy is written to a temp folder only while
    `node:sqlite` reads it. This export also provides genre, label, track
    colour and artwork. A USB usually carries both exports, and they can
    differ, so each appears as its own library.
  - **Serato**: reads `_Serato_/database V2` and the crates in
    `_Serato_/Subcrates`.
- `src/main/mediaProtocol.ts` serves audio to the UI as
  `media://track/<libraryId>/<trackId>`, with range requests so seeking works,
  and artwork as `media://artwork/<libraryId>/<trackId>`.
  AIFF files are converted to WAV on the fly (`src/main/aiff.ts`) because
  Chromium cannot play AIFF.
- `src/renderer/` is the UI: playlist tree, track list and transport bar.
  Click the Title, Artist or BPM header to sort. In a playlist, drag tracks
  (or press Alt+↑/↓) to reorder them. Custom orders are saved on this computer
  only (`useCustomOrders.ts`); the USB is not changed.
- `src/renderer/src/visualizers/` holds the full-window visualizers, opened
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

# Dj-MP3-Player
An MP3 Player for DJ USBs

Plug in a USB stick prepared with rekordbox or Serato and browse its playlists
and play its tracks, like a regular music player. This is a proof of concept.

## Running it

Requires Node.js 20.19 or newer.

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
  - **rekordbox**: reads `PIONEER/rekordbox/export.pdb` with the parser in
    `adapters/rekordbox/pdb.ts`.
  - **Serato**: reads `_Serato_/database V2` and the crates in
    `_Serato_/Subcrates`.
- `src/main/mediaProtocol.ts` serves audio to the UI as
  `media://track/<libraryId>/<trackId>`, with range requests so seeking works.
  AIFF files are converted to WAV on the fly (`src/main/aiff.ts`) because
  Chromium cannot play AIFF.
- `src/renderer/` is the UI: playlist tree, track list and transport bar.

The USB is only ever read, never written to.

## Not supported yet

- rekordbox USBs exported only in the newer Device Library Plus format
  (`exportLibrary.db`). The app says so when it sees one.
- Apple Lossless (ALAC) files and compressed AIFF-C. They are skipped and
  greyed out.
- Serato smart crates and Serato's custom crate order (crates are sorted by
  name).
- Waveforms, cue points, beatgrids and search.
- Volume detection is only tested on macOS. Windows and Linux have code paths
  but have not been run.

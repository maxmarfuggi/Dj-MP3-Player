# Refactor plan

Two products share one core:

- **Desktop app:** Electron and React, as today. It gets installers and
  automatic updates.
- **Wall player:** a Rust app on a Raspberry Pi, with an iced UI, two knobs,
  Bluetooth audio and MilkDrop through projectM.
- **Shared core:** a Rust library that reads rekordbox and Serato USBs. Both
  apps use it, so there is one copy of the hardest code.

The work is split into small pieces. Each piece has an ID, an estimate, what
it depends on, and a check that says when it's done. Tick them off as you go.

Estimates are in **focused working days** for one developer who knows
TypeScript, is learning Rust, and works with an AI assistant. The
[Estimates](#estimates) section at the end adds them up.

## Ground rules

These hold for every piece:

1. **The USB is never written to.** Readers and mounts are read-only.
   Anything that has to be modified, such as the encrypted Device Library
   Plus database, is copied to a temporary folder first.
2. **The only network use is desktop updates,** added in Phase 6. The
   desktop's main process checks for and downloads updates. Nothing else in
   either app goes online, and the wall player never does. It's updated from
   a USB stick.
3. **Fixtures are the source of truth.** A change in how a library is read
   updates the fixture's `expected.json` in the same change.
4. **IDs never change silently.** Saved playlist orders are keyed by library
   and playlist IDs.
5. **The desktop app always works.** No piece leaves it broken.

## Target structure

```
apps/
  desktop/        Electron + React
  device/         Rust wall player
crates/
  djusb-core/     Rust: data model, readers, volume scanning, media helpers, shared logic
  djusb-node/     Rust → Node bridge (napi-rs) so the desktop can use djusb-core
fixtures/         Sample USB folders with expected output, used by both languages
hardware/         Enclosure and build guide
docs/             This plan, the data contract, design tokens, licence notices
```

## Decisions

| Area | Choice | Licence |
| --- | --- | --- |
| Shared core and device app | Rust | — |
| Device UI | iced (confirmed by spike A) | MIT |
| MilkDrop on the device | projectM, linked as a shared library | LGPL 2.1 |
| Desktop to core | napi-rs | MIT |
| Device audio | symphonia to decode, cpal or PipeWire to play | MPL 2.0 / Apache 2.0 |
| Encrypted rekordbox database | rusqlite with bundled SQLCipher | MIT / BSD |
| Device system | Raspberry Pi OS Lite, cage kiosk, udisks2, bluer, gpiocdev | Free |
| Desktop updates | electron-builder and electron-updater, published to GitHub Releases | MIT |

---

## Phase 0: groundwork in TypeScript

Locks down today's behaviour and separates logic from Electron, so the Rust
port has an exact reference. Nothing changes for users.

- [x] **0.1 Move the app into `apps/desktop/`** · 0.5 day
  - Move `src/`, `tests/`, `package.json`, `electron.vite.config.ts`,
    `tsconfig.json` and `vitest.config.mts`. Add npm workspaces at the root
    so `npm run dev` and `npm test` still work from the top.
  - *Done when:* dev, build, typecheck and tests pass with no code changes.

- [x] **0.2 Shared fixtures** · 2–4 days · after 0.1
  - Write a script that runs `tests/helpers/pdbBuilder.ts`,
    `seratoBuilder.ts` and `sqlcipherBuilder.ts` and saves their output as
    `fixtures/<format>/<case>/volume/…`, each with an `expected.json` of the
    `Library` it should produce. Use a placeholder for the volume path and a
    fixed sort order, so the files compare the same on any machine.
  - Synthetic fixtures from the builders (committed) plus one real USB
    (local only, in `fixtures/local/`). Add other real exports later when
    someone reports a USB that doesn't load.
  - Switch the TypeScript tests to compare against `expected.json`.
  - *Done when:* every reader is checked against fixture files.

- [x] **0.3 Data contract** · 1 day · after 0.2
  - Write `docs/contract.md`: what each field in `src/shared/types.ts`
    means, and the exact ID rules (`libraryId` is `<source>-` plus the first
    12 hex characters of the SHA-1 of the volume path; how each reader builds
    track and playlist IDs).
  - Write down the read-only rule and the temporary copy used for Device
    Library Plus.
  - *Done when:* someone could write a compatible reader from the document
    and the fixtures alone.

- [x] **0.4 Separate Electron from media serving** · 0.5–1 day · after 0.1
  - Split `src/main/mediaProtocol.ts` into a `mediaService` with no Electron
    import (resolving, file types, ranges, AIFF to WAV, the NFC/NFD retry),
    returning `{ status, headers, body }`, and a thin Electron protocol
    handler that calls it. Keep `main/index.ts` to window and IPC wiring.
  - *Done when:* nothing outside the thin handler and `index.ts` imports
    `electron` in the main process.

- [x] **0.5 Move shared logic out of the UI folder** · 1–2 days · after 0.1
  - Move `trackOrder.ts`, `keyColor.ts`, `energyFlow.ts` and `format.ts` to
    a `logic/` folder.
  - Pull the logic out of `App.tsx` into pure, tested functions in
    `logic/libraryView.ts`: finding the selected playlist, building track
    entries, building the queue, and falling back when a USB is removed.
  - *Done when:* `App.tsx` only wires state to components, and the moved
    logic has tests.

- [x] **0.6 One back-end client in the UI** · 0.5 day · after 0.1
  - Add `renderer/src/backend.ts` with `getState`, `onStateChanged`,
    `openFolder`, `trackUrl` and `artworkUrl`. Move `trackUrl` and
    `artworkUrl` out of `shared/types.ts`.
  - *Done when:* nothing else in the UI uses `window.djusb` or `media://`.

- [x] **0.7 Settings format** · 0.5 day · after 0.3
  - Define the stored format for custom playlist orders and other settings
    in `docs/contract.md`, so the device can save the same thing to a file.

## Track D: desktop releases

Independent of the Rust work. Start it any time after 0.1. It's what you
need to put out downloadable builds.

These builds are unsigned until Phase 6. They still install and run, with
warnings: Windows shows a SmartScreen screen ("More info", then "Run
anyway"), and macOS blocks the app until it's allowed in System Settings >
Privacy & Security ("Open Anyway"). Users update by downloading the new
version.

- [x] **D.1 Installers** · 1–2 days · after 0.1
  - Add electron-builder: a Windows installer (NSIS), a macOS DMG, and Linux
    AppImage and `.deb` packages.
  - *Done when:* installers build locally and the installed app runs.

- [x] **D.2 Release builds in CI** · 1–2 days · after D.1
  - A GitHub Actions workflow builds and publishes installers to GitHub
    Releases when a version tag is pushed.
  - *Done when:* pushing a tag produces a release with all installers.

## Spikes on the Pi

Short experiments that confirm the device choices before real effort goes
in. Run them alongside Phase 0. Each is throwaway code.

- [ ] **A. iced feel test** · 2–3 days
  - A track list scrolled by a knob (or the keyboard) and a spinning record,
    cross-compiled to the Pi.
  - *Answers:* is iced smooth enough, how much memory does it use, or should
    the UI use Flutter instead?

- [ ] **B. projectM test** · 2–3 days
  - projectM full-screen with a few presets, fed by a test track.
  - *Answers:* frame rate at 720p and at a reduced internal resolution, and
    which preset packs can legally ship.

- [ ] **C. Audio test** · 2–3 days
  - symphonia and cpal playing through PipeWire to a Bluetooth speaker.
  - *Answers:* delay, gaps between tracks, and dropouts.

## Phase 1: the Rust core

Produces exactly the same `Library` output as TypeScript on every fixture.

- [x] **1.1 Cargo workspace and CI** · 1 day · after 0.1
  - `crates/djusb-core`, with `cargo test`, `clippy`, `fmt` and `cargo-deny`
    (licence checks) in CI.
- [x] **1.2 Data model** · 1–2 days · after 0.3
  - Structs with `serde` (camelCase) matching the contract. Generate
    TypeScript types with `ts-rs`, and fail CI if they differ from
    `types.ts`.
- [x] **1.3 Path helpers** · 0.5 day · after 1.2
  - Port `paths.ts`, including the safe resolve-inside-volume check, with
    its tests.
- [x] **1.4 rekordbox `export.pdb`** · 3–5 days · after 1.3
  - Port `pdb.ts` and the playlist tree builder from `rekordbox/index.ts`.
  - *Done when:* passes the `export.pdb` fixtures.
- [x] **1.5 Serato** · 2–3 days · after 1.3
  - Port `chunks.ts`, the database and the crates.
  - *Done when:* passes the Serato fixtures.
- [x] **1.6 rekordbox Device Library Plus** · 2–4 days · after 1.4
  - Open the database with rusqlite's bundled SQLCipher, after copying it
    and its `-wal` file to a temporary folder. This replaces `sqlcipher.ts`.
  - *Done when:* passes the Device Library Plus fixtures.
- [x] **1.7 Library manager** · 2–3 days · after 1.4–1.6
  - The scan loop, adding a folder, and resolving tracks and artwork.
- [x] **1.8 Volume listing** · 2–3 days · after 1.7
  - Windows labels through `GetVolumeInformationW` (faster than the current
    PowerShell call), macOS `/Volumes`, Linux `/media`, `/run/media` and
    udisks2.
- [x] **1.9 Media helpers** · 2–3 days · after 1.3
  - Range parsing, AIFF as WAV, file types and the NFC/NFD retry. Port
    `range.test.ts`, `aiff.test.ts` and `paths.test.ts`.
- [x] **1.10 Shared logic** · 2–4 days · after 0.5
  - Sorting, `reconcileOrder`, Camelot keys, energy flow and harmonic
    matching, with their tests.

*Phase done when:* the Rust core passes every fixture and every ported test.

## Phase 2: desktop on the Rust core

Not needed for the wall player, so it can wait until after the device ships
if time is short.

- [x] **2.1 napi-rs bridge** · 3–5 days · after Phase 1
  - `crates/djusb-node` exposes the library manager and the media service to
    Electron's main process.
- [x] **2.2 Prebuilt binaries** · 2–4 days · after 2.1 and D.2
  - Build the bridge for Windows, macOS and Linux on x64 and ARM64 in the
    release workflow.
- [x] **2.3 Comparison run** · 2–3 days · after 2.2
  - Behind a setting, run both cores and log any difference on real USBs.
- [x] **2.4 Remove the TypeScript readers** · 1 day · after 2.3
  - Once no differences remain. The fixtures stay as the shared tests.

## Phase 3: device foundations

On the Pi, list a USB and play it.

- [ ] **3.1 App skeleton** · 2–3 days · after Phase 1 readers and spike A
  - `apps/device`, an iced app that also runs on Windows for development.
    Cross-compile for the Pi with `cross` or `cargo-zigbuild`.
- [ ] **3.2 Input layer** · 2–3 days · after 3.1
  - Knob events (browse turned, pressed and held; volume turned and
    pressed) from the keyboard during development and from GPIO through
    `gpiocdev` on the Pi, with debouncing and speed-up for fast turns.
- [ ] **3.3 Playback engine** · 6–10 days · after 3.1 and spike C
  - Decode with symphonia, play through cpal or PipeWire. Queue, seek,
    volume, and a copy of the audio for the visualizers through a ring
    buffer.
- [ ] **3.4 Player rules in the core** · 2–4 days · after 3.3
  - Move the desktop's queue behaviour from `usePlayer.ts` into
    `djusb-core`: "previous" restarts the track within 3 seconds, the queue
    reorders without interrupting playback, failed tracks are skipped.
- [ ] **3.5 Settings storage** · 1 day · after 0.7
  - Settings and custom orders in a JSON file, in the contract's format.

*Phase done when:* the Pi lists a USB and plays to wired or Bluetooth output.

## Phase 4: device UI

The full experience with two knobs and the desktop's look.

- [ ] **4.1 Design tokens** · 1–2 days · after 3.1
  - One tokens file for colours, fonts and spacing, generating CSS variables
    for the desktop and Rust constants for the device.
- [ ] **4.2 Screens** · 8–12 days · after 3.2 and 4.1
  - USB sources, playlists, tracks, now playing, visualizers, and settings
    (Bluetooth, volume limit, about and licences).
- [ ] **4.3 Turntable** · 4–6 days · after 4.2
  - Render the deck's static parts from the existing SVG to images with
    `resvg`. Animate the record and tonearm in iced, reusing the maths in
    `tonearmGeometry.ts` and `labelLayout.ts`.
- [ ] **4.4 Waveform and equalizer** · 3–5 days · after 3.3
  - Port `Waveform.tsx` and `equalizerBands.ts`, fed by the audio copy and
    `rustfft`.
- [ ] **4.5 MilkDrop** · 4–8 days · after spike B and 3.3
  - projectM as a separate full-screen mode at a reduced resolution, with
    preset pack choice. Follow the LGPL: shared library, notices, source
    offer.

*Phase done when:* a whole browse-and-play session works with only the knobs.

## Phase 5: appliance

- [ ] **5.1 System image** · 4–7 days · after Phase 3
  - Raspberry Pi OS Lite, the app as a system service under the cage kiosk,
    a boot splash, and a read-only system partition so pulling the power
    can't corrupt it.
- [ ] **5.2 USB auto-mount** · 2–3 days · after 5.1
  - udisks2, forced read-only, with FAT32, exFAT and HFS+.
- [ ] **5.3 Bluetooth** · 4–6 days · after 4.2
  - bluer: scan, pair, connect, reconnect automatically, set the speaker as
    the default output, and a pairing screen.
- [ ] **5.4 Licence notices** · 1–2 days · after Phase 4
  - A `cargo-deny` report and a generated notices screen.
- [ ] **5.5 Updates from a USB stick** · 3–5 days · after 5.1
  - Signed update files on a USB stick, installed to a second system
    partition so a failed update rolls back (for example with RAUC).

## Phase 6: desktop signing and automatic updates

Saved for last on purpose. It only depends on Track D, so it can be pulled
earlier whenever the install warnings start costing users.

- [ ] **6.1 Code signing** · 1–3 days · after D.2
  - macOS: create a Developer ID Application certificate in the existing
    Apple Developer account, then sign and notarize. macOS only lets signed
    apps update themselves.
  - Windows: a code-signing certificate or Azure Trusted Signing (about $10
    a month; check the eligibility rules for individuals). Getting approved
    can take days to weeks, so apply a little before starting this piece.
  - *Done when:* signed installers install without security warnings.

- [ ] **6.2 Signing in the release workflow** · 0.5–1 day · after 6.1
  - Add the certificates to GitHub Actions secrets and sign in the D.2
    workflow.
  - *Done when:* pushing a tag produces signed, notarized installers.

- [ ] **6.3 Automatic updates** · 2–3 days · after 6.2
  - electron-updater in the main process checks GitHub Releases at startup
    and every few hours, downloads in the background, and asks before
    restarting. Add a setting to turn update checks off.
  - Keep the renderer's Content-Security-Policy as is. The updater runs in
    the main process, so the UI itself stays offline.
  - *Done when:* an installed older version updates itself to a newer
    release on Windows and macOS.

## Estimates

| Part | Focused days |
| --- | --- |
| Phase 0: groundwork | 6–10 |
| Track D: desktop releases | 2–4 |
| Spikes A–C | 6–9 |
| Phase 1: Rust core | 18–30 |
| Phase 2: desktop on the Rust core | 8–13 |
| Phase 3: device foundations | 13–21 |
| Phase 4: device UI | 20–33 |
| Phase 5: appliance | 14–23 |
| Phase 6: desktop signing and automatic updates | 4–7, plus waiting for Windows signing approval |
| **Total** | **91–150** |

**Milestones:**

| Milestone | Pieces | Focused days |
| --- | --- | --- |
| Desktop app released (unsigned, manual updates) | 0.1 and Track D | about 3–5 |
| Device choices confirmed | Spikes A–C | about 6–9 |
| Wall player plays music from a USB | Phase 0, spikes, Phase 1, Phase 3 | about 43–70 |
| Wall player feature-complete | plus Phase 4 | about 63–103 |
| Wall player ready as an appliance | plus Phase 5 | about 77–126 |
| Desktop running on the shared core | plus Phase 2 | about 8–13 more |
| Desktop signed, with automatic updates | plus Phase 6 | about 4–7 more |

**In calendar time:**

- **Full-time** (about 5 focused days a week): roughly 4–7 months for
  everything.
- **Part-time** (about 2 focused days a week): roughly 11–18 months for
  everything, but the first desktop release lands in about 2–3 weeks and a
  music-playing wall player in about 5–8 months.

The ranges are wide because the biggest unknowns are learning Rust, the
playback engine, and how the Pi performs. The spikes narrow them, so revisit
the estimates after spikes A–C.

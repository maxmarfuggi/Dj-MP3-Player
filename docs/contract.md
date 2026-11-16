# Data contract

A reader turns a rekordbox or Serato volume into a `Library`. The desktop app
and the future Rust core must produce the same values. The types live in
`apps/desktop/src/shared/types.ts`. This document is the rules behind those
types: what each field means, how each id is built, and the one place a reader
is allowed to write.

Paths below are relative to the volume root and use `/` separators. A leading
`/` on a path stored in a library file is removed and the rest is kept as
written, including any `\` characters.

## Library

| Field | Meaning |
| --- | --- |
| `id` | Stable id for this library on this volume. See [Library id](#library-id). |
| `source` | Which export was read: `rekordbox`, `deviceLibraryPlus`, or `serato`. |
| `volumeName` | Name shown for the volume. For a mounted stick this is its label. For a folder the user picked, it is the folder's own name. |
| `volumePath` | Absolute path of the volume root, using the path string the operating system reported. This string is hashed as-is for the library id. |
| `tracks` | Every track the export lists, in the order that export yields them. Tracks are not sorted again. |
| `playlists` | The root of the playlist tree. Order inside each sibling list is defined per source below. |

One volume can hold more than one export. Each export is its own library, so a
stick with both rekordbox files produces two libraries that share `volumePath`
and differ in `source` and `id`.

## Track

| Field | Meaning |
| --- | --- |
| `id` | Id of the track inside its library. See [Track and playlist ids](#track-and-playlist-ids). |
| `title` | Title from the export. When that is missing or empty, the file name of `relativePath` without its extension. Only a `/` separates directories from the file name, so `Artist/Some.Track.mp3` has the title `Some.Track`. |
| `artist` | Artist name, or `""` when the export has none. |
| `album` | Album name, or `""` when the export has none. |
| `bpm` | Beats per minute as a number, or `null` when the export has none or zero. |
| `key` | Musical key as the export spells it, or `null` when it is missing or empty. |
| `durationSec` | Length in seconds, or `null` when the export has none or zero. rekordbox stores whole seconds. Serato can include a fraction. |
| `relativePath` | Audio file, relative to the volume root. |
| `genre` | Genre. Only Device Library Plus sets this, and it sets it even when the value is `""`. The other two sources leave it off. |
| `label` | Record label. Same rule as `genre`. |
| `color` | Track colour. Only Device Library Plus sets this, and only for the eight colours below. `name` is the name stored in the export when that is non-empty, otherwise the name in the table. `value` is always the CSS colour from the table. |
| `artworkPath` | Artwork file that exists on the volume, relative to its root. Set only when a file was found. See [Artwork](#artwork). |

Device Library Plus colours, by `color_id`:

| `color_id` | Name | `value` |
| --- | --- | --- |
| 1 | Pink | `#f870c8` |
| 2 | Red | `#f5423c` |
| 3 | Orange | `#ff9a2e` |
| 4 | Yellow | `#f2d93a` |
| 5 | Green | `#3fcf5c` |
| 6 | Aqua | `#2bd0d6` |
| 7 | Blue | `#3d6cff` |
| 8 | Purple | `#a65cff` |

Any other `color_id`, including 0, means the track has no colour.

## Playlist

| Field | Meaning |
| --- | --- |
| `id` | Id of the playlist or folder inside its library. |
| `name` | Display name. For a nested Serato crate this is the last segment only, not the names of its parents. |
| `kind` | `folder` or `playlist`. |
| `children` | Nested playlists, in the order defined for that source. |
| `trackIds` | Track ids in playlist order. An id that does not match a track is left out. |

rekordbox folders are `folder` and exist to hold other nodes. Serato has no
folders: every crate is `playlist`, and a crate can have both `trackIds` and
`children`. A rekordbox node keeps whatever track entries it has even when its
kind is `folder`.

## Library id

```
libraryId = source + "-" + hex(SHA-1(volumePath))[0:12]
```

`volumePath` is hashed as UTF-8, with no trimming and no change of slash or
case. The hex digits are lowercase. The first 12 characters of the digest are
used.

`rekordbox` on the path `/Volumes/GIGS` is `rekordbox-857074f2fb38`. The same
path read as Device Library Plus is `deviceLibraryPlus-857074f2fb38`, and as
Serato is `serato-857074f2fb38`.

## Track and playlist ids

**rekordbox Device Library** (`export.pdb`) and **Device Library Plus**
(`exportLibrary.db`) use the integer ids stored in the export, written in
decimal with no padding. Track `7` has id `"7"`. A playlist or folder uses its
own integer id the same way. The number `0` is reserved for the root of the
tree and is never a playlist id.

**Serato** uses the track's `relativePath` as its id, so the id and the path
are the same string. A crate's id is its full name, with parents joined by
`%%`: a crate file `House%%Deep.crate` has id `House%%Deep` and display name
`Deep`. A parent that has no crate file of its own still gets a node, and its
id is the name used so far (`House`, or `Techno` when the only file is
`Techno%%Hard.crate`).

## rekordbox Device Library

Present when `PIONEER/rekordbox/export.pdb` exists. Device Library Plus on the
same stick is a separate library and does not satisfy this check.

`bpm` is the stored tempo divided by 100 when the tempo is greater than 0, and
`null` otherwise. A tempo of `12850` is `128.5`. `durationSec` is the stored
whole seconds when greater than 0, and `null` otherwise. Artist, album, and
key come from the matching rows. A key id with no row, or a row whose name is
empty, becomes `null`.

Tracks stay in the order the file lists them. Playlist siblings are ordered by
their sort index, ascending, and a tie keeps the file order. Entries inside a
playlist are ordered by their entry index the same way. A node whose parent is
missing is placed at the root. Entries that point at a track the file does not
contain are dropped.

`genre`, `label`, `color`, and `artworkPath` are left off. Rows that cannot be
decoded are skipped, and the rest of the library is still returned.

## Device Library Plus

Present when `PIONEER/rekordbox/exportLibrary.db` exists. The file is a
SQLCipher 4 database. Every export uses this passphrase:

```
r8gddnr4k847830ar6cqzbkk0el6qytmb3trbbx805jm74vez64i5o8fnrqryqls
```

The playlist tree uses the same rules as the Device Library. A playlist row
with `attribute` `1` is a folder, and any other attribute is a playlist.
`sequenceNo` is the sort index of a playlist and the entry index of a track
in a playlist. Parent `0` is the root.

`bpm` is `bpmx100 / 100` when `bpmx100` is greater than 0. `durationSec` is
the stored length when it is greater than 0. Tracks stay in the order the
`content` query returns them, with no extra sort. `genre` and `label` are
always set, using `""` when the export has none.

### Artwork

The path in the database is only a hint. The file shown for that path is the
first of these names that exists in the same directory, compared without
regard to case:

1. The stored name with `_m` before the extension (`b1.jpg` becomes `b1_m.jpg`).
2. The stored name itself.
3. When the stored name starts with a lowercase `b`, those same two candidates
   with that `b` replaced by `a` (`a1_m.jpg`, then `a1.jpg`). A name that
   starts with `B` does not get this pair.

`artworkPath` is the directory plus the name as it appears on disk. A path
that would leave the volume is ignored, and so is a path with no matching file.

### Temporary copy

The volume is only read. `exportLibrary.db` and, when it is present,
`exportLibrary.db-wal` are read into memory and decrypted there. The plain
database is then written to a new directory in the operating system's
temporary folder. The directory name starts with `djusb-`, and the file inside
it is `exportLibrary.db`. That file is opened read-only, queried, and the
directory is deleted afterwards, including when the query fails. Bytes 18 and
19 of the plain file are set to `1` before it is opened, so SQLite does not
look for a write-ahead log beside the copy. Nothing is written next to the
files on the volume.

## Serato

Present when `_Serato_/database V2` exists, or when `_Serato_/Subcrates`
contains at least one `*.crate` file whose name does not start with `._`. An
empty `_Serato_` folder is not a library.

From each track in the database, the fields used are the path (`pfil`), title
(`tsng`), artist (`tart`), album (`talb`), bpm (`tbpm`), key (`tkey`), and
length (`tlen`). Text is UTF-16BE. A track with no path is skipped. A
truncated chunk at the end of the file is ignored.

`bpm` is the parsed number when it is greater than 0, and `null` otherwise.
`durationSec` is parsed from a colon-separated length such as `06:22.93`:
each segment is added after multiplying the total so far by 60. A result of 0,
or a segment that is not a number, becomes `null`. `06:22.93` is `382.93`
seconds.

Tracks are the database tracks in file order. A crate may name a file the
database does not list. That file is appended, with an empty artist and album
and null bpm, key, and duration. The first time a path is seen wins.

Crate files are read in order of file name, comparing UTF-16 code units, so
the result does not depend on how the file system lists them. Their names,
without `.crate`, are split on `%%` to build the tree. Sibling crates are then
sorted by name with a numeric, case-insensitive comparison in the `en` locale,
so `crate 9` comes before `crate 10`.

## Fixtures

`fixtures/` holds sample volumes and the output a reader must produce for
each. Any implementation of this contract runs the same files.

```
fixtures/<format>/<case>/volume/…        the files of a volume
fixtures/<format>/<case>/expected.json   what reading it produces
fixtures/aiff/<case>/input.aiff          an AIFF file
fixtures/aiff/<case>/expected.wav        the WAV it is served as
```

`<format>` is `rekordbox`, `deviceLibraryPlus`, or `serato`, after the
reader each case mainly checks. A case's volume is read the way the app reads
a mounted USB: every reader that detects its export runs, in the order
rekordbox, Device Library Plus, Serato. The volume name is the case folder's
name. `expected.json` is:

| Field | Meaning |
| --- | --- |
| `volumeName` | The case folder's name. |
| `libraries` | Each `Library` read, in reader order. `volumePath` is `{volume}` and the hash in `id` is `{volumeHash}`, for example `rekordbox-{volumeHash}`. Replace both with the real values before comparing, or compare with those two fields set to the placeholders. |
| `errors` | The sources whose export was detected but could not be read. Messages are not part of the contract. |

An AIFF case without `expected.wav` is one the converter must decline, such
as compressed AIFF-C.

`npm run fixtures` rebuilds every case from the test builders in
`apps/desktop/tests/helpers`. The same input always produces the same bytes.
`expected.json` and `expected.wav` come from the current TypeScript code, so
a change to how a library is read shows up as a diff in them, and that diff is
reviewed with the change.

`npm run fixtures -- capture <volume> <name>` copies the library files of a
real USB (the rekordbox databases, `PIONEER/Artwork`, and Serato's database
and crates, but no audio) into `fixtures/local/<name>`, then writes its
`expected.json`. The USB is only read. `fixtures/local/` is not committed,
because it holds the owner's track and playlist names. Its cases run in the
tests only on machines that have them.

## Read-only rule

Readers and the media server never create, change, or delete a file on a volume. The only write a reader performs is the temporary Device Library Plus copy described above, and that copy is outside the volume.

## Desktop state

These types are the desktop shell, not the export. A reader does not produce them.

`LibraryState` is the libraries currently loaded and the `LibraryLoadError`s from exports that were present but could not be read. An error records the volume path, volume name, source, and message. A format that is simply absent is not an error.

`OpenFolderResult` is the result of the user picking a folder: `opened`, `cancelled`, or `no-library` with the `folderPath` they picked.

The UI asks for audio with `media://track/<libraryId>/<trackId>` and for artwork with `media://artwork/<libraryId>/<trackId>`. Both ids are percent-encoded. The UI never sends a file path.

## Settings

Custom playlist orders and the other preferences are saved off the USB, as one JSON object. The desktop keeps the fields in `localStorage` for now. The device writes the same object to a file. Either way the volume is not touched.

```json
{
  "customPlaylistOrders": {
    "rekordbox-857074f2fb38/2": ["7", "8", "1"]
  },
  "energyFlowOpen": false
}
```

`customPlaylistOrders` maps a playlist to the track ids in the order the user arranged. The key is the library id, a slash, and the playlist id (`rekordbox-857074f2fb38/2`). Library ids have no slash, so the first slash is the split. The values are the track ids from this contract, including repeats, in saved order.

When the list is shown, ids that are no longer in the playlist are dropped and tracks the saved order does not mention are appended in the playlist's current order. Repeats are matched by count. The stored array itself is not rewritten until the user arranges that playlist again.

A missing `customPlaylistOrders`, or one that is not a JSON object, means there are no custom orders. An entry whose value is not an array of strings is ignored. Other entries are kept.

`energyFlowOpen` is whether the energy-flow chart is open. It is one preference for the whole app, not one per playlist. Missing or any value other than `true` means closed.

The desktop stores the two fields under those names. `customPlaylistOrders` is the object above as a JSON string. `energyFlowOpen` is the string `true` or `false`. A device file is the whole object, with `energyFlowOpen` as a boolean. Unknown fields are kept, so a newer version of the file still loads.

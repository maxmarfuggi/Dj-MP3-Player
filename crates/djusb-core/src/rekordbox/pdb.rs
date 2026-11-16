//! Reader for rekordbox's `export.pdb` (the DeviceSQL database that CDJs read).
//!
//! The layout follows the reverse-engineering documented by Deep Symmetry and
//! implemented in rekordcrate. Only the tables needed to browse and play tracks
//! are decoded.

use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdbTrack {
    pub id: u32,
    pub title: String,
    pub artist_id: u32,
    pub album_id: u32,
    pub key_id: u32,
    /// BPM multiplied by 100.
    pub tempo: u32,
    pub duration_sec: u32,
    /// Path relative to the volume root, usually starting with `/`.
    pub file_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdbNamedRow {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdbPlaylistNode {
    pub id: u32,
    /// 0 for nodes at the top level.
    pub parent_id: u32,
    pub sort_order: u32,
    pub is_folder: bool,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdbPlaylistEntry {
    pub playlist_id: u32,
    pub track_id: u32,
    pub entry_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdbData {
    pub tracks: Vec<PdbTrack>,
    pub artists: Vec<PdbNamedRow>,
    pub albums: Vec<PdbNamedRow>,
    pub keys: Vec<PdbNamedRow>,
    pub playlist_nodes: Vec<PdbPlaylistNode>,
    pub playlist_entries: Vec<PdbPlaylistEntry>,
    /// Rows that were marked present but could not be decoded.
    pub skipped_rows: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdbError {
    TooSmall,
    InvalidHeader,
}

impl std::fmt::Display for PdbError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooSmall => write!(
                formatter,
                "export.pdb is too small to be a rekordbox database"
            ),
            Self::InvalidHeader => write!(formatter, "export.pdb has an invalid header"),
        }
    }
}

impl std::error::Error for PdbError {}

const TABLE_TRACKS: u32 = 0;
const TABLE_ARTISTS: u32 = 2;
const TABLE_ALBUMS: u32 = 3;
const TABLE_KEYS: u32 = 5;
const TABLE_PLAYLIST_TREE: u32 = 7;
const TABLE_PLAYLIST_ENTRIES: u32 = 8;

const FILE_HEADER_SIZE: usize = 28;
const TABLE_POINTER_SIZE: usize = 16;
/// Row data ("heap") starts after the page header.
const PAGE_HEAP_START: usize = 40;
const PAGE_FLAG_INDEX: u8 = 0x40;
const ROWS_PER_GROUP: usize = 16;
/// 16 two-byte row offsets, a two-byte presence bitmask and two unknown bytes.
const ROW_GROUP_SIZE: usize = 36;

/// Long strings start with one of these bytes; short ASCII strings have the low bit set.
const STRING_LONG_ASCII: u8 = 0x40;
const STRING_LONG_UTF16LE: u8 = 0x90;

/// Row subtypes with this bit set store string offsets as 16-bit instead of 8-bit values.
const SUBTYPE_WIDE_OFFSETS: u16 = 0x04;

const TRACK_STRING_OFFSETS_START: usize = 94;
const TRACK_STRING_TITLE: usize = 17;
const TRACK_STRING_FILE_PATH: usize = 20;

struct TablePointer {
    type_id: u32,
    first_page: u32,
    last_page: u32,
}

#[derive(Debug)]
struct Bounds;

pub fn parse_pdb(file: &[u8]) -> Result<PdbData, PdbError> {
    if file.len() < FILE_HEADER_SIZE {
        return Err(PdbError::TooSmall);
    }
    let page_size = read_u32(file, 4).ok_or(PdbError::InvalidHeader)? as usize;
    let table_count = read_u32(file, 8).ok_or(PdbError::InvalidHeader)? as usize;
    let tables_end = FILE_HEADER_SIZE
        .checked_add(
            table_count
                .checked_mul(TABLE_POINTER_SIZE)
                .ok_or(PdbError::InvalidHeader)?,
        )
        .ok_or(PdbError::InvalidHeader)?;
    if page_size < PAGE_HEAP_START + ROW_GROUP_SIZE
        || tables_end > page_size
        || tables_end > file.len()
    {
        return Err(PdbError::InvalidHeader);
    }

    let mut tables = Vec::with_capacity(table_count);
    for index in 0..table_count {
        let at = FILE_HEADER_SIZE + index * TABLE_POINTER_SIZE;
        tables.push(TablePointer {
            type_id: read_u32(file, at).ok_or(PdbError::InvalidHeader)?,
            first_page: read_u32(file, at + 8).ok_or(PdbError::InvalidHeader)?,
            last_page: read_u32(file, at + 12).ok_or(PdbError::InvalidHeader)?,
        });
    }

    let mut skipped_rows = 0u32;
    Ok(PdbData {
        tracks: read_table(
            file,
            page_size,
            find_table(&tables, TABLE_TRACKS),
            decode_track,
            &mut skipped_rows,
        ),
        artists: read_table(
            file,
            page_size,
            find_table(&tables, TABLE_ARTISTS),
            decode_artist,
            &mut skipped_rows,
        ),
        albums: read_table(
            file,
            page_size,
            find_table(&tables, TABLE_ALBUMS),
            decode_album,
            &mut skipped_rows,
        ),
        keys: read_table(
            file,
            page_size,
            find_table(&tables, TABLE_KEYS),
            decode_key,
            &mut skipped_rows,
        ),
        playlist_nodes: read_table(
            file,
            page_size,
            find_table(&tables, TABLE_PLAYLIST_TREE),
            decode_playlist_node,
            &mut skipped_rows,
        ),
        playlist_entries: read_table(
            file,
            page_size,
            find_table(&tables, TABLE_PLAYLIST_ENTRIES),
            decode_playlist_entry,
            &mut skipped_rows,
        ),
        skipped_rows,
    })
}

/// The last pointer for a type wins, matching the order the bytes are read.
fn find_table(tables: &[TablePointer], type_id: u32) -> Option<&TablePointer> {
    tables.iter().rev().find(|table| table.type_id == type_id)
}

fn read_table<T>(
    file: &[u8],
    page_size: usize,
    table: Option<&TablePointer>,
    decode: fn(&Page<'_>, usize) -> Result<T, Bounds>,
    skipped_rows: &mut u32,
) -> Vec<T> {
    let Some(table) = table else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    let mut visited = HashSet::new();
    let mut page_index = table.first_page;

    // Pages of a table form a linked list. The last page's "next" pointer leads
    // past the end of the file, so the table pointer says where to stop.
    while visited.insert(page_index) {
        let Some(page) = page_bytes(file, page_size, page_index) else {
            break;
        };
        if read_u32(page, 8) != Some(table.type_id) {
            break;
        }
        let flags = page[27];
        if flags & PAGE_FLAG_INDEX == 0 {
            // Three bytes pack the number of row offsets (13 bits) and the number of
            // rows that are still present (11 bits).
            let row_offset_count = read_u24(page, 24)
                .map(|count| (count & 0x1fff) as usize)
                .unwrap_or(0);
            for row_index in 0..row_offset_count {
                match read_row(page, page_size, row_index, decode) {
                    Ok(Some(row)) => rows.push(row),
                    Ok(None) => {}
                    Err(Bounds) => *skipped_rows += 1,
                }
            }
        }
        if page_index == table.last_page {
            break;
        }
        let Some(next) = read_u32(page, 12) else {
            break;
        };
        page_index = next;
    }
    rows
}

fn page_bytes(file: &[u8], page_size: usize, page_index: u32) -> Option<&[u8]> {
    let start = (page_index as u64).checked_mul(page_size as u64)?;
    let end = start.checked_add(page_size as u64)?;
    if end > file.len() as u64 {
        return None;
    }
    Some(&file[start as usize..end as usize])
}

/// `Ok(None)` is a row whose presence bit is clear. `Err` is a corrupt row.
fn read_row<T>(
    page: &[u8],
    page_size: usize,
    row_index: usize,
    decode: fn(&Page<'_>, usize) -> Result<T, Bounds>,
) -> Result<Option<T>, Bounds> {
    let group_offset = (row_index / ROWS_PER_GROUP)
        .checked_mul(ROW_GROUP_SIZE)
        .filter(|offset| *offset < page_size)
        .ok_or(Bounds)?;
    let group_end = page_size - group_offset;
    let index_in_group = row_index % ROWS_PER_GROUP;
    let present_mask = read_u16(page, group_end.checked_sub(4).ok_or(Bounds)?)?;
    if present_mask & (1u16 << index_in_group) == 0 {
        return Ok(None);
    }
    let offset_at = group_end
        .checked_sub(6 + 2 * index_in_group)
        .ok_or(Bounds)?;
    let row_start = PAGE_HEAP_START + usize::from(read_u16(page, offset_at)?);
    decode(&Page { bytes: page }, row_start).map(Some)
}

struct Page<'a> {
    bytes: &'a [u8],
}

impl Page<'_> {
    fn u8(&self, at: usize) -> Result<u8, Bounds> {
        self.bytes.get(at).copied().ok_or(Bounds)
    }

    fn u16(&self, at: usize) -> Result<u16, Bounds> {
        read_u16(self.bytes, at)
    }

    fn u32(&self, at: usize) -> Result<u32, Bounds> {
        read_u32(self.bytes, at).ok_or(Bounds)
    }

    fn string(&self, at: usize) -> Result<String, Bounds> {
        let kind = self.u8(at)?;
        if kind & 1 != 0 {
            // Short ASCII: the remaining bits hold the length including this byte.
            let end = at + usize::from(kind >> 1);
            if end < at + 1 {
                return Err(Bounds);
            }
            return latin1(self.bytes.get(at + 1..end).ok_or(Bounds)?);
        }
        // Long strings: a 16-bit length that includes the 4-byte header.
        let end = at + usize::from(self.u16(at + 1)?);
        if end < at + 4 || end > self.bytes.len() {
            return Err(Bounds);
        }
        let body = &self.bytes[at + 4..end];
        if kind == STRING_LONG_ASCII {
            return latin1(body);
        }
        if kind == STRING_LONG_UTF16LE {
            return Ok(utf16le(body));
        }
        Ok(String::new())
    }
}

fn latin1(bytes: &[u8]) -> Result<String, Bounds> {
    Ok(bytes.iter().map(|byte| char::from(*byte)).collect())
}

fn utf16le(bytes: &[u8]) -> String {
    let (pairs, _) = bytes.as_chunks::<2>();
    let units: Vec<u16> = pairs.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
    String::from_utf16_lossy(&units)
}

fn decode_track(page: &Page<'_>, row: usize) -> Result<PdbTrack, Bounds> {
    let string_offset = |index: usize| {
        page.u16(row + TRACK_STRING_OFFSETS_START + 2 * index)
            .map(usize::from)
    };
    Ok(PdbTrack {
        key_id: page.u32(row + 32)?,
        tempo: page.u32(row + 56)?,
        album_id: page.u32(row + 64)?,
        artist_id: page.u32(row + 68)?,
        id: page.u32(row + 72)?,
        duration_sec: u32::from(page.u16(row + 84)?),
        title: page.string(row + string_offset(TRACK_STRING_TITLE)?)?,
        file_path: page.string(row + string_offset(TRACK_STRING_FILE_PATH)?)?,
    })
}

fn decode_artist(page: &Page<'_>, row: usize) -> Result<PdbNamedRow, Bounds> {
    let wide = page.u16(row)? & SUBTYPE_WIDE_OFFSETS != 0;
    let name_offset = if wide {
        usize::from(page.u16(row + 10)?)
    } else {
        usize::from(page.u8(row + 9)?)
    };
    Ok(PdbNamedRow {
        id: page.u32(row + 4)?,
        name: page.string(row + name_offset)?,
    })
}

fn decode_album(page: &Page<'_>, row: usize) -> Result<PdbNamedRow, Bounds> {
    let wide = page.u16(row)? & SUBTYPE_WIDE_OFFSETS != 0;
    let name_offset = if wide {
        usize::from(page.u16(row + 22)?)
    } else {
        usize::from(page.u8(row + 21)?)
    };
    Ok(PdbNamedRow {
        id: page.u32(row + 12)?,
        name: page.string(row + name_offset)?,
    })
}

fn decode_key(page: &Page<'_>, row: usize) -> Result<PdbNamedRow, Bounds> {
    Ok(PdbNamedRow {
        id: page.u32(row)?,
        name: page.string(row + 8)?,
    })
}

fn decode_playlist_node(page: &Page<'_>, row: usize) -> Result<PdbPlaylistNode, Bounds> {
    Ok(PdbPlaylistNode {
        parent_id: page.u32(row)?,
        sort_order: page.u32(row + 8)?,
        id: page.u32(row + 12)?,
        is_folder: page.u32(row + 16)? != 0,
        name: page.string(row + 20)?,
    })
}

fn decode_playlist_entry(page: &Page<'_>, row: usize) -> Result<PdbPlaylistEntry, Bounds> {
    Ok(PdbPlaylistEntry {
        entry_index: page.u32(row)?,
        track_id: page.u32(row + 4)?,
        playlist_id: page.u32(row + 8)?,
    })
}

fn read_u16(bytes: &[u8], at: usize) -> Result<u16, Bounds> {
    let end = at.checked_add(2).ok_or(Bounds)?;
    let slice = bytes.get(at..end).ok_or(Bounds)?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u24(bytes: &[u8], at: usize) -> Option<u32> {
    let slice = bytes.get(at..at + 3)?;
    Some(u32::from(slice[0]) | (u32::from(slice[1]) << 8) | (u32::from(slice[2]) << 16))
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let slice = bytes.get(at..at + 4)?;
    Some(u32::from_le_bytes(slice.try_into().ok()?))
}

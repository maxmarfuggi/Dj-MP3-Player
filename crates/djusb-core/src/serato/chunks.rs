//! Serato's `database V2` and `.crate` files are sequences of chunks: a 4-byte
//! tag, a 4-byte big-endian length, then the data. Tags starting with `o` hold
//! nested chunks; `t` and `p` tags hold UTF-16BE text.

const CHUNK_HEADER_SIZE: usize = 8;

pub struct Chunk<'a> {
    pub tag: [u8; 4],
    pub data: &'a [u8],
}

/// Yields the chunks in `buffer`, stopping at the first truncated one.
pub fn read_chunks(buffer: &[u8]) -> Chunks<'_> {
    Chunks { buffer, at: 0 }
}

pub struct Chunks<'a> {
    buffer: &'a [u8],
    at: usize,
}

impl<'a> Iterator for Chunks<'a> {
    type Item = Chunk<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.at + CHUNK_HEADER_SIZE > self.buffer.len() {
            return None;
        }
        let tag = self.buffer[self.at..self.at + 4].try_into().unwrap();
        let length =
            u32::from_be_bytes(self.buffer[self.at + 4..self.at + 8].try_into().unwrap()) as usize;
        let start = self.at + CHUNK_HEADER_SIZE;
        let end = start.checked_add(length)?;
        if end > self.buffer.len() {
            return None;
        }
        self.at = end;
        Some(Chunk {
            tag,
            data: &self.buffer[start..end],
        })
    }
}

pub fn decode_text(data: &[u8]) -> String {
    let even = &data[..data.len() & !1];
    let (pairs, _) = even.as_chunks::<2>();
    let units: Vec<u16> = pairs.iter().map(|pair| u16::from_be_bytes(*pair)).collect();
    String::from_utf16_lossy(&units)
}

pub fn tag_is(tag: [u8; 4], name: &str) -> bool {
    tag == name.as_bytes()
}

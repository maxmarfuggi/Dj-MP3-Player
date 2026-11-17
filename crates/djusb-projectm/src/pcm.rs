//! How much audio is handed to projectM in one call.
//!
//! projectM stores a fixed number of frames per channel and drops the rest.
//! A slow reader also keeps only the newest frames, so a stall does not make
//! the next picture wait on old audio.

use std::ops::Range;

/// Newest frames kept when the visualizer is behind the output.
pub const BACKLOG_FRAMES: usize = 4096;

/// Splits `frames` into pieces no longer than `max_frames`.
///
/// An empty buffer, or a library that reports no room, produces no pieces.
#[must_use]
pub fn frame_chunks(frames: usize, max_frames: usize) -> Vec<Range<usize>> {
    if frames == 0 || max_frames == 0 {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < frames {
        let end = (start + max_frames).min(frames);
        chunks.push(start..end);
        start = end;
    }
    chunks
}

/// Keeps the newest `max_frames` stereo frames and drops a trailing odd sample.
pub fn tail_frames(samples: &mut Vec<f32>, max_frames: usize) {
    let frames = samples.len() / 2;
    samples.truncate(frames * 2);
    if max_frames == 0 {
        samples.clear();
        return;
    }
    if frames > max_frames {
        let start = (frames - max_frames) * 2;
        samples.drain(..start);
    }
}

#[cfg(test)]
mod tests {
    use super::{frame_chunks, tail_frames};

    #[test]
    fn chunks_cover_the_buffer_without_passing_the_cap() {
        assert_eq!(frame_chunks(10, 4), vec![0..4, 4..8, 8..10]);
    }

    #[test]
    fn nothing_to_send_produces_no_chunks() {
        assert!(frame_chunks(0, 4).is_empty());
        assert!(frame_chunks(10, 0).is_empty());
    }

    #[test]
    fn a_short_buffer_is_one_chunk() {
        assert_eq!(frame_chunks(3, 8), vec![0..3]);
    }

    #[test]
    fn the_tail_keeps_the_newest_frames_and_drops_a_stray_sample() {
        let mut samples = vec![1.0, 1.0, 2.0, 2.0, 3.0, 3.0, 4.0];
        tail_frames(&mut samples, 2);
        assert_eq!(samples, vec![2.0, 2.0, 3.0, 3.0]);
    }

    #[test]
    fn no_room_clears_the_buffer() {
        let mut samples = vec![1.0, 1.0];
        tail_frames(&mut samples, 0);
        assert!(samples.is_empty());
    }
}

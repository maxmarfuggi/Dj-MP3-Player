//! projectM, opened at runtime as a shared library.
//!
//! The player does not link projectM into its binary. The library file can be
//! replaced, which is what the LGPL asks for. [`licence_lines`] is the notice
//! and the written offer of source.

mod api;
mod cycle;
mod files;
mod gl;
mod notice;
mod pcm;
mod pixels;
mod show;
mod size;

pub use files::{discover_packs, library_names, Pack};
pub use notice::licence_lines;
pub use show::{Frame, Show};
pub use size::render_size;

/// How long one preset stays up before the next blends in.
/// The desktop MilkDrop uses the same 25 seconds.
pub const PRESET_SECONDS: f64 = 25.0;

/// The blend between two presets. The desktop uses the same 2.7 seconds.
pub const BLEND_SECONDS: f64 = 2.7;

/// Frames drawn each second. Half of a usual display rate, to spare the Pi.
pub const FRAME_RATE: i32 = 30;

/// Shown when the shared library is not next to the player and not on the system path.
pub const MISSING_LIBRARY: &str =
    "projectM is not installed. Place the shared library next to the player.";

/// Shown when the chosen pack has no presets left in it.
pub const EMPTY_PACK: &str = "This pack has no .milk presets.";

#[cfg(test)]
mod tests {
    use super::{BLEND_SECONDS, FRAME_RATE, PRESET_SECONDS};

    #[test]
    fn preset_timing_matches_the_desktop_player() {
        assert_eq!(PRESET_SECONDS, 25.0);
        assert_eq!(BLEND_SECONDS, 2.7);
        assert_eq!(FRAME_RATE, 30);
    }
}

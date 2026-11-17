//! The size projectM draws at.
//!
//! The panel is 1280×720. Drawing at half of that, then letting the window
//! scale the picture up, is the reduced resolution the wall player uses.

/// The panel is divided by this on each edge.
pub const RENDER_DIVISOR: u32 = 2;

/// Half of `width` and `height`, and at least one pixel on each edge.
#[must_use]
pub fn render_size(width: u32, height: u32) -> (u32, u32) {
    (
        (width / RENDER_DIVISOR).max(1),
        (height / RENDER_DIVISOR).max(1),
    )
}

#[cfg(test)]
mod tests {
    use super::render_size;

    #[test]
    fn the_panel_is_drawn_at_half_resolution() {
        assert_eq!(render_size(1280, 720), (640, 360));
    }

    #[test]
    fn a_tiny_surface_stays_one_pixel() {
        assert_eq!(render_size(0, 0), (1, 1));
        assert_eq!(render_size(1, 1), (1, 1));
    }

    #[test]
    fn odd_edges_round_down() {
        assert_eq!(render_size(3, 5), (1, 2));
    }
}

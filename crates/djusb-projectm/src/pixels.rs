//! OpenGL reads the bottom row first. The window draws the top row first.

/// Turns a bottom-up RGBA buffer upright. `false` means the buffer is the
/// wrong length and was left alone.
#[must_use]
pub fn flip_rows(width: usize, height: usize, pixels: &mut [u8]) -> bool {
    let Some(stride) = width.checked_mul(4) else {
        return false;
    };
    let Some(bytes) = stride.checked_mul(height) else {
        return false;
    };
    if stride == 0 || pixels.len() != bytes {
        return false;
    }
    let mut row = vec![0; stride];
    for y in 0..height / 2 {
        let top = y * stride;
        let bottom = (height - 1 - y) * stride;
        row.copy_from_slice(&pixels[top..top + stride]);
        pixels.copy_within(bottom..bottom + stride, top);
        pixels[bottom..bottom + stride].copy_from_slice(&row);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::flip_rows;

    #[test]
    fn the_bottom_row_becomes_the_top_row() {
        let mut pixels = vec![1, 1, 1, 1, 2, 2, 2, 2];
        assert!(flip_rows(1, 2, &mut pixels));
        assert_eq!(pixels, vec![2, 2, 2, 2, 1, 1, 1, 1]);
    }

    #[test]
    fn a_short_buffer_is_left_alone() {
        let mut pixels = vec![1, 2, 3];
        assert!(!flip_rows(1, 1, &mut pixels));
        assert_eq!(pixels, vec![1, 2, 3]);
    }
}

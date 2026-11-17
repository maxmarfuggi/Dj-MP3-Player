//! The colour of the wall display.
//!
//! The fixed colours follow the Kenwood Excelon KMM-X705 illumination wheel:
//! red through purple, then back. Variable scan is that wheel as one slow
//! loop, easing from each colour into the next. Green is the phosphor the
//! panel already used, and it is the colour a missing setting keeps.

use std::time::Duration;

use iced::Color;

/// One full turn of variable scan. Long enough that the change reads as ambient.
pub const CYCLE: Duration = Duration::from_secs(90);

/// Saved id for the flowing colour.
pub const SCAN: &str = "scan";

/// Saved id for the original phosphor.
pub const GREEN_ID: &str = "green";

/// One colour the platter can land on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Swatch {
    pub id: &'static str,
    pub label: &'static str,
}

/// Phosphor on black: the bright line, the quieter line, and the empty well.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glow {
    pub foreground: Color,
    pub dim: Color,
    pub well: Color,
}

/// The phosphor the panel shipped with.
pub const GREEN: Glow = Glow {
    foreground: Color::from_rgb8(0x5c, 0xff, 0x6a),
    dim: Color::from_rgb8(0x1f, 0x9a, 0x40),
    well: Color::from_rgb8(0x04, 0x14, 0x08),
};

struct Stop {
    id: &'static str,
    label: &'static str,
    hue: f32,
    /// Use [`GREEN`] rather than the hue, so the default is unchanged.
    exact: bool,
}

/// Variable scan first, then the wheel from red back toward red.
const STOPS: &[Stop] = &[
    Stop {
        id: "red",
        label: "Red",
        hue: 2.0,
        exact: false,
    },
    Stop {
        id: "orange",
        label: "Orange",
        hue: 22.0,
        exact: false,
    },
    Stop {
        id: "amber",
        label: "Amber",
        hue: 38.0,
        exact: false,
    },
    Stop {
        id: "yellow",
        label: "Yellow",
        hue: 52.0,
        exact: false,
    },
    Stop {
        id: "yellow-green",
        label: "Yellow green",
        hue: 88.0,
        exact: false,
    },
    Stop {
        id: GREEN_ID,
        label: "Green",
        hue: 125.0,
        exact: true,
    },
    Stop {
        id: "mint",
        label: "Mint",
        hue: 158.0,
        exact: false,
    },
    Stop {
        id: "aqua",
        label: "Aqua",
        hue: 174.0,
        exact: false,
    },
    Stop {
        id: "light-blue",
        label: "Light blue",
        hue: 192.0,
        exact: false,
    },
    Stop {
        id: "sky-blue",
        label: "Sky blue",
        hue: 206.0,
        exact: false,
    },
    Stop {
        id: "blue",
        label: "Blue",
        hue: 224.0,
        exact: false,
    },
    Stop {
        id: "violet",
        label: "Violet",
        hue: 258.0,
        exact: false,
    },
    Stop {
        id: "purple",
        label: "Purple",
        hue: 286.0,
        exact: false,
    },
    Stop {
        id: "rose",
        label: "Rose",
        hue: 332.0,
        exact: false,
    },
];

/// The rows on the colour screen, scan first.
#[must_use]
pub fn swatches() -> Vec<Swatch> {
    let mut rows = Vec::with_capacity(STOPS.len() + 1);
    rows.push(Swatch {
        id: SCAN,
        label: "Variable scan",
    });
    rows.extend(STOPS.iter().map(|stop| Swatch {
        id: stop.id,
        label: stop.label,
    }));
    rows
}

/// The name shown beside Color. An unknown id reads as Green.
#[must_use]
pub fn label(id: &str) -> &'static str {
    swatches()
        .iter()
        .find(|swatch| swatch.id == id)
        .map(|swatch| swatch.label)
        .unwrap_or("Green")
}

/// Where `id` sits in [`swatches`]. Unknown ids land on Green.
#[must_use]
pub fn index(id: &str) -> usize {
    swatches()
        .iter()
        .position(|swatch| swatch.id == id)
        .unwrap_or_else(|| {
            swatches()
                .iter()
                .position(|swatch| swatch.id == GREEN_ID)
                .unwrap_or(1)
        })
}

/// The colours to paint. `at` only moves variable scan.
#[must_use]
pub fn glow(id: &str, at: Duration) -> Glow {
    if id == SCAN {
        return scan(at);
    }
    STOPS
        .iter()
        .find(|stop| stop.id == id)
        .map(stop_glow)
        .unwrap_or(GREEN)
}

fn scan(at: Duration) -> Glow {
    let turns = at.as_secs_f64() / CYCLE.as_secs_f64();
    let scaled = turns.fract() * STOPS.len() as f64;
    let slot = scaled.floor() as usize % STOPS.len();
    let next = (slot + 1) % STOPS.len();
    let along = smooth(scaled.fract() as f32);
    mix(stop_glow(&STOPS[slot]), stop_glow(&STOPS[next]), along)
}

fn stop_glow(stop: &Stop) -> Glow {
    if stop.exact {
        GREEN
    } else {
        shade(stop.hue)
    }
}

/// A bright LED of `hue`, with a quieter line and a dark well of the same hue.
fn shade(hue: f32) -> Glow {
    Glow {
        foreground: color(hue, 1.0, 0.58),
        dim: color(hue, 0.72, 0.34),
        well: color(hue, 0.7, 0.05),
    }
}

fn color(hue: f32, saturation: f32, lightness: f32) -> Color {
    let (red, green, blue) = hsl(hue, saturation, lightness);
    Color::from_rgb(red, green, blue)
}

fn hsl(hue: f32, saturation: f32, lightness: f32) -> (f32, f32, f32) {
    let sector = hue.rem_euclid(360.0) / 60.0;
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let across = chroma * (1.0 - ((sector % 2.0) - 1.0).abs());
    let (red, green, blue) = match sector as u8 {
        0 => (chroma, across, 0.0),
        1 => (across, chroma, 0.0),
        2 => (0.0, chroma, across),
        3 => (0.0, across, chroma),
        4 => (across, 0.0, chroma),
        _ => (chroma, 0.0, across),
    };
    let match_light = lightness - chroma / 2.0;
    (red + match_light, green + match_light, blue + match_light)
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(from: Glow, to: Glow, t: f32) -> Glow {
    Glow {
        foreground: mix_color(from.foreground, to.foreground, t),
        dim: mix_color(from.dim, to.dim, t),
        well: mix_color(from.well, to.well, t),
    }
}

fn mix_color(from: Color, to: Color, t: f32) -> Color {
    Color {
        r: from.r + (to.r - from.r) * t,
        g: from.g + (to.g - from.g) * t,
        b: from.b + (to.b - from.b) * t,
        a: 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::{glow, index, label, swatches, CYCLE, GREEN, GREEN_ID, SCAN};
    use std::time::Duration;

    #[test]
    fn green_is_the_original_phosphor() {
        let lit = glow(GREEN_ID, Duration::from_secs(10));
        assert_eq!(lit, GREEN);
        assert_eq!(glow("no-such-color", Duration::ZERO), GREEN);
        assert_eq!(label("no-such-color"), "Green");
    }

    #[test]
    fn variable_scan_is_the_first_row_and_loops() {
        assert_eq!(swatches()[0].id, SCAN);
        assert_eq!(label(SCAN), "Variable scan");
        assert_eq!(index(SCAN), 0);
        assert_eq!(index(GREEN_ID), index("green"));
        let start = glow(SCAN, Duration::ZERO);
        let red = glow("red", Duration::ZERO);
        assert_eq!(start.foreground, red.foreground);
        assert_eq!(glow(SCAN, CYCLE).foreground, red.foreground);
        let one_stop = CYCLE / u32::try_from(swatches().len() - 1).unwrap();
        assert_eq!(
            glow(SCAN, one_stop).foreground,
            glow("orange", Duration::ZERO).foreground
        );
    }

    #[test]
    fn a_quarter_second_barely_moves_the_scan() {
        let start = glow(SCAN, Duration::ZERO).foreground;
        let later = glow(SCAN, Duration::from_millis(250)).foreground;
        let drift =
            (start.r - later.r).abs() + (start.g - later.g).abs() + (start.b - later.b).abs();
        assert!(drift < 0.05, "{drift}");
        assert!(drift > 0.0, "the scan should not be frozen");
    }
}

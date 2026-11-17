//! Platter turn and select-key press.
//!
//! One slow detent moves one row that does something. A row that does nothing
//! is drawn, but the platter skips it. Detents at least 80 ms apart stay at
//! one row; faster detents move two, four, or eight rows. The round key, and
//! Enter during development, press the highlighted row. The pinned row is
//! one detent above the first item and stays on screen while the list scrolls.
//! A track list adds Sort on the title line, between the pinned row and the
//! first track. That control stays put while the tracks scroll.
//! When a screen offers the playing line, it is one detent below the last
//! item and stays at the bottom of the screen.
//!
//! On the Pi, CLK is GPIO 17, DT is GPIO 27, and the round key is GPIO 25,
//! wired to ground. The encoder's own shaft switch is left unwired.
//!
//! The platter reader is compiled only on Linux, so the GPIO pieces of this
//! module are unused when the same app is built on Windows for development.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::time::Duration;

/// What the platter or the select key did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// Rows to move. Positive moves down the list, negative moves up.
    Turn(i32),
    /// The highlighted row was pressed.
    Press,
}

/// Development keys. Arrow keys turn and Enter presses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevKey {
    /// Up or Left.
    Up,
    /// Down or Right.
    Down,
    /// Enter.
    Press,
}

/// Encoder CLK. Header pin 11.
pub(crate) const PLATTER_CLK: u32 = 17;
/// Encoder DT. Header pin 13.
pub(crate) const PLATTER_DT: u32 = 27;
/// Round key, wired to ground. Header pin 22.
pub(crate) const SELECT_KEY: u32 = 25;

/// Which line changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Line {
    Clk,
    Dt,
    Key,
}

/// Applies one edge. `rose` is a change from low to high.
pub(crate) fn apply_level_change(levels: &mut Levels, line: Line, rose: bool) {
    match line {
        Line::Clk => levels.clk_high = rose,
        Line::Dt => levels.dt_high = rose,
        Line::Key => levels.switch_pressed = !rose,
    }
}

/// Levels of the platter encoder and the round key.
///
/// The encoder rests with both lines high. The key is wired to ground, so it
/// is pressed when its line is low.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Levels {
    pub clk_high: bool,
    pub dt_high: bool,
    pub switch_pressed: bool,
}

/// Turns encoder edges and key levels into [`Control`]s.
#[derive(Debug, Clone)]
pub struct DeviceInput {
    quadrature: Quadrature,
    switch: Switch,
    platter: Accelerator,
    keys: Accelerator,
    seeded: bool,
}

impl DeviceInput {
    #[must_use]
    pub fn new() -> Self {
        Self {
            quadrature: Quadrature::new(true, true),
            switch: Switch::new(false),
            platter: Accelerator::new(),
            keys: Accelerator::new(),
            seeded: false,
        }
    }

    /// Records the lines at startup. The first reading is not a turn or a press.
    pub fn seed(&mut self, levels: Levels) {
        self.quadrature = Quadrature::new(levels.clk_high, levels.dt_high);
        self.switch = Switch::new(levels.switch_pressed);
        self.seeded = true;
    }

    /// `at` is the time since this input started, on one clock.
    pub fn sample(&mut self, levels: Levels, at: Duration) -> Vec<Control> {
        if !self.seeded {
            self.seed(levels);
            return Vec::new();
        }
        let mut controls = Vec::new();
        if let Some(detent) = self.quadrature.update(levels.clk_high, levels.dt_high) {
            controls.push(Control::Turn(self.platter.scale(detent, at)));
        }
        if let Some(press) = self.switch.update(levels.switch_pressed, at) {
            controls.push(press);
        }
        controls
    }

    /// One development key. [`dev_key`] drops a repeating Enter before this is called.
    #[must_use]
    pub fn key(&mut self, key: DevKey, at: Duration) -> Option<Control> {
        match key {
            DevKey::Up => Some(Control::Turn(self.keys.scale(-1, at))),
            DevKey::Down => Some(Control::Turn(self.keys.scale(1, at))),
            DevKey::Press => Some(Control::Press),
        }
    }
}

impl Default for DeviceInput {
    fn default() -> Self {
        Self::new()
    }
}

/// Maps a development key. `repeat` is true when the operating system is
/// repeating a held key.
#[must_use]
pub fn dev_key(key: &iced::keyboard::Key, repeat: bool) -> Option<DevKey> {
    use iced::keyboard::key::Named;
    use iced::keyboard::Key;

    let dev = match key {
        Key::Named(Named::ArrowUp | Named::ArrowLeft) => DevKey::Up,
        Key::Named(Named::ArrowDown | Named::ArrowRight) => DevKey::Down,
        Key::Named(Named::Enter) => DevKey::Press,
        _ => return None,
    };
    if repeat && dev == DevKey::Press {
        None
    } else {
        Some(dev)
    }
}

/// Quadrature decoder. Four valid transitions in one direction are one detent.
#[derive(Debug, Clone)]
struct Quadrature {
    state: u8,
    accumulator: i8,
}

impl Quadrature {
    fn new(clk_high: bool, dt_high: bool) -> Self {
        Self {
            state: bits(clk_high, dt_high),
            accumulator: 0,
        }
    }

    fn update(&mut self, clk_high: bool, dt_high: bool) -> Option<i8> {
        let next = bits(clk_high, dt_high);
        let quarter = QUARTER[usize::from((self.state << 2) | next)];
        self.state = next;
        if quarter == 0 {
            return None;
        }
        self.accumulator += quarter;
        if self.accumulator.abs() < 4 {
            return None;
        }
        let detent = self.accumulator.signum();
        self.accumulator = 0;
        Some(detent)
    }
}

/// One quarter-step for `(previous << 2) | next`. A full detent is four of these.
const QUARTER: [i8; 16] = [
    0, -1, 1, 0, // 00 -> 00, 01, 10, 11
    1, 0, 0, -1, // 01
    -1, 0, 0, 1, // 10
    0, 1, -1, 0, // 11
];

fn bits(clk_high: bool, dt_high: bool) -> u8 {
    (u8::from(clk_high) << 1) | u8::from(dt_high)
}

/// How long the round key must stay down before it counts as a press.
const DEBOUNCE: Duration = Duration::from_millis(20);

/// The round key. A press is reported once it has stayed down.
#[derive(Debug, Clone)]
struct Switch {
    pressed: bool,
    since: Duration,
    emitted: bool,
}

impl Switch {
    fn new(pressed: bool) -> Self {
        Self {
            pressed,
            // A key already held when reading starts is not a new press.
            since: Duration::ZERO,
            emitted: pressed,
        }
    }

    fn update(&mut self, pressed: bool, at: Duration) -> Option<Control> {
        if pressed != self.pressed {
            self.pressed = pressed;
            self.since = at;
            self.emitted = false;
            return None;
        }
        if pressed && !self.emitted && at.saturating_sub(self.since) >= DEBOUNCE {
            self.emitted = true;
            return Some(Control::Press);
        }
        None
    }
}

/// At this gap, and anything slower, one detent is one row.
const SLOW_TURN: Duration = Duration::from_millis(80);
/// Detents closer than [`SLOW_TURN`] skip two rows.
const MEDIUM_TURN: Duration = Duration::from_millis(40);
/// Detents closer than [`MEDIUM_TURN`] skip four rows. Closer than this skips eight.
const FAST_TURN: Duration = Duration::from_millis(15);

/// A slow detent is one row. Detents that follow quickly skip more rows.
#[derive(Debug, Clone)]
struct Accelerator {
    last_sign: i8,
    last_at: Option<Duration>,
}

impl Accelerator {
    fn new() -> Self {
        Self {
            last_sign: 0,
            last_at: None,
        }
    }

    fn scale(&mut self, detent: i8, at: Duration) -> i32 {
        let sign = detent.signum();
        if sign == 0 {
            return 0;
        }
        let steps = match self.last_at {
            Some(then) if self.last_sign == sign => rows_for(at.saturating_sub(then)),
            _ => 1,
        };
        self.last_sign = sign;
        self.last_at = Some(at);
        i32::from(sign) * steps
    }
}

fn rows_for(gap: Duration) -> i32 {
    if gap >= SLOW_TURN {
        1
    } else if gap >= MEDIUM_TURN {
        2
    } else if gap >= FAST_TURN {
        4
    } else {
        8
    }
}

/// The highlighted row. Index 0 is the pinned top row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Highlight {
    index: usize,
    scroll: usize,
}

/// One row to draw. The pinned row is always first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Pinned {
        selected: bool,
    },
    /// Sort, drawn on the title line. It does not scroll with the tracks.
    Sort {
        selected: bool,
    },
    Item {
        index: usize,
        selected: bool,
    },
}

impl Highlight {
    /// Starts on the first item, or on the pinned row when the list is empty.
    #[cfg(test)]
    #[must_use]
    pub fn new(item_count: usize) -> Self {
        Self {
            index: usize::from(item_count > 0),
            scroll: 0,
        }
    }

    /// Starts on the pinned row. A level screen uses this: the platter changes
    /// the value, and the highlighted row is the one that leaves.
    #[must_use]
    pub fn pinned() -> Self {
        Self {
            index: 0,
            scroll: 0,
        }
    }

    #[must_use]
    pub fn index(self) -> usize {
        self.index
    }

    /// `footer` adds one stop after the last item. That stop is not one of
    /// the scrolling rows; the screen draws it under the list.
    #[cfg(test)]
    pub fn move_by(&mut self, steps: i32, item_count: usize, visible_items: usize, footer: bool) {
        self.move_skipping(steps, item_count, visible_items, footer, false, |_| false);
    }

    /// Moves by `steps`. Dead rows are jumped and never highlighted.
    ///
    /// `dead` sees the highlight index. The pinned row is 0. Item `n` is
    /// `n + 1`, or `n + 2` when `sort` is set. `sort` is the title-line
    /// control: index 1, drawn above the tracks, and it does not scroll.
    /// One step lands on the next row that does something. `footer` adds one
    /// stop after the last item. That stop is not one of the scrolling rows;
    /// the screen draws it under the list.
    pub fn move_skipping(
        &mut self,
        steps: i32,
        item_count: usize,
        visible_items: usize,
        footer: bool,
        sort: bool,
        dead: impl Fn(usize) -> bool,
    ) {
        let last = item_count + usize::from(sort) + usize::from(footer);
        let direction = steps.signum();
        let mut left = steps.unsigned_abs();
        while left > 0 {
            match next_live(self.index, direction, last, &dead) {
                Some(next) => self.index = next,
                None => break,
            }
            left -= 1;
        }
        self.reveal(item_count, visible_items, sort);
    }

    /// Keeps the highlight inside a list that shrank, and off a row that does nothing.
    #[cfg(test)]
    pub fn fit(&mut self, item_count: usize, visible_items: usize, footer: bool) {
        self.fit_live(item_count, visible_items, footer, false, |_| false);
    }

    /// Keeps the highlight inside a list that shrank.
    ///
    /// A dead row moves to the next live row, then the previous one.
    /// `sort` matches [`Self::move_skipping`].
    pub fn fit_live(
        &mut self,
        item_count: usize,
        visible_items: usize,
        footer: bool,
        sort: bool,
        dead: impl Fn(usize) -> bool,
    ) {
        let last = item_count + usize::from(sort) + usize::from(footer);
        if self.index > last {
            self.index = last;
        }
        if dead(self.index) {
            if let Some(next) = next_live(self.index, 1, last, &dead) {
                self.index = next;
            } else if let Some(prev) = next_live(self.index, -1, last, &dead) {
                self.index = prev;
            }
        }
        self.reveal(item_count, visible_items, sort);
    }

    /// The highlight is on the playing line under the list.
    #[must_use]
    pub fn on_footer(self, item_count: usize, sort: bool) -> bool {
        self.index == item_count + 1 + usize::from(sort)
    }

    fn reveal(&mut self, item_count: usize, visible_items: usize, sort: bool) {
        if item_count == 0 {
            self.scroll = 0;
            return;
        }
        let visible = visible_items.max(1);
        let shift = 1 + usize::from(sort);
        let last_item = item_count + usize::from(sort);
        if (shift..=last_item).contains(&self.index) {
            let item = self.index - shift;
            if item < self.scroll {
                self.scroll = item;
            } else if item >= self.scroll.saturating_add(visible) {
                self.scroll = item + 1 - visible;
            }
        } else if self.index > last_item {
            self.scroll = item_count.saturating_sub(visible);
        }
        let max_scroll = item_count.saturating_sub(visible);
        self.scroll = self.scroll.min(max_scroll);
    }

    #[must_use]
    pub fn rows(self, item_count: usize, visible_items: usize, sort: bool) -> Vec<Row> {
        let mut rows = vec![Row::Pinned {
            selected: self.index == 0,
        }];
        if sort {
            rows.push(Row::Sort {
                selected: self.index == 1,
            });
        }
        if item_count == 0 {
            return rows;
        }
        let visible = visible_items.max(1);
        let end = (self.scroll + visible).min(item_count);
        let shift = 1 + usize::from(sort);
        for index in self.scroll..end {
            rows.push(Row::Item {
                index,
                selected: self.index == index + shift,
            });
        }
        rows
    }
}

/// The next index in `direction` that `dead` rejects, within `0..=last`.
fn next_live(
    index: usize,
    direction: i32,
    last: usize,
    dead: &impl Fn(usize) -> bool,
) -> Option<usize> {
    if direction == 0 {
        return None;
    }
    let last = i64::try_from(last).unwrap_or(i64::MAX);
    let mut cursor = i64::try_from(index).unwrap_or(0) + i64::from(direction);
    while (0..=last).contains(&cursor) {
        let at = usize::try_from(cursor).unwrap_or(0);
        if !dead(at) {
            return Some(at);
        }
        cursor += i64::from(direction);
    }
    None
}

/// Moves the highlight, or reports which row was pressed.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    Moved(usize),
    Pressed(usize),
}

#[cfg(test)]
#[must_use]
pub fn apply_control(
    highlight: &mut Highlight,
    control: Control,
    item_count: usize,
    visible_items: usize,
    footer: bool,
) -> Applied {
    match control {
        Control::Turn(steps) => {
            highlight.move_by(steps, item_count, visible_items, footer);
            Applied::Moved(highlight.index())
        }
        Control::Press => Applied::Pressed(highlight.index()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_control, apply_level_change, dev_key, Accelerator, Applied, Control, DevKey,
        DeviceInput, Highlight, Levels, Line, Quadrature, Row, Switch,
    };
    use iced::keyboard::key::Named;
    use iced::keyboard::Key;
    use std::time::Duration;

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    #[test]
    fn a_clockwise_detent_is_one_step_down() {
        let mut encoder = Quadrature::new(true, true);
        let mut steps = Vec::new();
        // CLK falls first: both high, CLK low, both low, DT low, both high.
        for (clk, dt) in [(false, true), (false, false), (true, false), (true, true)] {
            if let Some(step) = encoder.update(clk, dt) {
                steps.push(step);
            }
        }
        assert_eq!(steps, vec![1]);
    }

    #[test]
    fn a_counter_clockwise_detent_is_one_step_up() {
        let mut encoder = Quadrature::new(true, true);
        let mut steps = Vec::new();
        // DT falls first.
        for (clk, dt) in [(true, false), (false, false), (false, true), (true, true)] {
            if let Some(step) = encoder.update(clk, dt) {
                steps.push(step);
            }
        }
        assert_eq!(steps, vec![-1]);
    }

    #[test]
    fn contact_bounce_does_not_count_as_a_detent() {
        let mut encoder = Quadrature::new(true, true);
        assert_eq!(encoder.update(false, true), None);
        assert_eq!(encoder.update(true, true), None);
    }

    #[test]
    fn half_a_turn_does_not_move() {
        let mut encoder = Quadrature::new(true, true);
        assert_eq!(encoder.update(false, true), None);
        assert_eq!(encoder.update(false, false), None);
    }

    #[test]
    fn a_press_shorter_than_the_debounce_is_ignored() {
        let mut switch = Switch::new(false);
        assert_eq!(switch.update(true, ms(0)), None);
        assert_eq!(switch.update(false, ms(10)), None);
        assert_eq!(switch.update(true, ms(12)), None);
        assert_eq!(switch.update(false, ms(18)), None);
    }

    #[test]
    fn a_settled_press_is_reported_once() {
        let mut switch = Switch::new(false);
        assert_eq!(switch.update(true, ms(0)), None);
        assert_eq!(switch.update(true, ms(19)), None);
        assert_eq!(switch.update(true, ms(20)), Some(Control::Press));
        assert_eq!(switch.update(true, ms(80)), None);
        assert_eq!(switch.update(false, ms(100)), None);
    }

    #[test]
    fn a_second_press_is_reported_again() {
        let mut switch = Switch::new(false);
        assert_eq!(switch.update(true, ms(0)), None);
        assert_eq!(switch.update(true, ms(20)), Some(Control::Press));
        assert_eq!(switch.update(false, ms(40)), None);
        assert_eq!(switch.update(true, ms(50)), None);
        assert_eq!(switch.update(true, ms(70)), Some(Control::Press));
    }

    #[test]
    fn a_key_held_when_reading_starts_does_not_press() {
        let mut switch = Switch::new(true);
        assert_eq!(switch.update(true, ms(0)), None);
        assert_eq!(switch.update(true, ms(50)), None);
    }

    #[test]
    fn letting_go_does_not_press() {
        let mut switch = Switch::new(true);
        assert_eq!(switch.update(false, ms(0)), None);
        assert_eq!(switch.update(false, ms(50)), None);
    }

    #[test]
    fn a_slow_detent_moves_one_row() {
        let mut accel = Accelerator::new();
        assert_eq!(accel.scale(1, ms(0)), 1);
        assert_eq!(accel.scale(1, ms(80)), 1);
        assert_eq!(accel.scale(-1, ms(200)), -1);
    }

    #[test]
    fn a_fast_turn_skips_several_rows() {
        let mut accel = Accelerator::new();
        assert_eq!(accel.scale(1, ms(0)), 1);
        assert_eq!(accel.scale(1, ms(79)), 2);
        assert_eq!(accel.scale(1, ms(118)), 4);
        assert_eq!(accel.scale(1, ms(132)), 8);
    }

    #[test]
    fn reversing_direction_does_not_keep_the_skip() {
        let mut accel = Accelerator::new();
        assert_eq!(accel.scale(1, ms(0)), 1);
        assert_eq!(accel.scale(1, ms(10)), 8);
        assert_eq!(accel.scale(-1, ms(15)), -1);
    }

    #[test]
    fn arrow_keys_turn_and_enter_presses() {
        assert_eq!(
            dev_key(&Key::Named(Named::ArrowUp), false),
            Some(DevKey::Up)
        );
        assert_eq!(
            dev_key(&Key::Named(Named::ArrowLeft), false),
            Some(DevKey::Up)
        );
        assert_eq!(
            dev_key(&Key::Named(Named::ArrowDown), false),
            Some(DevKey::Down)
        );
        assert_eq!(
            dev_key(&Key::Named(Named::ArrowRight), true),
            Some(DevKey::Down)
        );
        assert_eq!(
            dev_key(&Key::Named(Named::Enter), false),
            Some(DevKey::Press)
        );
        assert_eq!(dev_key(&Key::Named(Named::Enter), true), None);
        assert_eq!(dev_key(&Key::Named(Named::Space), false), None);
    }

    #[test]
    fn a_level_screen_starts_on_its_pinned_row() {
        let highlight = Highlight::pinned();
        assert_eq!(highlight.index(), 0);
        assert_eq!(
            highlight.rows(0, 8, false),
            vec![Row::Pinned { selected: true }]
        );
    }

    #[test]
    fn one_detent_up_from_the_first_item_lands_on_the_pinned_row() {
        let mut highlight = Highlight::new(5);
        assert_eq!(highlight.index(), 1);
        highlight.move_by(-1, 5, 8, false);
        assert_eq!(highlight.index(), 0);
        assert_eq!(
            highlight.rows(5, 8, false)[0],
            Row::Pinned { selected: true }
        );
    }

    #[test]
    fn one_detent_down_from_the_pinned_row_lands_on_the_first_item() {
        let mut highlight = Highlight::new(5);
        highlight.move_by(-1, 5, 8, false);
        assert_eq!(highlight.index(), 0);
        highlight.move_by(1, 5, 8, false);
        assert_eq!(highlight.index(), 1);
    }

    #[test]
    fn the_highlight_stops_at_the_ends() {
        let mut highlight = Highlight::new(3);
        highlight.move_by(-5, 3, 8, false);
        assert_eq!(highlight.index(), 0);
        highlight.move_by(20, 3, 8, false);
        assert_eq!(highlight.index(), 3);
    }

    #[test]
    fn a_turn_skips_a_row_that_does_nothing() {
        let dead = |index: usize| index == 1 || index == 4;
        let mut highlight = Highlight::pinned();
        highlight.move_skipping(1, 5, 8, false, false, dead);
        assert_eq!(highlight.index(), 2);
        highlight.move_skipping(2, 5, 8, false, false, dead);
        assert_eq!(highlight.index(), 5);
        highlight.move_skipping(1, 5, 8, false, false, dead);
        assert_eq!(highlight.index(), 5);
        highlight.move_skipping(-1, 5, 8, false, false, dead);
        assert_eq!(highlight.index(), 3);
        highlight.move_skipping(-5, 5, 8, false, false, dead);
        assert_eq!(highlight.index(), 0);
    }

    #[test]
    fn a_dead_row_is_left_when_the_list_changes() {
        let mut highlight = Highlight::new(5);
        highlight.fit_live(5, 8, false, false, |index| index == 1);
        assert_eq!(highlight.index(), 2);
    }

    #[test]
    fn the_pinned_row_stays_put_while_the_list_scrolls() {
        let mut highlight = Highlight::new(30);
        highlight.move_by(12, 30, 8, false);
        let rows = highlight.rows(30, 8, false);
        assert_eq!(rows[0], Row::Pinned { selected: false });
        assert!(rows
            .iter()
            .any(|row| matches!(row, Row::Item { selected: true, .. })));
        assert!(rows.len() <= 9);
    }

    #[test]
    fn the_playing_line_is_one_stop_past_the_last_item() {
        let mut highlight = Highlight::new(30);
        highlight.move_by(29, 30, 8, true);
        assert_eq!(highlight.index(), 30);
        highlight.move_by(1, 30, 8, true);
        assert!(highlight.on_footer(30, false));
        highlight.move_by(4, 30, 8, true);
        assert!(highlight.on_footer(30, false));
        let rows = highlight.rows(30, 8, false);
        assert_eq!(rows[0], Row::Pinned { selected: false });
        assert!(rows.iter().all(|row| match row {
            Row::Item { selected, .. } => !selected,
            Row::Pinned { selected } => !selected,
            Row::Sort { selected } => !selected,
        }));
        assert_eq!(
            rows.last(),
            Some(&Row::Item {
                index: 29,
                selected: false,
            })
        );
        highlight.move_by(-1, 30, 8, true);
        assert_eq!(highlight.index(), 30);
        highlight.fit(30, 8, false);
        assert_eq!(highlight.index(), 30);
        assert!(!highlight.on_footer(30, false));
    }

    #[test]
    fn sort_stays_on_the_title_line_while_the_tracks_scroll() {
        let mut highlight = Highlight::pinned();
        highlight.move_skipping(1, 30, 8, false, true, |_| false);
        assert_eq!(highlight.index(), 1);
        let rows = highlight.rows(30, 8, true);
        assert_eq!(rows[0], Row::Pinned { selected: false });
        assert_eq!(rows[1], Row::Sort { selected: true });
        highlight.move_skipping(1, 30, 8, false, true, |_| false);
        assert_eq!(highlight.index(), 2);
        highlight.move_skipping(20, 30, 8, false, true, |_| false);
        let rows = highlight.rows(30, 8, true);
        assert_eq!(rows[1], Row::Sort { selected: false });
        assert!(rows
            .iter()
            .any(|row| matches!(row, Row::Item { selected: true, .. })));
        highlight.move_skipping(100, 30, 8, true, true, |_| false);
        assert!(highlight.on_footer(30, true));
        highlight.move_skipping(-1, 30, 8, true, true, |_| false);
        assert_eq!(highlight.index(), 31);
        assert!(!highlight.on_footer(30, true));
    }

    #[test]
    fn a_shrinking_list_keeps_the_highlight_in_range() {
        let mut highlight = Highlight::new(10);
        highlight.move_by(8, 10, 8, false);
        assert_eq!(highlight.index(), 9);
        highlight.fit(2, 8, false);
        assert_eq!(highlight.index(), 2);
        assert_eq!(
            highlight.rows(2, 8, false),
            vec![
                Row::Pinned { selected: false },
                Row::Item {
                    index: 0,
                    selected: false,
                },
                Row::Item {
                    index: 1,
                    selected: true,
                },
            ]
        );
    }

    #[test]
    fn sampling_a_clockwise_detent_turns_down_one_row() {
        let mut input = DeviceInput::new();
        input.seed(Levels {
            clk_high: true,
            dt_high: true,
            switch_pressed: false,
        });
        let mut controls = Vec::new();
        for (clk, dt) in [(false, true), (false, false), (true, false), (true, true)] {
            controls.extend(input.sample(
                Levels {
                    clk_high: clk,
                    dt_high: dt,
                    switch_pressed: false,
                },
                ms(0),
            ));
        }
        assert_eq!(controls, vec![Control::Turn(1)]);
    }

    #[test]
    fn a_bouncing_key_presses_once_it_settles() {
        let mut input = DeviceInput::new();
        input.seed(Levels {
            clk_high: true,
            dt_high: true,
            switch_pressed: false,
        });
        let bounce = input.sample(
            Levels {
                clk_high: true,
                dt_high: true,
                switch_pressed: true,
            },
            ms(0),
        );
        let released = input.sample(
            Levels {
                clk_high: true,
                dt_high: true,
                switch_pressed: false,
            },
            ms(5),
        );
        let down = input.sample(
            Levels {
                clk_high: true,
                dt_high: true,
                switch_pressed: true,
            },
            ms(8),
        );
        let held = input.sample(
            Levels {
                clk_high: true,
                dt_high: true,
                switch_pressed: true,
            },
            ms(28),
        );
        assert_eq!(bounce, Vec::<Control>::new());
        assert_eq!(released, Vec::<Control>::new());
        assert_eq!(down, Vec::<Control>::new());
        assert_eq!(held, vec![Control::Press]);
    }

    #[test]
    fn development_keys_turn_and_press() {
        let mut input = DeviceInput::new();
        assert_eq!(input.key(DevKey::Down, ms(0)), Some(Control::Turn(1)));
        assert_eq!(input.key(DevKey::Down, ms(10)), Some(Control::Turn(8)));
        assert_eq!(input.key(DevKey::Press, ms(10)), Some(Control::Press));
        assert_eq!(input.key(DevKey::Up, ms(10)), Some(Control::Turn(-1)));
    }

    #[test]
    fn a_fast_development_turn_skips_rows() {
        let mut input = DeviceInput::new();
        let mut highlight = Highlight::new(30);
        let first = input.key(DevKey::Down, ms(0)).unwrap();
        assert_eq!(
            apply_control(&mut highlight, first, 30, 8, false),
            Applied::Moved(2)
        );
        let fast = input.key(DevKey::Down, ms(10)).unwrap();
        assert_eq!(
            apply_control(&mut highlight, fast, 30, 8, false),
            Applied::Moved(10)
        );
        let press = input.key(DevKey::Press, ms(10)).unwrap();
        assert_eq!(
            apply_control(&mut highlight, press, 30, 8, false),
            Applied::Pressed(10)
        );
    }

    #[test]
    fn the_round_key_is_pressed_when_its_line_falls() {
        let mut levels = Levels {
            clk_high: true,
            dt_high: true,
            switch_pressed: false,
        };
        apply_level_change(&mut levels, Line::Key, false);
        assert!(levels.switch_pressed);
        apply_level_change(&mut levels, Line::Key, true);
        assert!(!levels.switch_pressed);
        apply_level_change(&mut levels, Line::Clk, false);
        assert!(!levels.clk_high);
        assert!(levels.dt_high);
    }
}

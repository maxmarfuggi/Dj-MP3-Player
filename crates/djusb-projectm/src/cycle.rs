//! The order presets are shown in.
//!
//! The list is shuffled once when a pack opens, then each preset follows the
//! last. The same seed always produces the same order, so the shuffle can be
//! checked without the shared library.

use std::path::PathBuf;

/// Reorders `items` from `seed`. Fewer than two items stay as they are.
pub fn shuffle(items: &mut [PathBuf], seed: u64) {
    if items.len() < 2 {
        return;
    }
    let mut state = seed | 1;
    for index in (1..items.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let choice = (state as usize) % (index + 1);
        items.swap(index, choice);
    }
}

/// One preset at a time, wrapping after the last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cycle {
    files: Vec<PathBuf>,
    index: usize,
}

impl Cycle {
    #[must_use]
    pub fn new(files: Vec<PathBuf>) -> Self {
        Self { files, index: 0 }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    #[must_use]
    pub fn current(&self) -> Option<&std::path::Path> {
        self.files.get(self.index).map(PathBuf::as_path)
    }

    /// Moves to the next file and returns it. An empty cycle stays empty.
    pub fn advance(&mut self) -> Option<&std::path::Path> {
        if self.files.is_empty() {
            return None;
        }
        self.index = (self.index + 1) % self.files.len();
        self.current()
    }
}

#[cfg(test)]
mod tests {
    use super::{shuffle, Cycle};
    use std::path::PathBuf;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn the_same_seed_shuffles_the_same_way_and_keeps_every_file() {
        let mut first = paths(&["a.milk", "b.milk", "c.milk", "d.milk"]);
        let mut second = first.clone();
        shuffle(&mut first, 7);
        shuffle(&mut second, 7);
        assert_eq!(first, second);
        let mut sorted = first.clone();
        sorted.sort();
        let mut expected = paths(&["a.milk", "b.milk", "c.milk", "d.milk"]);
        expected.sort();
        assert_eq!(sorted, expected);
        assert_ne!(first, paths(&["a.milk", "b.milk", "c.milk", "d.milk"]));
    }

    #[test]
    fn one_file_and_none_are_left_in_place() {
        let mut one = paths(&["a.milk"]);
        shuffle(&mut one, 1);
        assert_eq!(one, paths(&["a.milk"]));
        let mut none = Vec::new();
        shuffle(&mut none, 1);
        assert!(none.is_empty());
    }

    #[test]
    fn the_cycle_wraps_to_the_first_preset() {
        let mut cycle = Cycle::new(paths(&["a.milk", "b.milk"]));
        assert_eq!(cycle.current().unwrap().as_os_str(), "a.milk");
        assert_eq!(cycle.advance().unwrap().as_os_str(), "b.milk");
        assert_eq!(cycle.advance().unwrap().as_os_str(), "a.milk");
    }

    #[test]
    fn an_empty_cycle_has_no_preset() {
        let mut cycle = Cycle::new(Vec::new());
        assert!(cycle.is_empty());
        assert!(cycle.current().is_none());
        assert!(cycle.advance().is_none());
    }
}

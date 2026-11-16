//! Playlist order, Camelot keys, and energy flow.
//!
//! The desktop and the wall player share these rules so a saved order and a
//! mix rating mean the same thing in both.

mod energy;
mod format;
mod key;
mod order;
mod view;

pub use energy::{
    bpm_grid, classify_harmonic, energy_flow, match_tempo, EnergyFlow, FlowMix, FlowPoint,
    FlowTrack, HarmonicCounts, HarmonicKind, HarmonicMix, TempoMatch, TEMPO_JUMP,
};
pub use format::{format_bpm, format_time, format_total_time};
pub use key::{camelot_hue, camelot_key, camelot_number, key_hue, CamelotKey, CamelotLetter};
pub use order::{
    move_item, next_sort, reconcile_order, sort_by_title, sort_entries, to_entries, SortColumn,
    SortDirection, SortState, TrackEntry,
};
pub use view::{
    current_entry_index, find_playlist, library_was_removed, list_entries, list_view_key,
    order_key, queue_from_entries, visible_library, visible_playlist, PlaylistSelection,
    QueueEntry,
};

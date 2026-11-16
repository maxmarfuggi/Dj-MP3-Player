//! Shared reader for rekordbox and Serato USB libraries.
//!
//! The desktop app and the wall player both depend on this crate, so there is
//! one copy of the library format code.

mod library;
mod model;
mod paths;
pub mod rekordbox;
pub mod serato;
mod volumes;

pub use library::{LibraryManager, ScanHandle, Volume, SCAN_INTERVAL};
pub use model::{
    library_id, Library, LibraryLoadError, LibrarySource, LibraryState, OpenFolderResult,
    PlaylistKind, PlaylistNode, Track, TrackColor,
};
pub use paths::{exists, file_name_without_extension, resolve_inside_volume, to_relative_path};
pub use volumes::list_volumes;

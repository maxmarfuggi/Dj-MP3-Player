//! Volume-relative paths. A library file is untrusted input, so a path that
//! would leave the volume is rejected.

use std::path::{Component, Path, PathBuf};

/// Whether `path` can be accessed. Any error, including a missing file, is false.
#[must_use]
pub fn exists(path: impl AsRef<Path>) -> bool {
    std::fs::metadata(path).is_ok()
}

/// Library paths are volume-relative. rekordbox adds a leading `/`, Serato does not.
/// Leading `/` characters are removed and the rest is kept, including any `\`.
#[must_use]
pub fn to_relative_path(library_path: &str) -> &str {
    library_path.trim_start_matches('/')
}

/// Resolves a volume-relative path to an absolute one.
///
/// Returns `None` when the path would land outside the volume, or on the volume
/// root itself. Only `/` separates segments, matching `paths.ts`. `.` and `..`
/// are collapsed without touching the filesystem, so a path that climbs out of
/// the volume is rejected even when the file does not exist.
#[must_use]
pub fn resolve_inside_volume(volume_path: &str, relative_path: &str) -> Option<PathBuf> {
    let root = normalize_lexical(&std::path::absolute(volume_path).ok()?);
    let mut candidate = root.clone();
    for segment in relative_path
        .split('/')
        .filter(|segment| !segment.is_empty())
    {
        candidate.push(segment);
    }
    let resolved = normalize_lexical(&candidate);
    if resolved.starts_with(&root) && resolved != root {
        Some(resolved)
    } else {
        None
    }
}

/// Collapses `.` and `..` without requiring the path to exist.
pub(crate) fn normalize_lexical(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component),
        }
    }
    normalized
}

/// The file name of a `/`-separated path, without its last extension.
/// `Artist/Some.Track.mp3` is `Some.Track`.
#[must_use]
pub fn file_name_without_extension(relative_path: &str) -> &str {
    let base = match relative_path.rsplit_once('/') {
        Some((_, base)) => base,
        None => relative_path,
    };
    if base == "." || base == ".." {
        return base;
    }
    match base.rfind('.') {
        Some(0) | None => base,
        Some(dot) => &base[..dot],
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{exists, file_name_without_extension, resolve_inside_volume, to_relative_path};

    #[test]
    fn to_relative_path_strips_the_leading_slash_rekordbox_adds() {
        assert_eq!(
            to_relative_path("/Contents/Artist/track.mp3"),
            "Contents/Artist/track.mp3"
        );
        assert_eq!(to_relative_path("Music/track.mp3"), "Music/track.mp3");
        assert_eq!(to_relative_path("/Artist\\track.mp3"), "Artist\\track.mp3");
    }

    #[test]
    fn resolve_inside_volume_resolves_paths_below_the_volume_root() {
        let volume = volume_root();
        let resolved = resolve_inside_volume(&volume_text(&volume), "Contents/Artist/track.mp3")
            .expect("path stays inside the volume");
        let mut expected = volume;
        expected.push("Contents");
        expected.push("Artist");
        expected.push("track.mp3");
        assert_eq!(resolved, expected);
    }

    #[test]
    fn resolve_inside_volume_rejects_paths_that_escape_the_volume() {
        let volume = volume_text(&volume_root());
        assert_eq!(resolve_inside_volume(&volume, "../../etc/passwd"), None);
        assert_eq!(
            resolve_inside_volume(&volume, "Contents/../../other/track.mp3"),
            None
        );
        assert_eq!(resolve_inside_volume(&volume, ""), None);
    }

    #[test]
    fn file_name_without_extension_keeps_dots_inside_the_name() {
        assert_eq!(
            file_name_without_extension("Artist/Some.Track.mp3"),
            "Some.Track"
        );
        assert_eq!(file_name_without_extension("track.mp3"), "track");
        assert_eq!(file_name_without_extension(".hidden"), ".hidden");
        assert_eq!(file_name_without_extension("dir/file"), "file");
    }

    #[test]
    fn exists_is_false_when_the_file_is_missing() {
        let path = std::env::temp_dir().join(format!("djusb-exists-{}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        assert!(!exists(&path));
        std::fs::write(&path, b"ok").unwrap();
        assert!(exists(&path));
        std::fs::remove_file(&path).unwrap();
    }

    /// The same root `paths.test.ts` builds with `path.resolve('/Volumes/GIGS')`.
    fn volume_root() -> PathBuf {
        std::path::absolute(Path::new("/Volumes/GIGS")).unwrap()
    }

    fn volume_text(path: &Path) -> String {
        path.to_str().unwrap().to_string()
    }
}

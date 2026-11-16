//! Which DJ libraries are currently available.
//!
//! Every mounted volume is checked for a rekordbox Device Library, a Device
//! Library Plus export, and a Serato library, in that order. A format that is
//! absent is tried again on the next scan. A format that loaded or failed is
//! kept until the volume disappears. Folders the user picked stay until the
//! folder itself is gone.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::paths::{exists, resolve_inside_volume};
use crate::{Library, LibraryLoadError, LibrarySource, LibraryState};

/// How often a running scan loop looks for volumes.
pub const SCAN_INTERVAL: Duration = Duration::from_millis(2000);

/// A mounted volume, or a folder the user picked in its place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    pub path: String,
    pub name: String,
}

enum LoadResult {
    Library {
        library: Library,
        track_paths: HashMap<String, String>,
        artwork_paths: HashMap<String, String>,
    },
    Error(String),
}

struct VolumeEntry {
    volume: Volume,
    /// Folders the user picked are kept until they disappear from disk.
    manual: bool,
    /// One result per format found on the volume, in reader order.
    results: Vec<(LibrarySource, LoadResult)>,
}

type ChangeCallback = Box<dyn FnMut(LibraryState) + Send>;

struct Inner {
    entries: Vec<VolumeEntry>,
    /// Set for the whole of a scan, including its change callback.
    scanning: bool,
}

/// Tracks which DJ libraries are currently available and keeps them loaded.
pub struct LibraryManager {
    inner: Mutex<Inner>,
    on_change: Mutex<Option<ChangeCallback>>,
}

/// Stops a scan loop when dropped.
pub struct ScanHandle {
    stop: std::sync::Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl LibraryManager {
    #[must_use]
    pub fn new(on_change: impl FnMut(LibraryState) + Send + 'static) -> Self {
        Self {
            inner: Mutex::new(Inner {
                entries: Vec::new(),
                scanning: false,
            }),
            on_change: Mutex::new(Some(Box::new(on_change))),
        }
    }

    /// Checks `volumes` for libraries and drops volumes that are no longer mounted.
    ///
    /// A scan that starts while another is still running does nothing. The
    /// callback runs only when the loaded set changes.
    pub fn scan(&self, volumes: &[Volume]) -> bool {
        {
            let mut inner = lock(&self.inner);
            if inner.scanning {
                return false;
            }
            inner.scanning = true;
        }
        let _clear = ClearScanning(&self.inner);

        let changed = {
            let mut inner = lock(&self.inner);
            inner.scan_volumes(volumes)
        };
        if changed {
            let state = lock(&self.inner).state();
            self.notify(state);
        }
        changed
    }

    /// Adds a folder the user picked. Returns false if it holds no DJ library.
    pub fn add_folder(&self, folder_path: &Path) -> bool {
        let Some(path) = folder_path.to_str() else {
            return false;
        };
        let changed = {
            let mut inner = lock(&self.inner);
            let existing = inner
                .entries
                .iter()
                .position(|entry| entry.volume.path == path);
            if let Some(index) = existing {
                inner.entries[index].load_new()
            } else {
                let mut entry = VolumeEntry {
                    volume: Volume {
                        path: path.to_string(),
                        name: folder_name(path),
                    },
                    manual: true,
                    results: Vec::new(),
                };
                let changed = entry.load_new();
                let found = !entry.results.is_empty();
                if found {
                    inner.entries.push(entry);
                }
                changed && found
            }
        };
        if changed {
            let state = lock(&self.inner).state();
            self.notify(state);
        }
        lock(&self.inner)
            .entries
            .iter()
            .any(|entry| entry.volume.path == path && !entry.results.is_empty())
    }

    /// Looks for volumes on a background thread until the returned handle is stopped.
    ///
    /// The thread holds only a weak reference, so stopping the handle or
    /// dropping the manager ends the loop.
    pub fn start_scanning<F>(
        self: &std::sync::Arc<Self>,
        interval: Duration,
        list_volumes: F,
    ) -> ScanHandle
    where
        F: Fn() -> Vec<Volume> + Send + 'static,
    {
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let stop_flag = stop.clone();
        let manager = std::sync::Arc::downgrade(self);
        let thread =
            std::thread::spawn(move || scan_loop(manager, interval, list_volumes, &stop_flag));
        ScanHandle {
            stop,
            thread: Some(thread),
        }
    }

    #[must_use]
    pub fn state(&self) -> LibraryState {
        lock(&self.inner).state()
    }

    /// Absolute path of a track's audio file, or `None` if the track is unknown.
    #[must_use]
    pub fn resolve_track_path(&self, library_id: &str, track_id: &str) -> Option<PathBuf> {
        self.resolve(library_id, track_id, false)
    }

    /// Absolute path of a track's artwork, or `None` if the track is unknown or has none.
    #[must_use]
    pub fn resolve_artwork_path(&self, library_id: &str, track_id: &str) -> Option<PathBuf> {
        self.resolve(library_id, track_id, true)
    }

    fn resolve(&self, library_id: &str, track_id: &str, artwork: bool) -> Option<PathBuf> {
        let inner = lock(&self.inner);
        for entry in &inner.entries {
            for (_, result) in &entry.results {
                let LoadResult::Library {
                    library,
                    track_paths,
                    artwork_paths,
                } = result
                else {
                    continue;
                };
                if library.id != library_id {
                    continue;
                }
                let relative_path = if artwork {
                    artwork_paths.get(track_id)?.as_str()
                } else {
                    track_paths.get(track_id)?.as_str()
                };
                return resolve_inside_volume(&entry.volume.path, relative_path);
            }
        }
        None
    }

    fn notify(&self, state: LibraryState) {
        let mut slot = lock(&self.on_change);
        if let Some(mut callback) = slot.take() {
            drop(slot);
            callback(state);
            *lock(&self.on_change) = Some(callback);
        }
    }
}

impl ScanHandle {
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for ScanHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

fn scan_loop<F>(
    manager: Weak<LibraryManager>,
    interval: Duration,
    list_volumes: F,
    stop: &AtomicBool,
) where
    F: Fn() -> Vec<Volume>,
{
    while !stop.load(Ordering::Relaxed) {
        let Some(live) = manager.upgrade() else {
            return;
        };
        live.scan(&list_volumes());
        drop(live);
        let slice = Duration::from_millis(50).min(interval);
        let mut slept = Duration::ZERO;
        while slept < interval {
            if stop.load(Ordering::Relaxed) || manager.strong_count() == 0 {
                return;
            }
            std::thread::sleep(slice);
            slept = slept.saturating_add(slice);
        }
    }
}

struct ClearScanning<'a>(&'a Mutex<Inner>);

impl Drop for ClearScanning<'_> {
    fn drop(&mut self) {
        lock(self.0).scanning = false;
    }
}

impl Inner {
    fn state(&self) -> LibraryState {
        let mut state = LibraryState {
            libraries: Vec::new(),
            errors: Vec::new(),
        };
        for entry in &self.entries {
            for (source, result) in &entry.results {
                match result {
                    LoadResult::Library { library, .. } => state.libraries.push(library.clone()),
                    LoadResult::Error(message) => state.errors.push(LibraryLoadError {
                        volume_path: entry.volume.path.clone(),
                        volume_name: entry.volume.name.clone(),
                        source: *source,
                        message: message.clone(),
                    }),
                }
            }
        }
        state
    }

    fn scan_volumes(&mut self, volumes: &[Volume]) -> bool {
        let mounted: std::collections::HashSet<&str> =
            volumes.iter().map(|volume| volume.path.as_str()).collect();
        let mut changed = false;
        self.entries.retain(|entry| {
            let gone = if entry.manual {
                !exists(&entry.volume.path)
            } else {
                !mounted.contains(entry.volume.path.as_str())
            };
            if gone {
                changed = true;
            }
            !gone
        });

        for volume in volumes {
            if let Some(index) = self
                .entries
                .iter()
                .position(|entry| entry.volume.path == volume.path)
            {
                if self.entries[index].load_new() {
                    changed = true;
                }
            } else {
                let mut entry = VolumeEntry {
                    volume: volume.clone(),
                    manual: false,
                    results: Vec::new(),
                };
                if entry.load_new() {
                    self.entries.push(entry);
                    changed = true;
                }
            }
        }
        changed
    }
}

impl VolumeEntry {
    /// Runs every reader that has no result for this volume yet.
    fn load_new(&mut self) -> bool {
        let mut changed = false;
        for source in [
            LibrarySource::Rekordbox,
            LibrarySource::DeviceLibraryPlus,
            LibrarySource::Serato,
        ] {
            if self.results.iter().any(|(found, _)| *found == source) {
                continue;
            }
            let Some(result) = read_source(source, &self.volume) else {
                continue;
            };
            self.results.push((source, result));
            changed = true;
        }
        changed
    }
}

fn read_source(source: LibrarySource, volume: &Volume) -> Option<LoadResult> {
    let root = Path::new(&volume.path);
    match source {
        LibrarySource::Rekordbox => {
            if !exists(root.join("PIONEER").join("rekordbox").join("export.pdb")) {
                return None;
            }
            Some(match crate::rekordbox::load_volume(root, &volume.name) {
                Ok(library) => loaded(library),
                Err(error) => LoadResult::Error(error.to_string()),
            })
        }
        LibrarySource::DeviceLibraryPlus => {
            if !exists(
                root.join("PIONEER")
                    .join("rekordbox")
                    .join("exportLibrary.db"),
            ) {
                return None;
            }
            Some(
                match crate::rekordbox::device_library_plus::load_volume(root, &volume.name) {
                    Ok(library) => loaded(library),
                    Err(error) => LoadResult::Error(error.to_string()),
                },
            )
        }
        LibrarySource::Serato => match crate::serato::load_volume(root, &volume.name) {
            Ok(None) => None,
            Ok(Some(library)) => Some(loaded(library)),
            Err(error) => Some(LoadResult::Error(error.to_string())),
        },
    }
}

fn loaded(library: Library) -> LoadResult {
    let track_paths = library
        .tracks
        .iter()
        .map(|track| (track.id.clone(), track.relative_path.clone()))
        .collect();
    let artwork_paths = library
        .tracks
        .iter()
        .filter_map(|track| {
            track
                .artwork_path
                .clone()
                .map(|path| (track.id.clone(), path))
        })
        .collect();
    LoadResult::Library {
        library,
        track_paths,
        artwork_paths,
    }
}

fn folder_name(path: &str) -> String {
    // `\` separates segments on Windows only. A Windows path still has a last segment on Linux.
    let unified = path.replace('\\', "/");
    Path::new(&unified)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

#[cfg(test)]
mod tests {
    use super::{folder_name, LibraryManager, Volume, SCAN_INTERVAL};
    use crate::{library_id, Library, LibrarySource};
    use serde::Deserialize;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    #[test]
    fn scan_interval_matches_the_desktop_poll() {
        assert_eq!(SCAN_INTERVAL, Duration::from_millis(2000));
    }

    #[test]
    fn folder_name_is_the_last_segment() {
        assert_eq!(folder_name(r"C:\DJ\GIGS"), "GIGS");
        assert_eq!(folder_name("/Volumes/GIGS"), "GIGS");
    }

    #[test]
    fn fixture_volumes_match_expected_json() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
        let mut cases = Vec::new();
        for group in ["rekordbox", "deviceLibraryPlus", "serato", "local"] {
            let directory = root.join(group);
            if !directory.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(directory).unwrap() {
                let case = entry.unwrap().path();
                if case.join("volume").is_dir() && case.join("expected.json").is_file() {
                    cases.push(case);
                }
            }
        }
        cases.sort();
        assert!(cases
            .iter()
            .any(|case| case.ends_with(Path::new("rekordbox").join("basic"))));

        for case in cases {
            let name = case.file_name().unwrap().to_str().unwrap().to_string();
            let volume_path = case.join("volume");
            let path = volume_path.to_str().unwrap();
            let expected: ExpectedVolume =
                serde_json::from_str(&std::fs::read_to_string(case.join("expected.json")).unwrap())
                    .unwrap();
            let manager = LibraryManager::new(|_| {});
            let changed = manager.scan(&[Volume {
                path: path.to_string(),
                name: name.clone(),
            }]);
            let mut state = manager.state();
            let expect_change = !expected.libraries.is_empty() || !expected.errors.is_empty();
            assert_eq!(changed, expect_change, "{name}");
            for library in &mut state.libraries {
                assert_eq!(library.id, library_id(library.source, path), "{name}");
                library.id = format!("{}-{{volumeHash}}", library.source.as_str());
                library.volume_path = "{volume}".into();
            }
            assert_eq!(state.libraries, expected.libraries, "{name}");
            let errors: Vec<_> = state.errors.iter().map(|error| error.source).collect();
            assert_eq!(errors, expected.errors, "{name}");
            assert!(state
                .errors
                .iter()
                .all(|error| error.volume_path == path && error.volume_name == name));
        }
    }

    #[test]
    fn add_folder_uses_the_folder_name_and_rejects_a_folder_with_no_library() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
        let notices = Arc::new(AtomicUsize::new(0));
        let notices_for_callback = notices.clone();
        let manager = LibraryManager::new(move |_| {
            notices_for_callback.fetch_add(1, Ordering::SeqCst);
        });

        let empty = root.join("serato/not-a-library/volume");
        assert!(!manager.add_folder(&empty));
        assert!(manager.state().libraries.is_empty());
        assert_eq!(notices.load(Ordering::SeqCst), 0);

        let basic = root.join("rekordbox/basic/volume");
        assert!(manager.add_folder(&basic));
        assert_eq!(notices.load(Ordering::SeqCst), 1);
        let state = manager.state();
        assert_eq!(state.libraries.len(), 1);
        assert_eq!(state.libraries[0].volume_name, "volume");
        assert_eq!(state.libraries[0].source, LibrarySource::Rekordbox);

        assert!(manager.add_folder(&basic));
        assert_eq!(notices.load(Ordering::SeqCst), 1);
        assert_eq!(manager.state().libraries.len(), 1);
    }

    #[test]
    fn resolves_track_and_artwork_inside_the_volume() {
        let volume = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/deviceLibraryPlus/basic/volume");
        let path = volume.to_str().unwrap().to_string();
        let manager = LibraryManager::new(|_| {});
        manager.scan(&[Volume {
            path: path.clone(),
            name: "basic".into(),
        }]);
        let library = manager.state().libraries.into_iter().next().unwrap();
        let track = library
            .tracks
            .iter()
            .find(|track| track.artwork_path.is_some())
            .unwrap();
        let audio = manager.resolve_track_path(&library.id, &track.id).unwrap();
        let artwork = manager
            .resolve_artwork_path(&library.id, &track.id)
            .unwrap();
        // `absolute` leaves `..` in place on Unix. Resolved paths have those collapsed.
        let root = crate::paths::normalize_lexical(&std::path::absolute(&volume).unwrap());
        assert!(audio.starts_with(&root), "{}", audio.display());
        assert!(artwork.starts_with(&root), "{}", artwork.display());
        assert_eq!(
            artwork.file_name().and_then(|name| name.to_str()),
            track
                .artwork_path
                .as_deref()
                .and_then(|path| path.rsplit('/').next())
        );
        assert!(manager.resolve_track_path(&library.id, "missing").is_none());
        assert!(manager
            .resolve_artwork_path("other-library", &track.id)
            .is_none());
    }

    #[test]
    fn scan_drops_unmounted_volumes_and_keeps_a_picked_folder_until_it_is_gone() {
        let directory = temp_dir("keep");
        let export = directory.join("PIONEER").join("rekordbox");
        std::fs::create_dir_all(&export).unwrap();
        std::fs::write(export.join("export.pdb"), []).unwrap();
        let path = directory.to_str().unwrap().to_string();
        let manager = LibraryManager::new(|_| {});

        manager.scan(&[Volume {
            path: path.clone(),
            name: "Stick".into(),
        }]);
        assert_eq!(manager.state().errors.len(), 1);

        manager.scan(&[]);
        assert!(manager.state().errors.is_empty());

        assert!(manager.add_folder(&directory));
        assert_eq!(manager.state().errors.len(), 1);
        manager.scan(&[]);
        assert_eq!(manager.state().errors.len(), 1, "a picked folder stays");

        std::fs::remove_dir_all(&directory).unwrap();
        manager.scan(&[]);
        assert!(manager.state().errors.is_empty());
    }

    #[test]
    fn a_missing_format_is_tried_again_and_a_failure_is_kept() {
        let directory = temp_dir("retry");
        let path = directory.to_str().unwrap().to_string();
        let manager = LibraryManager::new(|_| {});
        assert!(!manager.scan(&[Volume {
            path: path.clone(),
            name: "Stick".into(),
        }]));
        assert!(manager.state().libraries.is_empty());

        let export = directory.join("PIONEER").join("rekordbox");
        std::fs::create_dir_all(&export).unwrap();
        std::fs::write(export.join("export.pdb"), []).unwrap();
        assert!(manager.scan(&[Volume {
            path: path.clone(),
            name: "Stick".into(),
        }]));
        let message = manager.state().errors[0].message.clone();

        std::fs::write(export.join("export.pdb"), [0, 1, 2, 3]).unwrap();
        assert!(!manager.scan(&[Volume {
            path: path.clone(),
            name: "Renamed".into(),
        }]));
        let state = manager.state();
        assert_eq!(state.errors[0].message, message);
        assert_eq!(state.errors[0].volume_name, "Stick");
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn a_scan_started_from_the_change_callback_is_skipped() {
        let root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rekordbox/basic/volume");
        let path = root.to_str().unwrap().to_string();
        let manager_slot: Arc<Mutex<Option<Arc<LibraryManager>>>> = Arc::new(Mutex::new(None));
        let skipped = Arc::new(AtomicBool::new(false));
        let slot = manager_slot.clone();
        let skipped_flag = skipped.clone();
        let manager = Arc::new(LibraryManager::new(move |_| {
            let locked = slot.lock().unwrap();
            if let Some(manager) = locked.as_ref() {
                skipped_flag.store(!manager.scan(&[]), Ordering::SeqCst);
            }
        }));
        *manager_slot.lock().unwrap() = Some(manager.clone());
        assert!(manager.scan(&[Volume {
            path,
            name: "basic".into(),
        }]));
        assert!(skipped.load(Ordering::SeqCst));
        assert_eq!(manager.state().libraries.len(), 1);
    }

    #[test]
    fn the_scan_loop_reports_a_mounted_library_and_stops() {
        let volume =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rekordbox/basic/volume");
        let path = volume.to_str().unwrap().to_string();
        let seen = Arc::new(AtomicUsize::new(0));
        let seen_for_callback = seen.clone();
        let manager = Arc::new(LibraryManager::new(move |_| {
            seen_for_callback.fetch_add(1, Ordering::SeqCst);
        }));
        let mut handle = manager.start_scanning(Duration::from_millis(40), move || {
            vec![Volume {
                path: path.clone(),
                name: "basic".into(),
            }]
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        while seen.load(Ordering::SeqCst) == 0 {
            assert!(Instant::now() < deadline, "the scan loop did not report");
            std::thread::sleep(Duration::from_millis(10));
        }
        handle.stop();
        let reported = seen.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(80));
        assert_eq!(seen.load(Ordering::SeqCst), reported);
        assert_eq!(manager.state().libraries.len(), 1);
    }

    fn temp_dir(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("djusb-manager-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[derive(Deserialize)]
    struct ExpectedVolume {
        libraries: Vec<Library>,
        errors: Vec<LibrarySource>,
    }
}

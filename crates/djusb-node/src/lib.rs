//! Node-API bridge the desktop's main process loads.
//!
//! [`LibraryManager`] is the scan loop, picked folders, and path lookup.
//! [`serve_media`] is one `media://` response: file type, range, AIFF as WAV,
//! and the accent-normalization retry. The volume is only read.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use napi::bindgen_prelude::{Buffer, Function, Result};
use napi::threadsafe_function::{ThreadsafeCallContext, ThreadsafeFunctionCallMode};
use napi_derive::napi;

/// One `media://` response. `body` is `null` when there is nothing to send.
#[napi(object, use_nullable = true)]
pub struct MediaResponse {
    pub status: u32,
    pub headers: BTreeMap<String, String>,
    pub body: Option<Buffer>,
}

/// Libraries loaded from mounted volumes and from folders the user picked.
///
/// `onChange` receives the library state object from the data contract. It runs
/// on the JavaScript thread, and it may run after the call that caused the
/// change has returned.
#[napi]
pub struct LibraryManager {
    inner: Arc<djusb_core::LibraryManager>,
    scan: Mutex<Option<djusb_core::ScanHandle>>,
}

#[napi]
impl LibraryManager {
    #[napi(constructor, catch_unwind)]
    pub fn new(on_change: Function<serde_json::Value, ()>) -> Result<Self> {
        let on_change = on_change
            .build_threadsafe_function()
            .weak::<true>()
            .build_callback(|ctx: ThreadsafeCallContext<serde_json::Value>| Ok(ctx.value))?;
        let inner = Arc::new(djusb_core::LibraryManager::new(
            move |state| match serde_json::to_value(state) {
                Ok(value) => {
                    let _ = on_change.call(value, ThreadsafeFunctionCallMode::NonBlocking);
                }
                Err(error) => eprintln!("Reporting library state failed: {error}"),
            },
        ));
        Ok(Self {
            inner,
            scan: Mutex::new(None),
        })
    }

    /// Looks for volumes until `stop` is called.
    #[napi(catch_unwind)]
    pub fn start(&self) {
        self.stop_scan();
        let handle = self
            .inner
            .start_scanning(djusb_core::SCAN_INTERVAL, remembered_volumes());
        *lock(&self.scan) = Some(handle);
    }

    /// Stops the scan loop. A manager that is not scanning is left as it is.
    #[napi(catch_unwind)]
    pub fn stop(&self) {
        self.stop_scan();
    }

    /// The libraries and load errors currently held.
    #[napi(catch_unwind)]
    pub fn get_state(&self) -> Result<serde_json::Value> {
        serde_json::to_value(self.inner.state()).map_err(|error| {
            napi::Error::from_reason(format!("Reporting library state failed: {error}"))
        })
    }

    /// Adds a folder the user picked. Returns false when it holds no DJ library.
    #[napi(catch_unwind)]
    pub fn add_folder(&self, folder_path: String) -> bool {
        self.inner.add_folder(Path::new(&folder_path))
    }

    /// Absolute path of a track's audio file, or `null` when the track is unknown.
    #[napi(catch_unwind)]
    pub fn resolve_track_path(&self, library_id: String, track_id: String) -> Option<PathBuf> {
        self.inner.resolve_track_path(&library_id, &track_id)
    }

    /// Absolute path of a track's artwork, or `null` when it has none.
    #[napi(catch_unwind)]
    pub fn resolve_artwork_path(&self, library_id: String, track_id: String) -> Option<PathBuf> {
        self.inner.resolve_artwork_path(&library_id, &track_id)
    }

    fn stop_scan(&self) {
        let mut scan = lock(&self.scan);
        if let Some(mut handle) = scan.take() {
            drop(scan);
            handle.stop();
        }
    }
}

impl Drop for LibraryManager {
    fn drop(&mut self) {
        self.stop_scan();
    }
}

/// A mounted volume: its path, and the name shown for it.
#[napi(object)]
pub struct ListedVolume {
    pub path: String,
    pub name: String,
}

/// Mounted volumes that could hold a DJ library, excluding the system volume.
///
/// This is the list the scan loop reads.
#[napi(catch_unwind)]
pub fn list_volumes() -> Result<Vec<ListedVolume>> {
    djusb_core::list_volumes()
        .map(|volumes| {
            volumes
                .into_iter()
                .map(|volume| ListedVolume {
                    path: volume.path,
                    name: volume.name,
                })
                .collect()
        })
        .map_err(|error| napi::Error::from_reason(error.to_string()))
}

/// Reads one volume the way a scan does and returns its library state.
///
/// `name` is the display name stored on each library. The volume is only read.
/// A state with no libraries and no errors means no export was found.
#[napi(catch_unwind)]
pub fn read_volume(path: String, name: String) -> Result<serde_json::Value> {
    let manager = djusb_core::LibraryManager::new(|_| {});
    manager.scan(&[djusb_core::Volume { path, name }]);
    serde_json::to_value(manager.state()).map_err(|error| {
        napi::Error::from_reason(format!("Reporting library state failed: {error}"))
    })
}

/// Serves one `media://track/<libraryId>/<trackId>` or
/// `media://artwork/<libraryId>/<trackId>` request.
///
/// `rangeHeader` is the HTTP Range header, or `null` when the request has none.
/// A file that cannot be read after it was found throws. Anything that does not
/// name a listed file is a 404.
#[napi(catch_unwind)]
pub fn serve_media(
    url: String,
    range_header: Option<String>,
    libraries: &LibraryManager,
) -> Result<MediaResponse> {
    let response =
        djusb_core::media::serve_media(&url, range_header.as_deref(), libraries.inner.as_ref())
            .map_err(|error| napi::Error::from_reason(error.to_string()))?;
    Ok(MediaResponse {
        status: u32::from(response.status),
        headers: response.headers,
        body: response.body.map(Into::into),
    })
}

/// The last listing that succeeded, so a failed listing does not drop libraries.
fn remembered_volumes() -> impl Fn() -> Vec<djusb_core::Volume> + Send {
    let previous = Arc::new(Mutex::new(Vec::new()));
    move || match djusb_core::list_volumes() {
        Ok(volumes) => {
            *lock(&previous) = volumes.clone();
            volumes
        }
        Err(error) => {
            eprintln!("Scanning for volumes failed: {error}");
            lock(&previous).clone()
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

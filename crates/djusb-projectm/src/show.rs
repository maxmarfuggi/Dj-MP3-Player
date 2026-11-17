//! The MilkDrop picture, drawn on a background thread.
//!
//! The thread owns the OpenGL context and the projectM instance. The window
//! only reads the latest frame. Dropping the handle stops the thread and
//! closes the shared library.

use std::ffi::{c_char, CString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::api::Api;
use crate::cycle::{shuffle, Cycle};
use crate::files::{locate_library, preset_files};
use crate::gl::{self, Context};
use crate::pcm::{frame_chunks, tail_frames, BACKLOG_FRAMES};
use crate::pixels::flip_rows;
use crate::{EMPTY_PACK, FRAME_RATE, PRESET_SECONDS};

/// One upright RGBA picture.
#[derive(Clone)]
pub struct Frame {
    pub serial: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
}

struct Published {
    frame: Option<Frame>,
    fault: Option<String>,
}

/// A running projectM picture. Drop it to stop drawing.
pub struct Show {
    stop: Arc<AtomicBool>,
    published: Arc<Mutex<Published>>,
    thread: Option<JoinHandle<()>>,
}

impl Show {
    /// Opens the shared library and draws `pack` at `size`.
    ///
    /// `pull` appends the stereo frames written since its cursor. The cursor
    /// starts at zero inside the thread.
    pub fn start<F>(
        dirs: &[PathBuf],
        pack: &Path,
        size: (u32, u32),
        pull: F,
    ) -> Result<Self, String>
    where
        F: Fn(&mut u64, &mut Vec<f32>) + Send + Sync + 'static,
    {
        let mut presets = preset_files(pack);
        if presets.is_empty() {
            return Err(EMPTY_PACK.to_string());
        }
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(1, |elapsed| {
                u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX)
            });
        shuffle(&mut presets, seed);
        let library = locate_library(dirs)?;
        let texture_dir = pack.to_path_buf();
        let stop = Arc::new(AtomicBool::new(false));
        let published = Arc::new(Mutex::new(Published {
            frame: None,
            fault: None,
        }));
        let stop_thread = Arc::clone(&stop);
        let published_thread = Arc::clone(&published);
        let thread = thread::Builder::new()
            .name("milkdrop".to_string())
            .spawn(move || {
                let published_panic = Arc::clone(&published_thread);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    render(
                        &stop_thread,
                        &published_thread,
                        &library,
                        presets,
                        &texture_dir,
                        size,
                        &pull,
                    );
                }));
                if result.is_err() {
                    report(&published_panic, "MilkDrop stopped.".to_string());
                }
            })
            .map_err(|error| format!("MilkDrop could not start: {error}"))?;
        Ok(Self {
            stop,
            published,
            thread: Some(thread),
        })
    }

    /// The newest picture, if one has been drawn.
    #[must_use]
    pub fn frame(&self) -> Option<Frame> {
        lock(&self.published).frame.clone()
    }

    /// Why drawing stopped, if it did.
    #[must_use]
    pub fn fault(&self) -> Option<String> {
        lock(&self.published).fault.clone()
    }
}

impl Drop for Show {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn render<F>(
    stop: &AtomicBool,
    published: &Mutex<Published>,
    library: &Path,
    presets: Vec<PathBuf>,
    texture_dir: &Path,
    size: (u32, u32),
    pull: &F,
) where
    F: Fn(&mut u64, &mut Vec<f32>),
{
    let context = match Context::open(size.0, size.1) {
        Ok(context) => context,
        Err(message) => {
            report(published, message);
            return;
        }
    };
    let api = match Api::load(library) {
        Ok(api) => api,
        Err(message) => {
            report(published, message);
            return;
        }
    };
    let instance = api.create();
    if instance.is_null() {
        report(
            published,
            "projectM did not start. It needs OpenGL.".to_string(),
        );
        return;
    }
    let live = Live {
        api,
        instance,
        context,
    };
    let (width, height) = live.context.size();
    live.api.configure(instance, width, height);
    set_textures(&live.api, instance, texture_dir);
    let mut cycle = Cycle::new(presets);
    if cycle.is_empty() {
        report(published, EMPTY_PACK.to_string());
        return;
    }
    let Some(first) = cycle.current().map(Path::to_path_buf) else {
        report(published, EMPTY_PACK.to_string());
        return;
    };
    load_preset(&live.api, instance, &first, false);
    let mut cursor = 0;
    let mut samples = Vec::new();
    let mut due = Instant::now() + Duration::from_secs_f64(PRESET_SECONDS);
    let pace = Duration::from_millis(1000 / u64::try_from(FRAME_RATE.max(1)).unwrap_or(30));
    let bytes = match gl::byte_count(width, height) {
        Ok(bytes) => bytes,
        Err(message) => {
            report(published, message);
            return;
        }
    };
    let mut buffer = vec![0; bytes];
    while !stop.load(Ordering::Relaxed) {
        samples.clear();
        pull(&mut cursor, &mut samples);
        tail_frames(&mut samples, BACKLOG_FRAMES);
        feed(&live.api, instance, &samples);
        let fbo = live.context.fbo();
        if fbo.is_none() {
            live.context.bind_default();
        }
        live.api.render(instance, fbo);
        if live.context.read(&mut buffer).is_ok()
            && flip_rows(
                usize::try_from(width).unwrap_or(0),
                usize::try_from(height).unwrap_or(0),
                &mut buffer,
            )
        {
            publish_frame(published, width, height, &buffer);
        }
        if Instant::now() >= due {
            if let Some(next) = cycle.advance() {
                let path = next.to_path_buf();
                load_preset(&live.api, instance, &path, true);
            }
            due = Instant::now() + Duration::from_secs_f64(PRESET_SECONDS);
        }
        if stop.load(Ordering::Relaxed) {
            break;
        }
        thread::sleep(pace);
    }
}

struct Live {
    api: Api,
    instance: *mut std::ffi::c_void,
    context: Context,
}

impl Drop for Live {
    fn drop(&mut self) {
        self.api.destroy(self.instance);
        self.instance = std::ptr::null_mut();
    }
}

fn set_textures(api: &Api, instance: *mut std::ffi::c_void, texture_dir: &Path) {
    let mut owned = Vec::new();
    if let Some(path) = c_path(texture_dir) {
        owned.push(path);
    }
    let textures = texture_dir.join("textures");
    if textures.is_dir() {
        if let Some(path) = c_path(&textures) {
            owned.push(path);
        }
    }
    let pointers: Vec<*const c_char> = owned.iter().map(|path| path.as_c_str().as_ptr()).collect();
    api.set_texture_paths(instance, &pointers);
}

fn load_preset(api: &Api, instance: *mut std::ffi::c_void, path: &Path, blend: bool) {
    if let Some(path) = c_path(path) {
        api.load_preset(instance, path.as_ptr(), blend);
    }
}

fn feed(api: &Api, instance: *mut std::ffi::c_void, samples: &[f32]) {
    let frames = samples.len() / 2;
    let max_frames = api.max_frames();
    for chunk in frame_chunks(frames, max_frames) {
        let start = chunk.start * 2;
        let end = chunk.end * 2;
        api.add_pcm(instance, &samples[start..end]);
    }
}

fn c_path(path: &Path) -> Option<CString> {
    CString::new(path.to_string_lossy().as_ref()).ok()
}

fn publish_frame(published: &Mutex<Published>, width: u32, height: u32, buffer: &[u8]) {
    let mut state = lock(published);
    let serial = state.frame.as_ref().map_or(0, |frame| frame.serial) + 1;
    state.frame = Some(Frame {
        serial,
        width,
        height,
        rgba: Arc::from(buffer),
    });
}

fn report(published: &Mutex<Published>, message: String) {
    let mut state = lock(published);
    if state.fault.is_none() {
        state.fault = Some(message);
    }
}

fn lock(published: &Mutex<Published>) -> std::sync::MutexGuard<'_, Published> {
    published.lock().unwrap_or_else(|error| error.into_inner())
}

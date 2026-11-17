//! The projectM C API, loaded from the shared library after it is opened.
//!
//! Symbols are copied out of the library handle. The handle is kept so the
//! library stays mapped for as long as those symbols are called.

use std::ffi::{c_char, c_void};
use std::path::Path;

use libloading::Library;

use crate::files::open_library;

type Create = unsafe extern "C" fn() -> *mut c_void;
type Destroy = unsafe extern "C" fn(*mut c_void);
type LoadFile = unsafe extern "C" fn(*mut c_void, *const c_char, u8);
type SetSize = unsafe extern "C" fn(*mut c_void, usize, usize);
type SetFps = unsafe extern "C" fn(*mut c_void, i32);
type SetSeconds = unsafe extern "C" fn(*mut c_void, f64);
type SetFlag = unsafe extern "C" fn(*mut c_void, u8);
type SetPaths = unsafe extern "C" fn(*mut c_void, *const *const c_char, usize);
type Render = unsafe extern "C" fn(*mut c_void);
type RenderFbo = unsafe extern "C" fn(*mut c_void, u32);
type PcmAdd = unsafe extern "C" fn(*mut c_void, *const f32, u32, i32);
type PcmMax = unsafe extern "C" fn() -> u32;

pub(crate) struct Api {
    _library: Library,
    create: Create,
    destroy: Destroy,
    load_file: LoadFile,
    set_size: SetSize,
    set_fps: Option<SetFps>,
    set_soft_cut: Option<SetSeconds>,
    set_preset_duration: Option<SetSeconds>,
    set_aspect: Option<SetFlag>,
    set_mesh: Option<SetSize>,
    set_paths: Option<SetPaths>,
    render: Render,
    render_fbo: Option<RenderFbo>,
    pcm_add: PcmAdd,
    pcm_max: PcmMax,
}

impl Api {
    pub(crate) fn load(path: &Path) -> Result<Self, String> {
        let library = open_library(path)?;
        Ok(Self {
            create: required(&library, b"projectm_create\0")?,
            destroy: required(&library, b"projectm_destroy\0")?,
            load_file: required(&library, b"projectm_load_preset_file\0")?,
            set_size: required(&library, b"projectm_set_window_size\0")?,
            set_fps: optional(&library, b"projectm_set_fps\0"),
            set_soft_cut: optional(&library, b"projectm_set_soft_cut_duration\0"),
            set_preset_duration: optional(&library, b"projectm_set_preset_duration\0"),
            set_aspect: optional(&library, b"projectm_set_aspect_correction\0"),
            set_mesh: optional(&library, b"projectm_set_mesh_size\0"),
            set_paths: optional(&library, b"projectm_set_texture_search_paths\0"),
            render: required(&library, b"projectm_opengl_render_frame\0")?,
            render_fbo: optional(&library, b"projectm_opengl_render_frame_fbo\0"),
            pcm_add: required(&library, b"projectm_pcm_add_float\0")?,
            pcm_max: required(&library, b"projectm_pcm_get_max_samples\0")?,
            _library: library,
        })
    }

    pub(crate) fn create(&self) -> *mut c_void {
        unsafe { (self.create)() }
    }

    pub(crate) fn destroy(&self, instance: *mut c_void) {
        if !instance.is_null() {
            unsafe { (self.destroy)(instance) };
        }
    }

    pub(crate) fn configure(&self, instance: *mut c_void, width: u32, height: u32) {
        unsafe {
            (self.set_size)(instance, width as usize, height as usize);
            if let Some(set) = self.set_mesh {
                set(instance, MESH_WIDTH, MESH_HEIGHT);
            }
            if let Some(set) = self.set_fps {
                set(instance, crate::FRAME_RATE);
            }
            if let Some(set) = self.set_soft_cut {
                set(instance, crate::BLEND_SECONDS);
            }
            if let Some(set) = self.set_preset_duration {
                set(instance, crate::PRESET_SECONDS);
            }
            if let Some(set) = self.set_aspect {
                set(instance, 1);
            }
        }
    }

    pub(crate) fn set_texture_paths(&self, instance: *mut c_void, paths: &[*const c_char]) {
        if paths.is_empty() {
            return;
        }
        if let Some(set) = self.set_paths {
            unsafe { set(instance, paths.as_ptr(), paths.len()) };
        }
    }

    pub(crate) fn load_preset(&self, instance: *mut c_void, path: *const c_char, blend: bool) {
        unsafe { (self.load_file)(instance, path, u8::from(blend)) };
    }

    pub(crate) fn render(&self, instance: *mut c_void, fbo: Option<u32>) {
        unsafe {
            if let (Some(render_fbo), Some(fbo)) = (self.render_fbo, fbo) {
                render_fbo(instance, fbo);
            } else {
                (self.render)(instance);
            }
        }
    }

    pub(crate) fn max_frames(&self) -> usize {
        usize::try_from(unsafe { (self.pcm_max)() }).unwrap_or(0)
    }

    pub(crate) fn add_pcm(&self, instance: *mut c_void, samples: &[f32]) {
        let frames = samples.len() / 2;
        let count = u32::try_from(frames).unwrap_or(u32::MAX);
        if count == 0 {
            return;
        }
        unsafe { (self.pcm_add)(instance, samples.as_ptr(), count, STEREO) };
    }
}

/// Per-pixel mesh. Even, and inside the range projectM keeps (8 to 300).
const MESH_WIDTH: usize = 32;
const MESH_HEIGHT: usize = 18;

/// `projectm_channels::PROJECTM_STEREO`.
const STEREO: i32 = 2;

fn required<T: Copy>(library: &Library, name: &[u8]) -> Result<T, String> {
    let symbol = unsafe { library.get::<T>(name) }.map_err(|error| {
        format!(
            "projectM is missing {}: {error}",
            String::from_utf8_lossy(name).trim_end_matches('\0')
        )
    })?;
    Ok(*symbol)
}

fn optional<T: Copy>(library: &Library, name: &[u8]) -> Option<T> {
    let symbol = unsafe { library.get::<T>(name) }.ok()?;
    Some(*symbol)
}

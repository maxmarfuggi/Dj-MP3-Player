//! An offscreen OpenGL surface at the reduced MilkDrop resolution.
//!
//! Windows uses a hidden window. Linux uses an EGL pbuffer, trying desktop
//! OpenGL 3.3 and then OpenGL ES 3. projectM draws into that surface, or into
//! a framebuffer when the library can target one, and the pixels are read back
//! for the window.

use std::ffi::c_void;

const GL_RGBA: u32 = 0x1908;
const GL_RGBA8: i32 = 0x8058;
const GL_UNSIGNED_BYTE: u32 = 0x1401;
const GL_TEXTURE_2D: u32 = 0x0DE1;
const GL_FRAMEBUFFER: u32 = 0x8D40;
const GL_COLOR_ATTACHMENT0: u32 = 0x8CE0;
const GL_LINEAR: i32 = 0x2601;
const GL_CLAMP_TO_EDGE: i32 = 0x812F;
const GL_TEXTURE_MIN_FILTER: u32 = 0x2801;
const GL_TEXTURE_MAG_FILTER: u32 = 0x2800;
const GL_TEXTURE_WRAP_S: u32 = 0x2802;
const GL_TEXTURE_WRAP_T: u32 = 0x2803;
const GL_FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;
const GL_PACK_ALIGNMENT: u32 = 0x0D05;

type ReadPixels = unsafe extern "system" fn(i32, i32, i32, i32, u32, u32, *mut c_void);
type Viewport = unsafe extern "system" fn(i32, i32, i32, i32);
type PixelStore = unsafe extern "system" fn(u32, i32);
type BindFramebuffer = unsafe extern "system" fn(u32, u32);
type GenIds = unsafe extern "system" fn(i32, *mut u32);
type DeleteIds = unsafe extern "system" fn(i32, *const u32);
type BindTexture = unsafe extern "system" fn(u32, u32);
type TexImage = unsafe extern "system" fn(u32, i32, i32, i32, i32, i32, u32, u32, *const c_void);
type TexParam = unsafe extern "system" fn(u32, u32, i32);
type FramebufferTexture = unsafe extern "system" fn(u32, u32, u32, u32, i32);
type CheckFramebuffer = unsafe extern "system" fn(u32) -> u32;

struct Gl {
    read_pixels: ReadPixels,
    viewport: Viewport,
    pixel_store: PixelStore,
    bind_framebuffer: Option<BindFramebuffer>,
    gen_framebuffers: Option<GenIds>,
    delete_framebuffers: Option<DeleteIds>,
    gen_textures: Option<GenIds>,
    delete_textures: Option<DeleteIds>,
    bind_texture: Option<BindTexture>,
    tex_image: Option<TexImage>,
    tex_param: Option<TexParam>,
    framebuffer_texture: Option<FramebufferTexture>,
    check_framebuffer: Option<CheckFramebuffer>,
}

struct Capture {
    fbo: u32,
    texture: u32,
}

pub(crate) struct Context {
    width: u32,
    height: u32,
    gl: Gl,
    capture: Option<Capture>,
    /// Dropped last, so the OpenGL context stays current while the framebuffer is deleted.
    #[allow(dead_code, reason = "the context must outlive the framebuffer objects")]
    platform: Platform,
}

impl Context {
    pub(crate) fn open(width: u32, height: u32) -> Result<Self, String> {
        let width = width.max(1);
        let height = height.max(1);
        let platform = Platform::open(width, height)?;
        let (width, height) = platform.size().unwrap_or((width, height));
        let gl = load_gl(|name| platform.proc(name))?;
        let mut context = Self {
            width,
            height,
            gl,
            capture: None,
            platform,
        };
        context.capture = context.create_capture();
        Ok(context)
    }

    pub(crate) fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The framebuffer id projectM 4.2 can draw into, when one was created.
    pub(crate) fn fbo(&self) -> Option<u32> {
        self.capture.as_ref().map(|capture| capture.fbo)
    }

    pub(crate) fn bind_default(&self) {
        self.bind(0);
        self.apply_viewport();
    }

    /// Reads one buffer. `0` is the window; any other id is a framebuffer.
    pub(crate) fn read_from(&self, target: u32, into: &mut [u8]) -> Result<(), String> {
        let needed = byte_count(self.width, self.height)?;
        if into.len() < needed {
            return Err("MilkDrop frame is the wrong size.".to_string());
        }
        self.bind(target);
        self.apply_viewport();
        unsafe {
            (self.gl.pixel_store)(GL_PACK_ALIGNMENT, 1);
            (self.gl.read_pixels)(
                0,
                0,
                edge(self.width),
                edge(self.height),
                GL_RGBA,
                GL_UNSIGNED_BYTE,
                into.as_mut_ptr().cast(),
            );
        }
        Ok(())
    }

    fn bind(&self, fbo: u32) {
        if let Some(bind) = self.gl.bind_framebuffer {
            unsafe { bind(GL_FRAMEBUFFER, fbo) };
        }
    }

    fn apply_viewport(&self) {
        unsafe { (self.gl.viewport)(0, 0, edge(self.width), edge(self.height)) };
    }

    fn create_capture(&self) -> Option<Capture> {
        let (
            Some(gen_framebuffers),
            Some(gen_textures),
            Some(bind_texture),
            Some(tex_image),
            Some(tex_param),
            Some(framebuffer_texture),
            Some(check),
            Some(bind),
        ) = (
            self.gl.gen_framebuffers,
            self.gl.gen_textures,
            self.gl.bind_texture,
            self.gl.tex_image,
            self.gl.tex_param,
            self.gl.framebuffer_texture,
            self.gl.check_framebuffer,
            self.gl.bind_framebuffer,
        )
        else {
            return None;
        };
        let mut fbo = 0;
        let mut texture = 0;
        unsafe {
            gen_framebuffers(1, &mut fbo);
            gen_textures(1, &mut texture);
            bind_texture(GL_TEXTURE_2D, texture);
            tex_image(
                GL_TEXTURE_2D,
                0,
                GL_RGBA8,
                edge(self.width),
                edge(self.height),
                0,
                GL_RGBA,
                GL_UNSIGNED_BYTE,
                std::ptr::null(),
            );
            tex_param(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR);
            tex_param(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR);
            tex_param(GL_TEXTURE_2D, GL_TEXTURE_WRAP_S, GL_CLAMP_TO_EDGE);
            tex_param(GL_TEXTURE_2D, GL_TEXTURE_WRAP_T, GL_CLAMP_TO_EDGE);
            bind(GL_FRAMEBUFFER, fbo);
            framebuffer_texture(
                GL_FRAMEBUFFER,
                GL_COLOR_ATTACHMENT0,
                GL_TEXTURE_2D,
                texture,
                0,
            );
            let complete = check(GL_FRAMEBUFFER) == GL_FRAMEBUFFER_COMPLETE;
            bind(GL_FRAMEBUFFER, 0);
            if fbo == 0 || texture == 0 || !complete {
                if let Some(delete) = self.gl.delete_framebuffers {
                    if fbo != 0 {
                        delete(1, &fbo);
                    }
                }
                if let Some(delete) = self.gl.delete_textures {
                    if texture != 0 {
                        delete(1, &texture);
                    }
                }
                return None;
            }
        }
        Some(Capture { fbo, texture })
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        if let Some(capture) = self.capture.take() {
            unsafe {
                if let Some(delete) = self.gl.delete_framebuffers {
                    delete(1, &capture.fbo);
                }
                if let Some(delete) = self.gl.delete_textures {
                    delete(1, &capture.texture);
                }
            }
        }
    }
}

fn load_gl(proc: impl Fn(&str) -> *const c_void) -> Result<Gl, String> {
    Ok(Gl {
        read_pixels: required(&proc, "glReadPixels")?,
        viewport: required(&proc, "glViewport")?,
        pixel_store: required(&proc, "glPixelStorei")?,
        bind_framebuffer: optional(&proc, "glBindFramebuffer"),
        gen_framebuffers: optional(&proc, "glGenFramebuffers"),
        delete_framebuffers: optional(&proc, "glDeleteFramebuffers"),
        gen_textures: optional(&proc, "glGenTextures"),
        delete_textures: optional(&proc, "glDeleteTextures"),
        bind_texture: optional(&proc, "glBindTexture"),
        tex_image: optional(&proc, "glTexImage2D"),
        tex_param: optional(&proc, "glTexParameteri"),
        framebuffer_texture: optional(&proc, "glFramebufferTexture2D"),
        check_framebuffer: optional(&proc, "glCheckFramebufferStatus"),
    })
}

fn required<T>(proc: &impl Fn(&str) -> *const c_void, name: &str) -> Result<T, String> {
    optional(proc, name).ok_or_else(|| format!("OpenGL is missing {name}"))
}

fn optional<T>(proc: &impl Fn(&str) -> *const c_void, name: &str) -> Option<T> {
    // SAFETY: `proc` returns the entry point named `name`, or a null/failure
    // address. `fn_ptr` rejects those. The caller names the matching signature.
    unsafe { fn_ptr(proc(name)) }
}

/// # Safety
/// `ptr` is either null, a tiny failure code, or a function pointer with the
/// signature of `T`.
unsafe fn fn_ptr<T>(ptr: *const c_void) -> Option<T> {
    const {
        assert!(size_of::<T>() == size_of::<*const ()>());
    }
    if ptr.is_null() || (ptr as isize).unsigned_abs() <= 3 {
        return None;
    }
    // SAFETY: the caller promised this address has signature `T`, and the
    // sizes match, so the bit pattern is that function pointer.
    Some(unsafe { std::mem::transmute_copy(&ptr) })
}

fn edge(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

pub(crate) fn byte_count(width: u32, height: u32) -> Result<usize, String> {
    usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "MilkDrop resolution is too large.".to_string())
}

#[cfg(windows)]
mod platform {
    use std::ffi::{c_void, CString};

    use super::Platform;

    #[repr(C)]
    struct PixelFormat {
        size: u16,
        version: u16,
        flags: u32,
        pixel_type: u8,
        color_bits: u8,
        red_bits: u8,
        red_shift: u8,
        green_bits: u8,
        green_shift: u8,
        blue_bits: u8,
        blue_shift: u8,
        alpha_bits: u8,
        alpha_shift: u8,
        accum_bits: u8,
        accum_red: u8,
        accum_green: u8,
        accum_blue: u8,
        accum_alpha: u8,
        depth_bits: u8,
        stencil_bits: u8,
        aux_buffers: u8,
        layer_type: u8,
        reserved: u8,
        layer_mask: u32,
        visible_mask: u32,
        damage_mask: u32,
    }

    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[repr(C)]
    struct WindowClass {
        style: u32,
        procedure: Option<unsafe extern "system" fn(isize, u32, usize, isize) -> isize>,
        class_extra: i32,
        window_extra: i32,
        instance: isize,
        icon: isize,
        cursor: isize,
        background: isize,
        menu: *const u16,
        class_name: *const u16,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetModuleHandleW(name: *const u16) -> isize;
        fn GetModuleHandleA(name: *const i8) -> isize;
        fn GetProcAddress(module: isize, name: *const i8) -> *const c_void;
        fn GetLastError() -> u32;
    }

    #[link(name = "user32")]
    extern "system" {
        fn RegisterClassW(class: *const WindowClass) -> u16;
        fn CreateWindowExW(
            ex_style: u32,
            class_name: *const u16,
            window_name: *const u16,
            style: u32,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            parent: isize,
            menu: isize,
            instance: isize,
            param: *mut c_void,
        ) -> isize;
        fn DestroyWindow(window: isize) -> i32;
        fn DefWindowProcW(window: isize, message: u32, wparam: usize, lparam: isize) -> isize;
        fn GetDC(window: isize) -> isize;
        fn ReleaseDC(window: isize, dc: isize) -> i32;
        fn GetClientRect(window: isize, rect: *mut Rect) -> i32;
    }

    #[link(name = "gdi32")]
    extern "system" {
        fn ChoosePixelFormat(dc: isize, format: *const PixelFormat) -> i32;
        fn SetPixelFormat(dc: isize, format: i32, descriptor: *const PixelFormat) -> i32;
    }

    #[link(name = "opengl32")]
    extern "system" {
        fn wglCreateContext(dc: isize) -> isize;
        fn wglDeleteContext(context: isize) -> i32;
        fn wglMakeCurrent(dc: isize, context: isize) -> i32;
        fn wglGetProcAddress(name: *const i8) -> *const c_void;
    }

    const CLASS_ALREADY_EXISTS: u32 = 1410;
    const OWN_DC: u32 = 0x0020;
    const POPUP: u32 = 0x8000_0000;
    const TOOL_WINDOW: u32 = 0x0000_0080;
    const NO_ACTIVATE: u32 = 0x0800_0000;
    const DRAW_TO_WINDOW: u32 = 0x4;
    const SUPPORT_OPENGL: u32 = 0x20;
    const DOUBLE_BUFFER: u32 = 0x1;

    pub(super) struct Surface {
        window: isize,
        dc: isize,
        context: isize,
    }

    impl Platform {
        pub(super) fn open(width: u32, height: u32) -> Result<Self, String> {
            Surface::open(width, height).map(|surface| Self { surface })
        }

        pub(super) fn size(&self) -> Option<(u32, u32)> {
            self.surface.client_size()
        }

        pub(super) fn proc(&self, name: &str) -> *const c_void {
            self.surface.proc(name)
        }
    }

    impl Drop for Platform {
        fn drop(&mut self) {
            self.surface.destroy();
        }
    }

    impl Surface {
        fn open(width: u32, height: u32) -> Result<Self, String> {
            let class_name = wide("DjusbProjectM");
            let title = wide("DJ USB MilkDrop");
            let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
            let class = WindowClass {
                style: OWN_DC,
                procedure: Some(window_proc),
                class_extra: 0,
                window_extra: 0,
                instance,
                icon: 0,
                cursor: 0,
                background: 0,
                menu: std::ptr::null(),
                class_name: class_name.as_ptr(),
            };
            let registered = unsafe { RegisterClassW(&class) };
            if registered == 0 && unsafe { GetLastError() } != CLASS_ALREADY_EXISTS {
                return Err("MilkDrop could not open a drawing surface.".to_string());
            }
            let window = unsafe {
                CreateWindowExW(
                    TOOL_WINDOW | NO_ACTIVATE,
                    class_name.as_ptr(),
                    title.as_ptr(),
                    POPUP,
                    0,
                    0,
                    super::edge(width),
                    super::edge(height),
                    0,
                    0,
                    instance,
                    std::ptr::null_mut(),
                )
            };
            if window == 0 {
                return Err("MilkDrop could not open a drawing surface.".to_string());
            }
            let dc = unsafe { GetDC(window) };
            if dc == 0 {
                unsafe { DestroyWindow(window) };
                return Err("MilkDrop could not open a drawing surface.".to_string());
            }
            let format = pixel_format();
            let chosen = unsafe { ChoosePixelFormat(dc, &format) };
            if chosen == 0 || unsafe { SetPixelFormat(dc, chosen, &format) } == 0 {
                unsafe {
                    ReleaseDC(window, dc);
                    DestroyWindow(window);
                }
                return Err("MilkDrop could not choose an OpenGL format.".to_string());
            }
            let legacy = unsafe { wglCreateContext(dc) };
            if legacy == 0 || unsafe { wglMakeCurrent(dc, legacy) } == 0 {
                if legacy != 0 {
                    unsafe { wglDeleteContext(legacy) };
                }
                unsafe {
                    ReleaseDC(window, dc);
                    DestroyWindow(window);
                }
                return Err("MilkDrop could not start OpenGL.".to_string());
            }
            let context = match core_context(dc, legacy) {
                Ok(context) => context,
                Err(message) => {
                    unsafe {
                        ReleaseDC(window, dc);
                        DestroyWindow(window);
                    }
                    return Err(message);
                }
            };
            Ok(Self {
                window,
                dc,
                context,
            })
        }

        fn client_size(&self) -> Option<(u32, u32)> {
            let mut rect = Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            if unsafe { GetClientRect(self.window, &mut rect) } == 0 {
                return None;
            }
            let width = rect.right.saturating_sub(rect.left);
            let height = rect.bottom.saturating_sub(rect.top);
            if width <= 0 || height <= 0 {
                return None;
            }
            Some((
                u32::try_from(width).unwrap_or(1),
                u32::try_from(height).unwrap_or(1),
            ))
        }

        fn proc(&self, name: &str) -> *const c_void {
            let Ok(symbol) = CString::new(name) else {
                return std::ptr::null();
            };
            let found = unsafe { wglGetProcAddress(symbol.as_ptr()) };
            if !super::failed(found) {
                return found;
            }
            let module = unsafe { GetModuleHandleA(c"opengl32.dll".as_ptr()) };
            unsafe { GetProcAddress(module, symbol.as_ptr()) }
        }

        fn destroy(&self) {
            unsafe {
                wglMakeCurrent(0, 0);
                wglDeleteContext(self.context);
                ReleaseDC(self.window, self.dc);
                DestroyWindow(self.window);
            }
        }
    }

    /// Replaces the legacy context with an OpenGL 3.3 core context.
    ///
    /// projectM crashes inside `projectm_create` on the legacy context
    /// `wglCreateContext` returns. The legacy context is deleted here.
    fn core_context(dc: isize, legacy: isize) -> Result<isize, String> {
        type Create = unsafe extern "system" fn(isize, isize, *const i32) -> isize;
        let symbol = CString::new("wglCreateContextAttribsARB").expect("name is static");
        let create = unsafe { super::fn_ptr::<Create>(wglGetProcAddress(symbol.as_ptr())) };
        let Some(create) = create else {
            unsafe {
                wglMakeCurrent(0, 0);
                wglDeleteContext(legacy);
            }
            return Err("MilkDrop needs OpenGL 3.3.".to_string());
        };
        // WGL_CONTEXT_MAJOR_VERSION_ARB, MINOR, PROFILE_MASK, CORE_PROFILE_BIT.
        let attribs = [0x2091, 3, 0x2092, 3, 0x9126, 0x0001, 0];
        let core = unsafe { create(dc, 0, attribs.as_ptr()) };
        unsafe {
            wglMakeCurrent(0, 0);
            wglDeleteContext(legacy);
        }
        if core == 0 || unsafe { wglMakeCurrent(dc, core) } == 0 {
            if core != 0 {
                unsafe { wglDeleteContext(core) };
            }
            return Err("MilkDrop needs OpenGL 3.3.".to_string());
        }
        Ok(core)
    }

    unsafe extern "system" fn window_proc(
        window: isize,
        message: u32,
        wparam: usize,
        lparam: isize,
    ) -> isize {
        unsafe { DefWindowProcW(window, message, wparam, lparam) }
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn pixel_format() -> PixelFormat {
        PixelFormat {
            size: u16::try_from(size_of::<PixelFormat>()).unwrap_or(40),
            version: 1,
            flags: DRAW_TO_WINDOW | SUPPORT_OPENGL | DOUBLE_BUFFER,
            pixel_type: 0,
            color_bits: 32,
            red_bits: 0,
            red_shift: 0,
            green_bits: 0,
            green_shift: 0,
            blue_bits: 0,
            blue_shift: 0,
            alpha_bits: 8,
            alpha_shift: 0,
            accum_bits: 0,
            accum_red: 0,
            accum_green: 0,
            accum_blue: 0,
            accum_alpha: 0,
            depth_bits: 16,
            stencil_bits: 0,
            aux_buffers: 0,
            layer_type: 0,
            reserved: 0,
            layer_mask: 0,
            visible_mask: 0,
            damage_mask: 0,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::PixelFormat;

        #[test]
        fn the_pixel_format_matches_the_windows_struct() {
            assert_eq!(size_of::<PixelFormat>(), 40);
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::ffi::{c_void, CString};

    use libloading::Library;

    use super::Platform;

    type EglInt = i32;
    type GetDisplay = unsafe extern "C" fn(*const c_void) -> *mut c_void;
    type Initialize = unsafe extern "C" fn(*mut c_void, *mut EglInt, *mut EglInt) -> u32;
    type BindApi = unsafe extern "C" fn(u32) -> u32;
    type ChooseConfig = unsafe extern "C" fn(
        *mut c_void,
        *const EglInt,
        *mut *mut c_void,
        EglInt,
        *mut EglInt,
    ) -> u32;
    type CreatePbuffer =
        unsafe extern "C" fn(*mut c_void, *mut c_void, *const EglInt) -> *mut c_void;
    type CreateContext =
        unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const EglInt) -> *mut c_void;
    type MakeCurrent =
        unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut c_void) -> u32;
    type GetProc = unsafe extern "C" fn(*const i8) -> *const c_void;
    type DestroySurface = unsafe extern "C" fn(*mut c_void, *mut c_void) -> u32;
    type DestroyContext = unsafe extern "C" fn(*mut c_void, *mut c_void) -> u32;
    type Terminate = unsafe extern "C" fn(*mut c_void) -> u32;
    type GetError = unsafe extern "C" fn() -> EglInt;

    const OPENGL_API: u32 = 0x30A2;
    const GLES_API: u32 = 0x30A0;
    const SURFACE_TYPE: EglInt = 0x3033;
    const PBUFFER_BIT: EglInt = 0x0001;
    const RENDERABLE: EglInt = 0x3040;
    const OPENGL_BIT: EglInt = 0x0008;
    const GLES3_BIT: EglInt = 0x0040;
    const NONE: EglInt = 0x3038;
    const RED: EglInt = 0x3024;
    const GREEN: EglInt = 0x3023;
    const BLUE: EglInt = 0x3022;
    const ALPHA: EglInt = 0x3021;
    const WIDTH: EglInt = 0x3057;
    const HEIGHT: EglInt = 0x3056;
    const MAJOR: EglInt = 0x3098;
    const MINOR: EglInt = 0x30FB;

    struct Attempt {
        api: u32,
        renderable: EglInt,
        major: EglInt,
        minor: EglInt,
    }

    const ATTEMPTS: [Attempt; 2] = [
        Attempt {
            api: OPENGL_API,
            renderable: OPENGL_BIT,
            major: 3,
            minor: 3,
        },
        Attempt {
            api: GLES_API,
            renderable: GLES3_BIT,
            major: 3,
            minor: 0,
        },
    ];

    struct Egl {
        _library: Library,
        get_display: GetDisplay,
        initialize: Initialize,
        bind_api: BindApi,
        choose: ChooseConfig,
        pbuffer: CreatePbuffer,
        context: CreateContext,
        make_current: MakeCurrent,
        get_proc: GetProc,
        destroy_surface: DestroySurface,
        destroy_context: DestroyContext,
        terminate: Terminate,
        get_error: GetError,
    }

    pub(super) struct Surface {
        egl: Egl,
        display: *mut c_void,
        surface: *mut c_void,
        context: *mut c_void,
        width: u32,
        height: u32,
    }

    impl Platform {
        pub(super) fn open(width: u32, height: u32) -> Result<Self, String> {
            Surface::open(width, height).map(|surface| Self { surface })
        }

        pub(super) fn size(&self) -> Option<(u32, u32)> {
            Some((self.surface.width, self.surface.height))
        }

        pub(super) fn proc(&self, name: &str) -> *const c_void {
            self.surface.proc(name)
        }
    }

    impl Drop for Platform {
        fn drop(&mut self) {
            self.surface.destroy();
        }
    }

    impl Egl {
        fn load() -> Result<Self, String> {
            let library = open_egl()?;
            Ok(Self {
                get_display: symbol(&library, b"eglGetDisplay\0")?,
                initialize: symbol(&library, b"eglInitialize\0")?,
                bind_api: symbol(&library, b"eglBindAPI\0")?,
                choose: symbol(&library, b"eglChooseConfig\0")?,
                pbuffer: symbol(&library, b"eglCreatePbufferSurface\0")?,
                context: symbol(&library, b"eglCreateContext\0")?,
                make_current: symbol(&library, b"eglMakeCurrent\0")?,
                get_proc: symbol(&library, b"eglGetProcAddress\0")?,
                destroy_surface: symbol(&library, b"eglDestroySurface\0")?,
                destroy_context: symbol(&library, b"eglDestroyContext\0")?,
                terminate: symbol(&library, b"eglTerminate\0")?,
                get_error: symbol(&library, b"eglGetError\0")?,
                _library: library,
            })
        }
    }

    impl Surface {
        fn open(width: u32, height: u32) -> Result<Self, String> {
            let egl = Egl::load()?;
            let display = unsafe { (egl.get_display)(std::ptr::null()) };
            if display.is_null()
                || unsafe { (egl.initialize)(display, std::ptr::null_mut(), std::ptr::null_mut()) }
                    == 0
            {
                return Err(format!("MilkDrop could not open EGL ({}).", unsafe {
                    (egl.get_error)()
                }));
            }
            for attempt in ATTEMPTS {
                if let Some((surface, context)) = try_attempt(&egl, display, width, height, attempt)
                {
                    return Ok(Self {
                        egl,
                        display,
                        surface,
                        context,
                        width,
                        height,
                    });
                }
            }
            unsafe { (egl.terminate)(display) };
            Err("MilkDrop could not start OpenGL.".to_string())
        }

        fn proc(&self, name: &str) -> *const c_void {
            let Ok(symbol) = CString::new(name) else {
                return std::ptr::null();
            };
            unsafe { (self.egl.get_proc)(symbol.as_ptr()) }
        }

        fn destroy(&self) {
            unsafe {
                (self.egl.make_current)(
                    self.display,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                );
                (self.egl.destroy_context)(self.display, self.context);
                (self.egl.destroy_surface)(self.display, self.surface);
                (self.egl.terminate)(self.display);
            }
        }
    }

    fn try_attempt(
        egl: &Egl,
        display: *mut c_void,
        width: u32,
        height: u32,
        attempt: Attempt,
    ) -> Option<(*mut c_void, *mut c_void)> {
        if unsafe { (egl.bind_api)(attempt.api) } == 0 {
            return None;
        }
        let config_attribs = [
            SURFACE_TYPE,
            PBUFFER_BIT,
            RENDERABLE,
            attempt.renderable,
            RED,
            8,
            GREEN,
            8,
            BLUE,
            8,
            ALPHA,
            8,
            NONE,
        ];
        let mut config = std::ptr::null_mut();
        let mut count = 0;
        if unsafe { (egl.choose)(display, config_attribs.as_ptr(), &mut config, 1, &mut count) }
            == 0
            || count == 0
            || config.is_null()
        {
            return None;
        }
        let surface_attribs = [WIDTH, super::edge(width), HEIGHT, super::edge(height), NONE];
        let surface = unsafe { (egl.pbuffer)(display, config, surface_attribs.as_ptr()) };
        if surface.is_null() {
            return None;
        }
        let context_attribs = [MAJOR, attempt.major, MINOR, attempt.minor, NONE];
        let context = unsafe {
            (egl.context)(
                display,
                config,
                std::ptr::null_mut(),
                context_attribs.as_ptr(),
            )
        };
        if context.is_null()
            || unsafe { (egl.make_current)(display, surface, surface, context) } == 0
        {
            if !context.is_null() {
                unsafe { (egl.destroy_context)(display, context) };
            }
            unsafe { (egl.destroy_surface)(display, surface) };
            return None;
        }
        Some((surface, context))
    }

    fn open_egl() -> Result<Library, String> {
        for name in ["libEGL.so.1", "libEGL.so"] {
            if let Ok(library) = unsafe { Library::new(name) } {
                return Ok(library);
            }
        }
        Err("MilkDrop could not open libEGL.".to_string())
    }

    fn symbol<T: Copy>(library: &Library, name: &[u8]) -> Result<T, String> {
        let symbol = unsafe { library.get::<T>(name) }.map_err(|_| {
            format!(
                "libEGL is missing {}",
                String::from_utf8_lossy(name).trim_end_matches('\0')
            )
        })?;
        Ok(*symbol)
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use std::ffi::c_void;

    use super::Platform;

    pub(super) struct Surface;

    impl Platform {
        pub(super) fn open(_width: u32, _height: u32) -> Result<Self, String> {
            Err("MilkDrop runs on Windows and Linux.".to_string())
        }

        pub(super) fn size(&self) -> Option<(u32, u32)> {
            None
        }

        pub(super) fn proc(&self, _name: &str) -> *const c_void {
            std::ptr::null()
        }
    }
}

pub(crate) struct Platform {
    #[cfg(any(windows, target_os = "linux"))]
    surface: platform::Surface,
}

fn failed(ptr: *const c_void) -> bool {
    ptr.is_null() || (ptr as isize).unsigned_abs() <= 3
}

#[cfg(all(test, windows))]
mod tests {
    use std::path::PathBuf;

    #[test]
    fn projectm_starts_on_a_core_context() {
        let dll = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/device/projectm/projectM-4.dll");
        if !dll.is_file() {
            return;
        }
        let context = super::Context::open(64, 64).expect("context");
        let api = crate::api::Api::load(&dll).expect("library");
        let instance = api.create();
        assert!(!instance.is_null(), "projectM did not start");
        let (width, height) = context.size();
        api.configure(instance, width, height);
        let preset = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/device/presets/built-in/phosphor.milk");
        let path = std::ffi::CString::new(preset.to_string_lossy().as_ref()).expect("path");
        api.load_preset(instance, path.as_ptr(), false);
        let mut tone = Vec::with_capacity(4096);
        for frame in 0..2048 {
            let sample = (frame as f32 * 0.15).sin();
            tone.push(sample);
            tone.push(sample * 0.6);
        }
        let mut pixels = vec![0; usize::try_from(width * height * 4).unwrap_or(0)];
        let target = if api.renders_into_fbo() {
            context.fbo()
        } else {
            None
        };
        for _ in 0..30 {
            api.add_pcm(instance, &tone);
            if target.is_none() {
                context.bind_default();
            }
            api.render(instance, target);
        }
        context
            .read_from(target.unwrap_or(0), &mut pixels)
            .expect("read");
        let lit = pixels.iter().filter(|byte| **byte > 8).count();
        assert!(lit > 100, "the drawn buffer was black ({lit} lit pixels)");
        api.destroy(instance);
        drop(api);
        drop(context);
    }
}

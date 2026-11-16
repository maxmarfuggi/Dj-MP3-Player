//! Safe wrappers around the few operating-system calls the library needs.
//!
//! [`volume_label`] uses `GetVolumeInformationW` on Windows. Everywhere else
//! it returns an empty string, and the caller uses the mount name instead.

/// The volume label of a mounted drive, or an empty string when it has none
/// or the call fails.
///
/// `root` is a drive root such as `D:\`.
#[must_use]
pub fn volume_label(root: &std::ffi::OsStr) -> String {
    #[cfg(windows)]
    {
        windows::volume_label(root)
    }
    #[cfg(not(windows))]
    {
        let _ = root;
        String::new()
    }
}

#[cfg(windows)]
mod windows {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetVolumeInformationW(
            root: *const u16,
            name: *mut u16,
            name_capacity: u32,
            serial: *mut u32,
            max_component: *mut u32,
            flags: *mut u32,
            file_system: *mut u16,
            file_system_capacity: u32,
        ) -> i32;
    }

    pub(super) fn volume_label(root: &OsStr) -> String {
        let mut wide: Vec<u16> = root.encode_wide().collect();
        wide.push(0);
        let mut name = [0u16; 256];
        let ok = unsafe {
            GetVolumeInformationW(
                wide.as_ptr(),
                name.as_mut_ptr(),
                name.len() as u32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
            )
        };
        if ok == 0 {
            return String::new();
        }
        let end = name
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(name.len());
        String::from_utf16_lossy(&name[..end])
    }
}

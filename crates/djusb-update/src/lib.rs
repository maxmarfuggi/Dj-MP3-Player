//! Signed updates from a USB stick for the wall player.
//!
//! A stick carries a `DJUSB-UPDATE` directory. The signature is checked, the
//! payload is copied onto the system slot that is not the last known-good
//! one, and the next boot tries that slot once. The new system has to
//! confirm. A later boot that never confirms returns to the previous system.
//! The stick is only read.
//!
//! The image calls `djusb-update boot` before the player starts, `djusb-update
//! confirm` once `djusb.service` is up, and `djusb-update install-mounted`
//! when a stick is mounted. The slot record has to live on the data
//! partition (`/var/lib/djusb/slots.json`), not on the system slot being
//! replaced. Pack a stick with `djusb-update pack`. The private key stays off
//! the device.

mod bundle;
mod slots;

pub use bundle::{
    bundle_on_stick, find_bundles, write_bundle, write_bundle_from_file, SigningKey, VerifyingKey,
    BUNDLE_DIR,
};
pub use slots::{
    boot, confirm, init, install, install_mounted, BootChoice, Installed, Slot, STATE_PATH,
    TRIAL_BOOTS,
};

/// A failure while packing, checking, installing, or choosing a slot.
#[derive(Debug)]
pub enum Error {
    /// A bundle file or the slot record could not be read or written.
    Io(std::io::Error),
    /// A file or the slot record is not in the expected shape.
    Format(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Format(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Format(_) => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(DIGITS[(byte >> 4) as usize]));
        out.push(char::from(DIGITS[(byte & 0x0f) as usize]));
    }
    out
}

pub(crate) fn decode_hex(text: &str) -> Result<Vec<u8>, Error> {
    let text = text.trim();
    if !text.len().is_multiple_of(2) {
        return Err(Error::Format("a hex value is incomplete".to_string()));
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut index = 0;
    while index < bytes.len() {
        let high = hex_digit(bytes[index])?;
        let low = hex_digit(bytes[index + 1])?;
        out.push((high << 4) | low);
        index += 2;
    }
    Ok(out)
}

fn hex_digit(byte: u8) -> Result<u8, Error> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(Error::Format("a hex value is not hex".to_string())),
    }
}

/// Collapses `.` and `..` without looking at the filesystem.
pub(crate) fn lexical(path: &std::path::Path) -> std::path::PathBuf {
    let mut out = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

pub(crate) fn is_within(path: &std::path::Path, root: &std::path::Path) -> bool {
    let Ok(path) = std::path::absolute(path) else {
        return false;
    };
    let Ok(root) = std::path::absolute(root) else {
        return false;
    };
    lexical(&path).starts_with(lexical(&root))
}

pub(crate) fn sha256_file(path: &std::path::Path) -> Result<String, Error> {
    use std::io::Read;

    use sha2::{Digest, Sha256};

    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

pub(crate) fn write_replacing(path: &std::path::Path, bytes: &[u8]) -> Result<(), Error> {
    use std::io::Write;

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut temporary = path.as_os_str().to_os_string();
    temporary.push(".tmp");
    let temporary = std::path::PathBuf::from(temporary);
    {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    match std::fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(_) if path.exists() => {
            std::fs::remove_file(path)?;
            std::fs::rename(&temporary, path).inspect_err(|_| {
                let _ = std::fs::remove_file(&temporary);
            })?;
            Ok(())
        }
        Err(error) => {
            let _ = std::fs::remove_file(&temporary);
            Err(error.into())
        }
    }
}

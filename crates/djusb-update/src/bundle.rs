//! The directory a USB stick carries, and the key that signs it.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signer, Verifier};

use crate::{decode_hex, hex, sha256_file, write_replacing, Error};

/// Directory name at the root of an update stick.
pub const BUNDLE_DIR: &str = "DJUSB-UPDATE";

const SIGNING_HEADER: &str = "djusb-update signing key";
const VERIFYING_HEADER: &str = "djusb-update verifying key";
const PAYLOAD_NAME: &str = "payload";

/// Key that signs an update bundle. Stays off the device.
pub struct SigningKey {
    inner: ed25519_dalek::SigningKey,
}

/// Key the device uses to accept an update bundle.
#[derive(Clone)]
pub struct VerifyingKey {
    inner: ed25519_dalek::VerifyingKey,
}

impl std::fmt::Debug for SigningKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SigningKey")
    }
}

impl Clone for SigningKey {
    fn clone(&self) -> Self {
        Self {
            inner: ed25519_dalek::SigningKey::from_bytes(&self.inner.to_bytes()),
        }
    }
}

impl SigningKey {
    /// A new random signing key.
    pub fn generate() -> Result<Self, Error> {
        let mut bytes = [0u8; 32];
        getrandom::getrandom(&mut bytes)
            .map_err(|error| Error::Io(io::Error::other(error.to_string())))?;
        Ok(Self {
            inner: ed25519_dalek::SigningKey::from_bytes(&bytes),
        })
    }

    /// The verifying key that matches this signing key.
    #[must_use]
    pub fn verifying_key(&self) -> VerifyingKey {
        VerifyingKey {
            inner: self.inner.verifying_key(),
        }
    }

    /// Signature over the exact bytes of `message`.
    #[must_use]
    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.inner.sign(message).to_bytes()
    }

    /// Writes the signing key to `path`.
    pub fn save(&self, path: &Path) -> Result<(), Error> {
        let text = format!("{SIGNING_HEADER}\n{}\n", hex(&self.inner.to_bytes()));
        write_replacing(path, text.as_bytes())?;
        restrict_to_owner(path)?;
        Ok(())
    }

    /// Reads a signing key written by [`SigningKey::save`].
    pub fn load(path: &Path) -> Result<Self, Error> {
        let text = fs::read_to_string(path)?;
        let bytes = key_bytes(&text, SIGNING_HEADER)?;
        Ok(Self {
            inner: ed25519_dalek::SigningKey::from_bytes(&bytes),
        })
    }
}

impl std::fmt::Debug for VerifyingKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VerifyingKey")
    }
}

impl VerifyingKey {
    /// Writes the verifying key to `path`.
    pub fn save(&self, path: &Path) -> Result<(), Error> {
        let text = format!("{VERIFYING_HEADER}\n{}\n", hex(&self.inner.to_bytes()));
        write_replacing(path, text.as_bytes())
    }

    /// Reads a verifying key written by [`VerifyingKey::save`].
    pub fn load(path: &Path) -> Result<Self, Error> {
        let text = fs::read_to_string(path)?;
        let bytes = key_bytes(&text, VERIFYING_HEADER)?;
        let inner = ed25519_dalek::VerifyingKey::from_bytes(&bytes)
            .map_err(|_| Error::Format("the verifying key is not a key".to_string()))?;
        Ok(Self { inner })
    }

    pub(crate) fn verify(&self, message: &[u8], signature: &[u8; 64]) -> Result<(), Error> {
        let signature = ed25519_dalek::Signature::from_bytes(signature);
        self.inner
            .verify(message, &signature)
            .map_err(|_| Error::Format("the signature does not match the update".to_string()))
    }
}

/// Writes a signed `DJUSB-UPDATE` directory. `version` is the version the
/// device will record for the slot.
pub fn write_bundle(
    dir: &Path,
    version: &str,
    payload: &[u8],
    key: &SigningKey,
) -> Result<(), Error> {
    check_version(version)?;
    fs::create_dir_all(dir)?;
    let payload_path = dir.join(PAYLOAD_NAME);
    write_replacing(&payload_path, payload)?;
    finish_bundle(dir, version, &payload_path, key)
}

/// Writes a signed bundle, copying `payload` into it.
pub fn write_bundle_from_file(
    dir: &Path,
    version: &str,
    payload: &Path,
    key: &SigningKey,
) -> Result<(), Error> {
    check_version(version)?;
    fs::create_dir_all(dir)?;
    let payload_path = dir.join(PAYLOAD_NAME);
    copy_file(payload, &payload_path)?;
    finish_bundle(dir, version, &payload_path, key)
}

fn finish_bundle(dir: &Path, version: &str, payload: &Path, key: &SigningKey) -> Result<(), Error> {
    let digest = sha256_file(payload)?;
    let manifest = manifest_bytes(version, &digest);
    let signature = hex(&key.sign(&manifest));
    write_replacing(&dir.join("manifest.json"), &manifest)?;
    write_replacing(
        &dir.join("manifest.sig"),
        format!("{signature}\n").as_bytes(),
    )?;
    Ok(())
}

/// `stick/DJUSB-UPDATE` when that directory contains a manifest.
#[must_use]
pub fn bundle_on_stick(stick: &Path) -> Option<PathBuf> {
    let bundle = stick.join(BUNDLE_DIR);
    bundle.join("manifest.json").is_file().then_some(bundle)
}

/// Bundle directories under `roots`, including mounts one and two levels down.
#[must_use]
pub fn find_bundles(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for root in roots {
        visit(&mut found, root, 0);
    }
    found.sort();
    found.dedup();
    found
}

fn visit(found: &mut Vec<PathBuf>, dir: &Path, depth: usize) {
    if let Some(bundle) = bundle_on_stick(dir) {
        found.push(bundle);
    }
    if depth == 2 {
        return;
    }
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    let mut children = Vec::new();
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            children.push(path);
        }
    }
    children.sort();
    for child in children {
        visit(found, &child, depth + 1);
    }
}

pub(crate) fn check_version(version: &str) -> Result<(), Error> {
    if version.is_empty() || version.len() > 64 {
        return Err(Error::Format("the update version is missing".to_string()));
    }
    if !version
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'+' | b'-'))
    {
        return Err(Error::Format(
            "the update version is not a version".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn manifest_bytes(version: &str, digest: &str) -> Vec<u8> {
    format!(
        "{{\"payload\":\"{PAYLOAD_NAME}\",\"sha256\":\"{digest}\",\"version\":\"{version}\"}}\n"
    )
    .into_bytes()
}

pub(crate) struct Manifest {
    pub version: String,
    pub sha256: String,
    pub payload: String,
}

/// Reads and verifies a bundle. Does not write.
pub(crate) fn read_bundle(dir: &Path, key: &VerifyingKey) -> Result<Manifest, Error> {
    let manifest = fs::read(dir.join("manifest.json"))?;
    let signature = fs::read_to_string(dir.join("manifest.sig"))?;
    let signature = decode_hex(signature.trim())?;
    let signature: [u8; 64] = signature
        .try_into()
        .map_err(|_| Error::Format("the signature does not match the update".to_string()))?;
    key.verify(&manifest, &signature)?;
    let value: serde_json::Value = serde_json::from_slice(&manifest)
        .map_err(|_| Error::Format("the manifest is not an update".to_string()))?;
    let payload = string_field(&value, "payload")?;
    let sha256 = string_field(&value, "sha256")?;
    let version = string_field(&value, "version")?;
    check_version(&version)?;
    if payload != PAYLOAD_NAME {
        return Err(Error::Format(
            "the payload path is not inside the update stick".to_string(),
        ));
    }
    let payload_path = dir.join(PAYLOAD_NAME);
    let digest = sha256_file(&payload_path)?;
    if !digest.eq_ignore_ascii_case(&sha256) {
        return Err(Error::Format(
            "the payload hash does not match the signed manifest".to_string(),
        ));
    }
    Ok(Manifest {
        version,
        sha256: digest,
        payload: payload_path
            .to_str()
            .ok_or_else(|| {
                Error::Format("the payload path is not inside the update stick".to_string())
            })?
            .to_string(),
    })
}

fn string_field(value: &serde_json::Value, name: &str) -> Result<String, Error> {
    value
        .get(name)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| Error::Format(format!("the manifest has no {name}")))
}

pub(crate) fn copy_file(src: &Path, dest: &Path) -> Result<(), Error> {
    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let mut input = File::open(src)?;
    let mut output = open_slot(dest)?;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        output.write_all(&buffer[..read])?;
    }
    output.sync_all()?;
    Ok(())
}

/// Opens a slot image for replacement. A regular file is truncated. A
/// partition rejects truncate, so the copy overwrites it from the first byte.
fn open_slot(dest: &Path) -> Result<File, Error> {
    if !dest.exists() {
        return Ok(File::create(dest)?);
    }
    let file = OpenOptions::new().write(true).open(dest)?;
    if let Err(error) = file.set_len(0) {
        let unsupported = error.kind() == io::ErrorKind::InvalidInput
            || error.raw_os_error() == Some(22)
            || error.raw_os_error() == Some(25);
        if !unsupported {
            return Err(error.into());
        }
    }
    Ok(file)
}

fn key_bytes(text: &str, header: &str) -> Result<[u8; 32], Error> {
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    let got = lines.next().unwrap_or("");
    if got != header {
        return Err(Error::Format(
            "the key file is not a djusb-update key".to_string(),
        ));
    }
    let hex_line = lines
        .next()
        .ok_or_else(|| Error::Format("the key file has no key".to_string()))?;
    let bytes = decode_hex(hex_line)?;
    bytes
        .try_into()
        .map_err(|_| Error::Format("the key is not 32 bytes".to_string()))
}

fn restrict_to_owner(path: &Path) -> Result<(), Error> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

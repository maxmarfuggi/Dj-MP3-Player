//! The update stick installs onto the inactive system slot, and a system that
//! never confirms is left behind on the next boot.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use djusb_update::{
    boot, bundle_on_stick, confirm, find_bundles, init, install, install_mounted, write_bundle,
    BootChoice, SigningKey, Slot, BUNDLE_DIR,
};

static SCRATCH: AtomicU64 = AtomicU64::new(0);

struct Tmp {
    path: PathBuf,
}

impl Tmp {
    fn new() -> Self {
        let n = SCRATCH.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("djusb-update-{nanos}-{n}"));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn layout(dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
    (
        dir.join("slots.json"),
        dir.join("slot-a.img"),
        dir.join("slot-b.img"),
    )
}

fn stick_with(payload: &[u8], version: &str, key: &SigningKey) -> (Tmp, PathBuf) {
    let tmp = Tmp::new();
    let bundle = tmp.path().join(BUNDLE_DIR);
    write_bundle(&bundle, version, payload, key).unwrap();
    (tmp, bundle)
}

fn files_in(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut found = Vec::new();
    let mut names = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    names.sort();
    for path in names {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let bytes = if path.is_file() {
            fs::read(&path).unwrap()
        } else {
            Vec::new()
        };
        found.push((name, bytes));
    }
    found
}

#[test]
fn a_signed_bundle_verifies_and_a_tampered_one_does_not() {
    let key = SigningKey::generate().unwrap();
    let other = SigningKey::generate().unwrap();
    let tmp = Tmp::new();
    let bundle = tmp.path().join("stick").join(BUNDLE_DIR);
    write_bundle(&bundle, "1.2.0", b"rootfs-v2", &key).unwrap();

    let (state, slot_a, slot_b) = layout(&tmp.path().join("box"));
    fs::create_dir_all(state.parent().unwrap()).unwrap();
    init(&state, &slot_a, &slot_b).unwrap();
    let installed = install(&bundle, &state, &key.verifying_key()).unwrap();
    assert_eq!(installed.version, "1.2.0");
    assert_eq!(fs::read(&installed.image).unwrap(), b"rootfs-v2");

    let forged = tmp.path().join("forged");
    write_bundle(&forged, "9.9.9", b"rootfs-evil", &other).unwrap();
    let error = install(&forged, &state, &key.verifying_key()).unwrap_err();
    assert!(
        error.to_string().to_lowercase().contains("signature"),
        "{error}"
    );
    assert_eq!(fs::read(&installed.image).unwrap(), b"rootfs-v2");

    let manifest = fs::read(bundle.join("manifest.json")).unwrap();
    let mut payload = fs::read(bundle.join("payload")).unwrap();
    payload[0] ^= 0xff;
    fs::write(bundle.join("payload"), &payload).unwrap();
    let error = install(&bundle, &state, &key.verifying_key()).unwrap_err();
    assert!(
        error.to_string().to_lowercase().contains("payload")
            || error.to_string().to_lowercase().contains("hash"),
        "{error}"
    );
    assert_eq!(fs::read(bundle.join("manifest.json")).unwrap(), manifest);
}

#[test]
fn a_signed_update_installs_onto_the_inactive_slot_and_leaves_the_stick_unchanged() {
    let key = SigningKey::generate().unwrap();
    let (stick, bundle) = stick_with(b"system-image", "1.0.1", &key);
    let before = files_in(&bundle);
    let box_dir = Tmp::new();
    let (state, slot_a, slot_b) = layout(box_dir.path());
    fs::write(&slot_a, b"running-system").unwrap();
    init(&state, &slot_a, &slot_b).unwrap();

    let installed = install(&bundle, &state, &key.verifying_key()).unwrap();
    assert_eq!(installed.slot, Slot::B);
    assert_eq!(installed.image, slot_b);
    assert_eq!(fs::read(&slot_b).unwrap(), b"system-image");
    assert_eq!(fs::read(&slot_a).unwrap(), b"running-system");
    assert_eq!(files_in(&bundle), before);
    assert_eq!(
        files_in(stick.path()),
        vec![(BUNDLE_DIR.to_string(), Vec::new())]
    );

    let choice = boot(&state, "boot-1").unwrap();
    assert_eq!(
        choice,
        BootChoice {
            slot: Slot::B,
            version: "1.0.1".to_string(),
            image: slot_b,
            rolled_back: false,
        }
    );
}

#[test]
fn a_bad_update_boots_the_previous_system() {
    let key = SigningKey::generate().unwrap();
    let tmp = Tmp::new();
    let (state, slot_a, slot_b) = layout(tmp.path());
    init(&state, &slot_a, &slot_b).unwrap();
    let bundle = tmp.path().join("stick").join(BUNDLE_DIR);
    write_bundle(&bundle, "2.0.0", b"bad-system", &key).unwrap();
    install(&bundle, &state, &key.verifying_key()).unwrap();

    let trial = boot(&state, "boot-1").unwrap();
    assert_eq!(trial.slot, Slot::B);
    assert!(!trial.rolled_back);
    let again = boot(&state, "boot-1").unwrap();
    assert_eq!(again.slot, Slot::B, "the same power-on keeps the trial");

    let rolled = boot(&state, "boot-2").unwrap();
    assert_eq!(
        rolled,
        BootChoice {
            slot: Slot::A,
            version: "factory".to_string(),
            image: slot_a.clone(),
            rolled_back: true,
        }
    );
    let stayed = boot(&state, "boot-3").unwrap();
    assert_eq!(stayed.slot, Slot::A);
    assert!(!stayed.rolled_back);
}

#[test]
fn a_confirmed_update_keeps_the_new_system() {
    let key = SigningKey::generate().unwrap();
    let tmp = Tmp::new();
    let (state, slot_a, slot_b) = layout(tmp.path());
    init(&state, &slot_a, &slot_b).unwrap();
    let bundle = tmp.path().join("stick").join(BUNDLE_DIR);
    write_bundle(&bundle, "2.0.0", b"good-system", &key).unwrap();
    install(&bundle, &state, &key.verifying_key()).unwrap();
    boot(&state, "boot-1").unwrap();

    let confirmed = confirm(&state).unwrap();
    assert_eq!(confirmed.slot, Slot::B);
    assert_eq!(confirmed.version, "2.0.0");

    let next = boot(&state, "boot-2").unwrap();
    assert_eq!(next.slot, Slot::B);
    assert!(!next.rolled_back);

    let follow = tmp.path().join("follow-up");
    write_bundle(&follow, "3.0.0", b"also-bad", &key).unwrap();
    let installed = install(&follow, &state, &key.verifying_key()).unwrap();
    assert_eq!(installed.slot, Slot::A);
    assert_eq!(fs::read(&slot_b).unwrap(), b"good-system");
    boot(&state, "boot-3").unwrap();
    let rolled = boot(&state, "boot-4").unwrap();
    assert_eq!(rolled.slot, Slot::B);
    assert_eq!(rolled.version, "2.0.0");
    assert!(rolled.rolled_back);
}

#[test]
fn a_payload_path_that_leaves_the_bundle_is_rejected() {
    let key = SigningKey::generate().unwrap();
    let tmp = Tmp::new();
    let outside = tmp.path().join("secret");
    fs::write(&outside, b"do-not-read").unwrap();
    let bundle = tmp.path().join(BUNDLE_DIR);
    fs::create_dir_all(&bundle).unwrap();
    let manifest = b"{\"payload\":\"../secret\",\"sha256\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"version\":\"1.0.0\"}\n";
    fs::write(bundle.join("manifest.json"), manifest).unwrap();
    fs::write(bundle.join("manifest.sig"), hex(&key.sign(manifest))).unwrap();

    let (state, slot_a, slot_b) = layout(&tmp.path().join("box"));
    fs::create_dir_all(state.parent().unwrap()).unwrap();
    init(&state, &slot_a, &slot_b).unwrap();
    let error = install(&bundle, &state, &key.verifying_key()).unwrap_err();
    let message = error.to_string().to_lowercase();
    assert!(
        message.contains("path"),
        "a signed manifest that climbs out of the stick is a path error, got {error}"
    );
    assert_eq!(fs::read(&outside).unwrap(), b"do-not-read");
    assert!(!slot_b.exists());
}

#[test]
fn a_bundle_is_found_on_a_stick_two_mount_levels_down() {
    let key = SigningKey::generate().unwrap();
    let tmp = Tmp::new();
    let stick = tmp
        .path()
        .join("run")
        .join("media")
        .join("admin")
        .join("STICK");
    let bundle = stick.join(BUNDLE_DIR);
    write_bundle(&bundle, "4.0.0", b"image", &key).unwrap();
    assert_eq!(bundle_on_stick(&stick), Some(bundle.clone()));
    assert_eq!(
        find_bundles(&[tmp.path().join("run").join("media")]),
        vec![bundle.clone()]
    );

    let (state, slot_a, slot_b) = layout(&tmp.path().join("box"));
    init(&state, &slot_a, &slot_b).unwrap();
    let installed = install_mounted(
        &[tmp.path().join("run").join("media")],
        &state,
        &key.verifying_key(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(installed.version, "4.0.0");
    assert_eq!(fs::read(slot_b).unwrap(), b"image");

    let empty =
        install_mounted(&[tmp.path().join("nowhere")], &state, &key.verifying_key()).unwrap();
    assert!(empty.is_none());
}

#[test]
fn confirming_before_the_new_system_boots_does_not_cancel_or_accept_it() {
    let key = SigningKey::generate().unwrap();
    let tmp = Tmp::new();
    let (state, slot_a, slot_b) = layout(tmp.path());
    init(&state, &slot_a, &slot_b).unwrap();
    let bundle = tmp.path().join("stick").join(BUNDLE_DIR);
    write_bundle(&bundle, "2.0.0", b"pending", &key).unwrap();
    install(&bundle, &state, &key.verifying_key()).unwrap();

    let still = confirm(&state).unwrap();
    assert_eq!(still.slot, Slot::A);
    let choice = boot(&state, "boot-1").unwrap();
    assert_eq!(choice.slot, Slot::B);
    assert_eq!(choice.version, "2.0.0");
}

#[test]
fn an_install_does_not_write_elsewhere_on_the_stick() {
    let key = SigningKey::generate().unwrap();
    let tmp = Tmp::new();
    let stick = tmp.path().join("STICK");
    let bundle = stick.join(BUNDLE_DIR);
    write_bundle(&bundle, "1.0.0", b"image", &key).unwrap();
    let before = files_in(&stick);
    let box_dir = tmp.path().join("box");
    fs::create_dir_all(&box_dir).unwrap();
    let (state, slot_a, slot_b) = (
        box_dir.join("slots.json"),
        box_dir.join("a.img"),
        stick.join("sneaky.img"),
    );
    init(&state, &slot_a, &slot_b).unwrap();
    let error = install(&bundle, &state, &key.verifying_key()).unwrap_err();
    let message = error.to_string().to_lowercase();
    assert!(
        message.contains("stick") || message.contains("usb"),
        "{error}"
    );
    assert_eq!(files_in(&stick), before);
    assert!(!slot_b.exists());
}

#[test]
fn an_install_does_not_write_onto_the_update_stick() {
    let key = SigningKey::generate().unwrap();
    let tmp = Tmp::new();
    let bundle = tmp.path().join(BUNDLE_DIR);
    write_bundle(&bundle, "1.0.0", b"image", &key).unwrap();
    let before = files_in(&bundle);
    let state = tmp.path().join("slots.json");
    let slot_a = tmp.path().join("a.img");
    let slot_b = bundle.join("sneaky.img");
    init(&state, &slot_a, &slot_b).unwrap();
    let error = install(&bundle, &state, &key.verifying_key()).unwrap_err();
    let message = error.to_string().to_lowercase();
    assert!(
        message.contains("stick") || message.contains("usb"),
        "{error}"
    );
    assert_eq!(files_in(&bundle), before);
    assert!(!slot_b.exists());
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0xf) as usize] as char);
    }
    out
}

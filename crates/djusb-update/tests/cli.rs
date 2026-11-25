//! The `djusb-update` command installs a signed stick and rolls a bad one back.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

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
        let path = std::env::temp_dir().join(format!("djusb-update-cli-{nanos}-{n}"));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn tool() -> Command {
    let binary = option_env!("CARGO_BIN_EXE_djusb_update").unwrap_or("djusb-update");
    Command::new(binary)
}

fn run(cmd: &mut Command) -> String {
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        output.status.success(),
        "status {}\n{stdout}{stderr}",
        output.status
    );
    stdout
}

#[test]
fn a_stick_installs_and_a_bad_update_boots_the_previous_system() {
    let tmp = Tmp::new();
    let keys = tmp.path.join("keys");
    let stick = tmp.path.join("STICK").join("DJUSB-UPDATE");
    let payload = tmp.path.join("rootfs.bin");
    let state = tmp.path.join("slots.json");
    let slot_a = tmp.path.join("slot-a.img");
    let slot_b = tmp.path.join("slot-b.img");
    fs::write(&payload, b"signed-system").unwrap();
    fs::write(&slot_a, b"previous-system").unwrap();

    run(tool().arg("keygen").arg(&keys));
    run(tool()
        .arg("pack")
        .arg(&stick)
        .arg("--version")
        .arg("5.5.0")
        .arg("--payload")
        .arg(&payload)
        .arg("--key")
        .arg(keys.join("update.key")));
    run(tool()
        .arg("init")
        .arg("--state")
        .arg(&state)
        .arg("--slot-a")
        .arg(&slot_a)
        .arg("--slot-b")
        .arg(&slot_b));
    let installed = run(tool()
        .arg("install")
        .arg(&stick)
        .arg("--state")
        .arg(&state)
        .arg("--key")
        .arg(keys.join("update.pub")));
    assert!(installed.contains("slot b"), "{installed}");
    assert_eq!(fs::read(&slot_b).unwrap(), b"signed-system");
    assert_eq!(fs::read(&slot_a).unwrap(), b"previous-system");
    assert_eq!(fs::read(&payload).unwrap(), b"signed-system");

    let trial = boot(&state, "power-on-1");
    assert!(trial.contains("slot=b"), "{trial}");
    assert!(trial.contains("version=5.5.0"), "{trial}");
    assert!(trial.contains("rolled_back=false"), "{trial}");

    let rolled = boot(&state, "power-on-2");
    assert!(rolled.contains("slot=a"), "{rolled}");
    assert!(rolled.contains("version=factory"), "{rolled}");
    assert!(rolled.contains("rolled_back=true"), "{rolled}");
}

fn boot(state: &Path, boot_id: &str) -> String {
    run(tool()
        .arg("boot")
        .arg("--state")
        .arg(state)
        .arg("--boot-id")
        .arg(boot_id))
}

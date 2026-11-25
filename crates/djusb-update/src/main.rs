//! Packs a signed update, installs one from a stick, and chooses the system slot.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use djusb_update::{
    boot, confirm, init, install, install_mounted, write_bundle_from_file, BootChoice, Error,
    SigningKey, VerifyingKey, STATE_PATH,
};

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("djusb-update: {error}");
            ExitCode::from(1)
        }
    }
}

fn run(args: Vec<String>) -> Result<(), Error> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err(Error::Format(usage().to_string()));
    };
    match command {
        "keygen" => keygen(require(&args, 1, "directory")?),
        "pack" => pack(&args),
        "init" => init_slots(&args),
        "install" => install_one(&args),
        "install-mounted" => install_roots(&args),
        "boot" => print_boot(&args),
        "confirm" => {
            let choice = confirm(PathBuf::from(flag(&args, "--state")?).as_path())?;
            print_choice(&choice);
            Ok(())
        }
        "help" | "--help" | "-h" => {
            println!("{}", usage());
            Ok(())
        }
        _ => Err(Error::Format(usage().to_string())),
    }
}

fn usage() -> &'static str {
    "\
usage:
  djusb-update keygen <directory>
  djusb-update pack <bundle> --version <version> --payload <file> --key <signing-key>
  djusb-update init --state <slots.json> --slot-a <image> --slot-b <image>
  djusb-update install <bundle> --state <slots.json> --key <verifying-key>
  djusb-update install-mounted --state <slots.json> --key <verifying-key> [--root <mount>]...
  djusb-update boot --state <slots.json> [--boot-id <id>]
  djusb-update confirm --state <slots.json>"
}

fn keygen(directory: &str) -> Result<(), Error> {
    let directory = PathBuf::from(directory);
    fs::create_dir_all(&directory)?;
    let key = SigningKey::generate()?;
    key.save(&directory.join("update.key"))?;
    key.verifying_key().save(&directory.join("update.pub"))?;
    println!("wrote {}", directory.join("update.key").display());
    println!("wrote {}", directory.join("update.pub").display());
    Ok(())
}

fn pack(args: &[String]) -> Result<(), Error> {
    let bundle = PathBuf::from(require(args, 1, "bundle directory")?);
    write_bundle_from_file(
        &bundle,
        flag(args, "--version")?,
        PathBuf::from(flag(args, "--payload")?).as_path(),
        &SigningKey::load(PathBuf::from(flag(args, "--key")?).as_path())?,
    )?;
    println!("wrote {}", bundle.display());
    Ok(())
}

fn init_slots(args: &[String]) -> Result<(), Error> {
    init(
        PathBuf::from(flag(args, "--state")?).as_path(),
        PathBuf::from(flag(args, "--slot-a")?).as_path(),
        PathBuf::from(flag(args, "--slot-b")?).as_path(),
    )
}

fn install_one(args: &[String]) -> Result<(), Error> {
    let installed = install(
        PathBuf::from(require(args, 1, "bundle directory")?).as_path(),
        PathBuf::from(flag(args, "--state")?).as_path(),
        &VerifyingKey::load(PathBuf::from(flag(args, "--key")?).as_path())?,
    )?;
    println!(
        "installed {} onto slot {}",
        installed.version,
        slot_name(installed.slot)
    );
    println!("reboot to try it");
    Ok(())
}

fn install_roots(args: &[String]) -> Result<(), Error> {
    let roots = flags(args, "--root");
    let roots = if roots.is_empty() {
        vec![
            PathBuf::from("/media"),
            PathBuf::from("/run/media"),
            PathBuf::from("/mnt"),
        ]
    } else {
        roots.into_iter().map(PathBuf::from).collect()
    };
    match install_mounted(
        &roots,
        PathBuf::from(flag(args, "--state")?).as_path(),
        &VerifyingKey::load(PathBuf::from(flag(args, "--key")?).as_path())?,
    )? {
        Some(installed) => {
            println!(
                "installed {} onto slot {}",
                installed.version,
                slot_name(installed.slot)
            );
            println!("reboot to try it");
        }
        None => println!("no update"),
    }
    Ok(())
}

fn print_boot(args: &[String]) -> Result<(), Error> {
    let boot_id = match optional(args, "--boot-id") {
        Some(id) => id.to_string(),
        None => linux_boot_id()?,
    };
    let choice = boot(PathBuf::from(flag(args, "--state")?).as_path(), &boot_id)?;
    print_choice(&choice);
    Ok(())
}

fn print_choice(choice: &BootChoice) {
    println!("slot={}", slot_name(choice.slot));
    println!("version={}", choice.version);
    println!("image={}", choice.image.display());
    println!("rolled_back={}", choice.rolled_back);
}

fn slot_name(slot: djusb_update::Slot) -> &'static str {
    match slot {
        djusb_update::Slot::A => "a",
        djusb_update::Slot::B => "b",
    }
}

fn linux_boot_id() -> Result<String, Error> {
    if let Ok(text) = fs::read_to_string("/proc/sys/kernel/random/boot_id") {
        let id = text.trim();
        if !id.is_empty() {
            return Ok(id.to_string());
        }
    }
    Err(Error::Format(format!(
        "pass --boot-id; the default state file is {STATE_PATH}"
    )))
}

fn require<'a>(args: &'a [String], index: usize, name: &str) -> Result<&'a str, Error> {
    args.get(index)
        .map(String::as_str)
        .filter(|value| !value.starts_with('-'))
        .ok_or_else(|| Error::Format(format!("missing {name}")))
}

fn flag<'a>(args: &'a [String], name: &str) -> Result<&'a str, Error> {
    optional(args, name).ok_or_else(|| Error::Format(format!("missing {name}")))
}

fn optional<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}

fn flags<'a>(args: &'a [String], name: &str) -> Vec<&'a str> {
    let mut found = Vec::new();
    let mut args = args.iter().map(String::as_str);
    while let Some(arg) = args.next() {
        if arg == name {
            if let Some(value) = args.next() {
                found.push(value);
            }
        }
    }
    found
}

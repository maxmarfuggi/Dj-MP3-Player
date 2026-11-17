//! Writes the wall player's colour, type, and spacing constants from
//! `docs/design-tokens.json`. The desktop CSS is the same file, checked in
//! and compared by the device tests.

#[path = "src/token_gen.rs"]
mod token_gen;

use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/token_gen.rs");

    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let tokens_path = manifest.join("../../docs/design-tokens.json");
    println!("cargo:rerun-if-changed={}", tokens_path.display());

    let json = fs::read_to_string(&tokens_path).unwrap_or_else(|error| {
        panic!("reading {}: {error}", tokens_path.display());
    });
    let rust =
        token_gen::rust_constants(&json).unwrap_or_else(|error| panic!("design tokens: {error}"));

    let out_dir = env::var("OUT_DIR").unwrap_or_else(|error| panic!("OUT_DIR: {error}"));
    let out_path = Path::new(&out_dir).join("design_tokens.rs");
    fs::write(&out_path, rust)
        .unwrap_or_else(|error| panic!("writing {}: {error}", out_path.display()));
}

//! luish is a binary crate, so the fuzz targets can't link to it. Instead
//! this crate compiles luish's modules itself: `src/lib.rs` includes the
//! `mod` declarations written here, one per `mod NAME;` of `../src/main.rs`,
//! with a `#[path]` to its file.
//!
//! Also sets what luish's own build.rs sets for `env!`, and `cfg(fuzzing)`
//! (which cargo-fuzz sets too) so that the crate builds without cargo-fuzz.

use std::fmt::Write;
use std::path::Path;

fn main() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../src")
        .canonicalize()
        .unwrap();
    let main = src.join("main.rs");
    println!("cargo:rerun-if-changed={}", main.display());
    let mut out = String::new();
    for line in std::fs::read_to_string(&main).unwrap().lines() {
        let Some(name) = line.strip_prefix("mod ").and_then(|l| l.strip_suffix(';')) else {
            continue;
        };
        let dir = src.join(name).join("mod.rs");
        let file = if dir.exists() {
            dir
        } else {
            src.join(format!("{name}.rs"))
        };
        writeln!(out, "#[path = {:?}]\npub mod {name};", file.display().to_string()).unwrap();
    }
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("modules.rs");
    std::fs::write(dest, out).unwrap();

    println!("cargo:rustc-check-cfg=cfg(fuzzing)");
    println!("cargo:rustc-cfg=fuzzing");
    for var in ["LUISH_GIT_REV", "LUISH_GIT_REV_SHORT", "LUISH_BUILD_ID"] {
        println!("cargo:rustc-env={var}=fuzz");
    }
}

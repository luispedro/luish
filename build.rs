//! Embeds the git revision in the binary (`__luish_internal print-git-rev`).
//!
//! Sets three variables for `env!`:
//! - `LUISH_GIT_REV`: the commit's full hash, with `-dirty` if the sources
//!   differ from it, or `unknown` outside a git checkout;
//! - `LUISH_GIT_REV_SHORT`: the same with the abbreviated hash;
//! - `LUISH_BUILD_ID`: identifies the sources, for the startup cache. It is
//!   `LUISH_GIT_REV`, plus a hash of the sources when they are dirty (so
//!   that every change gets a new ID), or the package version and that hash
//!   outside a git checkout.

use std::path::Path;
use std::process::Command;

/// The files the binary is built from. Only changes to these make a build
/// dirty (and rerun this script).
const SOURCES: &[&str] = &["src", "build.rs", "Cargo.toml", "Cargo.lock"];

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8(out.stdout).ok()?.trim().to_string())
}

/// FNV-1a: small and, unlike std's hashers, stable across Rust versions.
fn fnv(h: &mut u64, bytes: &[u8]) {
    for &b in bytes {
        *h = (*h ^ b as u64).wrapping_mul(0x100000001b3);
    }
}

/// Hashes the names and contents of the files under `path`, in order.
fn hash_tree(h: &mut u64, path: &Path) {
    if path.is_dir() {
        let mut entries: Vec<_> = std::fs::read_dir(path)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .collect();
        entries.sort();
        for e in entries {
            hash_tree(h, &e);
        }
    } else if let Ok(data) = std::fs::read(path) {
        fnv(h, path.as_os_str().as_encoded_bytes());
        fnv(h, &[0]);
        fnv(h, &(data.len() as u64).to_le_bytes());
        fnv(h, &data);
    }
}

fn sources_hash() -> String {
    let mut h = 0xcbf29ce484222325;
    for s in SOURCES {
        hash_tree(&mut h, Path::new(s));
    }
    format!("{h:016x}")
}

fn main() {
    for s in SOURCES {
        println!("cargo:rerun-if-changed={s}");
    }
    let (rev, short, id) = match (git(&["rev-parse", "HEAD"]), git(&["rev-parse", "--short", "HEAD"])) {
        (Some(rev), Some(short)) => {
            // Rerun when HEAD moves: a commit, a checkout, or a reset.
            if let Some(dir) = git(&["rev-parse", "--absolute-git-dir"]) {
                println!("cargo:rerun-if-changed={dir}/HEAD");
                println!("cargo:rerun-if-changed={dir}/index");
            }
            if let Some(common) = git(&["rev-parse", "--path-format=absolute", "--git-common-dir"]) {
                println!("cargo:rerun-if-changed={common}/refs");
                if Path::new(&common).join("packed-refs").exists() {
                    println!("cargo:rerun-if-changed={common}/packed-refs");
                }
            }
            let mut status = vec!["status", "--porcelain", "--"];
            status.extend(SOURCES);
            let dirty = git(&status).is_none_or(|s| !s.is_empty());
            if dirty {
                let id = format!("{rev}-dirty-{}", sources_hash());
                (format!("{rev}-dirty"), format!("{short}-dirty"), id)
            } else {
                (rev.clone(), short, rev)
            }
        }
        _ => {
            let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
            let id = format!("{version}-{}", sources_hash());
            ("unknown".to_string(), "unknown".to_string(), id)
        }
    };
    println!("cargo:rustc-env=LUISH_GIT_REV={rev}");
    println!("cargo:rustc-env=LUISH_GIT_REV_SHORT={short}");
    println!("cargo:rustc-env=LUISH_BUILD_ID={id}");
}

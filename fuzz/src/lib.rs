//! luish's modules, compiled for the fuzz targets (see `build.rs`), and the
//! targets themselves (`targets`), which can reach the modules' crate-private
//! items. DEVELOPING.md (Fuzzing) has how to run them.

// What luish's binary uses, a library need not.
#![allow(dead_code, unused_imports, unused_macros, unreachable_pub)]

include!(concat!(env!("OUT_DIR"), "/modules.rs"));

pub mod targets;

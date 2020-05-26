#![warn(rust_2018_idioms)]
#![warn(missing_docs)]

//! Rust binding for DPDK
//!
//! Currently, build.rs cannot configure linker options, thus, a user must set RUSTFLAGS env
//! variable as this library's panic message says.

#[allow(warnings, clippy)]
mod dpdk;
pub use dpdk::*;

include!(concat!(env!("OUT_DIR"), "/lib.rs"));

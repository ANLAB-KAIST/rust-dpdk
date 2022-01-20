#![warn(rust_2018_idioms)]
#![allow(missing_docs)]

//! Rust binding for DPDK
//!
//! Currently, build.rs cannot configure linker options, thus, a user must set RUSTFLAGS env
//! variable as this library's panic message says.

#[allow(warnings, clippy::all)]
mod dpdk;
pub use dpdk::*;

include!(concat!(env!("OUT_DIR"), "/lib.rs"));

#![warn(rust_2018_idioms)]

extern crate arrayvec;
extern crate dpdk_sys;
extern crate log;
extern crate thiserror;

mod ffi;

pub mod eal;

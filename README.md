# rust-dpdk

[![Build Status](https://jenkins.kaist.ac.kr/buildStatus/icon?job=ANLAB-KAIST%2Frust-dpdk%2Fmaster)](https://jenkins.kaist.ac.kr/job/ANLAB-KAIST/job/rust-dpdk/job/master/)

Tested with <https://github.com/rust-lang/rust-bindgen> v0.47.
Tested with <https://github.com/DPDK/dpdk.git> v19.05.


## How to use

DPDK should be built with `EXTRA_CFLAGS=" -fPIC "` flag.

Currently, Rust cargo does not support changing the linker options via build.rs.
If not set, DPDK's dynamic driver loading will not work so that no PMD will be loaded.
The build script will generate `export RUSTFLAGS=...` message on its first run.
Please execute the command to build your DPDK applications.

## Issues

Test fails with v0.48 to v0.51 (2019-11-13).
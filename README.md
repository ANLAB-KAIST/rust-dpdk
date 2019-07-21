# rust-dpdk

[![Build Status](https://jenkins.kaist.ac.kr/buildStatus/icon?job=ANLAB-KAIST%2Frust-dpdk%2Fmaster)](https://jenkins.kaist.ac.kr/job/ANLAB-KAIST/job/rust-dpdk/job/master/)

Tested with <https://github.com/rust-lang/rust-bindgen> v0.47

## How to use

Currently, Rust cargo does not support changing the linker options via build.rs.
If not set, DPDK's dynamic driver loading will not work so that no PMD will be loaded.
The build script will generate `export RUSTFLAGS=...` message on its first run.
Please execute the command to build your DPDK applications.

## Issues

Test fails with v0.48 and v0.49 (2019-04-28).

Related issue: <https://github.com/rust-lang/rust-bindgen/issues/1498>

However, similar problems still occur, so we fix to use bindgen 0.47.

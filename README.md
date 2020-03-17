# rust-dpdk

[![Build Status](https://jenkins.kaist.ac.kr/buildStatus/icon?job=ANLAB-KAIST%2Frust-dpdk%2Fmaster)](https://jenkins.kaist.ac.kr/job/ANLAB-KAIST/job/rust-dpdk/job/master/)

Tested with <https://github.com/rust-lang/rust-bindgen> v0.47.
Tested with <https://github.com/DPDK/dpdk.git> v20.02.


## How to use

DPDK should be built with `EXTRA_CFLAGS=" -fPIC "` flag.

Currently, Rust cargo does not support changing the linker options via build.rs.
If not set, DPDK's dynamic driver loading will not work so that no PMD will be loaded.
The build script will generate `export RUSTFLAGS=...` message on its first run.
Please execute the command to build your DPDK applications.

## Maintenance

1. Update Rust stable
1. Try new DPDK release with current bindgen
1. Check whether `clang` and `bindgen` repository share `clang-sys` versions
1. Try update bindgen (Do not sole update `clang` or `bindgen`)
1. Test build
1. Test run (refer to Dockerfile and Jenkinsfile)

## Issues

List of failed bindgen builds (last update: 2019-11-13)

* v0.48
* v0.51
* v0.53


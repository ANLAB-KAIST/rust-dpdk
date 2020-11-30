# rust-dpdk

[![Build Status](https://jenkins.kaist.ac.kr/buildStatus/icon?job=ANLAB-KAIST%2Frust-dpdk%2Fmaster)](https://jenkins.kaist.ac.kr/job/ANLAB-KAIST/job/rust-dpdk/job/master/)

Tested with <https://github.com/rust-lang/rust-bindgen> v0.47.
Tested with <https://github.com/DPDK/dpdk.git> v20.11.

## Goals

There are other `rust-dpdk` implementations and you may choose most proper implementation to your purpose.
(https://github.com/flier/rust-dpdk, https://github.com/netsys/netbricks)
This library is built for following design goals.

1. Minimize hand-written binding code.
1. Do not include `bindgen`'s output in this repository.
1. Statically link DPDK libraries instead of using shared libraries.
1. (TODO) Rust wrapper (`rust-dpdk`) for low-level DPDK APIs (`rust-dpdk-sys`).

| Library   | No bindgen output | Static linking  | Inline function wrappers | Prevent PMD opt-out | Rust-style wrappers |
| --------- | ----------------- | --------------- | ------------------------ | ------------------- | ------------------- | 
| flier     | bindgen snapshot  | O               | O (manual)               | X                   | O                   |
| netbricks | manual FFI        | X               | X                        | O (via dynload)     | X                   |
| ANLAB     | ondemand creation | O               | O (automatic)            | O                   | (under construction)|

## Prerequisites

First, this library depends on Intel Data Plane Development Kit (DPDK).
Refer to official DPDK document to install DPDK (http://doc.dpdk.org/guides/linux_gsg/index.html).

Here, we include basic instructions to build DPDK and use this library.

Commonly, following packages are required to build DPDK.
```{.sh}
apt-get install -y curl git build-essential libnuma-dev meson # To download and build DPDK
apt-get install -y linux-headers-amd64 # To build kernel drivers
apt-get install -y libclang-dev clang llvm-dev # To analyze DPDK headers and create bindings
```

DPDK can be installed by following commands:
```{.sh}
meson build
ninja -C build
ninja -C build install # sudo required
```
Since v20.11, kernel drivers are moved to https://git.dpdk.org/dpdk-kmods/.
If your NIC requires kernel drivers, they are found at the above link.


Now add `rust-dpdk` to your project's `Cargo.toml` and use it!
```{.toml}
[dependencies]
rust-dpdk-sys = { git = "https://github.com/ANLAB-KAIST/rust-dpdk.git" }
```

## Maintenance

1. Update Rust stable
1. Try new DPDK release with current bindgen
1. Check whether `clang` and `bindgen` repository share `clang-sys` versions
1. Try update bindgen (Do not sole update `clang` or `bindgen`)
1. Test build
1. Test run (refer to Dockerfile and Jenkinsfile)

## Issues

List of failed bindgen builds (last update: 2020-11-30)

* v0.48
* v0.51
* v0.53 - v0.56
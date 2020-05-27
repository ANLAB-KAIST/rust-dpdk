# rust-dpdk

[![Build Status](https://jenkins.kaist.ac.kr/buildStatus/icon?job=ANLAB-KAIST%2Frust-dpdk%2Fmaster)](https://jenkins.kaist.ac.kr/job/ANLAB-KAIST/job/rust-dpdk/job/master/)

Tested with <https://github.com/rust-lang/rust-bindgen> v0.47.
Tested with <https://github.com/DPDK/dpdk.git> v20.02.

## Goals

There are other `rust-dpdk` implementations and you may choose most proper implementation to your purpose.
(https://github.com/flier/rust-dpdk, https://github.com/netsys/netbricks)
This library is built for following design goals.

1. Minimize hand-written binding code.
1. Do not include `bindgen`'s output in this repository.
1. Statically link DPDK libraries instead of using shared libraries.
1. (TODO) Rust wrapper (`rust-dpdk`) for low-level DPDK APIs (`rust-dpdk-sys`).

| Library   | No bindgen output | Static linking  | Inline function wrappers | Prevent PMD opt-out | Rust-stype wrappers |
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
apt-get install -y curl git build-essential libnuma-dev # To download and build DPDK
apt-get install -y linux-headers-amd64 # To build kernel drivers
apt-get install -y libclang-dev clang llvm-dev # To analyze DPDK headers and create bindings
```

We recognized existing DPDK installation from `RTE_SDK` and `RTE_TARGET` environment variables.
If none of them is set, it will download and build DPDK in its temp directory.

If you want to install your own DPDK, download source code from DPDK official website.
```{.sh}
wget http://fast.dpdk.org/rel/dpdk-20.02.tar.xz
tar xf dpdk-20.02.tar.xz
mv dpdk-20.02 dpdk
cd dpdk
export RTE_SDK=`pwd`
```

If you prepare your own DPDK build, DPDK must be built with `-fPIC` flag.
```{.sh}
# in DPDK directory
EXTRA_CFLAGS=" -fPIC " make config T=x86_64-native-linux-clang
EXTRA_CFLAGS=" -fPIC " make -j install

# T=x86_64-native-linux-clang becomes RTE_TARGET
export RTE_TARGET="x86_64-native-linux-clang"
```

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

List of failed bindgen builds (last update: 2020-03-18)

* v0.48
* v0.51
* v0.53 (clang-sys mismatch)

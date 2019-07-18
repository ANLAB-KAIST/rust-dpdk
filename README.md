# rust-dpdk

[![Build Status](https://jenkins.kaist.ac.kr/buildStatus/icon?job=ANLAB-KAIST%2Frust-dpdk%2Fmaster)](https://jenkins.kaist.ac.kr/job/ANLAB-KAIST/job/rust-dpdk/job/master/)

Tested with <https://github.com/rust-lang/rust-bindgen> v0.47

## Building
There are a couple of ways to build the sources.

### Using `docker build`
The following command will build the base container needed.

```bash
docker build -t rust-dpdk .
```

To build the source-code using this container:
```bash
docker run --rm -v `pwd`:/workdir --workdir /workdir rust-dpdk cargo build
```

### Using `cargo build`
`RTE_SDK` environment variable needs to be defined and exported. (See DPDK documentation for building DPDK)

```bash
# Assuming the libraries that were built are located at $RTE_SDK/build/lib
export RUSTFLAGS="-C link-arg=-L$RTE_SDK/build/lib -C link-arg=-Wl,--whole-archive -C link-arg=-ldpdk -C link-arg=-Wl,--no-whole-archive -C link-arg=-lnuma -C link-arg=-lm"
cargo build
```

## Issues

Test fails with v0.48 and v0.49 (2019-04-28).

Related issue: <https://github.com/rust-lang/rust-bindgen/issues/1498>

However, similar problems still occur, so we fix to use bindgen 0.47.

# rust-dpdk

Tested with https://github.com/servo/rust-bindgen v0.20.0

DPDK should be built with EXTRA_CFLAGS=-fPIC

## How to use

Run make.py with Python3.
Then this project folder becomes a complete Cargo project.
You can specify DPDK with manual input or RTE_SDK environment variable.
It may automatically downloads DPDK if you put empty string at the prompt.

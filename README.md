# rust-dpdk

Tested with https://github.com/servo/rust-bindgen 7d995d75e07798c1527d5ea3dd8a647ed79fe209

DPDK should be built with EXTRA_CFLAGS=-fPIC

## How to use

Run make.py with Python3.
Then this project folder becomes a complete Cargo project.
You can specify DPDK with manual input or RTE_SDK environment variable.
It may automatically downloads DPDK if you put empty string at the prompt.

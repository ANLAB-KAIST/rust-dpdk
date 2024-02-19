#!/bin/bash
echo "hello world"

# Check program version
clang --version
llvm-config --version
cargo --version
gcc --version

# Build and test
cargo build --verbose
cargo test --verbose

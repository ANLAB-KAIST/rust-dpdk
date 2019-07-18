FROM ubuntu:18.04

ENV RTE_SDK=/usr/local/share/dpdk/
ENV RTE_TARGET=x86_64-native-linuxapp-gcc

RUN apt-get update -y && apt-get install -y build-essential \
                         libnuma-dev git linux-headers-$(uname -r) \
                         apt-utils libclang-dev clang curl
RUN git clone -b v18.11 "https://github.com/DPDK/dpdk.git" /dpdk

WORKDIR /dpdk

RUN echo "${RTE_TARGET}" > RTE_TARGET_EXPECTED
RUN make defconfig | sed -r 's/(.*)\s(\w+)/\2/g' > RTE_TARGET
RUN diff -w -q RTE_TARGET RTE_TARGET_EXPECTED
RUN make -j$(nproc)
RUN make -j$(nproc) install

WORKDIR /
RUN rm -rf /dpdk

# For rustup
ENV RUSTUP_HOME=/usr/local/rustup
ENV CARGO_HOME=/usr/local/cargo
ENV PATH=/usr/local/cargo/bin:$PATH
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y --no-modify-path
RUN chmod -R a+w ${RUSTUP_HOME} ${CARGO_HOME}

# Recover env and verify
RUN rustup --version

# For rust-dpdk build
ENV RUSTFLAGS="-C link-arg=-L/usr/local/share/dpdk/x86_64-native-linuxapp-gcc/lib -C link-arg=-Wl,--whole-archive -C link-arg=-ldpdk -C link-arg=-Wl,--no-whole-archive -C link-arg=-lnuma -C link-arg=-lm"

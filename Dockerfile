FROM debian:latest

RUN apt update -y && apt dist-upgrade -y && apt autoremove -y && apt autoclean -y
RUN apt install -y build-essential libnuma-dev git linux-headers-$(uname -r)

RUN git clone -b releases "https://gitlab.kaist.ac.kr/3rdparty/dpdk" /dpdk

WORKDIR /dpdk

RUN make defconfig
RUN make -j$(nproc)
RUN make -j$(nproc) install

WORKDIR /
ENV RTE_SDK=/dpdk
ENV RTE_TARGET=build

# For rustup
ENV RUSTUP_HOME=/usr/local/rustup
ENV CARGO_HOME=/usr/local/cargo
ENV PATH=/usr/local/cargo/bin:$PATH
RUN mkdir -p ${RUSTUP_HOME} ${CARGO_HOME}
RUN chmod -R a+w ${RUSTUP_HOME} ${CARGO_HOME}
RUN apt install -y curl
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y --no-modify-path

# Recover env and verify
RUN rustup --version

# For rust-dpdk
# We need both clang and libclang to work with
# https://bugs.launchpad.net/ubuntu/+source/llvm-defaults/+bug/1242300
RUN apt install -y libclang-dev clang

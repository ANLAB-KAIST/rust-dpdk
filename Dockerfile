FROM debian:latest

RUN apt update -y && apt dist-upgrade -y && apt autoremove -y && apt autoclean -y
RUN apt install -y build-essential libnuma-dev git linux-headers-$(uname -r)

RUN git clone -b releases "https://gitlab.kaist.ac.kr/3rdparty/dpdk" /dpdk

WORKDIR /dpdk

RUN make defconfig
RUN make -j$(nproc)
RUN make -j$(nproc) install

WORKDIR /
RUN rm -rf /dpdk

# For rustup
RUN apt install -y curl
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y
ENV PATH="${HOME}/.cargo/bin:${PATH}"

# Recover env and verify
RUN rustup --version

# For rust-dpdk
# We need both clang and libclang to work with
# https://bugs.launchpad.net/ubuntu/+source/llvm-defaults/+bug/1242300
RUN apt install -y libclang-dev clang
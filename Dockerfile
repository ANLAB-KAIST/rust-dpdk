FROM debian:latest

RUN apt update -y && apt dist-upgrade -y && apt autoremove -y && apt autoclean -y
RUN apt install -y build-essential linux-headers-amd64 libnuma-dev git

RUN git clone -b releases "https://gitlab.kaist.ac.kr/3rdparty/dpdk" /dpdk

WORKDIR /dpdk

RUN make defconfig
RUN make -j$(nproc)
RUN make -j$(nproc) install


# For rustup
RUN apt install -y curl
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Recover env and verify
WORKDIR /
RUN rustup --version

# For rust-dpdk
RUN apt install -y libclang-dev
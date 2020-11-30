FROM debian:latest

ENV RTE_SDK=/usr/local/share/dpdk

RUN echo "APT last updated: 2020/11/30"

RUN apt-get update -y && apt-get dist-upgrade -y && apt-get autoremove -y && apt-get autoclean -y
RUN apt-get install -y linux-headers-amd64
#RUN apt-get install -y linux-headers-$(uname -r)-all
RUN apt-get install -y build-essential libnuma-dev git meson
RUN apt-get install -y curl
RUN apt-get install -y libclang-dev clang llvm-dev

# For rustup
ENV RUSTUP_HOME=/usr/local/rustup
ENV CARGO_HOME=/usr/local/cargo
ENV PATH=/usr/local/cargo/bin:$PATH
RUN curl -f --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y --no-modify-path
RUN chmod -R a+w ${RUSTUP_HOME} ${CARGO_HOME}

# Recover env and verify
RUN rustup --version

RUN git clone -b v20.11 "http://dpdk.org/git/dpdk" /dpdk

WORKDIR /dpdk

RUN meson build
RUN ninja -C build
RUN ninja -C build install
RUN ldconfig

WORKDIR /
RUN rm -rf /dpdk

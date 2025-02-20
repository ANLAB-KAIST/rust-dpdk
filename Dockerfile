FROM debian:latest

WORKDIR /
RUN echo "APT last updated: 2024/01/01"

RUN apt-get update -y && apt-get dist-upgrade -y && apt-get autoremove -y && apt-get autoclean -y
RUN apt-get install -y linux-headers-generic build-essential libnuma-dev git meson python3-pyelftools curl libclang-dev clang llvm-dev libbsd-dev pkg-config
RUN apt-get install -y curl git tar

ENV RTE_SDK=/usr/local/share/dpdk
ENV DPDK_VERSION 22.11.4

# Install DPDK
RUN mkdir /dpdk
RUN curl -s -o dpdk.tar.xz https://fast.dpdk.org/rel/dpdk-${DPDK_VERSION}.tar.xz
RUN tar -xvJf dpdk.tar.xz -C dpdk --strip-components=1
WORKDIR /dpdk
RUN meson setup build
RUN ninja -C build
RUN ninja -C build install
RUN ldconfig
WORKDIR /
RUN rm -rf /dpdk

# Init user account
ENV USER_NAME user
RUN useradd -ms /bin/bash $USER_NAME

# Rust user install
WORKDIR /home/$USER_NAME
RUN su -c "curl -f -sSf https://sh.rustup.rs | bash -s -- -y --default-toolchain none" - $USER_NAME
COPY --chown=$USER_NAME:$USER_NAME ./rust-toolchain /
RUN su -c "rustup toolchain install `cat /rust-toolchain | tr -d ' \n'` --profile minimal --component clippy rustfmt" - $USER_NAME
RUN rm /rust-toolchain
WORKDIR /

# User ci script
COPY --chown=$USER_NAME:$USER_NAME . /home/$USER_NAME/
WORKDIR /home/$USER_NAME
RUN su -c "./ci.sh" - $USER_NAME
WORKDIR /

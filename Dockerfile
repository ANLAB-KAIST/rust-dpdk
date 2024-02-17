FROM debian:latest

ENV RTE_SDK=/usr/local/share/dpdk

RUN echo "APT last updated: 2023/05/01"

RUN apt-get update -y && apt-get dist-upgrade -y && apt-get autoremove -y && apt-get autoclean -y
RUN apt-get install -y linux-headers-amd64
#RUN apt-get install -y linux-headers-$(uname -r)-all
RUN apt-get install -y build-essential libnuma-dev git meson python3-pyelftools
RUN apt-get install -y curl
RUN apt-get install -y libclang-dev clang llvm-dev
# install libbsd-dev
RUN apt-get install -y libbsd-dev

RUN git clone -b v22.11 "http://dpdk.org/git/dpdk" /dpdk

WORKDIR /dpdk

RUN meson build
RUN ninja -C build
RUN ninja -C build install
RUN ldconfig

WORKDIR /
RUN rm -rf /dpdk

# For rustup
ENV USER_NAME jenkins
RUN useradd -ms /bin/bash $USER_NAME

# Beginning of rust user install
RUN su -c "curl -f --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y --profile default" - $USER_NAME
# End of rust user install

ADD ./rust-toolchain /
RUN chmod 444 /rust-toolchain
RUN su -c "rustup toolchain install `cat /rust-toolchain | tr -d ' \n'`" - $USER_NAME
RUN rm /rust-toolchain

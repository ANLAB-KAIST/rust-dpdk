FROM debian:latest

ENV RTE_SDK=/usr/local/share/dpdk

RUN echo "APT last updated: 2023/05/01"

RUN apt-get update -y && apt-get dist-upgrade -y && apt-get autoremove -y && apt-get autoclean -y
RUN apt-get install -y linux-headers-generic build-essential libnuma-dev git meson python3-pyelftools curl libclang-dev clang llvm-dev libbsd-dev
RUN apt-get install -y curl git

RUN git clone -b v22.11 "http://dpdk.org/git/dpdk" /dpdk

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


# Beginning of rust user install
WORKDIR /home/$USER_NAME
RUN su -c "curl -f -sSf https://sh.rustup.rs | bash -s -- -y --default-toolchain none" - $USER_NAME
ADD ./rust-toolchain /
RUN chmod 444 /rust-toolchain
RUN su -c "rustup toolchain install `cat /rust-toolchain | tr -d ' \n'` --profile minimal --component clippy rustfmt" - $USER_NAME
RUN rm /rust-toolchain
# End of rust user install

# Beginning of user ci script
ADD . /home/$USER_NAME/ci
RUN chown -R $USER_NAME:$USER_NAME /home/$USER_NAME/ci
WORKDIR /home/$USER_NAME/ci
RUN su -c "./ci.sh" - $USER_NAME
# End of user ci script

WORKDIR /

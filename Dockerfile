FROM debian:latest

RUN apt update -y && apt dist-upgrade -y
RUN apt install -y build-essential linux-headers-amd64 libnuma-dev git
RUN apt autoremove -y && apt autoclean -y

RUN git clone -b releases "https://gitlab.kaist.ac.kr/3rdparty/dpdk" /dpdk

WORKDIR /dpdk

RUN make defconfig
RUN make -j$(nproc)
RUN make -j$(nproc) install



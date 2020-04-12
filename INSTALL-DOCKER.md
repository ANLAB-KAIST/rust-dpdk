# DPDK NIC in Docker

## Installation

- Install packages.

  ```
  apt update && apt install -yy build-essential libnuma-dev linux-headers-generic libhugetlbfs0

  # `librdmacm-dev`, `librdmacm1` are additionally neccessary for Mellanox NICs.
  # `libmnl-dev`: was neccessary in Azure?
  ```

- Install DPDK.

  ```sh
  wget http://fast.dpdk.org/rel/dpdk-20.02.tar.xz
  tar xf dpdk-20.02.tar.xz
  mv dpdk-20.02 dpdk
  cd dpdk
  DPDK=${DPDK:="$PWD"}
  ```

- Configure DPDK.

  ```sh
  echo "CONFIG_RTE_EAL_IGB_UIO=y" >> config/common_linux
  echo "CONFIG_RTE_KNI_KMOD=y" >> config/common_linux
  ```

- Build DPDK.

  ```sh
  make config T=x86_64-native-linux-clang
  make -j         # build target: `./build`

  # make defconfig --> Makes target x86_64-native-linux-gcc
  # but gcc suffers segmentation fault, while clang doesn't (Not sure why...)
  ```

- Allow `docker` to access huge pages. The script requires `root` privileges.

  ```sh
  DOCKER_GID=`getent group docker | cut -d: -f3`
  NR_HUGE=${NR_HUGE:="8192"}

  sysctl -w vm.hugetlb_shm_group=${DOCKER_GID}
  sysctl -w vm.nr_hugepages=${NR_HUGE}

  # Create hugepage mount location on /mnt with non-default directory
  mkdir -p /mnt/dpdk-hugepage
  chgrp docker /mnt/dpdk-hugepage
  mount -t hugetlbfs -o gid=${DOCKER_GID},mode=1770 none /mnt/dpdk-hugepage
  ```

    + TODO: make it persistent at `/etc/sysctl.conf`

- Install DPDK drivers. The script requires `root` privileges.

  ```sh
  modprobe uio
  insmod ${DPDK}/build/kmod/igb_uio.ko
  insmod ${DPDK}/build/kmod/rte_kni.ko
  ```

- Check the two PCIe addresses of the "Intel Corporation Ethernet Controller XL710 for 40GbE QSFP+ (rev 02)".

  ```sh
  lspci | grep XL710
  # `81:00.0` and `81:00.1` in this example

  IFACE_LIST=${IFACE_LIST:="81:00.0 81:00.1"}
  ```

- Bind and grant permissions for devices. The script requires `root` privileges.

  ```sh
  ${DPDK}/usertools/dpdk-devbind.py -b igb_uio ${IFACE_LIST}

  chgrp docker /dev/uio* /dev/hpet
  chmod g+rwx /dev/uio*
  chmod g+rw /dev/hpet
  ```

- Check if everything is okay by running a test docker image.

  ```sh
  docker run --rm -it --privileged -v /mnt/dpdk-hugepage:/mnt/huge anlabkaist/rust-dpdk:latest ${DPDK}/build/app/testpmd 
  ```

    + Remove `--privileged`.

- TODO: Run the dev docker image.

  ```sh
  docker run -it --privileged -v /mnt/dpdk-hugepage:/mnt/huge ubuntu:latest bash
  ```
=
  ```sh
  # Build Container Image
  git clone https://{personal_token}@github.com/ANLAB-KAIST/FPS.git
  docker build -t dpdk-fps:20.02 ./FPS

  # 현재 서버에서 실행되고 있는 형태를 반영하는게 필요할 듯
  # Volume Mount를 하는 것으로
  docker run --rm -it --privileged -v /sys/bus/pci/devices:/sys/bus/pci/devices -v /sys/kernel/mm/hugepages:/sys/kernel/mm/hugepages -v /sys/devices/system/node:/sys/devices/system/node -v /mnt/huge:/mnt/huge -v /dev:/dev dpdk-fps:20.02
  ```

- TODO: rebuild when kernel is updated.

  ```sh
  uname -r > ${DPDK}/CURRENT
  diff -q ${DPDK}/CURRENT ${DPDK}/RECENT
  if [ $? -ne 0 ]
  then
          echo "Rebuilding DPDK for a new kernel..."
          make -C ${DPDK} clean
          make -C ${DPDK} -j`nproc`
          cp ${DPDK}/CURRENT ${DPDK}/RECENT
  fi
  ```

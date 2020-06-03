//! Wrapper for DPDK's environment abstraction layer (EAL).
use ffi;
use log::{debug, info, warn};
use std::collections::{HashMap, HashSet};
use std::convert::{TryFrom, TryInto};
use std::ffi::CString;
use std::mem::size_of;
use std::ptr::NonNull;
use std::sync::{Arc, RwLock};
use thiserror::Error;

const MAGIC: &str = "be0dd4ab";

const DEFAULT_TX_DESC: u16 = 128;
const DEFAULT_RX_DESC: u16 = 128;
const DEFAULT_RX_POOL_SIZE: usize = 1024;
const DEFAULT_RX_PER_CORE_CACHE: usize = 0;
const DEFAULT_PACKET_DATA_LENGTH: usize = 2048;
const DEFAULT_PROMISC: bool = true;

#[derive(Debug)]
struct EalSharedInner {} // TODO Remove this if unnecessary

#[derive(Debug)]
struct EalInner {
    shared: RwLock<EalSharedInner>,
}

/// DPDK's environment abstraction layer (EAL).
///
/// This object indicates that EAL has been initialized and its APIs are available now.
#[derive(Debug, Clone)]
pub struct Eal {
    inner: Arc<EalInner>,
}

#[derive(Debug, Error)]
pub enum EalError {
    #[error("EAL function returned an error code: {}", code)]
    ErrorCode { code: i32 },
}

/// How to create NIC queues for a CPU.
pub enum Affinity {
    /// All NICs create queues for the CPU.
    Full,
    /// NICs on the same NUMA node create queues for the CPU.
    Numa,
}

/// Abstract type for DPDK port
#[derive(Debug, Clone)]
pub struct Port {
    inner: Arc<PortInner>,
}

#[derive(Debug)]
struct PortInner {
    port_id: u16,
    eal_ref: Arc<EalInner>,
}

/// Placeholder for DPDK Pool's private data
#[derive(Debug)]
pub struct MPoolPriv {}

/// Abstract type for DPDK MPool
#[derive(Debug, Clone)]
pub struct MPool {
    inner: Arc<MPoolInner>,
}

#[derive(Debug)]
struct MPoolInner {
    eal: Eal,
    ptr: NonNull<dpdk_sys::rte_mempool>,
}

/// # Safety
/// Mempools are thread-safe.
/// https://doc.dpdk.org/guides/prog_guide/thread_safety_dpdk_functions.html
unsafe impl Send for MPoolInner {}
unsafe impl Sync for MPoolInner {}

impl Drop for MPoolInner {
    #[inline]
    fn drop(&mut self) {
        // Safety: foreign function.
        unsafe { dpdk_sys::rte_mempool_free(self.ptr.as_ptr()) }
    }
}

impl MPool {
    /// Create a new `MPool`.
    /// Note: Pool name must be globally unique.
    #[inline]
    pub fn new<StringLike: Into<Vec<u8>>>(
        eal: &Eal,
        name: StringLike,
        size: usize,
        per_core_cache_size: usize,
        data_len: usize,
        socket_id: i32,
    ) -> Self {
        let pool_name = CString::new(name).unwrap();

        // Safety: foreign function.
        let ptr = unsafe {
            dpdk_sys::rte_pktmbuf_pool_create(
                pool_name.into_bytes_with_nul().as_ptr() as *mut i8,
                size.try_into().unwrap(),
                per_core_cache_size as u32,
                (((size_of::<MPoolPriv>() + 7) / 8) * 8) as u16,
                data_len.try_into().unwrap(),
                socket_id,
            )
        };
        // The pointer to the new allocated mempool, on success. NULL on error with rte_errno set appropriately.
        // https://doc.dpdk.org/api/rte__mbuf_8h.html
        MPool {
            inner: Arc::new(MPoolInner {
                eal: eal.clone(),
                ptr: NonNull::new(ptr).unwrap(),
            }),
        }
    }

    /// Allocate a `Packet` from the pool.
    /// # Safety
    /// Returned item must not outlive this pool.
    #[inline]
    pub unsafe fn allloc(&self) -> Option<Packet> {
        // Safety: foreign function.
        // `alloc` is temporarily unsafe. Leaving this unsafe block.
        let pkt_ptr = unsafe { dpdk_sys::rte_pktmbuf_alloc(self.inner.ptr.as_ptr()) };

        NonNull::new(pkt_ptr).map(|ptr| Packet { ptr })
    }
}

#[derive(Debug)]
pub struct Packet {
    ptr: NonNull<dpdk_sys::rte_mbuf>,
}

impl Drop for Packet {
    #[inline]
    fn drop(&mut self) {
        // Safety: foreign function.
        unsafe { dpdk_sys::rte_pktmbuf_free(self.ptr.as_ptr()) }
    }
}

/// Abstract type for DPDK RxQ
#[derive(Debug, Clone)]
pub struct RxQ {
    inner: Arc<RxQInner>,
}

#[derive(Debug)]
struct RxQInner {
    queue_id: u16,
    port_ref: Arc<PortInner>,
    mpool_ref: Arc<MPoolInner>,
}

impl RxQ {}

/// Abstract type for DPDK TxQ
#[derive(Debug, Clone)]
pub struct TxQ {
    inner: Arc<TxQInner>,
}

#[derive(Debug)]
struct TxQInner {
    queue_id: u16,
    port_ref: Arc<PortInner>,
}

impl TxQ {}

impl Eal {
    /// Create an `Eal` instance.
    ///
    /// It takes command-line arguments and consumes used arguments.
    #[inline]
    pub fn new(args: &mut Vec<String>) -> Result<Self, EalError> {
        Ok(Eal {
            inner: Arc::new(EalInner::new(args)?),
        })
    }

    /// Candidate 1, return (thread, rxqs, txqs)
    ///
    /// Note: rte_lcore_count: -c ff 옵션에 따라 줄어듬.
    #[inline]
    pub fn setup(
        &self,
        rx_affinity: Affinity,
        tx_affinity: Affinity,
    ) -> Vec<(u32, Vec<RxQ>, Vec<TxQ>)> {
        // List of valid logical core ids.
        // Note: If some cores are masked, range (0..rte_lcore_count()) will include disabled cores.
        let lcore_id_list = (0..dpdk_sys::RTE_MAX_LCORE)
            .filter(|index| unsafe { dpdk_sys::rte_lcore_is_enabled(*index) > 0 });

        // List of `(lcore_id, socket_id)` pairs.
        let lcore_socket_pair_list: Vec<_> = lcore_id_list
            .clone()
            .map(|lcore_id| {
                // Safety: foreign function.
                let lcore_socket_id =
                    unsafe { dpdk_sys::rte_lcore_to_socket_id(lcore_id.try_into().unwrap()) };
                let cpu_id = unsafe { dpdk_sys::rte_lcore_to_cpu_id(lcore_id.try_into().unwrap()) };
                let is_enabled = unsafe { dpdk_sys::rte_lcore_is_enabled(lcore_id) > 0 };
                assert!(is_enabled);
                debug!(
                    "Logical core {} is enabled at physical core {} (NUMA node {})",
                    lcore_id, cpu_id, lcore_socket_id
                );
                (lcore_id, lcore_socket_id)
            })
            .collect();
        println!("lcore count: {}", lcore_socket_pair_list.len());

        // Sort lcore ids with map
        let socket_to_lcore_map = lcore_socket_pair_list.iter().fold(
            HashMap::new(),
            |mut sort_by_socket, (lcore_id, socket_id)| {
                sort_by_socket
                    .entry(socket_id)
                    .or_insert_with(HashSet::new)
                    .insert(lcore_id);
                sort_by_socket
            },
        );

        // Safety: foreign function.
        let port_id_list = (0..u16::try_from(dpdk_sys::RTE_MAX_ETHPORTS).unwrap())
            .filter(|index| unsafe { dpdk_sys::rte_eth_dev_is_valid_port(*index) > 0 });
        println!("port_id_list {:?}", port_id_list);

        // List of `(port, port_socket_id, vec<rx_lcore_ids>, vec<tx_lcore_ids>)`.
        // Note: We need number of rx cores and tx cores at the same time (`rte_eth_dev_configure`)
        let port_socket_rx_tx_pairs = port_id_list.clone().map(|port_id| {
            // Safety: foreign function.
            let port_socket_id = unsafe { dpdk_sys::rte_eth_dev_socket_id(port_id) };
            let rx_lcore_for_this_port: Vec<_> = match rx_affinity {
                Affinity::Full => lcore_id_list.clone().collect(),
                Affinity::Numa => socket_to_lcore_map
                    .get(&(port_socket_id as u32))
                    .unwrap()
                    .iter()
                    .cloned()
                    .cloned()
                    .collect(),
            };
            let tx_lcore_for_this_port: Vec<_> = match tx_affinity {
                Affinity::Full => lcore_id_list.clone().collect(),
                Affinity::Numa => socket_to_lcore_map
                    .get(&(port_socket_id as u32))
                    .unwrap()
                    .iter()
                    .cloned()
                    .cloned()
                    .collect(),
            };
            (
                Port {
                    inner: Arc::new(PortInner {
                        port_id,
                        eal_ref: self.inner.clone(),
                    }),
                },
                port_socket_id,
                rx_lcore_for_this_port,
                tx_lcore_for_this_port,
            )
        });

        // Init each port
        let configured_port_socket_rx_tx_pairs =
            port_socket_rx_tx_pairs.map(|(port, port_socket_id, rx_cpus, tx_cpus)| {
                let port_id = port.inner.port_id;
                // Safety: `rte_eth_dev_info` contains primitive integer types. Safe to fill with zeros.
                let mut dev_info: dpdk_sys::rte_eth_dev_info = unsafe { std::mem::zeroed() };
                // Safety: foreign function.
                unsafe { dpdk_sys::rte_eth_dev_info_get(port_id, &mut dev_info) };

                let rx_queue_limit = dev_info.max_rx_queues;
                let tx_queue_limit = dev_info.max_tx_queues;
                let rx_queue_count: u16 = rx_cpus.len().try_into().unwrap();
                let tx_queue_count: u16 = tx_cpus.len().try_into().unwrap();

                assert!(rx_queue_count <= rx_queue_limit);
                assert!(tx_queue_count <= tx_queue_limit);

                assert!(DEFAULT_RX_DESC <= dev_info.rx_desc_lim.nb_max);
                assert!(DEFAULT_RX_DESC >= dev_info.rx_desc_lim.nb_min);
                assert!(DEFAULT_RX_DESC % dev_info.rx_desc_lim.nb_align == 0);

                assert!(DEFAULT_TX_DESC <= dev_info.tx_desc_lim.nb_max);
                assert!(DEFAULT_TX_DESC >= dev_info.tx_desc_lim.nb_min);
                assert!(DEFAULT_TX_DESC % dev_info.tx_desc_lim.nb_align == 0);

                // Safety: `rte_eth_conf` contains primitive integer types. Safe to fill with zeros.
                let mut port_conf: dpdk_sys::rte_eth_conf = unsafe { std::mem::zeroed() };
                port_conf.rxmode.max_rx_pkt_len = dpdk_sys::RTE_ETHER_MAX_LEN;
                port_conf.rxmode.mq_mode = dpdk_sys::rte_eth_rx_mq_mode_ETH_MQ_RX_NONE;
                port_conf.txmode.mq_mode = dpdk_sys::rte_eth_tx_mq_mode_ETH_MQ_TX_NONE;
                if rx_queue_count > 1 {
                    port_conf.rxmode.mq_mode = dpdk_sys::rte_eth_rx_mq_mode_ETH_MQ_RX_RSS;
                    port_conf.rx_adv_conf.rss_conf.rss_hf = (dpdk_sys::ETH_RSS_NONFRAG_IPV4_UDP
                        | dpdk_sys::ETH_RSS_NONFRAG_IPV4_TCP)
                        .into();
                    // TODO set symmetric RSS for TCP/IP
                }

                // Enable offload flags
                if dev_info.rx_offload_capa & u64::from(dpdk_sys::DEV_RX_OFFLOAD_CHECKSUM) > 0 {
                    info!("RX CKSUM Offloading is on for port {}", port_id);
                    port_conf.rxmode.offloads |= u64::from(dpdk_sys::DEV_RX_OFFLOAD_CHECKSUM);
                }
                if dev_info.tx_offload_capa & u64::from(dpdk_sys::DEV_TX_OFFLOAD_IPV4_CKSUM) > 0 {
                    info!("TX IPv4 CKSUM Offloading is on for port {}", port_id);
                    port_conf.txmode.offloads |= u64::from(dpdk_sys::DEV_TX_OFFLOAD_IPV4_CKSUM);
                }
                if dev_info.tx_offload_capa & u64::from(dpdk_sys::DEV_TX_OFFLOAD_UDP_CKSUM) > 0 {
                    info!("TX UDP CKSUM Offloading is on for port {}", port_id);
                    port_conf.txmode.offloads |= u64::from(dpdk_sys::DEV_TX_OFFLOAD_UDP_CKSUM);
                }
                if dev_info.tx_offload_capa & u64::from(dpdk_sys::DEV_TX_OFFLOAD_TCP_CKSUM) > 0 {
                    info!("TX TCP CKSUM Offloading is on for port {}", port_id);
                    port_conf.txmode.offloads |= u64::from(dpdk_sys::DEV_TX_OFFLOAD_TCP_CKSUM);
                }

                // Configure ports
                let ret = unsafe {
                    dpdk_sys::rte_eth_dev_configure(
                        port_id,
                        rx_queue_count,
                        tx_queue_count,
                        &port_conf,
                    )
                };
                assert_eq!(ret, 0);
                (port, dev_info, port_socket_id, rx_cpus, tx_cpus)
            });

        // Configure RX queues
        let with_rxqs = configured_port_socket_rx_tx_pairs.map(
            |(port, dev_info, port_socket_id, rx_cpus, tx_cpus)| {
                let port_id = port.inner.port_id;

                let cpu_rxq_list = rx_cpus
                    .iter()
                    .enumerate()
                    .map(|(rxq_idx, rx_cpu)| {
                        // Create MPool for RX
                        let pool_name = format!("rxq_{}_{}_{}", MAGIC, port_id, rxq_idx);
                        let mpool = MPool::new(
                            self,
                            pool_name,
                            DEFAULT_RX_POOL_SIZE,
                            DEFAULT_RX_PER_CORE_CACHE,
                            DEFAULT_PACKET_DATA_LENGTH,
                            port_socket_id,
                        );
                        let ret = unsafe {
                            dpdk_sys::rte_eth_rx_queue_setup(
                                port_id,
                                rxq_idx as u16,
                                DEFAULT_RX_DESC,
                                port_socket_id as u32,
                                &dev_info.default_rxconf,
                                mpool.inner.ptr.as_ptr(),
                            )
                        };
                        assert_eq!(ret, 0);

                        (
                            *rx_cpu,
                            RxQ {
                                inner: Arc::new(RxQInner {
                                    queue_id: rxq_idx as u16,
                                    port_ref: port.inner.clone(),
                                    mpool_ref: mpool.inner,
                                }),
                            },
                        )
                    })
                    .collect::<Vec<_>>();
                (port, dev_info, port_socket_id, cpu_rxq_list, tx_cpus)
            },
        );

        // Configure TX queues
        let with_rxtxqs: Vec<_> = with_rxqs
            .map(|(port, dev_info, port_socket_id, cpu_rxq_list, tx_cpus)| {
                let port_id = port.inner.port_id;

                let cpu_txq_list = tx_cpus
                    .iter()
                    .enumerate()
                    .map(|(txq_idx, tx_cpu)| {
                        let ret = unsafe {
                            dpdk_sys::rte_eth_tx_queue_setup(
                                port_id,
                                txq_idx as u16,
                                DEFAULT_RX_DESC,
                                port_socket_id as u32,
                                &dev_info.default_txconf,
                            )
                        };
                        assert_eq!(ret, 0);

                        (
                            *tx_cpu,
                            TxQ {
                                inner: Arc::new(TxQInner {
                                    queue_id: txq_idx as u16,
                                    port_ref: port.inner.clone(),
                                }),
                            },
                        )
                    })
                    .collect::<Vec<_>>();

                //
                (port, dev_info, port_socket_id, cpu_rxq_list, cpu_txq_list)
            })
            .collect();

        // Sort via lcore ids.
        let lcore_to_rxtx_map = with_rxtxqs.into_iter().fold(
            HashMap::new(),
            |mut lcore_to_rxtx_map, (port, _, _, cpu_rxq_list, cpu_txq_list)| {
                let port_id = port.inner.port_id;

                // Set promisc.
                // Safety: foreign function.
                unsafe {
                    if DEFAULT_PROMISC {
                        dpdk_sys::rte_eth_promiscuous_enable(port_id);
                    } else {
                        dpdk_sys::rte_eth_promiscuous_disable(port_id);
                    }
                };

                // Start port.
                // Safety: foreign function.
                let ret = unsafe { dpdk_sys::rte_eth_dev_start(port_id) };
                assert_eq!(ret, 0);

                cpu_rxq_list.into_iter().for_each(|(lcore_id, rxq)| {
                    lcore_to_rxtx_map
                        .entry(lcore_id)
                        .or_insert_with(|| (Vec::new(), Vec::new()))
                        .0
                        .push(rxq);
                });
                cpu_txq_list.into_iter().for_each(|(lcore_id, txq)| {
                    lcore_to_rxtx_map
                        .entry(lcore_id)
                        .or_insert_with(|| (Vec::new(), Vec::new()))
                        .1
                        .push(txq);
                });

                lcore_to_rxtx_map
            },
        );

        // Return array of `(LCore, Vec<RxQ>, Vec<TxQ>)`.
        lcore_to_rxtx_map
            .into_iter()
            .map(|(lcore_id, (rxqs, txqs))| (lcore_id, rxqs, txqs))
            .collect()
    }
}

pub use super::dpdk_sys::EalStaticFunctions as EalGlobalApi;

unsafe impl EalGlobalApi for Eal {}

impl EalInner {
    // Create `EalInner`.
    #[inline]
    fn new(args: &mut Vec<String>) -> Result<Self, EalError> {
        // To prevent DPDK PMDs' being unlinked, we explicitly create symbolic dependency via
        // calling `load_drivers`.
        dpdk_sys::load_drivers();

        // DPDK returns number of consumed argc
        // Safety: foriegn function (safe unless there is a bug)
        let ret = unsafe { ffi::run_with_args(dpdk_sys::rte_eal_init, &*args) };
        if ret < 0 {
            return Err(EalError::ErrorCode { code: ret });
        }

        // Strip first n args and return the remaining
        args.drain(..ret as usize);
        Ok(EalInner {
            shared: RwLock::new(EalSharedInner {}),
        })
    }
}

impl Drop for EalInner {
    #[inline]
    fn drop(&mut self) {
        // Safety: foriegn function (safe unless there is a bug)
        unsafe {
            let ret = dpdk_sys::rte_eal_cleanup();
            if ret == -(dpdk_sys::ENOTSUP as i32) {
                warn!("EAL Cleanup is not implemented.");
                return;
            }
            assert_eq!(ret, 0);
        }
    }
}

pub enum EalErrorCode {
    EPERM = -1,
    ENOENT = -2,
    ESRCH = -3,
    EINTR = -4,
    EIO = -5,
    ENXIO = -6,
    E2BIG = -7,
    ENOEXEC = -8,
    EBADF = -9,
    ECHILD = -10,
    EAGAIN = -11,
    ENOMEM = -12,
    EACCES = -13,
    EFAULT = -14,
    ENOTBLK = -15,
    EBUSY = -16,
    EEXIST = -17,
    EXDEV = -18,
    ENODEV = -19,
    ENOTDIR = -20,
    EISDIR = -21,
    EINVAL = -22,
    ENFILE = -23,
    EMFILE = -24,
    ENOTTY = -25,
    ETXTBSY = -26,
    EFBIG = -27,
    ENOSPC = -28,
    ESPIPE = -29,
    EROFS = -30,
    EMLINK = -31,
    EPIPE = -32,
    EDOM = -33,
    ERANGE = -34,
    //EDEADLK = -35, Dup to EDEADLOCK
    ENAMETOOLONG = -36,
    ENOLCK = -37,
    ENOSYS = -38,
    ENOTEMPTY = -39,
    ELOOP = -40,
    //EWOULDBLOCK = -11, Dup to EAGAIN
    ENOMSG = -42,
    EIDRM = -43,
    ECHRNG = -44,
    EL2NSYNC = -45,
    EL3HLT = -46,
    EL3RST = -47,
    ELNRNG = -48,
    EUNATCH = -49,
    ENOCSI = -50,
    EL2HLT = -51,
    EBADE = -52,
    EBADR = -53,
    EXFULL = -54,
    ENOANO = -55,
    EBADRQC = -56,
    EBADSLT = -57,
    EDEADLOCK = -35,
    EBFONT = -59,
    ENOSTR = -60,
    ENODATA = -61,
    ETIME = -62,
    ENOSR = -63,
    ENONET = -64,
    ENOPKG = -65,
    EREMOTE = -66,
    ENOLINK = -67,
    EADV = -68,
    ESRMNT = -69,
    ECOMM = -70,
    EPROTO = -71,
    EMULTIHOP = -72,
    EDOTDOT = -73,
    EBADMSG = -74,
    EOVERFLOW = -75,
    ENOTUNIQ = -76,
    EBADFD = -77,
    EREMCHG = -78,
    ELIBACC = -79,
    ELIBBAD = -80,
    ELIBSCN = -81,
    ELIBMAX = -82,
    ELIBEXEC = -83,
    EILSEQ = -84,
    ERESTART = -85,
    ESTRPIPE = -86,
    EUSERS = -87,
    ENOTSOCK = -88,
    EDESTADDRREQ = -89,
    EMSGSIZE = -90,
    EPROTOTYPE = -91,
    ENOPROTOOPT = -92,
    EPROTONOSUPPORT = -93,
    ESOCKTNOSUPPORT = -94,
    //EOPNOTSUPP = -95, Dup to ENOTSUP
    EPFNOSUPPORT = -96,
    EAFNOSUPPORT = -97,
    EADDRINUSE = -98,
    EADDRNOTAVAIL = -99,
    ENETDOWN = -100,
    ENETUNREACH = -101,
    ENETRESET = -102,
    ECONNABORTED = -103,
    ECONNRESET = -104,
    ENOBUFS = -105,
    EISCONN = -106,
    ENOTCONN = -107,
    ESHUTDOWN = -108,
    ETOOMANYREFS = -109,
    ETIMEDOUT = -110,
    ECONNREFUSED = -111,
    EHOSTDOWN = -112,
    EHOSTUNREACH = -113,
    EALREADY = -114,
    EINPROGRESS = -115,
    ESTALE = -116,
    EUCLEAN = -117,
    ENOTNAM = -118,
    ENAVAIL = -119,
    EISNAM = -120,
    EREMOTEIO = -121,
    EDQUOT = -122,
    ENOMEDIUM = -123,
    EMEDIUMTYPE = -124,
    ECANCELED = -125,
    ENOKEY = -126,
    EKEYEXPIRED = -127,
    EKEYREVOKED = -128,
    EKEYREJECTED = -129,
    EOWNERDEAD = -130,
    ENOTRECOVERABLE = -131,
    ERFKILL = -132,
    EHWPOISON = -133,
    ENOTSUP = -95,
}

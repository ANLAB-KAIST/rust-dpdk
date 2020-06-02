//! Wrapper for DPDK's environment abstraction layer (EAL).
use ffi;
use log::{info, warn};
use std::collections::{HashMap, HashSet};
use std::convert::{TryFrom, TryInto};
use std::ffi::CString;
use std::mem::size_of;
use std::sync::{Arc, Mutex, RwLock};
use thiserror::Error;

#[derive(Debug)]
struct EalSharedInner {} // TODO Remove this if unnecessary

#[derive(Debug)]
struct EalInner {
    shared: RwLock<EalSharedInner>,

    /// DPDK Eal object must be initialized by a single core.
    ///
    /// Note: " The creation and initialization functions for these objects are not multi-thread safe.
    /// However, once initialized, the objects themselves can safely be used in multiple threads
    /// simultaneously."
    /// - https://doc.dpdk.org/guides/prog_guide/env_abstraction_layer.html
    global_lock: Mutex<bool>,
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
    port_id: i32,
}

impl Port {}

/// Placeholder for DPDK Pool's private data
#[derive(Debug)]
pub struct MPoolPriv {}

/// Abstract type for DPDK MPool
#[derive(Debug, Clone)]
pub struct MPool {
    inner: Arc<MPoolInner>,
}

#[derive(Debug)]
struct MPoolInner {}

impl MPoolInner {}

impl MPool {
    /// Create a new `MPool`.
    /// Note: Pool name must be globally unique.
    #[inline]
    pub fn new<StringLike: Into<Vec<u8>>>(
        name: StringLike,
        size: usize,
        per_core_cache_size: usize,
        data_len: usize,
        socket_id: i32,
    ) -> Self {
        let pool_name = CString::new(name).unwrap();
        let pool = dpdk_sys::rte_pktmbuf_pool_create(
            pool_name.into_bytes_with_nul().as_ptr() as *mut i8,
            size.try_into().unwrap(),
            per_core_cache_size as u32,
            (((size_of::<MPoolPriv>() + 7) / 8) * 8) as u16,
            data_len.try_into().unwrap(),
            socket_id,
        );
        // TODO doing here
    }
}

/// Abstract type for DPDK RxQ
#[derive(Debug, Clone)]
pub struct RxQ {
    inner: Arc<RxQInner>,
}

#[derive(Debug)]
struct RxQInner {
    //queue_id: i32,
}

impl RxQ {}

/// Abstract type for DPDK TxQ
#[derive(Debug, Clone)]
pub struct TxQ {
    inner: Arc<TxQInner>,
}

#[derive(Debug)]
struct TxQInner {
    //queue_id: i32,
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
    ///
    #[inline]
    pub fn setup(
        &self,
        rx_affinity: Affinity,
        tx_affinity: Affinity,
        num_rx_desc: u16,
        num_tx_desc: u16,
    ) {
        /// # Safety
        /// All unsafe lines are for calling foriegn functions.
        unsafe {
            // List of valid logical core ids.
            // Note: If some cores are masked, range (0..rte_lcore_count()) will include disabled cores.
            let lcore_id_list = (0..dpdk_sys::RTE_MAX_LCORE)
                .filter(|index| dpdk_sys::rte_lcore_is_enabled(*index) > 0);

            // List of `(lcore_id, socket_id)` pairs.
            let lcore_socket_pair_list: Vec<_> = lcore_id_list
                .clone()
                .map(|lcore_id| {
                    let lcore_socket_id =
                        dpdk_sys::rte_lcore_to_socket_id(lcore_id.try_into().unwrap());
                    let cpu_id = dpdk_sys::rte_lcore_to_cpu_id(lcore_id.try_into().unwrap());
                    let is_enabled = dpdk_sys::rte_lcore_is_enabled(lcore_id) > 0;
                    assert!(is_enabled);
                    println!(
                        "lcore id {} {}: socket {}, core {}.",
                        lcore_id, is_enabled, lcore_socket_id, cpu_id
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

            let port_id_list = (0..u16::try_from(dpdk_sys::RTE_MAX_ETHPORTS).unwrap())
                .filter(|index| dpdk_sys::rte_eth_dev_is_valid_port(*index) > 0);
            println!("port_id_list {:?}", port_id_list);

            // List of `(port, vec<rx_lcore_ids>, vec<tx_lcore_ids>)`.
            // Note: We need number of rx cores and tx cores at the same time (`rte_eth_dev_configure`)
            let port_rx_tx_pairs = port_id_list.clone().map(|port_id| {
                let port_socket_id = dpdk_sys::rte_eth_dev_socket_id(port_id);
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
                (port_id, rx_lcore_for_this_port, tx_lcore_for_this_port)
            });

            // Init each port
            port_rx_tx_pairs.map(|(port_id, rx_cpus, tx_cpus)| {
                // Safety: `rte_eth_dev_info` contains primitive integer types. Safe to fill with zeros.
                let mut dev_info: dpdk_sys::rte_eth_dev_info = unsafe { std::mem::zeroed() };
                dpdk_sys::rte_eth_dev_info_get(port_id, &mut dev_info);

                let rx_queue_limit = dev_info.max_rx_queues;
                let tx_queue_limit = dev_info.max_tx_queues;
                let rx_queue_count: u16 = rx_cpus.len().try_into().unwrap();
                let tx_queue_count: u16 = tx_cpus.len().try_into().unwrap();

                assert!(rx_queue_count <= rx_queue_limit);
                assert!(tx_queue_count <= tx_queue_limit);

                assert!(num_rx_desc <= dev_info.rx_desc_lim.nb_max);
                assert!(num_rx_desc >= dev_info.rx_desc_lim.nb_min);
                assert!(num_rx_desc % dev_info.rx_desc_lim.nb_align == 0);

                assert!(num_tx_desc <= dev_info.tx_desc_lim.nb_max);
                assert!(num_tx_desc >= dev_info.tx_desc_lim.nb_min);
                assert!(num_tx_desc % dev_info.tx_desc_lim.nb_align == 0);

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
                let ret = dpdk_sys::rte_eth_dev_configure(
                    port_id,
                    rx_queue_count,
                    tx_queue_count,
                    &port_conf,
                );
                assert!(ret == 0);

                // Create MPool for RX
            });

            // TODO Doing here
            panic!("Not implemented");
        }
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
            global_lock: Mutex::new(false),
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

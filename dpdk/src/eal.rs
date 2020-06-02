//! Wrapper for DPDK's environment abstraction layer (EAL).
use std::collections::{HashMap, HashSet};
use ffi;
use std::sync::{Arc, RwLock, Mutex};
use thiserror::Error;
use std::convert::TryInto;
use log::{info, warn};

#[derive(Debug)]
struct EalSharedInner {
} // TODO Remove this if unnecessary

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
    Numa 
}

/// Supported CPU layout
pub struct CpuLayout { rx: Affinity, tx: Affinity }


/// Abstract type for DPDK port
#[derive(Debug, Clone)]
pub struct Port {
    inner: Arc<PortInner>,
}

#[derive(Debug)]
struct PortInner {
    port_id: i32,
}

impl Port {

}

/// Abstract type for DPDK MPool
#[derive(Debug, Clone)]
pub struct MPool {
    inner: Arc<MPoolInner>,
}

#[derive(Debug)]
struct MPoolInner {
}

impl MPool {

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

impl RxQ {

}

/// Abstract type for DPDK TxQ
#[derive(Debug, Clone)]
pub struct TxQ {
    inner: Arc<TxQInner>,
}

#[derive(Debug)]
struct TxQInner {
    //queue_id: i32,
}

impl TxQ {

}

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
    pub fn setup(&self, layout: CpuLayout) {
        /// # Safety
        /// All unsafe lines are for calling foriegn functions.
        unsafe {
            // List of valid logical core ids.
            // Note: If some cores are masked, range (0..rte_lcore_count()) will include disabled cores.
            let lcore_id_list = (0..dpdk_sys::RTE_MAX_LCORE).filter(|index| dpdk_sys::rte_lcore_is_enabled(*index) > 0);

            // List of `(lcore_id, socket_id)` pairs.
            let lcore_socket_pair_list: Vec<_> = lcore_id_list.map(|lcore_id|{
                let lcore_socket_id = dpdk_sys::rte_lcore_to_socket_id(lcore_id.try_into().unwrap());
                let cpu_id = dpdk_sys::rte_lcore_to_cpu_id(lcore_id.try_into().unwrap());
                let is_enabled = dpdk_sys::rte_lcore_is_enabled(lcore_id) > 0;
                assert!(is_enabled);
                println!("lcore id {} {}: socket {}, core {}.", lcore_id, is_enabled, lcore_socket_id, cpu_id);
                (lcore_id, lcore_socket_id)
            }).collect();
            println!("lcore count: {}", lcore_socket_pair_list.len());

            // Sort lcore ids with map
            let socket_to_lcore_map = lcore_socket_pair_list.iter().fold(HashMap::new(), |mut sort_by_socket, (lcore_id, socket_id)|{
                sort_by_socket.entry(socket_id).or_insert_with(HashSet::new).insert(lcore_id);
                sort_by_socket
            });

            let port_id_list = (0..dpdk_sys::RTE_MAX_ETHPORTS).filter(|index| dpdk_sys::rte_eth_dev_is_valid_port(*index as u16) > 0);
            println!("port_id_list {:?}", port_id_list);
            
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
            global_lock: Mutex::new(false)
        })
    }
}

impl Drop for EalInner {
    #[inline]
    fn drop(&mut self) {
        // Safety: foriegn function (safe unless there is a bug)
        unsafe {
            let ret = dpdk_sys::rte_eal_cleanup();
            if ret == - (dpdk_sys::ENOTSUP as i32) {
                warn!("EAL Cleanup is not implemented.");
                return;
            }
            assert_eq!(ret, 0);
        }
    }
}

//! Wrapper for DPDK's environment abstraction layer (EAL).
use ffi;
use std::sync::{Arc, RwLock};
use thiserror::Error;

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

/// Supported CPU layout
#[derive(Debug)]
pub enum CPULayout {
    /// Each CPU has dedicated RX/TX queues for every NIC.
    FullMesh,
    /// Each CPU has dedicated TX queues for every NIC, but RX queues for NICs on same NUMA node.
    RxNumaAffinity,
    /// Each CPU has dedicated RX/TX queues for NICs on same NUMA node.
    RxTxNumaAffinity,
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
    #[inline]
    pub fn setup(&self, layout: CPULayout) -> impl Iterator<RteThread, Vec<RxQ>, Vec<TxQ>> {
        panic!("not implemented.")
    }

    /// Candidate 2, get a functor for per-thread functions
    #[inline]
    pub fn launch(&self, layout: CPULayout, per_thread: Fn(Vec<RxQ>, Vec<TxQ>) -> Fn() -> impl Future) {
        panic!("not implemented.")
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
            assert_eq!(ret, 0);
        }
    }
}

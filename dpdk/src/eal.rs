//! Wrapper for DPDK's environment abstraction layer (EAL).
use ffi;
use std::convert::TryFrom;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use thiserror::Error;

#[derive(Debug)]
struct EalSharedInner {} // TODO Remove this if unnecessary

#[derive(Debug)]
struct EalInner {
    shared: RwLock<EalSharedInner>,
}

#[derive(Debug, Clone)]
pub struct Eal {
    inner: Arc<EalInner>,
}

#[derive(Debug, Error)]
pub enum EalError {
    #[error("EAL instance should be only created once")]
    Singleton,
    #[error("EAL function returned an error code: {}", code)]
    ErrorCode { code: i32 },
}

impl Eal {
    /// Create an `Eal` instance (DPDK's environment abstraction layer).
    ///
    /// It takes command-line arguments and returns unused arguments.
    ///
    /// DPDK does not recommand users to repeat initialize and clear.
    /// https://doc.dpdk.org/api/rte__eal_8h.html#a7a745887f62a82dc83f1524e2ff2a236
    /// "It is expected that common usage of this function is to call it just before terminating the process."
    #[inline]
    pub fn new(args: &mut Vec<String>) -> Result<Self, EalError> {
        Ok(Eal {
            inner: Arc::new(EalInner::new(args)?),
        })
    }
}

//static INITIALIZED: AtomicBool = AtomicBool::new(false);

impl EalInner {
    const INITIALIZED: AtomicBool = AtomicBool::new(false);
    // Create `EalInner`.
    #[inline]
    fn new(args: &mut Vec<String>) -> Result<Self, EalError> {
        if EalInner::INITIALIZED
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Err(EalError::Singleton);
        }

        // 1. DPDK returns number of consumed argc
        // Safety: foriegn function (safe unless there is a bug)
        let ret = unsafe { ffi::run_with_args(dpdk_sys::rte_eal_init, &*args) };
        if ret < 0 {
            Err(EalError::ErrorCode { code: ret })
        } else {
            // 2. Strip first n args and return the remaining
            let _: Vec<_> = args.drain(..usize::try_from(ret).unwrap()).collect();
            Ok(EalInner {
                shared: RwLock::new(EalSharedInner {}),
            })
        }
    }
}

impl Drop for EalInner {
    #[inline]
    fn drop(&mut self) {
        // TODO: Release lock when repeating `eal_init` and `eal_cleanup` is stabilized.
        // See `Eal::new` for more information.
        // EalInner::INITIALIZED.store(false, Ordering::Release);

        // Safety: foriegn function (safe unless there is a bug)
        unsafe {
            let ret = dpdk_sys::rte_eal_cleanup();
            assert_eq!(ret, 0);
        }
    }
}

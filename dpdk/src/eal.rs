//! Wrapper for DPDK's environment abstraction layer (EAL).
use ffi;
use std::convert::TryInto;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

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

#[derive(Debug, Fail)]
pub enum EalError {
    #[fail(display = "EAL instance should be only created once")]
    Singleton,
    #[fail(display = "EAL function returned an error code: {}", code)]
    ErrorCode { code: i32 },
}

impl Eal {
    /// Create an `Eal` instance.
    ///
    /// It takes command-line arguments and returns unused arguments.
    ///
    /// DPDK does not recommand users to repeat initialize and clear.
    /// https://doc.dpdk.org/api/rte__eal_8h.html#a7a745887f62a82dc83f1524e2ff2a236
    /// "It is expected that common usage of this function is to call it just before terminating the process."
    #[inline]
    pub fn new(args: Vec<String>) -> Result<(Self, Vec<String>), EalError> {
        EalInner::new(args).map(|(inner, rem)| {
            (
                Eal {
                    inner: Arc::new(inner),
                },
                rem,
            )
        })
    }
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);

impl EalInner {
    // Create `EalInner`.
    #[inline]
    fn new(args: Vec<String>) -> Result<(Self, Vec<String>), EalError> {
        if INITIALIZED
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            // 1. DPDK returns number of consumed argc
            // Safety: foriegn function (safe unless there is a bug)
            let ret = unsafe { ffi::run_with_args(dpdk_sys::rte_eal_init, &args) };
            if ret < 0 {
                Err(EalError::ErrorCode { code: ret })
            } else {
                // 2. Strip first n args and return the remaining
                let remaining: Vec<String> =
                    args.into_iter().skip(ret.try_into().unwrap()).collect();
                let eal_inner = EalInner {
                    shared: RwLock::new(EalSharedInner {}),
                };
                Ok((eal_inner, remaining))
            }
        } else {
            Err(EalError::Singleton)
        }
    }
}

impl Drop for EalInner {
    #[inline]
    fn drop(&mut self) {
        // TODO: Release lock when repeating `eal_init` and `eal_cleanup` is stabilized.
        // INITIALIZED.store(false, Ordering::Release);

        // Safety: foriegn function (safe unless there is a bug)
        unsafe {
            let ret = dpdk_sys::rte_eal_cleanup();
            assert_eq!(ret, 0);
        }
    }
}

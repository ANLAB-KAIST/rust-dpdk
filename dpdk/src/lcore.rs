//! DPDK-related LCore and Socket types.

use log::*;

/// Logical Core ID
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct LCoreId(u32);

impl From<LCoreId> for u32 {
    #[inline]
    fn from(from: LCoreId) -> Self {
        from.0
    }
}

#[derive(Debug)]
pub struct LCoreHandle {
    id: LCoreId,
    ptr: *mut dyn FnOnce(),
}
impl Drop for LCoreHandle {
    fn drop(&mut self) {
        // Safety: foreign function.
        unsafe {
            dpdk_sys::rte_eal_wait_lcore(self.id.0);
            Box::from_raw(self.ptr)
        }
    }
}

impl LCoreId {
    /// Create a new LCoreId.
    pub(crate) fn new(id: u32) -> Self {
        Self(id)
    }
    /// Bind the current thread to this core.
    pub fn bind_current_thread(&self) {
        // Safety: foreign function.
        let ret = unsafe {
            dpdk_sys::rte_thread_set_affinity(&mut dpdk_sys::rte_lcore_cpuset(self.0))
        };
        if ret < 0 {
            warn!("Failed to set affinity on lcore {}", self.0);
        }
    }

    /// Get the socket ID for this core.
    pub fn socket(&self) -> SocketId {
        // Safety: foreign function.
        let socket_id = unsafe { dpdk_sys::rte_lcore_to_socket_id(self.0) };
        if socket_id == dpdk_sys::RTE_MAX_NUMA_NODES as i32 {
            warn!("Failed to get socket id for lcore {}", self.0);
        }
        SocketId::new(socket_id as u32)
    }

    /// Get the CPU ID for this core.
    pub fn cpu_id(&self) -> SocketId {
        let cpu_id = unsafe { dpdk_sys::rte_lcore_to_cpu_id(self.0 as i32) };
        if cpu_id < 0 {
            warn!("Failed to get cpu id for lcore {}", self.0);
        }
        SocketId::new(cpu_id as u32)
    }

    /// Check if this core is enabled.
    pub fn is_enabled(&self) -> bool {
        // Safety: foreign function.
        unsafe { dpdk_sys::rte_lcore_is_enabled(self.0) != 0 }
    }

    /// Get the current core.
    pub fn current() -> Self {
        // Safety: foreign function.
        let id = unsafe { dpdk_sys::rte_lcore_id() };
        Self::new(id)
    }

    /// Run a closure on this core.
    pub fn run<F: FnOnce() + Send>(&self, f: F) {
        let mut f: Box<dyn FnOnce()> = Box::new(f);
        // Safety: foreign function.
        unsafe {
            let fn_ptr = Box::into_raw(f);
            let ret = dpdk_sys::rte_eal_remote_launch(
                fn_ptr,
                std::ptr::null_mut(),
                self.0,
            );
            if ret < 0 {
                panic!("Failed to launch on lcore {}", self.0);
            }
            LCoreHandle {
                id: self.clone(),
                ptr: fn_ptr,
            }
        };
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct SocketId(u32);

impl From<SocketId> for u32 {
    #[inline]
    fn from(from: SocketId) -> Self {
        from.0
    }
}

impl SocketId {
    #[inline]
    pub(crate) fn new(id: u32) -> Self {
        Self(id)
    }
}

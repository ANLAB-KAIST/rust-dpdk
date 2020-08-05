//! Wrapper for DPDK's environment abstraction layer (EAL).
use crate::ffi;
use arrayvec::*;
use crossbeam::thread::{Scope, ScopedJoinHandle};
use log::{debug, info, warn};
use std::collections::{HashMap, HashSet};
use std::convert::{TryFrom, TryInto};
use std::ffi::CString;
use std::fmt;
use std::marker::PhantomData;
use std::mem::{size_of, MaybeUninit};
use std::ptr::{self, NonNull};
use std::slice;
use std::sync::{Arc, Mutex};
use std::thread;
use thiserror::Error;

const MAGIC: &str = "be0dd4ab";

pub const DEFAULT_TX_DESC: u16 = 128;
pub const DEFAULT_RX_DESC: u16 = 128;
pub const DEFAULT_RX_POOL_SIZE: usize = 1024;
pub const DEFAULT_RX_PER_CORE_CACHE: usize = 0;
pub const DEFAULT_PACKET_DATA_LENGTH: usize = 2048;
pub const DEFAULT_PROMISC: bool = true;
pub const DEFAULT_RX_BURST: usize = 32;
pub const DEFAULT_TX_BURST: usize = 32;

/// A garbage collection request.
trait Garbage {
    /// Try to do garbage collection for a certain resource.
    /// Returns true if it succeeded to free an object.
    ///
    /// # Safety
    /// `try_collect` must not be called after it returned `true`.
    unsafe fn try_collect(&mut self) -> bool;
}

/// Shared mutating states that all `Eal` instances share.
struct EalGlobalInner {
    // Whether `setup` has been successfully invoked.
    setup_initialized: bool,
    // List of garbage collection requrests.
    // Each req tries garbage collection and returns true on success.
    // (e.g. `try_free`).
    // TODO: periodically do cleanup.
    garbages: Vec<Box<dyn Garbage>>,
} // TODO Remove this if unnecessary

impl fmt::Debug for EalGlobalInner {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EalGlobalInner")
            .field("setup_initialized", &self.setup_initialized)
            .field("garbages (count)", &self.garbages.len())
            .finish()
    }
}

// Safety: rte_mempool is thread-safe.
unsafe impl Send for EalGlobalInner {}
unsafe impl Sync for EalGlobalInner {}

impl Default for EalGlobalInner {
    #[inline]
    fn default() -> Self {
        Self {
            setup_initialized: false,
            garbages: Default::default(),
        }
    }
}

#[derive(Debug)]
struct EalInner {
    shared: Mutex<EalGlobalInner>,
}

/// DPDK's environment abstraction layer (EAL).
///
/// This object indicates that EAL has been initialized and its APIs are available now.
#[derive(Debug, Clone)]
pub struct Eal {
    inner: Arc<EalInner>,
}

/// How to create NIC queues for a CPU.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct LCoreId(u32);

impl Into<u32> for LCoreId {
    #[inline]
    fn into(self) -> u32 {
        self.0
    }
}

impl LCoreId {
    #[inline]
    fn new(id: u32) -> Self {
        Self(id)
    }

    /// Launch a thread pined to this core.
    /// TODO: change it to crossbeam's `spawn` signature when we start to use crossbeam.
    pub fn launch<F, T>(self, f: F) -> thread::JoinHandle<T>
    where
        F: FnOnce() -> T,
        F: Send + 'static,
        T: Send + 'static,
    {
        let lcore_id = self.0;
        thread::spawn(move || {
            // Safety: foreign function.
            let ret = unsafe {
                dpdk_sys::rte_thread_set_affinity(&mut dpdk_sys::rte_lcore_cpuset(lcore_id))
            };
            if ret < 0 {
                warn!("Failed to set affinity on lcore {}", lcore_id);
            }
            f()
        })
    }

    /// Launch a thread pined to this core.
    /// TODO: change it to crossbeam's `spawn` signature when we start to use crossbeam.
    pub fn launch_scoped<'scope, 'env, F, T>(
        self,
        s: &'scope Scope<'env>,
        f: F,
    ) -> ScopedJoinHandle<'scope, T>
    where
        F: FnOnce() -> T,
        F: Send + 'env,
        T: Send + 'env,
    {
        let lcore_id = self.0;
        s.spawn(move |_| {
            // Safety: foreign function.
            let ret = unsafe {
                dpdk_sys::rte_thread_set_affinity(&mut dpdk_sys::rte_lcore_cpuset(lcore_id))
            };
            if ret < 0 {
                warn!("Failed to set affinity on lcore {}", lcore_id);
            }
            f()
        })
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct SocketId(u32);

impl Into<u32> for SocketId {
    #[inline]
    fn into(self) -> u32 {
        self.0
    }
}

impl SocketId {
    #[inline]
    fn new(id: u32) -> Self {
        Self(id)
    }
}

#[derive(Debug, Error, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorCode {
    #[error("Unknown error code: {}", code)]
    Unknown { code: u8 },
}

impl From<u8> for ErrorCode {
    #[inline]
    fn from(code: u8) -> Self {
        Self::Unknown { code }
    }
}
impl TryFrom<u32> for ErrorCode {
    type Error = <u8 as TryFrom<u32>>::Error;
    #[inline]
    fn try_from(code: u32) -> Result<Self, Self::Error> {
        Ok(Self::Unknown {
            code: code.try_into()?,
        })
    }
}
impl TryFrom<i32> for ErrorCode {
    type Error = <u8 as TryFrom<i32>>::Error;
    #[inline]
    fn try_from(code: i32) -> Result<Self, Self::Error> {
        Ok(Self::Unknown {
            code: (-code).try_into()?,
        })
    }
}

impl Port {
    /// Returns current port index.
    #[inline]
    pub fn port_id(&self) -> u16 {
        self.inner.port_id
    }

    /// Returns NUMA node of current port.
    #[inline]
    pub fn socket_id(&self) -> SocketId {
        SocketId::new(unsafe {
            dpdk_sys::rte_eth_dev_socket_id(self.inner.port_id)
                .try_into()
                .unwrap()
        })
    }

    /// Returns NUMA node of current port.
    #[inline]
    pub fn mac_addr(&self) -> [u8; 6] {
        unsafe {
            let mut mac_addr = MaybeUninit::uninit();
            let ret = dpdk_sys::rte_eth_macaddr_get(self.inner.port_id, mac_addr.as_mut_ptr());
            assert_eq!(ret, 0);
            mac_addr.assume_init().addr_bytes
        }
    }

    /// Returns current statistics
    #[inline]
    pub fn get_stat(&self) -> PortStat {
        // Safety: foreign function. Uninitialized data structure will be filled.
        let dpdk_stat = unsafe {
            let mut temp = MaybeUninit::uninit();
            let ret = dpdk_sys::rte_eth_stats_get(self.inner.port_id, temp.as_mut_ptr());
            assert_eq!(ret, 0);
            temp.assume_init()
        };
        if self.inner.has_stats_reset {
            PortStat {
                ipackets: dpdk_stat.ipackets,
                opackets: dpdk_stat.opackets,
                ibytes: dpdk_stat.ibytes,
                obytes: dpdk_stat.obytes,
                ierrors: dpdk_stat.ierrors,
                oerrors: dpdk_stat.oerrors,
                imissed: dpdk_stat.imissed,
                rx_nombuf: dpdk_stat.rx_nombuf,
                q_ipackets: dpdk_stat.q_ipackets,
                q_opackets: dpdk_stat.q_opackets,
                q_ibytes: dpdk_stat.q_ibytes,
                q_obytes: dpdk_stat.q_obytes,
                q_errors: dpdk_stat.q_errors,
            }
        } else {
            let prev_stat = self.inner.prev_stat.lock().unwrap();
            fn subtract_array(x: [u64; 16], y: [u64; 16]) -> [u64; 16] {
                let subtract_vals = x.iter().zip(y.iter()).map(|(x, y)| x - y);
                let mut temp: [u64; 16] = Default::default();
                for (ret, val) in (&mut temp).iter_mut().zip(subtract_vals) {
                    *ret = val;
                }
                temp
            }
            PortStat {
                ipackets: dpdk_stat.ipackets - prev_stat.ipackets,
                opackets: dpdk_stat.opackets - prev_stat.opackets,
                ibytes: dpdk_stat.ibytes - prev_stat.ibytes,
                obytes: dpdk_stat.obytes - prev_stat.obytes,
                ierrors: dpdk_stat.ierrors - prev_stat.ierrors,
                oerrors: dpdk_stat.oerrors - prev_stat.oerrors,

                imissed: dpdk_stat.imissed - prev_stat.imissed,
                rx_nombuf: dpdk_stat.rx_nombuf - prev_stat.rx_nombuf,
                q_ipackets: subtract_array(dpdk_stat.q_ipackets, prev_stat.q_ipackets),
                q_opackets: subtract_array(dpdk_stat.q_opackets, prev_stat.q_opackets),
                q_ibytes: subtract_array(dpdk_stat.q_ibytes, prev_stat.q_ibytes),
                q_obytes: subtract_array(dpdk_stat.q_obytes, prev_stat.q_obytes),
                q_errors: subtract_array(dpdk_stat.q_errors, prev_stat.q_errors),
            }
        }
    }

    /// Returns current statistics
    #[inline]
    pub fn reset_stat(&self) {
        // Safety: foreign function.
        if self.inner.has_stats_reset {
            let ret = unsafe { dpdk_sys::rte_eth_stats_reset(self.inner.port_id) };
            assert_eq!(ret, 0);
        } else {
            // Safety: foreign function. Uninitialized data structure will be filled.
            let dpdk_stat = unsafe {
                let mut temp = MaybeUninit::uninit();
                let ret = dpdk_sys::rte_eth_stats_get(self.inner.port_id, temp.as_mut_ptr());
                assert_eq!(ret, 0);
                temp.assume_init()
            };
            let mut prev_stat = self.inner.prev_stat.lock().unwrap();
            prev_stat.ipackets = dpdk_stat.ipackets;
            prev_stat.opackets = dpdk_stat.opackets;
            prev_stat.ibytes = dpdk_stat.ibytes;
            prev_stat.obytes = dpdk_stat.obytes;
            prev_stat.ierrors = dpdk_stat.ierrors;
            prev_stat.oerrors = dpdk_stat.oerrors;
        }
    }

    /// Get link status
    /// Note: this function might block up to 9 seconds.
    /// https://doc.dpdk.org/api/rte__ethdev_8h.html#a56200b0c25f3ecab5abe9bd2b647c215
    #[inline]
    fn get_link(&self) -> LinkStatus {
        // Safety: foreign function.
        unsafe {
            let mut temp = MaybeUninit::uninit();
            let ret = dpdk_sys::rte_eth_link_get(self.inner.port_id, temp.as_mut_ptr());
            assert_eq!(ret, 0);
            temp.assume_init()
        }
    }

    /// Returns true if link is up (connected), false if down.
    #[inline]
    pub fn is_link_up(&self) -> bool {
        self.get_link().link_status() == dpdk_sys::ETH_LINK_UP as u16
    }
}

use dpdk_sys::rte_eth_link as LinkStatus;
pub use dpdk_sys::rte_eth_stats as PortStat;

#[derive(Debug)]
struct PortInner {
    port_id: u16,
    owner_id: u64,
    has_stats_reset: bool,
    prev_stat: Mutex<PortStat>,
    eal: Arc<EalInner>,
}

impl Drop for PortInner {
    #[inline]
    fn drop(&mut self) {
        // Safety: foreign function.
        let ret = unsafe { dpdk_sys::rte_eth_dev_owner_unset(self.port_id, self.owner_id) };
        assert_eq!(ret, 0);
        unsafe {
            dpdk_sys::rte_eth_dev_stop(self.port_id);
            dpdk_sys::rte_eth_dev_close(self.port_id);
        }
        // TODO following code causes segmentation fault.  Its DPDK's bug that
        // `rte_eth_dev_owner_delete` does not check whether `rte_eth_devices[port_id].data` is
        // null.  Safety: foreign function.
        // let ret = unsafe { dpdk_sys::rte_eth_dev_owner_delete(self.owner_id) };
        // assert_eq!(ret, 0);
        info!("Port {} cleaned up", self.port_id);
    }
}

/// Traits for `zeroable` structures.
///
/// Related issue: https://github.com/rust-lang/rfcs/issues/2626
///
/// DPDK provides customizable per-packet metadata. However, it is initialized via
/// `memset(.., 0, ..)`, and its destructor is not called.
/// A structure must be safe from `MaybeUninit::zeroed().assume_init()`
/// and it must not implement `Drop` trait.
pub unsafe trait Zeroable: Sized {
    fn zeroed() -> Self {
        // Safety: contraints from this trait.
        unsafe { MaybeUninit::zeroed().assume_init() }
    }
}

/// Abstract type for DPDK MPool
#[derive(Debug, Clone)]
pub struct MPool<MPoolPriv: Zeroable> {
    inner: Arc<MPoolInner<MPoolPriv>>,
}

#[derive(Debug)]
struct MPoolInner<MPoolPriv: Zeroable> {
    ptr: NonNull<dpdk_sys::rte_mempool>,
    eal: Arc<EalInner>,
    _phantom: PhantomData<MPoolPriv>,
}

/// # Safety
/// Mempools are thread-safe.
/// https://doc.dpdk.org/guides/prog_guide/thread_safety_dpdk_functions.html
unsafe impl<MPoolPriv: Zeroable> Send for MPoolInner<MPoolPriv> {}
unsafe impl<MPoolPriv: Zeroable> Sync for MPoolInner<MPoolPriv> {}

impl<MPoolPriv: Zeroable> Drop for MPoolInner<MPoolPriv> {
    #[inline]
    fn drop(&mut self) {
        // Check whether the pool can be destroyed now.
        // Note: I am the only reference to the pool object.
        struct MPoolGcReq {
            ptr: NonNull<dpdk_sys::rte_mempool>,
        }
        impl Garbage for MPoolGcReq {
            #[inline]
            unsafe fn try_collect(&mut self) -> bool {
                if dpdk_sys::rte_mempool_full(self.ptr.as_ptr()) > 0 {
                    dpdk_sys::rte_mempool_free(self.ptr.as_ptr());
                    true
                } else {
                    false
                }
            }
        }
        let mut ret = MPoolGcReq { ptr: self.ptr };
        if !unsafe { ret.try_collect() } {
            // Case: with dangling mbufs
            // Note: deferred free via Eal
            self.eal.shared.lock().unwrap().garbages.push(Box::new(ret));
        }
    }
}

impl<MPoolPriv: Zeroable> MPool<MPoolPriv> {
    /// Allocate a `Packet` from the pool.
    ///
    /// # Safety
    ///
    /// Returned item must not outlive this pool.
    #[inline]
    pub unsafe fn alloc(&self) -> Option<Packet<MPoolPriv>> {
        // Safety: foreign function.
        // `alloc` is temporarily unsafe. Leaving this unsafe block.
        let pkt_ptr = unsafe { dpdk_sys::rte_pktmbuf_alloc(self.inner.ptr.as_ptr()) };

        Some(Packet {
            ptr: NonNull::new(pkt_ptr)?,
            _phantom: PhantomData {},
        })
    }

    /// Allocate packets and fill them in the remaining capacity of the given `ArrayVec`.
    ///
    /// # Safety
    ///
    /// Returned items must not outlive this pool.
    #[inline]
    pub unsafe fn alloc_bulk<A: Array<Item = Packet<MPoolPriv>>>(
        &self,
        buffer: &mut ArrayVec<A>,
    ) -> bool {
        let current_offset = buffer.len();
        let capacity = buffer.capacity();
        let remaining = capacity - current_offset;
        // Safety: foreign function.
        // Safety: manual arrayvec manipulation.
        // `alloc_bulk` is temporarily unsafe. Leaving this unsafe block.
        unsafe {
            let pkt_buffer = buffer.as_mut_ptr() as *mut *mut dpdk_sys::rte_mbuf;
            let ret = dpdk_sys::rte_pktmbuf_alloc_bulk(
                self.inner.ptr.as_ptr(),
                pkt_buffer.add(current_offset),
                remaining as u32,
            );

            if ret == 0 {
                buffer.set_len(capacity);
                return true;
            }
        }
        false
    }
}

/// An owned reference to `Packet`.
/// TODO Verify that `*mut Packet` can be transformed to `*mut *mut rte_mbuf`.
#[derive(Debug)]
pub struct Packet<MPoolPriv: Zeroable> {
    ptr: NonNull<dpdk_sys::rte_mbuf>,
    _phantom: PhantomData<MPoolPriv>,
}

unsafe impl<MPoolPriv: Zeroable> Send for Packet<MPoolPriv> {}
unsafe impl<MPoolPriv: Zeroable> Sync for Packet<MPoolPriv> {}

impl<MPoolPriv: Zeroable> Packet<MPoolPriv> {
    /// Returns whether `data_len` is zero.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Read data_len field
    #[inline]
    pub fn len(&self) -> usize {
        unsafe { self.ptr.as_ref().data_len }.into()
    }

    /// Read buf_len field
    #[inline]
    pub fn capacity(&self) -> usize {
        unsafe { self.ptr.as_ref().buf_len }.into()
    }

    /// Read priv_data field
    /// TODO we will save non-public, FPS-specific metadata to `MPoolPriv`.
    #[inline]
    pub fn priv_data(&self) -> &MPoolPriv {
        // Safety: All MPool instances have reserved private data for `MPoolPriv`.
        unsafe { &*(dpdk_sys::rte_mbuf_to_priv(self.ptr.as_ptr()) as *const MPoolPriv) }
    }

    /// Read/Write priv_data field
    /// TODO we will save non-public, FPS-specific metadata to `MPoolPriv`.
    #[inline]
    pub fn priv_data_mut(&mut self) -> &mut MPoolPriv {
        // Safety: All MPool instances have reserved private data for `MPoolPriv`.
        unsafe { &mut *(dpdk_sys::rte_mbuf_to_priv(self.ptr.as_ptr()) as *mut MPoolPriv) }
    }

    /// Retrieve read-only slice of packet buffer (regardless of `data_offset`).
    /// TODO: use `rte_pktmbuf_read` later?
    #[inline]
    pub fn buffer(&self) -> &[u8] {
        unsafe {
            let mbuf_ptr = self.ptr.as_ptr();
            slice::from_raw_parts(
                (*mbuf_ptr)
                    .buf_addr
                    .add((*mbuf_ptr).data_off.try_into().unwrap()) as *const u8,
                (*mbuf_ptr).buf_len.into(),
            )
        }
    }

    /// Retrieve writable slice of packet buffer (regardless of `data_offset`).
    #[inline]
    pub fn buffer_mut(&mut self) -> &mut [u8] {
        unsafe {
            let mbuf_ptr = self.ptr.as_ptr();
            slice::from_raw_parts_mut(
                (*mbuf_ptr)
                    .buf_addr
                    .add((*mbuf_ptr).data_off.try_into().unwrap()) as *mut u8,
                (*mbuf_ptr).buf_len.into(),
            )
        }
    }

    /// Change the packet length
    /// TODO: Do we need this? Shall we replace it with prepend/append?
    #[inline]
    pub fn set_len(&mut self, size: usize) {
        // Safety: buffer boundary is guarded by the assert statement.
        unsafe {
            let mbuf_ptr = self.ptr.as_ptr();
            assert!((*mbuf_ptr).buf_len >= size as u16);
            (*mbuf_ptr).data_len = size as u16;
            (*mbuf_ptr).pkt_len = size as u32;
        }
    }

    /// Retrieve read-only slice of packet's data buffer.
    /// TODO: use `rte_pktmbuf_read` instead?
    #[inline]
    pub fn data(&self) -> &[u8] {
        &self.buffer()[0..self.len()]
    }

    /// Retrieve writable slice of packet's data buffer.
    #[inline]
    pub fn data_mut(&mut self) -> &mut [u8] {
        let len = self.len();
        &mut self.buffer_mut()[0..len]
    }

    /// Skip first n bytes of this packet.
    /// Panic: when size is out of bound.
    #[inline]
    pub fn trim_head(&mut self, size: usize) {
        // Safety: foreign function.
        unsafe {
            let ret = dpdk_sys::rte_pktmbuf_adj(self.ptr.as_ptr(), size as u16);
            assert_ne!(ret, ptr::null_mut());
        }
    }

    /// Skip last n bytes of this packet.
    /// Panic: when size is out of bound.
    #[inline]
    pub fn trim_tail(&mut self, size: usize) {
        // Safety: foreign function.
        unsafe {
            let ret = dpdk_sys::rte_pktmbuf_trim(self.ptr.as_ptr(), size as u16);
            assert_eq!(ret, 0);
        }
    }

    /// Reset headroom.
    /// Note: tail can be reset by setting `data_len` to its buffer capacity.
    #[inline]
    pub fn reset_headroom(&mut self) {
        // Safety: foreign function.
        unsafe {
            dpdk_sys::rte_pktmbuf_reset_headroom(self.ptr.as_ptr());
        }
    }

    /// Prepend packet's data buffer to left.
    /// Panic: when size is out of bound.
    #[inline]
    pub fn prepend(&mut self, size: usize) {
        // Safety: foreign function.
        unsafe {
            let ret = dpdk_sys::rte_pktmbuf_prepend(self.ptr.as_ptr(), size as u16);
            assert_ne!(ret, ptr::null_mut());
        }
    }

    /// Prepend packet's data buffer to right.
    /// Panic: when size is out of bound.
    #[inline]
    pub fn append(&mut self, size: usize) {
        // Safety: foreign function.
        unsafe {
            let ret = dpdk_sys::rte_pktmbuf_append(self.ptr.as_ptr(), size as u16);
            assert_ne!(ret, ptr::null_mut());
        }
    }
}

impl<MPoolPriv: Zeroable> Drop for Packet<MPoolPriv> {
    #[inline]
    fn drop(&mut self) {
        // Safety: foreign function.
        unsafe {
            dpdk_sys::rte_pktmbuf_free(self.ptr.as_ptr());
        }
    }
}

/// Abstract type for DPDK RxQ
///
/// TODO Support per-queue RX operations
#[derive(Debug, Clone)]
pub struct RxQ<MPoolPriv: Zeroable> {
    inner: Arc<RxQInner<MPoolPriv>>,
}

/// Note: RxQ requires a dedicated mempool to receive incoming packets.
#[derive(Debug)]
struct RxQInner<MPoolPriv: Zeroable> {
    queue_id: u16,
    port: Port,
    mpool: Arc<MPoolInner<MPoolPriv>>,
}

impl<MPoolPriv: Zeroable> Drop for RxQInner<MPoolPriv> {
    #[inline]
    fn drop(&mut self) {
        // Safety: foreign function.
        //
        // Note: dynamically starting/stopping queue may not be supported by the driver.
        let ret =
            unsafe { dpdk_sys::rte_eth_dev_rx_queue_stop(self.port.inner.port_id, self.queue_id) };
        if ret != 0 {
            warn!(
                "RxQInner::drop, non-severe error code({}) while stopping queue {}:{}",
                ret, self.port.inner.port_id, self.queue_id
            );
        }
    }
}

impl<MPoolPriv: Zeroable> RxQ<MPoolPriv> {
    /// Returns current queue index.
    #[inline]
    pub fn queue_id(&self) -> u16 {
        self.inner.queue_id
    }

    /// Receive packets and store it to the given arrayvec.
    #[inline]
    pub fn rx<A: Array<Item = Packet<MPoolPriv>>>(&self, buffer: &mut ArrayVec<A>) {
        let current = buffer.len();
        let remaining = buffer.capacity() - current;
        unsafe {
            let pkt_buffer = buffer.as_mut_ptr() as *mut *mut dpdk_sys::rte_mbuf;
            let cnt = dpdk_sys::rte_eth_rx_burst(
                self.inner.port.inner.port_id,
                self.inner.queue_id,
                pkt_buffer.add(current),
                remaining as u16,
            );
            buffer.set_len(current + cnt as usize);
        }
    }

    /// Get port of this queue.
    #[inline]
    pub fn port(&self) -> &Port {
        &self.inner.port
    }
}

/// Abstract type for DPDK TxQ
#[derive(Debug, Clone)]
pub struct TxQ {
    inner: Arc<TxQInner>,
}

/// Note: while RxQ requires a dedicated mempool, Tx operation takes `MBuf`s which are allocated by
/// other RxQ's mempool or other externally allocated mempools. Thus, TxQ itself does not require
/// its own mempool.
#[derive(Debug)]
struct TxQInner {
    queue_id: u16,
    port: Port,
}

impl Drop for TxQInner {
    #[inline]
    fn drop(&mut self) {
        // Safety: foreign function.
        //
        // Note: dynamically starting/stopping queue may not be supported by the driver.
        let ret =
            unsafe { dpdk_sys::rte_eth_dev_tx_queue_stop(self.port.inner.port_id, self.queue_id) };
        if ret != 0 {
            warn!(
                "TxQInner::drop, non-severe error code({}) while stopping queue {}:{}",
                ret, self.port.inner.port_id, self.queue_id
            );
        }
    }
}

impl TxQ {
    /// Returns current queue index.
    #[inline]
    pub fn queue_id(&self) -> u16 {
        self.inner.queue_id
    }

    /// Try transmit packets in the given arrayvec buffer.
    /// All packets in the buffer will be sent.
    #[inline]
    pub fn tx<MPoolPriv: Zeroable, A: Array<Item = Packet<MPoolPriv>>>(
        &self,
        buffer: &mut ArrayVec<A>,
    ) {
        let current = buffer.len();
        // Safety: this block is very dangerous.

        // Get raw pointer of arrayvec
        let pkt_buffer = buffer.as_mut_ptr() as *mut *mut dpdk_sys::rte_mbuf;

        // Try transmit packets. It will return number of successfully transmitted packets.
        // Successfully transmitted packets are automatically dropped by `rte_eth_tx_burst`.
        // Safety: foreign function.
        // Safety: `pkt_buffer` is safe to read till `pkt_buffer[current]`.
        let cnt = unsafe {
            dpdk_sys::rte_eth_tx_burst(
                self.inner.port.inner.port_id,
                self.inner.queue_id,
                pkt_buffer,
                current as u16,
            ) as usize
        };

        // Remaining packets are moved to the beginning of the vector.
        let remaining = current - cnt;
        // Safety: pkt_buffer[cur...len] are unsent thus safe to be accessed.
        // This line moves pkts at tail to the head of the array.
        unsafe { ptr::copy(pkt_buffer.add(cnt), pkt_buffer, remaining) };

        // Safety: headers are filled with unsent packets and it is safe to set the length.
        unsafe { buffer.set_len(remaining) };
    }

    /// Make copies of MBufs and transmit them.
    /// All packets in the buffer will be sent or be abandoned.
    #[inline]
    pub fn tx_cloned<MPoolPriv: Zeroable, A: Array<Item = Packet<MPoolPriv>>>(
        &self,
        buffer: &ArrayVec<A>,
    ) {
        let current = buffer.len();

        for pkt in buffer {
            // Safety: foreign function.
            // Note: It does not cause memory leak as tx_burst decreases the reference count.
            unsafe { dpdk_sys::rte_pktmbuf_refcnt_update(pkt.ptr.as_ptr(), 1) };
        }

        // Get raw pointer of arrayvec
        let pkt_buffer = buffer.as_ptr() as *mut *mut dpdk_sys::rte_mbuf;

        // Try transmit packets. It will return number of successfully transmitted packets.
        // Successfully transmitted packets are automatically dropped by `rte_eth_tx_burst`.
        //
        // Safety: foreign function.
        // Safety: `pkt_buffer` is safe to read till `pkt_buffer[current]`.
        let cnt = unsafe {
            dpdk_sys::rte_eth_tx_burst(
                self.inner.port.inner.port_id,
                self.inner.queue_id,
                pkt_buffer,
                current as u16,
            )
        };

        // We have to manually free unsent packets, or some packets will leak.
        for i in cnt as usize..current {
            // Safety: foreign function.
            // Safety: pkt's refcount is already increased thus there is no use-after-free.
            unsafe { dpdk_sys::rte_pktmbuf_free(*(pkt_buffer.add(i))) };
        }
        // As all mbuf's references are already increases, we do not have to free the arrayvec.
    }

    /// Get port of this queue.
    #[inline]
    pub fn port(&self) -> &Port {
        &self.inner.port
    }
}

impl Eal {
    /// Create an `Eal` instance.
    ///
    /// It takes command-line arguments and consumes used arguments.
    #[inline]
    pub fn new(args: &mut Vec<String>) -> Result<Self, ErrorCode> {
        Ok(Eal {
            inner: Arc::new(EalInner::new(args)?),
        })
    }

    /// Create a new `MPool`.
    ///
    /// # Panic
    /// Pool name must be globally unique, otherwise it will panic.
    ///
    /// @param n The number of elements in the mbuf pool.
    ///
    /// @param cache_size Size of the per-core object cache.
    ///
    /// @param data_room_size Size of data buffer in each mbuf, including RTE_PKTMBUF_HEADROOM.
    ///
    /// @param socket_id The socket identifier where the memory should be allocated. The value can
    /// be `None` (corresponds to DPDK's *SOCKET_ID_ANY*) if there is no NUMA constraint for the
    /// reserved zone.
    #[inline]
    pub fn create_mpool<S: AsRef<str>, MPoolPriv: Zeroable>(
        &self,
        name: S,
        n: usize,
        cache_size: usize,
        data_room_size: usize,
        socket_id: Option<SocketId>,
    ) -> MPool<MPoolPriv> {
        let pool_name = CString::new(name.as_ref()).unwrap();

        // Safety: foreign function.
        let ptr = unsafe {
            dpdk_sys::rte_pktmbuf_pool_create(
                pool_name.into_bytes_with_nul().as_ptr() as *mut i8,
                n.try_into().unwrap(),
                cache_size as u32,
                (((size_of::<MPoolPriv>() + 7) / 8) * 8) as u16,
                data_room_size.try_into().unwrap(),
                socket_id
                    .map(|x| x.0 as i32)
                    .unwrap_or(dpdk_sys::SOCKET_ID_ANY),
            )
        };

        let inner = Arc::new(MPoolInner {
            ptr: NonNull::new(ptr).unwrap(), // will panic if the given name is not unique.
            eal: self.inner.clone(),
            _phantom: PhantomData {},
        });

        // The pointer to the new allocated mempool, on success. NULL on error with rte_errno set appropriately.
        // https://doc.dpdk.org/api/rte__mbuf_8h.html
        MPool { inner }
    }

    /// Setup per-core Rx queues and Tx queues according to the given affinity.  Currently, this
    /// must be called once for the whole program. Otherwise it will return an error code.  Returns
    /// array of `(logical core id, assigned rx queues, assigned tx queues)` on success.
    ///
    /// Note: rte_lcore_count: -c ff 옵션에 따라 줄어듬.
    /// Note: we have clippy warning: complex return type.
    /// Note: we have clippy warning: cognitive complexity.
    #[inline]
    pub fn setup<MPoolPriv: Zeroable>(
        &self,
        rx_affinity: Affinity,
        tx_affinity: Affinity,
    ) -> Result<Vec<(LCoreId, Vec<RxQ<MPoolPriv>>, Vec<TxQ>)>, ErrorCode> {
        // Acquire globally shared state and check whether already initialized.
        let mut shared_mut = self.inner.shared.lock().unwrap();
        if shared_mut.setup_initialized {
            // Already initialized.
            return Err(dpdk_sys::EALREADY.try_into().unwrap());
        }

        // List of valid logical core ids.
        // Note: If some cores are masked, range (0..rte_lcore_count()) will include disabled cores.
        let lcore_id_list = (0..dpdk_sys::RTE_MAX_LCORE)
            .filter(|index| unsafe { dpdk_sys::rte_lcore_is_enabled(*index) > 0 })
            .collect::<Vec<_>>();

        // Map of `socket_id` to set of `lcore_id`s belong to the socket.
        let mut socket_to_lcore_map = HashMap::new();
        for lcore_id in &lcore_id_list {
            let lcore_id = *lcore_id;
            // Safety: foreign function.
            let socket_id = unsafe { dpdk_sys::rte_lcore_to_socket_id(lcore_id) };
            // Safety: foreign function.
            let cpu_id = unsafe { dpdk_sys::rte_lcore_to_cpu_id(lcore_id.try_into().unwrap()) };
            debug!(
                "Logical core {} is enabled at physical core {} (NUMA node {})",
                lcore_id, cpu_id, socket_id
            );

            // Classify `lcore_id`s according to their socket IDs.
            socket_to_lcore_map
                .entry(SocketId::new(socket_id))
                .or_insert_with(HashSet::new)
                .insert(LCoreId::new(lcore_id));
        }
        debug!("lcore count: {}", socket_to_lcore_map.len());

        // Generate list of `Port`s from selected `port_id`s.
        let port_list = (0..u16::try_from(dpdk_sys::RTE_MAX_ETHPORTS).unwrap())
            .filter(|index| {
                // Safety: foreign function.
                unsafe { dpdk_sys::rte_eth_dev_is_valid_port(*index) > 0 }
            })
            .map(|port_id| {
                let mut owner_id = 0;
                // Safety: foreign function.
                let ret = unsafe { dpdk_sys::rte_eth_dev_owner_new(&mut owner_id) };
                assert_eq!(ret, 0);

                let mut owner = dpdk_sys::rte_eth_dev_owner {
                    id: owner_id,
                    // Safety: `c_char` array can accept zeroed data.
                    name: unsafe { MaybeUninit::zeroed().assume_init() },
                };
                let owner_name = format!("rust_dpdk_port_owner_{}", port_id);
                let name_cstring = CString::new(owner_name).unwrap();
                let name_bytes = name_cstring.as_bytes_with_nul();
                // Safety: converting &[u8] string into &[i8] string.
                owner.name[0..name_bytes.len()]
                    .copy_from_slice(unsafe { &*(name_bytes as *const [u8] as *const [i8]) });
                // Safety: foreign function.
                let ret = unsafe { dpdk_sys::rte_eth_dev_owner_set(port_id, &owner) };
                assert_eq!(ret, 0);

                Port {
                    inner: Arc::new(PortInner {
                        port_id,
                        owner_id,
                        has_stats_reset: true,
                        // Safety: PortStat allows zeroed structure.
                        prev_stat: Mutex::new(unsafe { MaybeUninit::zeroed().assume_init() }),
                        eal: self.inner.clone(),
                    }),
                }
            })
            .collect::<Vec<_>>();

        // Map from `lcore_id` to its assigned `(rxq, txq)`.
        let mut lcore_to_rxqtxq_map = HashMap::new();
        for mut port in port_list {
            let port_id = port.inner.port_id;
            let socket_id = port.socket_id();
            // Extract RX cores and TX cores according to the given affinity information.
            // For `Full` affinity, all cores are assigned to all cores.
            // For `Numa` affinity, only cores on the same NUMA node are assigned to each core.
            let rx_lcores = match rx_affinity {
                Affinity::Full => socket_to_lcore_map.values().flatten().cloned().collect(),
                Affinity::Numa => socket_to_lcore_map.get(&socket_id).unwrap().clone(),
            };
            let tx_lcores = match tx_affinity {
                Affinity::Full => socket_to_lcore_map.values().flatten().cloned().collect(),
                Affinity::Numa => socket_to_lcore_map.get(&socket_id).unwrap().clone(),
            };

            // Extract each port's HW spec.
            // Safety: `rte_eth_dev_info` contains primitive integer types. Safe to fill with zeros.
            let mut dev_info: dpdk_sys::rte_eth_dev_info = unsafe { std::mem::zeroed() };
            // Safety: foreign function.
            unsafe { dpdk_sys::rte_eth_dev_info_get(port_id, &mut dev_info) };
            let rx_queue_limit = dev_info.max_rx_queues;
            let tx_queue_limit = dev_info.max_tx_queues;
            let rx_queue_count: u16 = rx_lcores.len().try_into().unwrap();
            let tx_queue_count: u16 = tx_lcores.len().try_into().unwrap();

            // Validate configuration numbers.
            assert!(rx_queue_count <= rx_queue_limit);
            assert!(tx_queue_count <= tx_queue_limit);
            assert!(DEFAULT_RX_DESC <= dev_info.rx_desc_lim.nb_max);
            assert!(DEFAULT_RX_DESC >= dev_info.rx_desc_lim.nb_min);
            assert!(DEFAULT_RX_DESC % dev_info.rx_desc_lim.nb_align == 0);
            assert!(DEFAULT_TX_DESC <= dev_info.tx_desc_lim.nb_max);
            assert!(DEFAULT_TX_DESC >= dev_info.tx_desc_lim.nb_min);
            assert!(DEFAULT_TX_DESC % dev_info.tx_desc_lim.nb_align == 0);

            // Prepart HW configuration with offload flags.
            // Safety: `rte_eth_conf` contains primitive integer types. Safe to fill with zeros.
            let mut port_conf: dpdk_sys::rte_eth_conf = unsafe { std::mem::zeroed() };
            port_conf.rxmode.max_rx_pkt_len = dpdk_sys::RTE_ETHER_MAX_LEN;
            port_conf.rxmode.mq_mode = dpdk_sys::rte_eth_rx_mq_mode_ETH_MQ_RX_NONE;
            port_conf.txmode.mq_mode = dpdk_sys::rte_eth_tx_mq_mode_ETH_MQ_TX_NONE;
            if rx_queue_count > 1 {
                // Enable RSS.
                port_conf.rxmode.mq_mode = dpdk_sys::rte_eth_rx_mq_mode_ETH_MQ_RX_RSS;
                port_conf.rx_adv_conf.rss_conf.rss_hf = (dpdk_sys::ETH_RSS_NONFRAG_IPV4_UDP
                    | dpdk_sys::ETH_RSS_NONFRAG_IPV4_TCP)
                    .into();
                // TODO set symmetric RSS for TCP/IP
            }
            // Enable other offload flags
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

            // Configure each port with number of RX/TX queues and offload flags.
            // Safety: foreign function.
            let ret = unsafe {
                dpdk_sys::rte_eth_dev_configure(port_id, rx_queue_count, tx_queue_count, &port_conf)
            };
            assert_eq!(ret, 0);

            // For each rx core, configure RxQ.
            for (rxq_idx, rx_lcore) in rx_lcores.into_iter().enumerate() {
                // Create a `MPool` dedicated for for each RxQ.
                let mpool = self.create_mpool(
                    format!("rxq_{}_{}_{}", MAGIC, port_id, rxq_idx),
                    DEFAULT_RX_POOL_SIZE,
                    DEFAULT_RX_PER_CORE_CACHE,
                    DEFAULT_PACKET_DATA_LENGTH,
                    Some(port.socket_id()),
                );
                // Safety: foreign function.
                let ret = unsafe {
                    dpdk_sys::rte_eth_rx_queue_setup(
                        port_id,
                        rxq_idx as u16,
                        DEFAULT_RX_DESC,
                        port.socket_id().into(),
                        &dev_info.default_rxconf,
                        mpool.inner.ptr.as_ptr(),
                    )
                };
                assert_eq!(ret, 0);
                let rxq = RxQ {
                    inner: Arc::new(RxQInner {
                        queue_id: rxq_idx as u16,
                        port: port.clone(),
                        mpool: mpool.inner,
                    }),
                };
                // Insert created RxQ to the `lcore, (rxqs, txqs)` map.
                lcore_to_rxqtxq_map
                    .entry(rx_lcore)
                    .or_insert_with(|| (Vec::new(), Vec::new()))
                    .0
                    .push(rxq);
            }

            // For each rx core, configure TxQ.
            // Note: TxQ sends packets which is already allocated by other `MPool`s.
            // Thus, we do not require dedicated `MPool`s for TxQs.
            for (txq_idx, tx_lcore) in tx_lcores.into_iter().enumerate() {
                // Safety: foreign function.
                let ret = unsafe {
                    dpdk_sys::rte_eth_tx_queue_setup(
                        port_id,
                        txq_idx as u16,
                        DEFAULT_RX_DESC,
                        port.socket_id().into(),
                        &dev_info.default_txconf,
                    )
                };
                assert_eq!(ret, 0);
                let txq = TxQ {
                    inner: Arc::new(TxQInner {
                        queue_id: txq_idx as u16,
                        port: port.clone(),
                    }),
                };
                // Insert created TxQ to the `lcore, (rxqs, txqs)` map.
                lcore_to_rxqtxq_map
                    .entry(tx_lcore)
                    .or_insert_with(|| (Vec::new(), Vec::new()))
                    .1
                    .push(txq);
            }

            // Set promisc.
            // Safety: foreign function.
            unsafe {
                if DEFAULT_PROMISC {
                    dpdk_sys::rte_eth_promiscuous_enable(port_id);
                } else {
                    dpdk_sys::rte_eth_promiscuous_disable(port_id);
                }
            };

            // Start the configured port.
            // Safety: foreign function.
            let ret = unsafe { dpdk_sys::rte_eth_dev_start(port_id) };
            assert_eq!(ret, 0);

            // Check whether stats_reset is supported.
            // Safety: foreign function. Uninitialized data structure will be filled.
            let ret = unsafe { dpdk_sys::rte_eth_stats_reset(port_id) };
            if ret == -(dpdk_sys::ENOTSUP as i32) {
                warn!("stats_reset is not supported. Fallback to software emulation.");
                Arc::get_mut(&mut port.inner).unwrap().has_stats_reset = false;
            }

            info!("Port {} initialized", port_id);
        }

        // Initialization finished
        shared_mut.setup_initialized = true;

        // Return array of `(LCore, Vec<RxQ>, Vec<TxQ>)`.
        Ok(lcore_to_rxqtxq_map
            .into_iter()
            .map(|(lcore_id, (rxqs, txqs))| (lcore_id, rxqs, txqs))
            .collect())
    }
}

pub use dpdk_sys::EalStaticFunctions as EalGlobalApi;

unsafe impl EalGlobalApi for Eal {}

impl EalInner {
    // Create `EalInner`.
    #[inline]
    fn new(args: &mut Vec<String>) -> Result<Self, ErrorCode> {
        // To prevent DPDK PMDs' being unlinked, we explicitly create symbolic dependency via
        // calling `load_drivers`.
        dpdk_sys::load_drivers();

        // DPDK returns number of consumed argc
        // Safety: foriegn function (safe unless there is a bug)
        let ret = unsafe { ffi::run_with_args(dpdk_sys::rte_eal_init, &*args) };
        if ret < 0 {
            return Err(ret.try_into().unwrap());
        }

        // Strip first n args and return the remaining
        args.drain(..ret as usize);
        Ok(EalInner {
            shared: Mutex::new(Default::default()),
        })
    }
}

impl Drop for EalInner {
    #[inline]
    fn drop(&mut self) {
        // Safety: foriegn function (safe unless there is a bug)
        unsafe {
            for mut gc_req in self.shared.get_mut().unwrap().garbages.drain(..) {
                let ret = gc_req.try_collect();
                assert_eq!(ret, true);
            }

            let ret = dpdk_sys::rte_eal_cleanup();
            if ret == -(dpdk_sys::ENOTSUP as i32) {
                warn!("EAL cleanup is not implemented.");
                return;
            }
            assert_eq!(ret, 0);
            info!("EAL cleaned up");
        }
    }
}

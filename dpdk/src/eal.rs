//! Wrapper for DPDK's environment abstraction layer (EAL).

use std::sync::{Arc, RwLock};

#[derive(Debug)]
struct EalSharedInner {}

#[derive(Debug)]
struct EalInner {
    shared: RwLock<EalSharedInner>,
}

#[derive(Debug, Clone)]
pub struct Eal {
    inner: Arc<EalInner>,
}

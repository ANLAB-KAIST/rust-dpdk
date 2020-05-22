//! Wrapper for DPDK's environment abstraction layer (EAL).

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

impl Eal {
    /// Create an `Eal` instance.
    ///
    /// DPDK does not recommand users to repeat initialize and clear.
    pub fn new(args: Vec<String>) -> Result<Self, ()> {
        Err(())
    }
}

extern crate dpdk;
extern crate log;
extern crate simple_logger;

use dpdk::eal::*;
use log::debug;
use std::env;

fn main() {
    simple_logger::init().unwrap();
    let mut args: Vec<String> = env::args().collect();
    let eal = Eal::new(&mut args).unwrap();
    debug!("TSC Hz: {}", eal.get_tsc_hz());
    debug!("TSC Cycles: {}", eal.get_tsc_cycles());
    debug!("Random: {}", eal.rand());
    eal.setup(Affinity::Full, Affinity::Full).ok();
}

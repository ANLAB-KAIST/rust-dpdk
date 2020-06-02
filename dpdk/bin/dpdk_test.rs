extern crate dpdk;
extern crate simple_logger;

use dpdk::eal::*;
use std::env;

fn main() {
    simple_logger::init().unwrap();
    let mut args: Vec<String> = env::args().collect();
    let eal = Eal::new(&mut args).unwrap();
    println!("TSC Hz: {}", eal.get_tsc_hz());
    println!("TSC Cycles: {}", eal.get_tsc_cycles());
    println!("Random: {}", eal.rand());
    eal.setup(Affinity::Full, Affinity::Full, 512, 512, 4096, 64);
}

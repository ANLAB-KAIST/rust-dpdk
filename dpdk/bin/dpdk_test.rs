extern crate dpdk;

use dpdk::eal;
use dpdk::prelude::*;
use std::env;

fn main() {
    let mut args: Vec<String> = env::args().collect();
    let eal = eal::Eal::new(&mut args).unwrap();
    println!("TSC Hz: {}", eal.rte_get_tsc_hz());
    println!("TSC Cycles: {}", eal.rte_get_tsc_cycles());
    println!("Random: {}", eal.rte_rand());
    drop(eal);
}

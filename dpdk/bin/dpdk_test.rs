extern crate dpdk;

use dpdk::eal::*;
use std::env;

fn main() {
    let mut args: Vec<String> = env::args().collect();
    let eal = Eal::new(&mut args).unwrap();
    println!("TSC Hz: {}", eal.get_tsc_hz());
    println!("TSC Cycles: {}", eal.get_tsc_cycles());
    println!("Random: {}", eal.rand());
    drop(eal);
}

extern crate dpdk;

use dpdk::eal;
use std::env;

fn main() {
    let mut args: Vec<String> = env::args().collect();
    let eal = eal::Eal::new(&mut args).unwrap();
    drop(eal);
}

extern crate anyhow;
extern crate arrayvec;
extern crate dpdk;
extern crate log;
extern crate simple_logger;

use anyhow::{anyhow, Result};
use arrayvec::*;
use dpdk::eal::*;
use log::{debug, info};
use std::env;

/// Private metadata structure for this test case.
///
/// Note: we need to use `is_xx_set` because we cannot safely use `Option<T>` with `zeroed()`.
#[derive(Debug, Clone, Copy)]
struct TestPriv {
    is_from_set: bool,
    from_port: u16,
    from_queue: u16,
    is_to_set: bool,
    to_port: u16,
    to_queue: u16,
}
unsafe impl Zeroable for TestPriv {}

fn sender(eal: Eal, mpool: MPool<TestPriv>, tx_queue: TxQ) {
    let tx_port = tx_queue.port();
    info!("Start TX from {:?}", tx_port.mac_addr());

    // Wait for the link to be connected.
    while !tx_port.is_link_up() {
        eal.pause();
    }
    info!("TX Link is up {:?}", tx_port.mac_addr());

    let mut pkts = ArrayVec::<[Packet<TestPriv>; DEFAULT_TX_BURST]>::new();
    // Safety: packet is created and transmitted before `mpool` is destroyed.
    unsafe { mpool.alloc_bulk(&mut pkts) };
    pkts.iter_mut().for_each(|pkt| {
        pkt.priv_data_mut().to_port = tx_port.port_id();
        pkt.priv_data_mut().to_queue = tx_queue.queue_id();
        pkt.priv_data_mut().is_to_set = true;
    });

    for pkt in &mut pkts {
        // Prepare toy arp request packets
        let pkt_buf = pkt.buffer_mut();

        // Fill Ethernet
        // TODO Use https://doc.rust-lang.org/beta/std/primitive.slice.html#method.fill
        pkt_buf[0..6].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]); // Dst MAC
        pkt_buf[6..12].copy_from_slice(&tx_port.mac_addr()); // Src MAC
        pkt_buf[12..14].copy_from_slice(&[0x08, 0x06]); // Ethertype: ARP

        // Fill ARP
        pkt_buf[14..16].copy_from_slice(&[0x00, 0x01]); // HTYPE: Ethernet
        pkt_buf[16..18].copy_from_slice(&[0x08, 0x00]); // PTYPE: IP
        pkt_buf[18..20].copy_from_slice(&[0x06, 0x04]); // HLEN: 6, PLEN: 4
        pkt_buf[20..22].copy_from_slice(&[0x00, 0x01]); // OPER: request (1)
        pkt_buf[22..28].copy_from_slice(&tx_port.mac_addr()); // SHA (6byte)
        pkt_buf[28..32].copy_from_slice(&[0x10, 0x00, 0x00, 0x02]); // SPA (10.0.0.2)
        pkt_buf[32..38].copy_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]); // THA (6byte)
        pkt_buf[38..42].copy_from_slice(&[0x10, 0x00, 0x00, 0x03]); // THA (10.0.0.3)

        pkt.set_len(42);

        assert_eq!(pkt.priv_data().is_to_set, true);
        assert_eq!(pkt.priv_data().to_port, tx_port.port_id());
        assert_eq!(pkt.priv_data().to_queue, tx_queue.queue_id());
    }

    // Send packet
    tx_queue.tx(&mut pkts);

    // Wait for pkts to be transmitted
    while tx_port.get_stat().opackets as usize > DEFAULT_TX_BURST {
        eal.pause();
    }

    info!("TX finished. {:?}", tx_port.get_stat());

    // Safety: mpool must not be deallocated before TxQ is destroyed.
}

fn receiver(eal: Eal, rx_queue: RxQ<TestPriv>) {
    let rx_port = rx_queue.port();
    info!("RX started at {:?}", rx_port.mac_addr());

    // Wait for the link to be connected.
    while !rx_port.is_link_up() {
        eal.pause();
    }
    info!("RX Link is up {:?}", rx_port.mac_addr());

    // We will try to collect every TX packets.
    // We will collect all sent packets and additional background packets.
    // Thus we need 2 * TX_BURST to collect everything.
    let mut pkts = ArrayVec::<[Packet<TestPriv>; DEFAULT_TX_BURST * 2]>::new();
    loop {
        rx_queue.rx(&mut pkts);
        if pkts.len() >= DEFAULT_TX_BURST {
            break;
        }
        eal.pause();
    }
    info!("RX finished. {:?}", rx_port.get_stat());
}

/// Note: this test function only works with `sudo target/debug/dpdk_test -c 1`
fn main() -> Result<()> {
    simple_logger::init().unwrap();
    let mut args: Vec<String> = env::args().collect();
    let eal = Eal::new(&mut args).unwrap();
    debug!("TSC Hz: {}", eal.get_tsc_hz());
    debug!("TSC Cycles: {}", eal.get_tsc_cycles());
    debug!("Random: {}", eal.rand());

    // Create a `MPool` to create packets.
    let default_mpool = eal.create_mpool(
        "default_tx_pool",
        DEFAULT_RX_POOL_SIZE,
        DEFAULT_RX_PER_CORE_CACHE,
        DEFAULT_PACKET_DATA_LENGTH,
        None,
    );

    crossbeam::thread::scope(|s| {
        let threads = eal
            .setup(Affinity::Full, Affinity::Full)?
            .into_iter()
            .map(|(lcore, rxs, txs)| {
                let local_eal = eal.clone();
                let local_mpool = default_mpool.clone();
                lcore.launch(s, move || {
                    match lcore.into() {
                        // Core 0 action: TX packets to txq[0]
                        0 => {
                            sender(local_eal.clone(), local_mpool, txs[0].clone());
                            receiver(local_eal, rxs[1].clone());
                            true
                        }
                        // Otherwise, do nothing
                        _ => true,
                    }
                })
            })
            .collect::<Vec<_>>();
        let ret = threads.into_iter().map(|x| x.join().unwrap()).all(|x| x);
        assert_eq!(ret, true);
        Ok(())
    })
    .map_err(|err| anyhow!("{:?}", err))?
}

extern crate anyhow;
extern crate arrayvec;
extern crate dpdk;
extern crate log;
extern crate simple_logger;

use anyhow::Result;
use arrayvec::*;
use dpdk::eal::*;
use log::{debug, info};
use std::env;

fn sender(eal: Eal, tx_queue: TxQ) {
    eal.delay_us_sleep(2_000_000);
    let tx_port = tx_queue.port();
    info!("Start TX from {:?}", tx_port.mac_addr());

    // Create a `MPool` to create packets.
    let mpool = eal.create_mpool(
        format!("tx_pool_for_core_0"),
        DEFAULT_RX_POOL_SIZE,
        DEFAULT_RX_PER_CORE_CACHE,
        DEFAULT_PACKET_DATA_LENGTH,
        Some(tx_port.socket_id()),
    );

    let mut pkts = ArrayVec::<[Packet; DEFAULT_TX_BURST]>::new();
    // Safety: packet is created and transmitted before `mpool` is destroyed.
    unsafe { mpool.alloc_bulk(&mut pkts) };

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
    }
    // Send packet
    tx_queue.tx(&mut pkts);
    eal.delay_us_sleep(2_000_000);

    // Safety: mpool must not be deallocated before TxQ is destroyed.
}

fn receiver(eal: Eal, rx_queue: RxQ) {
    eal.delay_us_sleep(2_000_000);
    let rx_port = rx_queue.port();

    // We will try to collect every TX packets. Thus, we use TX_BURST.
    let mut pkts = ArrayVec::<[Packet; DEFAULT_TX_BURST]>::new();

    info!("RX started at {:?}", rx_port.mac_addr());
    while pkts.len() < DEFAULT_TX_BURST {
        rx_queue.rx(&mut pkts);
        eal.pause();
    }
    info!("RX finished. {:?}", rx_port.get_stat());
}

fn main() -> Result<()> {
    simple_logger::init().unwrap();
    let mut args: Vec<String> = env::args().collect();
    let eal = Eal::new(&mut args).unwrap();
    debug!("TSC Hz: {}", eal.get_tsc_hz());
    debug!("TSC Cycles: {}", eal.get_tsc_cycles());
    debug!("Random: {}", eal.rand());

    let threads = eal
        .setup(Affinity::Full, Affinity::Full)?
        .into_iter()
        .map(|(lcore, rxs, txs)| {
            match lcore.into() {
                // Core 0 action: TX packets to txq[0]
                0 => {
                    let local_eal = eal.clone();
                    let txq0 = txs[0].clone();
                    lcore.launch(|| {
                        sender(local_eal, txq0);
                        true
                    })
                }
                // Core 1 action: RX packets from rxq[1]
                1 => {
                    let local_eal = eal.clone();
                    let rxq1 = rxs[1].clone();
                    lcore.launch(|| {
                        receiver(local_eal, rxq1);
                        true
                    })
                }
                // Otherwise, do nothing
                _ => lcore.launch(|| true),
            }
        })
        .collect::<Vec<_>>();
    let ret = threads.into_iter().map(|x| x.join().unwrap()).all(|x| x);
    assert_eq!(ret, true);
    Ok(())
}

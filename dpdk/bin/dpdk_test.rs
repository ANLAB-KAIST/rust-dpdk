extern crate anyhow;
extern crate arrayvec;
extern crate dpdk;
extern crate log;
extern crate simple_logger;

use anyhow::Result;
use arrayvec::*;
use dpdk::eal::*;
use log::debug;
use std::env;

fn main() -> Result<()> {
    simple_logger::init().unwrap();
    let mut args: Vec<String> = env::args().collect();
    let eal = Eal::new(&mut args).unwrap();
    debug!("TSC Hz: {}", eal.get_tsc_hz());
    debug!("TSC Cycles: {}", eal.get_tsc_cycles());
    debug!("Random: {}", eal.rand());

    let threads =
        eal.setup(Affinity::Full, Affinity::Full)?
            .into_iter()
            .map(|(lcore, rxs, txs)| {
                match lcore.into() {
                    // Core 0 action: TX a packet to txq[1]
                    0 => {
                        let tx_queue = &txs[0];
                        let tx_port = tx_queue.port();

                        // Create a `MPool` to create packets.
                        let pool_name = format!("tx_pool_for_core_0");
                        let mpool = MPool::new(
                            &eal,
                            pool_name,
                            DEFAULT_RX_POOL_SIZE,
                            DEFAULT_RX_PER_CORE_CACHE,
                            DEFAULT_PACKET_DATA_LENGTH,
                            Some(tx_port.socket_id()),
                        );

                        // Safety: packet is created and transmitted before `mpool` is destroyed.
                        unsafe {
                            let mut pkts = ArrayVec::<[Packet; DEFAULT_TX_BURST]>::new();
                            mpool.alloc_bulk(&mut pkts);

                            for pkt in &mut pkts {
                                // Prepare toy arp request packets
                                let pkt_buf = pkt.data_mut();

                                // Fill Ethernet
                                // TODO Use https://doc.rust-lang.org/beta/std/primitive.slice.html#method.fill
                                pkt_buf[0..6]
                                    .copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]); // Dst MAC
                                pkt_buf[6..12].copy_from_slice(&tx_port.mac_addr()); // Src MAC
                                pkt_buf[12..14].copy_from_slice(&[0x08, 0x06]); // Ethertype: ARP

                                // Fill ARP
                                pkt_buf[14..16].copy_from_slice(&[0x00, 0x01]); // HTYPE: Ethernet
                                pkt_buf[16..18].copy_from_slice(&[0x08, 0x00]); // PTYPE: IP
                                pkt_buf[18..20].copy_from_slice(&[0x06, 0x04]); // HLEN: 6, PLEN: 4
                                pkt_buf[20..22].copy_from_slice(&[0x00, 0x01]); // OPER: request (1)
                                pkt_buf[22..28].copy_from_slice(&tx_port.mac_addr()); // SHA (6byte)
                                pkt_buf[28..32].copy_from_slice(&[0x10, 0x00, 0x00, 0x02]); // SPA (10.0.0.2)
                                pkt_buf[32..38]
                                    .copy_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]); // THA (6byte)
                                pkt_buf[38..42].copy_from_slice(&[0x10, 0x00, 0x00, 0x03]); // THA (10.0.0.3)

                                pkt.set_length(42);
                            }
                            // Send packet
                            tx_queue.tx(&mut pkts);
                        }
                        lcore.launch(|| true)
                    }
                    1 => {
                        // TODO Core 1 action
                        lcore.launch(|| true)
                    }
                    _ => {
                        // Otherwise, do nothing
                        lcore.launch(|| true)
                    }
                }
            });
    Ok(())
}

#include "dpdk.h"

uint16_t
inline_rte_eth_rx_burst(uint8_t port_id, uint16_t queue_id, struct rte_mbuf **rx_pkts, uint16_t nb_pkts)
{
    return rte_eth_rx_burst(port_id, queue_id, rx_pkts, nb_pkts);
}

uint16_t
inline_rte_eth_tx_burst(uint8_t port_id, uint16_t queue_id, struct rte_mbuf **tx_pkts, uint16_t nb_pkts)
{
    return rte_eth_tx_burst(port_id, queue_id, tx_pkts, nb_pkts);
}

void inline_rte_pktmbuf_free(struct rte_mbuf* m)
{
    rte_pktmbuf_free(m);
}

struct rte_mbuf* inline_rte_pktmbuf_alloc(struct rte_mempool* mp)
{
    return rte_pktmbuf_alloc(mp);
}


void* macro_rte_pktmbuf_mtod(struct rte_mbuf* pkt)
{
    return rte_pktmbuf_mtod(pkt, void*);
}

uint64_t inline_rte_get_tsc_cycles(void)
{
    return rte_get_tsc_cycles();
}

uint64_t inline_rte_get_timer_cycles (void)
{
    return rte_get_timer_cycles();
}

uint64_t inline_rte_get_timer_hz (void)
{
    return rte_get_timer_hz();
}

void* macro_rte_pktmbuf_mtod_offset(struct rte_mbuf* pkt, size_t offset)
{
    return rte_pktmbuf_mtod_offset(pkt, void*, offset);
}

phys_addr_t macro_rte_pktmbuf_mtophys_offset(struct rte_mbuf* pkt, size_t offset)
{
    return rte_pktmbuf_mtophys_offset(pkt, offset);
}

phys_addr_t macro_rte_pktmbuf_mtophys(struct rte_mbuf* pkt)
{
    return rte_pktmbuf_mtophys(pkt);
}

size_t macro_rte_pktmbuf_pkt_len(struct rte_mbuf* pkt)
{
    return rte_pktmbuf_pkt_len(pkt);
}

size_t macro_rte_pktmbuf_data_len(struct rte_mbuf* pkt)
{
    return rte_pktmbuf_data_len(pkt);
}
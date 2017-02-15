#include "dpdk.h"

uint16_t
inline_rte_eth_rx_burst(uint8_t port_id, uint16_t queue_id, struct rte_mbuf **rx_pkts, const uint16_t nb_pkts)
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
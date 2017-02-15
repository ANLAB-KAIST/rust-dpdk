
uint16_t
inline_rte_eth_rx_burst(uint8_t port_id, uint16_t queue_id, struct rte_mbuf **rx_pkts, const uint16_t nb_pkts);

uint16_t
inline_rte_eth_tx_burst(uint8_t port_id, uint16_t queue_id, struct rte_mbuf **tx_pkts, uint16_t nb_pkts);

void inline_rte_pktmbuf_free(struct rte_mbuf* m);

struct rte_mbuf* inline_rte_pktmbuf_alloc(struct rte_mempool* mp);

void* macro_rte_pktmbuf_mtod(struct rte_mbuf* pkt);
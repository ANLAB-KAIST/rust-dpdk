
uint16_t
inline_rte_eth_rx_burst(uint8_t port_id, uint16_t queue_id, struct rte_mbuf **rx_pkts, const uint16_t nb_pkts);

uint16_t
inline_rte_eth_tx_burst(uint8_t port_id, uint16_t queue_id, struct rte_mbuf **tx_pkts, uint16_t nb_pkts);

void inline_rte_pktmbuf_free(struct rte_mbuf* m);

struct rte_mbuf* inline_rte_pktmbuf_alloc(struct rte_mempool* mp);

void* macro_rte_pktmbuf_mtod(struct rte_mbuf* pkt);

uint64_t inline_rte_get_tsc_cycles(void);

uint64_t inline_rte_get_timer_cycles (void);

uint64_t inline_rte_get_timer_hz (void);

void* macro_rte_pktmbuf_mtod_offset(struct rte_mbuf* pkt, size_t offset);

phys_addr_t macro_rte_pktmbuf_mtophys_offset(struct rte_mbuf* pkt, size_t offset);

phys_addr_t macro_rte_pktmbuf_mtophys(struct rte_mbuf* pkt);

size_t macro_rte_pktmbuf_pkt_len(struct rte_mbuf* pkt);

size_t macro_rte_pktmbuf_data_len(struct rte_mbuf* pkt);

void inline_rte_spinlock_init (rte_spinlock_t *sl);
void inline_rte_spinlock_lock (rte_spinlock_t *sl);
void inline_rte_spinlock_unlock (rte_spinlock_t *sl);
int inline_rte_spinlock_trylock (rte_spinlock_t *sl);
int inline_rte_spinlock_is_locked (rte_spinlock_t *sl);
int inline_rte_tm_supported (void);
void inline_rte_spinlock_lock_tm (rte_spinlock_t *sl);
void inline_rte_spinlock_unlock_tm (rte_spinlock_t *sl);
int inline_rte_spinlock_trylock_tm (rte_spinlock_t *sl);
void inline_rte_spinlock_recursive_init (rte_spinlock_recursive_t *slr);
void inline_rte_spinlock_recursive_lock (rte_spinlock_recursive_t *slr);
void inline_rte_spinlock_recursive_unlock (rte_spinlock_recursive_t *slr);
int inline_rte_spinlock_recursive_trylock (rte_spinlock_recursive_t *slr);
void inline_rte_spinlock_recursive_lock_tm (rte_spinlock_recursive_t *slr);
void inline_rte_spinlock_recursive_unlock_tm (rte_spinlock_recursive_t *slr);
int inline_rte_spinlock_recursive_trylock_tm (rte_spinlock_recursive_t *slr);

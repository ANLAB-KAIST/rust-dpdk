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

void inline_rte_spinlock_init (rte_spinlock_t *sl)
{
	rte_spinlock_init(sl);
}

void inline_rte_spinlock_lock (rte_spinlock_t *sl)
{
	rte_spinlock_lock(sl);
}

void inline_rte_spinlock_unlock (rte_spinlock_t *sl)
{
	rte_spinlock_unlock(sl);
}

int inline_rte_spinlock_trylock (rte_spinlock_t *sl)
{
	return rte_spinlock_trylock(sl);
}

int inline_rte_spinlock_is_locked (rte_spinlock_t *sl)
{
	return rte_spinlock_is_locked(sl);
}

int inline_rte_tm_supported (void)
{
	return rte_tm_supported();
}

void inline_rte_spinlock_lock_tm (rte_spinlock_t *sl)
{
	rte_spinlock_lock_tm(sl);
}

void inline_rte_spinlock_unlock_tm (rte_spinlock_t *sl)
{
	rte_spinlock_unlock_tm(sl);
}

int inline_rte_spinlock_trylock_tm (rte_spinlock_t *sl)
{
	return rte_spinlock_trylock_tm(sl);
}

void inline_rte_spinlock_recursive_init (rte_spinlock_recursive_t *slr)
{
	rte_spinlock_recursive_init(slr);
}

void inline_rte_spinlock_recursive_lock (rte_spinlock_recursive_t *slr)
{
	rte_spinlock_recursive_lock(slr);
}

void inline_rte_spinlock_recursive_unlock (rte_spinlock_recursive_t *slr)
{
	rte_spinlock_recursive_unlock(slr);
}

int inline_rte_spinlock_recursive_trylock (rte_spinlock_recursive_t *slr)
{
	return rte_spinlock_recursive_trylock(slr);
}

void inline_rte_spinlock_recursive_lock_tm (rte_spinlock_recursive_t *slr)
{
	rte_spinlock_recursive_lock_tm(slr);
}

void inline_rte_spinlock_recursive_unlock_tm (rte_spinlock_recursive_t *slr)
{
	rte_spinlock_recursive_unlock_tm(slr);
}

int inline_rte_spinlock_recursive_trylock_tm (rte_spinlock_recursive_t *slr)
{
	return rte_spinlock_recursive_trylock_tm(slr);
}

void inline_rte_pause(void)
{
	rte_pause();
}

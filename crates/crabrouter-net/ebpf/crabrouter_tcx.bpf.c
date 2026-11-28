// SPDX-License-Identifier: GPL-2.0-or-later
#include <linux/bpf.h>
#include <linux/if_ether.h>
#include <linux/in.h>
#include <linux/ip.h>
#include <linux/ipv6.h>
#include <linux/pkt_cls.h>
#include <linux/tcp.h>

#define AF_INET 2
#define AF_INET6 10

#define __uint(name, val) int (*name)[val]
#define __type(name, val) typeof(val) *name
#define SEC(name) __attribute__((section(name), used))

static void *(*const bpf_map_lookup_elem)(void *map, const void *key) = (void *)1;
static long (*const bpf_map_update_elem)(void *map, const void *key, const void *value, __u64 flags) = (void *)2;
static __u64 (*const bpf_ktime_get_ns)(void) = (void *)5;
static long (*const bpf_l3_csum_replace)(struct __sk_buff *skb, __u32 offset, __u64 from, __u64 to, __u64 size) = (void *)10;
static long (*const bpf_redirect)(__u32 ifindex, __u64 flags) = (void *)23;
static long (*const bpf_fib_lookup)(void *ctx, struct bpf_fib_lookup *params, int plen, __u32 flags) = (void *)69;

#if __BYTE_ORDER__ == __ORDER_LITTLE_ENDIAN__
#define bpf_htons(x) __builtin_bswap16(x)
#else
#define bpf_htons(x) (x)
#endif

struct lpm_key_v4 {
    __u32 prefixlen;
    __u32 addr;
};

struct lpm_key_v6 {
    __u32 prefixlen;
    __u8 addr[16];
};

struct policy_val {
    __u32 fwmark;
    __u32 tag_id;
    __u64 expires_at_ns;
};

struct stats_key {
    __u32 client_ip;
    __u32 tag_id;
};

struct stats_key_v6 {
    __u8 client_ip[16];
    __u32 tag_id;
};

struct traffic_counters {
    __u64 rx_packets;
    __u64 rx_bytes;
    __u64 tx_packets;
    __u64 tx_bytes;
};

struct syn_bucket {
    __u64 window_sec;
    __u32 count;
    __u32 pad;
};

struct {
    __uint(type, BPF_MAP_TYPE_LPM_TRIE);
    __uint(max_entries, 16384);
    __uint(map_flags, BPF_F_NO_PREALLOC);
    __type(key, struct lpm_key_v4);
    __type(value, struct policy_val);
} DOMAIN_POLICY_LPM SEC(".maps");

struct {
    __uint(type, BPF_MAP_TYPE_LPM_TRIE);
    __uint(max_entries, 16384);
    __uint(map_flags, BPF_F_NO_PREALLOC);
    __type(key, struct lpm_key_v6);
    __type(value, struct policy_val);
} DOMAIN_POLICY_LPM_V6 SEC(".maps");

struct {
    __uint(type, BPF_MAP_TYPE_PERCPU_HASH);
    __uint(max_entries, 8192);
    __type(key, struct stats_key);
    __type(value, struct traffic_counters);
} FLOW_STATS SEC(".maps");

struct {
    __uint(type, BPF_MAP_TYPE_PERCPU_HASH);
    __uint(max_entries, 8192);
    __type(key, struct stats_key_v6);
    __type(value, struct traffic_counters);
} FLOW_STATS_V6 SEC(".maps");

struct {
    __uint(type, BPF_MAP_TYPE_LPM_TRIE);
    __uint(max_entries, 16384);
    __uint(map_flags, BPF_F_NO_PREALLOC);
    __type(key, struct lpm_key_v4);
    __type(value, __u64);
} XDP_BLOCKLIST_LPM SEC(".maps");

struct {
    __uint(type, BPF_MAP_TYPE_LPM_TRIE);
    __uint(max_entries, 16384);
    __uint(map_flags, BPF_F_NO_PREALLOC);
    __type(key, struct lpm_key_v6);
    __type(value, __u64);
} XDP_BLOCKLIST_LPM_V6 SEC(".maps");

struct {
    __uint(type, BPF_MAP_TYPE_ARRAY);
    __uint(max_entries, 1);
    __type(key, __u32);
    __type(value, __u32);
} XDP_SYN_LIMIT_CFG SEC(".maps");

struct {
    __uint(type, BPF_MAP_TYPE_LRU_HASH);
    __uint(max_entries, 16384);
    __type(key, __u32);
    __type(value, struct syn_bucket);
} XDP_SYN_RATE SEC(".maps");

struct {
    __uint(type, BPF_MAP_TYPE_LRU_HASH);
    __uint(max_entries, 16384);
    __type(key, __u8[16]);
    __type(value, struct syn_bucket);
} XDP_SYN_RATE_V6 SEC(".maps");

struct {
    __uint(type, BPF_MAP_TYPE_PERCPU_ARRAY);
    __uint(max_entries, 2);
    __type(key, __u32);
    __type(value, __u64);
} XDP_DROP_STATS SEC(".maps");

struct {
    __uint(type, BPF_MAP_TYPE_ARRAY);
    __uint(max_entries, 1);
    __type(key, __u32);
    __type(value, __u32);
} FAST_FWD_CFG SEC(".maps");

struct {
    __uint(type, BPF_MAP_TYPE_PERCPU_ARRAY);
    __uint(max_entries, 1);
    __type(key, __u32);
    __type(value, __u64);
} FAST_FWD_STATS SEC(".maps");

static __always_inline void record_stats_v4(__u32 client_ip, __u32 tag_id, __u64 bytes, int is_tx)
{
    struct stats_key key = {
        .client_ip = client_ip,
        .tag_id = tag_id,
    };
    struct traffic_counters *cnt = bpf_map_lookup_elem(&FLOW_STATS, &key);
    if (cnt) {
        if (is_tx) {
            cnt->tx_packets += 1;
            cnt->tx_bytes += bytes;
        } else {
            cnt->rx_packets += 1;
            cnt->rx_bytes += bytes;
        }
        return;
    }

    struct traffic_counters init = {};
    if (is_tx) {
        init.tx_packets = 1;
        init.tx_bytes = bytes;
    } else {
        init.rx_packets = 1;
        init.rx_bytes = bytes;
    }
    bpf_map_update_elem(&FLOW_STATS, &key, &init, BPF_NOEXIST);
}

static __always_inline void record_stats_v6(const __u8 addr[16], __u32 tag_id, __u64 bytes, int is_tx)
{
    struct stats_key_v6 key = {};
    __builtin_memcpy(key.client_ip, addr, 16);
    key.tag_id = tag_id;

    struct traffic_counters *cnt = bpf_map_lookup_elem(&FLOW_STATS_V6, &key);
    if (cnt) {
        if (is_tx) {
            cnt->tx_packets += 1;
            cnt->tx_bytes += bytes;
        } else {
            cnt->rx_packets += 1;
            cnt->rx_bytes += bytes;
        }
        return;
    }

    struct traffic_counters init = {};
    if (is_tx) {
        init.tx_packets = 1;
        init.tx_bytes = bytes;
    } else {
        init.rx_packets = 1;
        init.rx_bytes = bytes;
    }
    bpf_map_update_elem(&FLOW_STATS_V6, &key, &init, BPF_NOEXIST);
}

static __always_inline int is_fast_fwd_enabled(void)
{
    __u32 key = 0;
    __u32 *val = bpf_map_lookup_elem(&FAST_FWD_CFG, &key);
    return val && *val != 0;
}

static __always_inline void inc_fast_fwd(void)
{
    __u32 key = 0;
    __u64 *cnt = bpf_map_lookup_elem(&FAST_FWD_STATS, &key);
    if (cnt)
        *cnt += 1;
}

static __always_inline int try_fast_fwd_v4(struct __sk_buff *skb)
{
    if (!is_fast_fwd_enabled() || skb->ingress_ifindex == 0)
        return TC_ACT_OK;

    void *data = (void *)(long)skb->data;
    void *data_end = (void *)(long)skb->data_end;
    struct ethhdr *eth = data;
    if ((void *)(eth + 1) > data_end)
        return TC_ACT_OK;
    struct iphdr *ip = (void *)(eth + 1);
    if ((void *)(ip + 1) > data_end)
        return TC_ACT_OK;
    if (ip->ttl <= 1)
        return TC_ACT_OK;

    struct bpf_fib_lookup fib = {};
    fib.family = AF_INET;
    fib.tos = ip->tos;
    fib.l4_protocol = ip->protocol;
    fib.ipv4_src = ip->saddr;
    fib.ipv4_dst = ip->daddr;
    fib.ifindex = skb->ingress_ifindex;

    long rc = bpf_fib_lookup(skb, &fib, sizeof(fib), 0);
    if (rc != BPF_FIB_LKUP_RET_SUCCESS || fib.ifindex == skb->ingress_ifindex)
        return TC_ACT_OK;

    __u16 old_ttl_proto = ((__u16)ip->ttl << 8) | ip->protocol;
    __u16 new_ttl_proto = ((__u16)(ip->ttl - 1) << 8) | ip->protocol;
    ip->ttl -= 1;
    bpf_l3_csum_replace(skb, sizeof(struct ethhdr) + 10, old_ttl_proto, new_ttl_proto, 2);

    data = (void *)(long)skb->data;
    data_end = (void *)(long)skb->data_end;
    eth = data;
    if ((void *)(eth + 1) > data_end)
        return TC_ACT_OK;
    __builtin_memcpy(eth->h_dest, fib.dmac, 6);
    __builtin_memcpy(eth->h_source, fib.smac, 6);
    inc_fast_fwd();
    return bpf_redirect(fib.ifindex, 0);
}

static __always_inline int try_fast_fwd_v6(struct __sk_buff *skb)
{
    if (!is_fast_fwd_enabled() || skb->ingress_ifindex == 0)
        return TC_ACT_OK;

    void *data = (void *)(long)skb->data;
    void *data_end = (void *)(long)skb->data_end;
    struct ethhdr *eth = data;
    if ((void *)(eth + 1) > data_end)
        return TC_ACT_OK;
    struct ipv6hdr *ip6 = (void *)(eth + 1);
    if ((void *)(ip6 + 1) > data_end)
        return TC_ACT_OK;
    if (ip6->hop_limit <= 1)
        return TC_ACT_OK;

    struct bpf_fib_lookup fib = {};
    fib.family = AF_INET6;
    fib.l4_protocol = ip6->nexthdr;
    __builtin_memcpy(fib.ipv6_src, ip6->saddr.in6_u.u6_addr8, 16);
    __builtin_memcpy(fib.ipv6_dst, ip6->daddr.in6_u.u6_addr8, 16);
    fib.ifindex = skb->ingress_ifindex;

    long rc = bpf_fib_lookup(skb, &fib, sizeof(fib), 0);
    if (rc != BPF_FIB_LKUP_RET_SUCCESS || fib.ifindex == skb->ingress_ifindex)
        return TC_ACT_OK;

    ip6->hop_limit -= 1;
    __builtin_memcpy(eth->h_dest, fib.dmac, 6);
    __builtin_memcpy(eth->h_source, fib.smac, 6);
    inc_fast_fwd();
    return bpf_redirect(fib.ifindex, 0);
}

SEC("classifier")
int crabrouter_tcx_ingress(struct __sk_buff *skb)
{
    void *data = (void *)(long)skb->data;
    void *data_end = (void *)(long)skb->data_end;
    struct ethhdr *eth = data;
    if ((void *)(eth + 1) > data_end)
        return TC_ACT_OK;

    if (eth->h_proto == bpf_htons(ETH_P_IP)) {
        struct iphdr *ip = (void *)(eth + 1);
        if ((void *)(ip + 1) > data_end)
            return TC_ACT_OK;
        __u32 saddr = ip->saddr;
        __u32 daddr = ip->daddr;
        struct lpm_key_v4 lpm_key = { .prefixlen = 32, .addr = daddr };
        __u32 tag_id = 0;
        struct policy_val *pol = bpf_map_lookup_elem(&DOMAIN_POLICY_LPM, &lpm_key);
        if (pol) {
            __u64 now = bpf_ktime_get_ns();
            if (pol->expires_at_ns == 0 || now <= pol->expires_at_ns) {
                if (pol->fwmark != 0)
                    skb->mark = pol->fwmark;
                tag_id = pol->tag_id;
            }
        }
        record_stats_v4(saddr, tag_id, (__u64)skb->len, 1);
        return try_fast_fwd_v4(skb);
    }

    if (eth->h_proto == bpf_htons(ETH_P_IPV6)) {
        struct ipv6hdr *ip6 = (void *)(eth + 1);
        if ((void *)(ip6 + 1) > data_end)
            return TC_ACT_OK;
        struct lpm_key_v6 lpm_key = { .prefixlen = 128 };
        __builtin_memcpy(lpm_key.addr, ip6->daddr.in6_u.u6_addr8, 16);
        __u32 tag_id = 0;
        struct policy_val *pol = bpf_map_lookup_elem(&DOMAIN_POLICY_LPM_V6, &lpm_key);
        if (pol) {
            __u64 now = bpf_ktime_get_ns();
            if (pol->expires_at_ns == 0 || now <= pol->expires_at_ns) {
                if (pol->fwmark != 0)
                    skb->mark = pol->fwmark;
                tag_id = pol->tag_id;
            }
        }
        record_stats_v6(ip6->saddr.in6_u.u6_addr8, tag_id, (__u64)skb->len, 1);
        return try_fast_fwd_v6(skb);
    }

    return TC_ACT_OK;
}

SEC("classifier")
int crabrouter_tcx_egress(struct __sk_buff *skb)
{
    void *data = (void *)(long)skb->data;
    void *data_end = (void *)(long)skb->data_end;
    struct ethhdr *eth = data;
    if ((void *)(eth + 1) > data_end)
        return TC_ACT_OK;

    if (eth->h_proto == bpf_htons(ETH_P_IP)) {
        struct iphdr *ip = (void *)(eth + 1);
        if ((void *)(ip + 1) > data_end)
            return TC_ACT_OK;
        __u32 saddr = ip->saddr;
        __u32 daddr = ip->daddr;
        struct lpm_key_v4 lpm_key = { .prefixlen = 32, .addr = saddr };
        __u32 tag_id = 0;
        struct policy_val *pol = bpf_map_lookup_elem(&DOMAIN_POLICY_LPM, &lpm_key);
        if (pol) {
            __u64 now = bpf_ktime_get_ns();
            if (pol->expires_at_ns == 0 || now <= pol->expires_at_ns)
                tag_id = pol->tag_id;
        }
        record_stats_v4(daddr, tag_id, (__u64)skb->len, 0);
        return TC_ACT_OK;
    }

    if (eth->h_proto == bpf_htons(ETH_P_IPV6)) {
        struct ipv6hdr *ip6 = (void *)(eth + 1);
        if ((void *)(ip6 + 1) > data_end)
            return TC_ACT_OK;
        struct lpm_key_v6 lpm_key = { .prefixlen = 128 };
        __builtin_memcpy(lpm_key.addr, ip6->saddr.in6_u.u6_addr8, 16);
        __u32 tag_id = 0;
        struct policy_val *pol = bpf_map_lookup_elem(&DOMAIN_POLICY_LPM_V6, &lpm_key);
        if (pol) {
            __u64 now = bpf_ktime_get_ns();
            if (pol->expires_at_ns == 0 || now <= pol->expires_at_ns)
                tag_id = pol->tag_id;
        }
        record_stats_v6(ip6->daddr.in6_u.u6_addr8, tag_id, (__u64)skb->len, 0);
        return TC_ACT_OK;
    }

    return TC_ACT_OK;
}

static __always_inline void inc_xdp_drop(__u32 idx)
{
    __u64 *cnt = bpf_map_lookup_elem(&XDP_DROP_STATS, &idx);
    if (cnt)
        *cnt += 1;
}

static __always_inline int check_syn_limit_v4(__u32 src_ip, __u64 now_ns)
{
    __u32 cfg_key = 0;
    __u32 *limit = bpf_map_lookup_elem(&XDP_SYN_LIMIT_CFG, &cfg_key);
    if (!limit || *limit == 0)
        return XDP_PASS;
    __u64 sec = now_ns / 1000000000ULL;
    struct syn_bucket *b = bpf_map_lookup_elem(&XDP_SYN_RATE, &src_ip);
    if (b && b->window_sec == sec) {
        if (b->count >= *limit) {
            inc_xdp_drop(1);
            return XDP_DROP;
        }
        b->count += 1;
    } else {
        struct syn_bucket fresh = { .window_sec = sec, .count = 1, .pad = 0 };
        bpf_map_update_elem(&XDP_SYN_RATE, &src_ip, &fresh, BPF_ANY);
    }
    return XDP_PASS;
}

static __always_inline int check_syn_limit_v6(const __u8 src_ip[16], __u64 now_ns)
{
    __u32 cfg_key = 0;
    __u32 *limit = bpf_map_lookup_elem(&XDP_SYN_LIMIT_CFG, &cfg_key);
    if (!limit || *limit == 0)
        return XDP_PASS;
    __u64 sec = now_ns / 1000000000ULL;
    __u8 key[16];
    __builtin_memcpy(key, src_ip, 16);
    struct syn_bucket *b = bpf_map_lookup_elem(&XDP_SYN_RATE_V6, key);
    if (b && b->window_sec == sec) {
        if (b->count >= *limit) {
            inc_xdp_drop(1);
            return XDP_DROP;
        }
        b->count += 1;
    } else {
        struct syn_bucket fresh = { .window_sec = sec, .count = 1, .pad = 0 };
        bpf_map_update_elem(&XDP_SYN_RATE_V6, key, &fresh, BPF_ANY);
    }
    return XDP_PASS;
}

SEC("xdp")
int crabrouter_xdp_wan_guard(struct xdp_md *ctx)
{
    void *data = (void *)(long)ctx->data;
    void *data_end = (void *)(long)ctx->data_end;

    struct ethhdr *eth = data;
    if ((void *)(eth + 1) > data_end)
        return XDP_PASS;

    __u64 now_ns = bpf_ktime_get_ns();

    if (eth->h_proto == bpf_htons(ETH_P_IP)) {
        struct iphdr *ip = (void *)(eth + 1);
        if ((void *)(ip + 1) > data_end)
            return XDP_PASS;
        __u32 src_ip = ip->saddr;
        struct lpm_key_v4 blk_key = { .prefixlen = 32, .addr = src_ip };
        __u64 *blocked_exp = bpf_map_lookup_elem(&XDP_BLOCKLIST_LPM, &blk_key);
        if (blocked_exp && (*blocked_exp == 0 || now_ns <= *blocked_exp)) {
            inc_xdp_drop(0);
            return XDP_DROP;
        }
        if (ip->protocol == IPPROTO_TCP && ip->ihl >= 5) {
            struct tcphdr *tcp = (void *)ip + ((__u32)ip->ihl * 4);
            if ((void *)(tcp + 1) > data_end)
                return XDP_PASS;
            if (tcp->syn && !tcp->ack)
                return check_syn_limit_v4(src_ip, now_ns);
        }
        return XDP_PASS;
    }

    if (eth->h_proto == bpf_htons(ETH_P_IPV6)) {
        struct ipv6hdr *ip6 = (void *)(eth + 1);
        if ((void *)(ip6 + 1) > data_end)
            return XDP_PASS;
        struct lpm_key_v6 blk_key = { .prefixlen = 128 };
        __builtin_memcpy(blk_key.addr, ip6->saddr.in6_u.u6_addr8, 16);
        __u64 *blocked_exp = bpf_map_lookup_elem(&XDP_BLOCKLIST_LPM_V6, &blk_key);
        if (blocked_exp && (*blocked_exp == 0 || now_ns <= *blocked_exp)) {
            inc_xdp_drop(0);
            return XDP_DROP;
        }
        if (ip6->nexthdr == IPPROTO_TCP) {
            struct tcphdr *tcp = (void *)(ip6 + 1);
            if ((void *)(tcp + 1) > data_end)
                return XDP_PASS;
            if (tcp->syn && !tcp->ack)
                return check_syn_limit_v6(ip6->saddr.in6_u.u6_addr8, now_ns);
        }
        return XDP_PASS;
    }

    return XDP_PASS;
}

char _license[] SEC("license") = "GPL";

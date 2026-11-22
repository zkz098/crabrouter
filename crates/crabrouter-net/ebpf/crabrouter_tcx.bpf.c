// SPDX-License-Identifier: GPL-2.0-or-later
#include <linux/bpf.h>
#include <linux/if_ether.h>
#include <linux/in.h>
#include <linux/ip.h>
#include <linux/pkt_cls.h>
#include <linux/tcp.h>

#define __uint(name, val) int (*name)[val]
#define __type(name, val) typeof(val) *name
#define SEC(name) __attribute__((section(name), used))

static void *(*const bpf_map_lookup_elem)(void *map, const void *key) = (void *)1;
static long (*const bpf_map_update_elem)(void *map, const void *key, const void *value, __u64 flags) = (void *)2;
static __u64 (*const bpf_ktime_get_ns)(void) = (void *)5;

#if __BYTE_ORDER__ == __ORDER_LITTLE_ENDIAN__
#define bpf_htons(x) __builtin_bswap16(x)
#else
#define bpf_htons(x) (x)
#endif

struct lpm_key_v4 {
    __u32 prefixlen;
    __u32 addr;
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

struct traffic_counters {
    __u64 rx_packets;
    __u64 rx_bytes;
    __u64 tx_packets;
    __u64 tx_bytes;
};

struct {
    __uint(type, BPF_MAP_TYPE_LPM_TRIE);
    __uint(max_entries, 16384);
    __uint(map_flags, BPF_F_NO_PREALLOC);
    __type(key, struct lpm_key_v4);
    __type(value, struct policy_val);
} DOMAIN_POLICY_LPM SEC(".maps");

struct {
    __uint(type, BPF_MAP_TYPE_PERCPU_HASH);
    __uint(max_entries, 8192);
    __type(key, struct stats_key);
    __type(value, struct traffic_counters);
} FLOW_STATS SEC(".maps");

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
    __type(value, __u64);
} XDP_BLOCKLIST_LPM SEC(".maps");

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
    __uint(type, BPF_MAP_TYPE_PERCPU_ARRAY);
    __uint(max_entries, 2);
    __type(key, __u32);
    __type(value, __u64);
} XDP_DROP_STATS SEC(".maps");

static __always_inline int parse_ipv4(struct __sk_buff *skb, __u32 *saddr, __u32 *daddr)
{
    void *data = (void *)(long)skb->data;
    void *data_end = (void *)(long)skb->data_end;

    struct ethhdr *eth = data;
    if ((void *)(eth + 1) > data_end)
        return -1;
    if (eth->h_proto != bpf_htons(ETH_P_IP))
        return -1;

    struct iphdr *ip = (void *)(eth + 1);
    if ((void *)(ip + 1) > data_end)
        return -1;

    *saddr = ip->saddr;
    *daddr = ip->daddr;
    return 0;
}

static __always_inline void record_stats(__u32 client_ip, __u32 tag_id, __u64 bytes, int is_tx)
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

SEC("tc")
int crabrouter_tcx_ingress(struct __sk_buff *skb)
{
    __u32 saddr = 0;
    __u32 daddr = 0;
    if (parse_ipv4(skb, &saddr, &daddr) < 0)
        return TC_ACT_OK;

    struct lpm_key_v4 lpm_key = {
        .prefixlen = 32,
        .addr = daddr,
    };
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

    record_stats(saddr, tag_id, (__u64)skb->len, 1);
    return TC_ACT_OK;
}

SEC("tc")
int crabrouter_tcx_egress(struct __sk_buff *skb)
{
    __u32 saddr = 0;
    __u32 daddr = 0;
    if (parse_ipv4(skb, &saddr, &daddr) < 0)
        return TC_ACT_OK;

    struct lpm_key_v4 lpm_key = {
        .prefixlen = 32,
        .addr = saddr,
    };
    __u32 tag_id = 0;
    struct policy_val *pol = bpf_map_lookup_elem(&DOMAIN_POLICY_LPM, &lpm_key);
    if (pol) {
        __u64 now = bpf_ktime_get_ns();
        if (pol->expires_at_ns == 0 || now <= pol->expires_at_ns)
            tag_id = pol->tag_id;
    }

    record_stats(daddr, tag_id, (__u64)skb->len, 0);
    return TC_ACT_OK;
}

static __always_inline void inc_xdp_drop(__u32 idx)
{
    __u64 *cnt = bpf_map_lookup_elem(&XDP_DROP_STATS, &idx);
    if (cnt)
        *cnt += 1;
}

SEC("xdp")
int crabrouter_xdp_wan_guard(struct xdp_md *ctx)
{
    void *data = (void *)(long)ctx->data;
    void *data_end = (void *)(long)ctx->data_end;

    struct ethhdr *eth = data;
    if ((void *)(eth + 1) > data_end)
        return XDP_PASS;
    if (eth->h_proto != bpf_htons(ETH_P_IP))
        return XDP_PASS;

    struct iphdr *ip = (void *)(eth + 1);
    if ((void *)(ip + 1) > data_end)
        return XDP_PASS;

    __u32 src_ip = ip->saddr;
    struct lpm_key_v4 blk_key = {
        .prefixlen = 32,
        .addr = src_ip,
    };
    __u64 *blocked_exp = bpf_map_lookup_elem(&XDP_BLOCKLIST_LPM, &blk_key);
    __u64 now_ns = bpf_ktime_get_ns();
    if (blocked_exp && (*blocked_exp == 0 || now_ns <= *blocked_exp)) {
        inc_xdp_drop(0);
        return XDP_DROP;
    }

    if (ip->protocol == IPPROTO_TCP && ip->ihl >= 5) {
        struct tcphdr *tcp = (void *)ip + ((__u32)ip->ihl * 4);
        if ((void *)(tcp + 1) > data_end)
            return XDP_PASS;
        if (tcp->syn && !tcp->ack) {
            __u32 cfg_key = 0;
            __u32 *limit = bpf_map_lookup_elem(&XDP_SYN_LIMIT_CFG, &cfg_key);
            if (limit && *limit > 0) {
                __u64 sec = now_ns / 1000000000ULL;
                struct syn_bucket *b = bpf_map_lookup_elem(&XDP_SYN_RATE, &src_ip);
                if (b && b->window_sec == sec) {
                    if (b->count >= *limit) {
                        inc_xdp_drop(1);
                        return XDP_DROP;
                    }
                    b->count += 1;
                } else {
                    struct syn_bucket fresh = {
                        .window_sec = sec,
                        .count = 1,
                        .pad = 0,
                    };
                    bpf_map_update_elem(&XDP_SYN_RATE, &src_ip, &fresh, BPF_ANY);
                }
            }
        }
    }

    return XDP_PASS;
}

char _license[] SEC("license") = "GPL";

import { For, Show, createResource, createSignal } from "solid-js";
import {
  getDdnsStatus,
  getEbpfStatus,
  getMultiWanStatus,
  getTlsStatus,
  triggerDdnsSync,
  triggerMultiWanProbe,
  triggerTlsRenew,
} from "../api";
import type { AcmeChallengeType } from "../types/AcmeChallengeType";
import type { AcmeConfig } from "../types/AcmeConfig";
import type { BondConfig } from "../types/BondConfig";
import type { BondLacpRate } from "../types/BondLacpRate";
import type { BondMode } from "../types/BondMode";
import type { BondXmitHashPolicy } from "../types/BondXmitHashPolicy";
import type { DdnsConfig } from "../types/DdnsConfig";
import type { DdnsProvider } from "../types/DdnsProvider";
import type { EbpfConfig } from "../types/EbpfConfig";
import type { HostInterfaceSnapshot } from "../types/HostInterfaceSnapshot";
import type { InterfaceRole } from "../types/InterfaceRole";
import type { LanConfig } from "../types/LanConfig";
import type { MultiWanConfig } from "../types/MultiWanConfig";
import type { MultiWanMode } from "../types/MultiWanMode";
import type { ProbeProtocol } from "../types/ProbeProtocol";
import type { RouterConfig } from "../types/RouterConfig";
import type { WanConfig } from "../types/WanConfig";
import type { WanHealthState } from "../types/WanHealthState";
import type { WanProto } from "../types/WanProto";
import type { WanRuntimeState } from "../types/WanRuntimeState";

interface Props {
  draft: RouterConfig;
  interfaces: HostInterfaceSnapshot[];
  wanStates: WanRuntimeState[];
  onChange: (next: RouterConfig) => void;
}

export function NetworkPage(props: Props) {
  const [draggedIface, setDraggedIface] = createSignal<string | null>(null);
  const [multiWanStatus, { mutate: setMultiWanStatus, refetch: refetchMultiWan }] =
    createResource(getMultiWanStatus);
  const [ebpfStatus, { refetch: refetchEbpf }] = createResource(getEbpfStatus);
  const [ddnsStatus, { mutate: setDdnsStatus }] = createResource(getDdnsStatus);
  const [tlsStatus, { mutate: setTlsStatus }] = createResource(getTlsStatus);
  const [probing, setProbing] = createSignal(false);

  async function runProbeNow() {
    setProbing(true);
    try {
      const res = await triggerMultiWanProbe();
      setMultiWanStatus(res);
    } finally {
      setProbing(false);
    }
  }

  function updateMultiWan(patch: Partial<MultiWanConfig>) {
    const next = structuredClone(props.draft);
    next.multi_wan = { ...next.multi_wan, ...patch };
    props.onChange(next);
  }

  function updateEbpf(patch: Partial<EbpfConfig>) {
    const next = structuredClone(props.draft);
    next.ebpf = { ...next.ebpf, ...patch };
    props.onChange(next);
  }

  function updateDdns(patch: Partial<DdnsConfig>) {
    const next = structuredClone(props.draft);
    next.ddns = { ...next.ddns, ...patch };
    props.onChange(next);
  }

  function updateAcme(patch: Partial<AcmeConfig>) {
    const next = structuredClone(props.draft);
    next.acme = { ...next.acme, ...patch };
    props.onChange(next);
  }

  function addEbpfRule() {
    const next = structuredClone(props.draft);
    const maxTag = next.ebpf.domain_rules.reduce((m, r) => Math.max(m, r.tag_id), 0);
    next.ebpf.domain_rules.push({
      domain: "*.example.com",
      wan_name: next.wans[0]?.name ?? "wan",
      tag_id: maxTag + 1,
    });
    props.onChange(next);
  }

  function healthBadgeClass(h: WanHealthState): string {
    switch (h) {
      case "healthy":
        return "bg-emerald-500/20 text-emerald-300";
      case "degraded":
        return "bg-amber-500/20 text-amber-300";
      case "down":
        return "bg-rose-500/20 text-rose-300";
      default:
        return "bg-slate-700 text-slate-300";
    }
  }

  const primaryLan = (): LanConfig | undefined => props.draft.lans[0];

  const allPhysicalNames = () => {
    const names = new Set<string>(props.interfaces.map((i) => i.name));
    for (const wan of props.draft.wans) {
      names.add(wan.device);
    }
    for (const lan of props.draft.lans) {
      for (const p of lan.ports) {
        names.add(p);
      }
    }
    return [...names].filter((n) => !props.draft.lans.some((l) => l.bridge === n));
  };

  function togglePortInLan(lanIdx: number, portName: string) {
    const next = structuredClone(props.draft);
    const lan = next.lans[lanIdx];
    if (!lan) {
      return;
    }
    if (lan.ports.includes(portName)) {
      lan.ports = lan.ports.filter((p) => p !== portName);
    } else {
      for (const other of next.lans) {
        other.ports = other.ports.filter((p) => p !== portName);
      }
      lan.ports.push(portName);
    }
    props.onChange(next);
  }

  function dropToLan(lanIdx: number) {
    const iface = draggedIface();
    setDraggedIface(null);
    if (!iface) {
      return;
    }
    const next = structuredClone(props.draft);
    for (const lan of next.lans) {
      lan.ports = lan.ports.filter((p) => p !== iface);
    }
    const target = next.lans[lanIdx];
    if (target && !target.ports.includes(iface)) {
      target.ports.push(iface);
    }
    props.onChange(next);
  }

  function dropToUnassigned() {
    const iface = draggedIface();
    setDraggedIface(null);
    if (!iface) {
      return;
    }
    const next = structuredClone(props.draft);
    for (const lan of next.lans) {
      lan.ports = lan.ports.filter((p) => p !== iface);
    }
    props.onChange(next);
  }

  function updateWan(idx: number, patch: Partial<WanConfig>) {
    const next = structuredClone(props.draft);
    const cur = next.wans[idx];
    if (!cur) {
      return;
    }
    next.wans[idx] = { ...cur, ...patch };
    props.onChange(next);
  }

  function addWan() {
    const next = structuredClone(props.draft);
    next.wans.push({
      name: `wan${next.wans.length + 1}`,
      device: "eth0",
      proto: "dhcp",
      static_cidr: null,
      gateway: null,
      pppoe_username: null,
      pppoe_password: null,
      mtu: 1500,
      ipv6_pd: true,
      ipv6_pd_prefix_len: 60,
      metric: 100 + next.wans.length * 10,
      weight: 1,
      table_id: null,
      health_check_enabled: true,
      probe_targets: [],
    });
    props.onChange(next);
  }

  function removeWan(idx: number) {
    const next = structuredClone(props.draft);
    next.wans.splice(idx, 1);
    props.onChange(next);
  }

  function updateLan(idx: number, patch: Partial<LanConfig>) {
    const next = structuredClone(props.draft);
    const cur = next.lans[idx];
    if (!cur) {
      return;
    }
    next.lans[idx] = { ...cur, ...patch };
    props.onChange(next);
  }

  function addBond() {
    const next = structuredClone(props.draft);
    next.bonds.push({
      name: `bond${next.bonds.length}`,
      slaves: [],
      mode: "802.3ad",
      xmit_hash_policy: "layer2+3",
      lacp_rate: "slow",
      miimon_ms: 100,
      mtu: null,
      primary: null,
    });
    props.onChange(next);
  }

  function updateBond(idx: number, patch: Partial<BondConfig>) {
    const next = structuredClone(props.draft);
    const cur = next.bonds[idx];
    if (!cur) {
      return;
    }
    next.bonds[idx] = { ...cur, ...patch };
    props.onChange(next);
  }

  function removeBond(idx: number) {
    const next = structuredClone(props.draft);
    next.bonds.splice(idx, 1);
    props.onChange(next);
  }

  function addVlan() {
    const next = structuredClone(props.draft);
    next.vlans.push({
      parent: allPhysicalNames()[0] ?? "eth1",
      id: 20,
      role: "lan",
      cidr: "192.168.20.1/24",
      ipv6_ra_enabled: true,
      ipv6_subnet_id: null,
    });
    props.onChange(next);
  }

  function addPin(mac = "", name = "") {
    const next = structuredClone(props.draft);
    next.interface_pins.push({ mac, name });
    props.onChange(next);
  }

  function addRoute() {
    const next = structuredClone(props.draft);
    next.routes.push({
      table_id: 254,
      destination: "10.0.0.0/8",
      gateway: "192.168.10.254",
      device: primaryLan()?.bridge ?? "br-lan",
      metric: 100,
    });
    props.onChange(next);
  }

  function addPolicyRule() {
    const next = structuredClone(props.draft);
    next.policy_rules.push({
      priority: 1000,
      src: "192.168.20.0/24",
      dst: null,
      iif: null,
      table_id: 100,
    });
    props.onChange(next);
  }

  const unassignedIfaces = () =>
    allPhysicalNames().filter(
      (n) =>
        !props.draft.lans.some((l) => l.ports.includes(n)) &&
        !props.draft.wans.some((w) => w.device === n),
    );

  return (
    <div class="space-y-6">
      {/* Drag & Drop + Checkbox Bridge Port Assigner */}
      <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
        <div class="mb-3">
          <h2 class="text-sm font-semibold text-slate-200">
            物理端口编排（支持拖拽或勾选加入 Bridge）
          </h2>
          <p class="mt-0.5 text-xs text-slate-400">
            将物理网口拖入目标 LAN Bridge 区域或直接勾选下方复选框。
          </p>
        </div>

        <div class="grid gap-4 md:grid-cols-2">
          <div
            onDragOver={(e) => e.preventDefault()}
            onDrop={() => dropToUnassigned()}
            class="rounded-xl border border-dashed border-slate-700 bg-slate-950/60 p-3"
          >
            <div class="mb-2 text-xs font-medium text-slate-400">未分配 / WAN 物理接口池</div>
            <div class="flex flex-wrap gap-2">
              <For each={unassignedIfaces()}>
                {(name) => (
                  <div
                    draggable
                    onDragStart={() => setDraggedIface(name)}
                    class="cursor-grab rounded-lg border border-slate-700 bg-slate-900 px-3 py-1.5 font-mono text-xs text-slate-200 active:cursor-grabbing"
                  >
                    {name}
                  </div>
                )}
              </For>
              <For each={props.draft.wans}>
                {(wan) => (
                  <div class="rounded-lg border border-orange-500/40 bg-orange-500/10 px-3 py-1.5 font-mono text-xs text-orange-300">
                    {wan.device} (WAN: {wan.name})
                  </div>
                )}
              </For>
            </div>
          </div>

          <For each={props.draft.lans}>
            {(lan, lanIdx) => (
              <div
                onDragOver={(e) => e.preventDefault()}
                onDrop={() => dropToLan(lanIdx())}
                class="rounded-xl border border-sky-500/40 bg-sky-950/20 p-3"
              >
                <div class="mb-2 flex items-center justify-between text-xs">
                  <span class="font-mono font-semibold text-sky-300">
                    {lan.bridge} ({lan.name})
                  </span>
                  <span class="text-slate-400">拖放端口到此处</span>
                </div>
                <div class="mb-2 flex flex-wrap gap-2">
                  <For each={lan.ports}>
                    {(port) => (
                      <div
                        draggable
                        onDragStart={() => setDraggedIface(port)}
                        class="flex cursor-grab items-center gap-1.5 rounded-lg border border-sky-500/50 bg-sky-500/15 px-2.5 py-1 font-mono text-xs text-sky-200"
                      >
                        <span>{port}</span>
                        <button
                          type="button"
                          onClick={() => togglePortInLan(lanIdx(), port)}
                          class="text-sky-300 hover:text-white"
                          title="移出 Bridge"
                        >
                          ×
                        </button>
                      </div>
                    )}
                  </For>
                </div>
                <div class="flex flex-wrap gap-2 border-t border-slate-800/80 pt-2">
                  <For each={allPhysicalNames()}>
                    {(iface) => (
                      <label class="flex items-center gap-1.5 text-xs text-slate-300">
                        <input
                          type="checkbox"
                          checked={lan.ports.includes(iface)}
                          onChange={() => togglePortInLan(lanIdx(), iface)}
                        />
                        <span class="font-mono">{iface}</span>
                      </label>
                    )}
                  </For>
                </div>
              </div>
            )}
          </For>
        </div>
      </section>

      {/* WAN Config */}
      <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
        <div class="mb-3 flex items-center justify-between">
          <h2 class="text-sm font-semibold text-slate-200">WAN 上行接口配置</h2>
          <button
            type="button"
            onClick={addWan}
            class="rounded-lg border border-slate-700 bg-slate-800 px-3 py-1 text-xs hover:bg-slate-700"
          >
            + 添加 WAN
          </button>
        </div>

        <div class="space-y-4">
          <For each={props.draft.wans}>
            {(wan, idx) => {
              const runtime = () => props.wanStates.find((s) => s.wan_name === wan.name);
              return (
                <div class="rounded-xl border border-slate-800 bg-slate-950/60 p-4">
                  <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
                    <div class="flex items-center gap-2">
                      <span class="font-mono text-sm font-semibold text-orange-300">
                        {wan.name}
                      </span>
                      <Show when={runtime()}>
                        {(st) => (
                          <span class="rounded bg-emerald-500/20 px-2 py-0.5 text-[11px] text-emerald-300">
                            {st().state} · {st().ip ?? "无 IP"}
                          </span>
                        )}
                      </Show>
                    </div>
                    <button
                      type="button"
                      onClick={() => removeWan(idx())}
                      class="text-xs text-red-400 hover:text-red-300"
                    >
                      删除
                    </button>
                  </div>

                  <div class="grid gap-3 sm:grid-cols-3">
                    <label class="block text-xs">
                      <span class="text-slate-400">名称</span>
                      <input
                        type="text"
                        value={wan.name}
                        onInput={(e) => updateWan(idx(), { name: e.currentTarget.value })}
                        class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-xs"
                      />
                    </label>
                    <label class="block text-xs">
                      <span class="text-slate-400">物理/VLAN 设备</span>
                      <input
                        type="text"
                        value={wan.device}
                        onInput={(e) => updateWan(idx(), { device: e.currentTarget.value })}
                        class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 font-mono text-xs"
                      />
                    </label>
                    <label class="block text-xs">
                      <span class="text-slate-400">协议</span>
                      <select
                        value={wan.proto}
                        onChange={(e) =>
                          updateWan(idx(), { proto: e.currentTarget.value as WanProto })
                        }
                        class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-xs"
                      >
                        <option value="dhcp">DHCP</option>
                        <option value="pppoe">PPPoE</option>
                        <option value="static">Static</option>
                      </select>
                    </label>
                    <label class="block text-xs">
                      <span class="text-slate-400">MTU</span>
                      <input
                        type="number"
                        value={wan.mtu}
                        onInput={(e) =>
                          updateWan(idx(), { mtu: Number(e.currentTarget.value) || 1500 })
                        }
                        class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 font-mono text-xs"
                      />
                    </label>
                    <label class="block text-xs">
                      <span class="text-slate-400">路由跃点 (Metric)</span>
                      <input
                        type="number"
                        value={wan.metric}
                        onInput={(e) =>
                          updateWan(idx(), { metric: Number(e.currentTarget.value) || 100 })
                        }
                        class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 font-mono text-xs"
                      />
                    </label>
                    <label class="block text-xs">
                      <span class="text-slate-400">分流权重 (Weight)</span>
                      <input
                        type="number"
                        min="1"
                        value={wan.weight}
                        onInput={(e) =>
                          updateWan(idx(), {
                            weight: Math.max(1, Number(e.currentTarget.value) || 1),
                          })
                        }
                        class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 font-mono text-xs"
                      />
                    </label>
                    <label class="block text-xs">
                      <span class="text-slate-400">路由表 ID (可选)</span>
                      <input
                        type="number"
                        value={wan.table_id ?? ""}
                        placeholder={`自动 (${101 + idx()})`}
                        onInput={(e) => {
                          const raw = e.currentTarget.value.trim();
                          updateWan(idx(), { table_id: raw ? Number(raw) : null });
                        }}
                        class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 font-mono text-xs"
                      />
                    </label>
                    <div class="flex flex-wrap items-center gap-3 pt-5 text-xs text-slate-300">
                      <label class="flex items-center gap-2">
                        <input
                          type="checkbox"
                          checked={wan.health_check_enabled}
                          onChange={(e) =>
                            updateWan(idx(), { health_check_enabled: e.currentTarget.checked })
                          }
                        />
                        健康探测
                      </label>
                      <label class="flex items-center gap-2">
                        <input
                          type="checkbox"
                          checked={wan.ipv6_pd}
                          onChange={(e) => updateWan(idx(), { ipv6_pd: e.currentTarget.checked })}
                        />
                        启用 IPv6-PD
                      </label>
                      <Show when={wan.ipv6_pd}>
                        <select
                          value={wan.ipv6_pd_prefix_len}
                          onChange={(e) =>
                            updateWan(idx(), {
                              ipv6_pd_prefix_len: Number(e.currentTarget.value) || 60,
                            })
                          }
                          class="rounded-lg border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs"
                          title="申请的 IPv6-PD 前缀长度提示"
                        >
                          <option value={56}>/56 (256 个 /64 子网)</option>
                          <option value={60}>/60 (16 个 /64 子网)</option>
                          <option value={64}>/64 (单子网)</option>
                          <option value={48}>/48 (企业级)</option>
                        </select>
                      </Show>
                    </div>
                  </div>

                  <Show when={runtime()?.ipv6_pd_prefix}>
                    {(pd) => (
                      <div class="mt-3 rounded-lg border border-sky-500/30 bg-sky-950/20 p-2.5 text-xs">
                        <div class="flex flex-wrap items-center justify-between gap-2">
                          <span class="font-mono text-sky-300">
                            已获取 IPv6-PD 委派前缀: {pd()}
                          </span>
                          <span class="font-mono text-slate-400">
                            WAN IPv6: {runtime()?.ipv6_addr ?? "-"}
                          </span>
                        </div>
                        <Show when={(runtime()?.ipv6_slices.length ?? 0) > 0}>
                          <div class="mt-2 flex flex-wrap gap-1.5">
                            <For each={runtime()?.ipv6_slices}>
                              {(sl) => (
                                <span class="rounded border border-sky-500/40 bg-sky-500/10 px-2 py-0.5 font-mono text-[11px] text-sky-200">
                                  {sl.interface} (#{sl.subnet_id}) → {sl.prefix_cidr}
                                </span>
                              )}
                            </For>
                          </div>
                        </Show>
                      </div>
                    )}
                  </Show>

                  <Show when={wan.proto === "static"}>
                    <div class="mt-3 grid gap-3 sm:grid-cols-2">
                      <label class="block text-xs">
                        <span class="text-slate-400">静态 CIDR</span>
                        <input
                          type="text"
                          value={wan.static_cidr ?? ""}
                          onInput={(e) => updateWan(idx(), { static_cidr: e.currentTarget.value })}
                          class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 font-mono text-xs"
                        />
                      </label>
                      <label class="block text-xs">
                        <span class="text-slate-400">网关</span>
                        <input
                          type="text"
                          value={wan.gateway ?? ""}
                          onInput={(e) =>
                            updateWan(idx(), { gateway: e.currentTarget.value || null })
                          }
                          class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 font-mono text-xs"
                        />
                      </label>
                    </div>
                  </Show>

                  <Show when={wan.proto === "pppoe"}>
                    <div class="mt-3 grid gap-3 sm:grid-cols-2">
                      <label class="block text-xs">
                        <span class="text-slate-400">PPPoE 账号</span>
                        <input
                          type="text"
                          value={wan.pppoe_username ?? ""}
                          onInput={(e) =>
                            updateWan(idx(), { pppoe_username: e.currentTarget.value })
                          }
                          class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-xs"
                        />
                      </label>
                      <label class="block text-xs">
                        <span class="text-slate-400">PPPoE 密码</span>
                        <input
                          type="password"
                          value={wan.pppoe_password ?? ""}
                          onInput={(e) =>
                            updateWan(idx(), { pppoe_password: e.currentTarget.value })
                          }
                          class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-xs"
                        />
                      </label>
                    </div>
                  </Show>
                </div>
              );
            }}
          </For>
        </div>
      </section>

      {/* Multi-WAN Load Balancing & Failover */}
      <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
        <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
          <div>
            <h2 class="text-sm font-semibold text-slate-200">多 WAN 负载均衡与故障切换</h2>
            <p class="mt-0.5 text-xs text-slate-400">
              基于独立路由表、nftables ct mark 策略路由与 ICMP/HTTP
              健康探测，支持主备切换、按权重分流及故障切换精确清理 conntrack。
            </p>
          </div>
          <div class="flex items-center gap-3">
            <label class="flex items-center gap-2 text-xs text-slate-300">
              <input
                type="checkbox"
                checked={props.draft.multi_wan.enabled}
                onChange={(e) => updateMultiWan({ enabled: e.currentTarget.checked })}
              />
              启用多 WAN 调度
            </label>
            <button
              type="button"
              disabled={probing()}
              onClick={() => void runProbeNow()}
              class="rounded-lg border border-sky-500/40 bg-sky-500/15 px-3 py-1.5 text-xs font-medium text-sky-300 hover:bg-sky-500/25 disabled:opacity-50"
            >
              {probing() ? "探测中..." : "立即探测"}
            </button>
            <button
              type="button"
              onClick={() => void refetchMultiWan()}
              class="rounded-lg border border-slate-700 bg-slate-800 px-2.5 py-1.5 text-xs text-slate-300 hover:bg-slate-700"
            >
              刷新
            </button>
          </div>
        </div>

        <div class="grid gap-3 sm:grid-cols-4">
          <label class="block text-xs">
            <span class="text-slate-400">调度模式</span>
            <select
              value={props.draft.multi_wan.mode}
              onChange={(e) => updateMultiWan({ mode: e.currentTarget.value as MultiWanMode })}
              class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-xs"
            >
              <option value="failover">主备故障切换 (Failover)</option>
              <option value="load_balance">按权重负载均衡 (Load Balance)</option>
            </select>
          </label>
          <label class="block text-xs">
            <span class="text-slate-400">探测协议</span>
            <select
              value={props.draft.multi_wan.probe_protocol}
              onChange={(e) =>
                updateMultiWan({ probe_protocol: e.currentTarget.value as ProbeProtocol })
              }
              class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-xs"
            >
              <option value="icmp">ICMP (Ping)</option>
              <option value="http">HTTP (TCP/HEAD)</option>
              <option value="icmp_and_http">ICMP + HTTP 混合</option>
            </select>
          </label>
          <label class="block text-xs">
            <span class="text-slate-400">探测间隔 (秒)</span>
            <input
              type="number"
              min="1"
              value={props.draft.multi_wan.probe_interval_secs}
              onInput={(e) =>
                updateMultiWan({
                  probe_interval_secs: Math.max(1, Number(e.currentTarget.value) || 5),
                })
              }
              class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 font-mono text-xs"
            />
          </label>
          <label class="block text-xs">
            <span class="text-slate-400">探测超时 (ms)</span>
            <input
              type="number"
              min="200"
              value={props.draft.multi_wan.probe_timeout_ms}
              onInput={(e) =>
                updateMultiWan({
                  probe_timeout_ms: Math.max(200, Number(e.currentTarget.value) || 2000),
                })
              }
              class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 font-mono text-xs"
            />
          </label>
          <label class="block text-xs sm:col-span-2">
            <span class="text-slate-400">全局探测目标 (逗号分隔 IP 或 URL)</span>
            <input
              type="text"
              value={props.draft.multi_wan.probe_targets.join(", ")}
              onInput={(e) =>
                updateMultiWan({
                  probe_targets: e.currentTarget.value
                    .split(",")
                    .map((s) => s.trim())
                    .filter(Boolean),
                })
              }
              class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 font-mono text-xs"
            />
          </label>
          <label class="block text-xs">
            <span class="text-slate-400">离线/恢复阈值 (次)</span>
            <div class="mt-1 flex gap-2">
              <input
                type="number"
                min="1"
                value={props.draft.multi_wan.fail_threshold}
                onInput={(e) =>
                  updateMultiWan({
                    fail_threshold: Math.max(1, Number(e.currentTarget.value) || 3),
                  })
                }
                title="连续失败次数判定离线"
                class="w-1/2 rounded-lg border border-slate-700 bg-slate-900 px-2 py-1.5 font-mono text-xs"
              />
              <input
                type="number"
                min="1"
                value={props.draft.multi_wan.recover_threshold}
                onInput={(e) =>
                  updateMultiWan({
                    recover_threshold: Math.max(1, Number(e.currentTarget.value) || 2),
                  })
                }
                title="连续成功次数判定恢复"
                class="w-1/2 rounded-lg border border-slate-700 bg-slate-900 px-2 py-1.5 font-mono text-xs"
              />
            </div>
          </label>
          <div class="flex flex-col justify-end gap-1.5 text-xs text-slate-300">
            <label class="flex items-center gap-2">
              <input
                type="checkbox"
                checked={props.draft.multi_wan.sticky_sessions}
                onChange={(e) => updateMultiWan({ sticky_sessions: e.currentTarget.checked })}
              />
              源 IP 会话粘滞 (Sticky)
            </label>
            <label class="flex items-center gap-2">
              <input
                type="checkbox"
                checked={props.draft.multi_wan.flush_conntrack_on_failover}
                onChange={(e) =>
                  updateMultiWan({ flush_conntrack_on_failover: e.currentTarget.checked })
                }
              />
              切换时精确清理 conntrack
            </label>
          </div>
        </div>

        <Show when={multiWanStatus()}>
          {(mw) => (
            <div class="mt-4 rounded-xl border border-slate-800 bg-slate-950/60 p-3 text-xs">
              <div class="mb-2 flex flex-wrap items-center justify-between gap-2 text-slate-400">
                <span>
                  当前活跃上行:{" "}
                  <span class="font-mono text-emerald-300">
                    {mw().active_wans.length > 0
                      ? mw().active_wans.join(", ")
                      : "未启用 / 等待探测"}
                  </span>
                </span>
                <span class="font-mono">
                  切换次数: {mw().failover_count}
                  {mw().last_failover_at ? ` (最近: ${mw().last_failover_at})` : ""}
                </span>
              </div>
              <Show when={mw().wans.length > 0}>
                <div class="grid gap-2 sm:grid-cols-2">
                  <For each={mw().wans}>
                    {(item) => (
                      <div class="flex items-center justify-between rounded-lg border border-slate-800 bg-slate-900/70 px-3 py-2">
                        <div>
                          <div class="flex items-center gap-2">
                            <span class="font-mono font-semibold text-slate-200">
                              {item.wan_name}
                            </span>
                            <span
                              class={`rounded-full px-2 py-0.5 text-[10px] font-medium ${healthBadgeClass(item.health)}`}
                            >
                              {item.health}
                            </span>
                            <Show when={item.active}>
                              <span class="rounded bg-sky-500/20 px-1.5 py-0.5 text-[10px] text-sky-300">
                                active
                              </span>
                            </Show>
                          </div>
                          <div class="mt-0.5 font-mono text-[11px] text-slate-400">
                            {item.device} · table {item.table_id} · mark 0x
                            {item.fwmark.toString(16)} · eff_weight {item.effective_weight}/
                            {item.weight}
                          </div>
                        </div>
                        <div class="text-right font-mono text-[11px] text-slate-300">
                          <div>
                            {typeof item.rtt_ms === "number" ? `${item.rtt_ms.toFixed(1)} ms` : "-"}
                          </div>
                          <div class="text-slate-500">loss {item.packet_loss_pct}%</div>
                        </div>
                      </div>
                    )}
                  </For>
                </div>
              </Show>
            </div>
          )}
        </Show>
      </section>

      {/* eBPF / TCX Domain Policy & Traffic Stats */}
      <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
        <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
          <div>
            <h2 class="text-sm font-semibold text-slate-200">
              eBPF / TCX 域名策略分流与精细化统计
            </h2>
            <p class="mt-0.5 text-xs text-slate-400">
              基于 aya 加载内核 tc/tcx 分类器，将 crabdns 解析结果写入 LPM Trie 实现 skb-&gt;mark
              分流，并通过 PerCpuHashMap 采集客户端/域名维度实时流量。
            </p>
          </div>
          <div class="flex items-center gap-3">
            <label class="flex items-center gap-2 text-xs text-slate-300">
              <input
                type="checkbox"
                checked={props.draft.ebpf.enabled}
                onChange={(e) => updateEbpf({ enabled: e.currentTarget.checked })}
              />
              启用 eBPF TCX
            </label>
            <button
              type="button"
              onClick={addEbpfRule}
              class="rounded-lg border border-sky-500/40 bg-sky-500/15 px-3 py-1.5 text-xs font-medium text-sky-300 hover:bg-sky-500/25"
            >
              + 添加域名规则
            </button>
            <button
              type="button"
              onClick={() => void refetchEbpf()}
              class="rounded-lg border border-slate-700 bg-slate-800 px-2.5 py-1.5 text-xs text-slate-300 hover:bg-slate-700"
            >
              刷新状态
            </button>
          </div>
        </div>

        <label class="mb-3 block text-xs">
          <span class="text-slate-400">挂载接口 (逗号分隔，留空默认自动挂载到全部 LAN Bridge)</span>
          <input
            type="text"
            value={props.draft.ebpf.attach_interfaces.join(", ")}
            placeholder="例如: br-lan"
            onInput={(e) =>
              updateEbpf({
                attach_interfaces: e.currentTarget.value
                  .split(",")
                  .map((s) => s.trim())
                  .filter(Boolean),
              })
            }
            class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 font-mono text-xs"
          />
        </label>

        <div class="mb-3 grid gap-3 rounded-lg border border-slate-800 bg-slate-950/40 p-3 sm:grid-cols-3">
          <label class="flex items-center gap-2 text-xs text-slate-300">
            <input
              type="checkbox"
              checked={props.draft.ebpf.xdp_guard_enabled}
              onChange={(e) => updateEbpf({ xdp_guard_enabled: e.currentTarget.checked })}
            />
            启用 WAN 边缘 XDP 防护 (黑名单 / SYN 限速)
          </label>
          <label class="flex items-center gap-2 text-xs text-slate-300 sm:col-span-2">
            <input
              type="checkbox"
              checked={props.draft.ebpf.fast_forward_enabled}
              onChange={(e) => updateEbpf({ fast_forward_enabled: e.currentTarget.checked })}
            />
            启用 eBPF L3 FIB 路由快转加速 (IPv4 / IPv6 直通转发)
          </label>
          <label class="block text-xs">
            <span class="text-slate-400">XDP 挂载口 (留空默认全部 WAN)</span>
            <input
              type="text"
              value={props.draft.ebpf.xdp_interfaces.join(", ")}
              placeholder="例如: eth0"
              onInput={(e) =>
                updateEbpf({
                  xdp_interfaces: e.currentTarget.value
                    .split(",")
                    .map((s) => s.trim())
                    .filter(Boolean),
                })
              }
              class="mt-1 w-full rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs"
            />
          </label>
          <label class="block text-xs">
            <span class="text-slate-400">单源 IP SYN 限速 (pkt/s, 0=关闭)</span>
            <input
              type="number"
              min="0"
              value={props.draft.ebpf.xdp_syn_rate_limit}
              onInput={(e) =>
                updateEbpf({ xdp_syn_rate_limit: Math.max(0, Number(e.currentTarget.value) || 0) })
              }
              class="mt-1 w-full rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs"
            />
          </label>
          <label class="block text-xs sm:col-span-3">
            <span class="text-slate-400">
              XDP 黑名单 IPv4 / IPv6 / CIDR (逗号分隔，线速 XDP_DROP)
            </span>
            <input
              type="text"
              value={props.draft.ebpf.xdp_blocklist.join(", ")}
              placeholder="例如: 198.51.100.0/24, 2001:db8::/32"
              onInput={(e) =>
                updateEbpf({
                  xdp_blocklist: e.currentTarget.value
                    .split(",")
                    .map((s) => s.trim())
                    .filter(Boolean),
                })
              }
              class="mt-1 w-full rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs"
            />
          </label>
        </div>

        <div class="space-y-2">
          <For each={props.draft.ebpf.domain_rules}>
            {(rule, idx) => (
              <div class="grid items-center gap-2 rounded-lg border border-slate-800 bg-slate-950/60 p-2.5 sm:grid-cols-4">
                <input
                  type="text"
                  value={rule.domain}
                  placeholder="*.example.com"
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    const item = next.ebpf.domain_rules[idx()];
                    if (item) {
                      item.domain = e.currentTarget.value;
                      props.onChange(next);
                    }
                  }}
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs"
                />
                <select
                  value={rule.wan_name}
                  onChange={(e) => {
                    const next = structuredClone(props.draft);
                    const item = next.ebpf.domain_rules[idx()];
                    if (item) {
                      item.wan_name = e.currentTarget.value;
                      props.onChange(next);
                    }
                  }}
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1 text-xs"
                >
                  <For each={props.draft.wans}>
                    {(wan) => <option value={wan.name}>WAN: {wan.name}</option>}
                  </For>
                </select>
                <input
                  type="number"
                  min="1"
                  value={rule.tag_id}
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    const item = next.ebpf.domain_rules[idx()];
                    if (item) {
                      item.tag_id = Math.max(1, Number(e.currentTarget.value) || 1);
                      props.onChange(next);
                    }
                  }}
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs"
                />
                <button
                  type="button"
                  onClick={() => {
                    const next = structuredClone(props.draft);
                    next.ebpf.domain_rules.splice(idx(), 1);
                    props.onChange(next);
                  }}
                  class="rounded border border-rose-500/40 bg-rose-500/10 px-2 py-1 text-xs text-rose-300 hover:bg-rose-500/20"
                >
                  删除
                </button>
              </div>
            )}
          </For>
        </div>

        <Show when={ebpfStatus()}>
          {(st) => (
            <div class="mt-3 rounded-xl border border-slate-800 bg-slate-950/60 p-3 text-xs">
              <div class="flex flex-wrap items-center justify-between gap-2 text-slate-400">
                <span>
                  挂载接口:{" "}
                  <span class="font-mono text-slate-200">
                    {st().attached_interfaces.length > 0
                      ? st().attached_interfaces.join(", ")
                      : "未挂载"}
                  </span>
                </span>
                <span class="font-mono">
                  IPv4 LPM: {st().active_lpm_entries} · IPv6 LPM: {st().active_lpm_v6_entries} ·
                  快转包: {st().fast_forward_packets} · XDP 挂载:{" "}
                  {st().xdp_attached_interfaces.join(", ") || "未启用"} · XDP 丢弃: 黑名单{" "}
                  {st().xdp_blocklist_drops} / SYN {st().xdp_syn_drops}
                </span>
              </div>
              <Show when={st().last_error}>
                {(err) => <div class="mt-1 font-mono text-rose-400">错误: {err()}</div>}
              </Show>
              <Show when={st().stats.length > 0}>
                <div class="mt-2 overflow-x-auto">
                  <table class="w-full text-left font-mono text-[11px]">
                    <thead class="text-slate-500">
                      <tr>
                        <th class="py-1">客户端 IP</th>
                        <th class="py-1">Tag / 域名</th>
                        <th class="py-1">目标 WAN</th>
                        <th class="py-1 text-right">上行 (TX)</th>
                        <th class="py-1 text-right">下行 (RX)</th>
                      </tr>
                    </thead>
                    <tbody class="divide-y divide-slate-800/70 text-slate-300">
                      <For each={st().stats.slice(0, 20)}>
                        {(row) => (
                          <tr>
                            <td class="py-1">{row.client_ip}</td>
                            <td class="py-1">
                              #{row.tag_id} {row.domain_label ?? "未分类"}
                            </td>
                            <td class="py-1">{row.wan_name ?? "-"}</td>
                            <td class="py-1 text-right">
                              {row.tx_bytes} B ({row.tx_packets} pkts)
                            </td>
                            <td class="py-1 text-right">
                              {row.rx_bytes} B ({row.rx_packets} pkts)
                            </td>
                          </tr>
                        )}
                      </For>
                    </tbody>
                  </table>
                </div>
              </Show>
            </div>
          )}
        </Show>
      </section>

      {/* DDNS & ACME Dynamic TLS */}
      <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
        <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
          <div>
            <h2 class="text-sm font-semibold text-slate-200">DDNS 动态域名与 ACME 证书</h2>
            <p class="mt-0.5 text-xs text-slate-400">
              支持 Cloudflare / DynDNS2 / Webhook 动态域名更新，以及 Let&apos;s Encrypt ACME 动态
              TLS 证书签发与续期。
            </p>
          </div>
          <div class="flex items-center gap-2">
            <button
              type="button"
              onClick={() => void triggerDdnsSync().then(setDdnsStatus)}
              class="rounded-lg border border-sky-500/40 bg-sky-500/15 px-3 py-1.5 text-xs font-medium text-sky-300 hover:bg-sky-500/25"
            >
              立即同步 DDNS
            </button>
            <button
              type="button"
              onClick={() => void triggerTlsRenew().then(setTlsStatus)}
              class="rounded-lg border border-emerald-500/40 bg-emerald-500/15 px-3 py-1.5 text-xs font-medium text-emerald-300 hover:bg-emerald-500/25"
            >
              签发 / 续期证书
            </button>
          </div>
        </div>

        <div class="grid gap-4 sm:grid-cols-2">
          <div class="space-y-2 rounded-lg border border-slate-800 bg-slate-950/50 p-3">
            <div class="flex items-center justify-between">
              <span class="text-xs font-semibold text-slate-300">DDNS 配置</span>
              <label class="flex items-center gap-2 text-xs text-slate-300">
                <input
                  type="checkbox"
                  checked={props.draft.ddns.enabled}
                  onChange={(e) => updateDdns({ enabled: e.currentTarget.checked })}
                />
                启用
              </label>
            </div>
            <div class="grid gap-2 sm:grid-cols-2">
              <select
                value={props.draft.ddns.provider}
                onChange={(e) => updateDdns({ provider: e.currentTarget.value as DdnsProvider })}
                class="rounded border border-slate-700 bg-slate-900 px-2 py-1 text-xs"
              >
                <option value="cloudflare">Cloudflare</option>
                <option value="dyn_dns2">DynDNS2</option>
                <option value="webhook">Webhook</option>
              </select>
              <input
                type="text"
                value={props.draft.ddns.domain}
                placeholder="域名 (如 gw.example.com)"
                onInput={(e) => updateDdns({ domain: e.currentTarget.value })}
                class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs"
              />
              <input
                type="text"
                value={props.draft.ddns.token ?? ""}
                placeholder="API Token / 密码"
                onInput={(e) => updateDdns({ token: e.currentTarget.value || null })}
                class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs"
              />
              <input
                type="text"
                value={props.draft.ddns.zone_id ?? ""}
                placeholder="Zone ID (Cloudflare)"
                onInput={(e) => updateDdns({ zone_id: e.currentTarget.value || null })}
                class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs"
              />
            </div>
            <Show when={ddnsStatus()}>
              {(ds) => (
                <div class="font-mono text-[11px] text-slate-400">
                  上次 IP: {ds().last_ip ?? "-"} · 同步: {ds().last_synced_at ?? "-"}
                  {ds().last_error ? ` · 错误: ${ds().last_error}` : ""}
                </div>
              )}
            </Show>
          </div>

          <div class="space-y-2 rounded-lg border border-slate-800 bg-slate-950/50 p-3">
            <div class="flex items-center justify-between">
              <span class="text-xs font-semibold text-slate-300">ACME 动态 TLS</span>
              <label class="flex items-center gap-2 text-xs text-slate-300">
                <input
                  type="checkbox"
                  checked={props.draft.acme.enabled}
                  onChange={(e) => updateAcme({ enabled: e.currentTarget.checked })}
                />
                启用
              </label>
            </div>
            <div class="grid gap-2 sm:grid-cols-2">
              <input
                type="text"
                value={props.draft.acme.domain}
                placeholder="证书域名 (gw.example.com)"
                onInput={(e) => updateAcme({ domain: e.currentTarget.value })}
                class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs"
              />
              <input
                type="email"
                value={props.draft.acme.email}
                placeholder="通知邮箱 (ops@example.com)"
                onInput={(e) => updateAcme({ email: e.currentTarget.value })}
                class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs"
              />
              <select
                value={props.draft.acme.challenge_type}
                onChange={(e) =>
                  updateAcme({ challenge_type: e.currentTarget.value as AcmeChallengeType })
                }
                class="rounded border border-slate-700 bg-slate-900 px-2 py-1 text-xs"
              >
                <option value="http01">HTTP-01 (Standalone)</option>
                <option value="dns_cloudflare">DNS-01 (Cloudflare)</option>
              </select>
              <input
                type="number"
                min="1"
                value={props.draft.acme.renew_before_days}
                onInput={(e) =>
                  updateAcme({
                    renew_before_days: Math.max(1, Number(e.currentTarget.value) || 30),
                  })
                }
                class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs"
              />
            </div>
            <Show when={tlsStatus()}>
              {(ts) => (
                <div class="font-mono text-[11px] text-slate-400">
                  证书: {ts().cert_path ?? "自签/未配置"} · 到期: {ts().expires_at ?? "-"}
                  {ts().last_error ? ` · 错误: ${ts().last_error}` : ""}
                </div>
              )}
            </Show>
          </div>
        </div>
      </section>

      {/* LAN Bridge Parameters */}
      <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
        <h2 class="mb-3 text-sm font-semibold text-slate-200">LAN 网桥与 DHCP 地址池</h2>
        <div class="space-y-4">
          <For each={props.draft.lans}>
            {(lan, idx) => (
              <div class="grid gap-3 rounded-xl border border-slate-800 bg-slate-950/60 p-4 sm:grid-cols-3">
                <label class="block text-xs">
                  <span class="text-slate-400">Bridge 设备名</span>
                  <input
                    type="text"
                    value={lan.bridge}
                    onInput={(e) => updateLan(idx(), { bridge: e.currentTarget.value })}
                    class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 font-mono text-xs"
                  />
                </label>
                <label class="block text-xs">
                  <span class="text-slate-400">LAN CIDR</span>
                  <input
                    type="text"
                    value={lan.cidr}
                    onInput={(e) => updateLan(idx(), { cidr: e.currentTarget.value })}
                    class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 font-mono text-xs"
                  />
                </label>
                <label class="block text-xs">
                  <span class="text-slate-400">本地域名后缀</span>
                  <input
                    type="text"
                    value={lan.domain}
                    onInput={(e) => updateLan(idx(), { domain: e.currentTarget.value })}
                    class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 font-mono text-xs"
                  />
                </label>
                <label class="block text-xs">
                  <span class="text-slate-400">DHCP 起始偏移</span>
                  <input
                    type="number"
                    value={lan.dhcp_start_offset}
                    onInput={(e) =>
                      updateLan(idx(), {
                        dhcp_start_offset: Number(e.currentTarget.value) || 100,
                      })
                    }
                    class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 font-mono text-xs"
                  />
                </label>
                <label class="block text-xs">
                  <span class="text-slate-400">DHCP 地址数</span>
                  <input
                    type="number"
                    value={lan.dhcp_limit}
                    onInput={(e) =>
                      updateLan(idx(), { dhcp_limit: Number(e.currentTarget.value) || 150 })
                    }
                    class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 font-mono text-xs"
                  />
                </label>
                <label class="flex items-center gap-2 pt-5 text-xs text-slate-300">
                  <input
                    type="checkbox"
                    checked={lan.dhcp_enabled}
                    onChange={(e) => updateLan(idx(), { dhcp_enabled: e.currentTarget.checked })}
                  />
                  启用 dnsmasq DHCPv4 服务
                </label>
                <label class="flex items-center gap-2 pt-5 text-xs text-slate-300">
                  <input
                    type="checkbox"
                    checked={lan.ipv6_ra_enabled}
                    onChange={(e) => updateLan(idx(), { ipv6_ra_enabled: e.currentTarget.checked })}
                  />
                  启用 IPv6 RA + /64 SLAAC 下发
                </label>
                <label class="block text-xs">
                  <span class="text-slate-400">IPv6-PD /64 子网序号 (可选 SLA ID)</span>
                  <input
                    type="number"
                    min={0}
                    max={255}
                    placeholder="自动分配 (0, 1...)"
                    value={lan.ipv6_subnet_id ?? ""}
                    onInput={(e) => {
                      const val = e.currentTarget.value.trim();
                      updateLan(idx(), {
                        ipv6_subnet_id: val === "" ? null : Number(val),
                      });
                    }}
                    class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 font-mono text-xs"
                  />
                </label>
                <label class="block text-xs">
                  <span class="text-slate-400">IPv6 ULA 静态前缀 (可选, 如 fd00:10::1/64)</span>
                  <input
                    type="text"
                    placeholder="留空仅使用 WAN PD 前缀"
                    value={lan.ipv6_ula_cidr ?? ""}
                    onInput={(e) =>
                      updateLan(idx(), { ipv6_ula_cidr: e.currentTarget.value.trim() || null })
                    }
                    class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 font-mono text-xs"
                  />
                </label>
                <label class="flex items-center gap-2 pt-5 text-xs text-slate-300">
                  <input
                    type="checkbox"
                    checked={lan.stp_enabled}
                    onChange={(e) => updateLan(idx(), { stp_enabled: e.currentTarget.checked })}
                  />
                  启用 Bridge STP 生成树防环
                </label>
                <label class="block text-xs">
                  <span class="text-slate-400">STP Forward Delay (cs, 0=默认)</span>
                  <input
                    type="number"
                    min={0}
                    value={lan.forward_delay_cs}
                    onInput={(e) =>
                      updateLan(idx(), {
                        forward_delay_cs: Math.max(0, Number(e.currentTarget.value) || 0),
                      })
                    }
                    class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 font-mono text-xs"
                  />
                </label>
              </div>
            )}
          </For>
        </div>
      </section>

      {/* Link Aggregation (Bonding / LACP) */}
      <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
        <div class="mb-3 flex items-center justify-between">
          <div>
            <h2 class="text-sm font-semibold text-slate-200">链路聚合 (Bonding / 802.3ad LACP)</h2>
            <p class="mt-0.5 text-xs text-slate-400">
              将多个物理网卡聚合为逻辑接口 (如 bond0)，再接入 LAN Bridge、VLAN 或作为 WAN 设备
            </p>
          </div>
          <button
            type="button"
            onClick={addBond}
            class="rounded-lg border border-slate-700 bg-slate-800 px-3 py-1 text-xs text-slate-200 hover:bg-slate-700"
          >
            + 添加聚合组
          </button>
        </div>
        <div class="space-y-3">
          <For
            each={props.draft.bonds}
            fallback={
              <p class="text-xs text-slate-500">暂无链路聚合配置，点击右上角添加 bond 接口。</p>
            }
          >
            {(bond, idx) => (
              <div class="grid gap-3 rounded-xl border border-slate-800 bg-slate-950/60 p-3 sm:grid-cols-4">
                <label class="block text-xs">
                  <span class="text-slate-400">Bond 接口名</span>
                  <input
                    type="text"
                    value={bond.name}
                    placeholder="例如: bond0"
                    onInput={(e) => updateBond(idx(), { name: e.currentTarget.value.trim() })}
                    class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 font-mono text-xs"
                  />
                </label>
                <label class="block text-xs">
                  <span class="text-slate-400">聚合模式</span>
                  <select
                    value={bond.mode}
                    onChange={(e) => updateBond(idx(), { mode: e.currentTarget.value as BondMode })}
                    class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-xs"
                  >
                    <option value="802.3ad">802.3ad (LACP)</option>
                    <option value="active-backup">active-backup (主备)</option>
                    <option value="balance-xor">balance-xor</option>
                    <option value="balance-rr">balance-rr (轮询)</option>
                    <option value="broadcast">broadcast</option>
                    <option value="balance-tlb">balance-tlb</option>
                    <option value="balance-alb">balance-alb</option>
                  </select>
                </label>
                <label class="block text-xs">
                  <span class="text-slate-400">哈希策略</span>
                  <select
                    value={bond.xmit_hash_policy}
                    onChange={(e) =>
                      updateBond(idx(), {
                        xmit_hash_policy: e.currentTarget.value as BondXmitHashPolicy,
                      })
                    }
                    class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-xs"
                  >
                    <option value="layer2+3">layer2+3</option>
                    <option value="layer3+4">layer3+4</option>
                    <option value="layer2">layer2</option>
                    <option value="encap2+3">encap2+3</option>
                    <option value="encap3+4">encap3+4</option>
                  </select>
                </label>
                <label class="block text-xs">
                  <span class="text-slate-400">LACP 速率</span>
                  <select
                    value={bond.lacp_rate}
                    onChange={(e) =>
                      updateBond(idx(), { lacp_rate: e.currentTarget.value as BondLacpRate })
                    }
                    class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 text-xs"
                  >
                    <option value="slow">slow (30s)</option>
                    <option value="fast">fast (1s)</option>
                  </select>
                </label>
                <label class="block text-xs sm:col-span-2">
                  <span class="text-slate-400">成员接口 (逗号分隔，如 eth1, eth2)</span>
                  <input
                    type="text"
                    value={bond.slaves.join(", ")}
                    placeholder="eth1, eth2"
                    onInput={(e) =>
                      updateBond(idx(), {
                        slaves: e.currentTarget.value
                          .split(",")
                          .map((s) => s.trim())
                          .filter(Boolean),
                      })
                    }
                    class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2.5 py-1.5 font-mono text-xs"
                  />
                </label>
                <label class="block text-xs">
                  <span class="text-slate-400">MII 监测 (ms) / MTU (可选)</span>
                  <div class="mt-1 flex gap-2">
                    <input
                      type="number"
                      min={10}
                      value={bond.miimon_ms}
                      onInput={(e) =>
                        updateBond(idx(), { miimon_ms: Number(e.currentTarget.value) || 100 })
                      }
                      class="w-1/2 rounded-lg border border-slate-700 bg-slate-900 px-2 py-1.5 font-mono text-xs"
                    />
                    <input
                      type="number"
                      min={576}
                      max={9216}
                      placeholder="MTU"
                      value={bond.mtu ?? ""}
                      onInput={(e) => {
                        const val = e.currentTarget.value.trim();
                        updateBond(idx(), { mtu: val === "" ? null : Number(val) });
                      }}
                      class="w-1/2 rounded-lg border border-slate-700 bg-slate-900 px-2 py-1.5 font-mono text-xs"
                    />
                  </div>
                </label>
                <div class="flex items-end justify-between gap-2">
                  <label class="block flex-1 text-xs">
                    <span class="text-slate-400">主备 Primary (可选)</span>
                    <input
                      type="text"
                      placeholder="eth1"
                      value={bond.primary ?? ""}
                      onInput={(e) =>
                        updateBond(idx(), { primary: e.currentTarget.value.trim() || null })
                      }
                      class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-900 px-2 py-1.5 font-mono text-xs"
                    />
                  </label>
                  <button
                    type="button"
                    onClick={() => removeBond(idx())}
                    class="rounded-lg bg-rose-900/50 px-2.5 py-1.5 text-xs text-rose-200 hover:bg-rose-800/70"
                  >
                    删除
                  </button>
                </div>
              </div>
            )}
          </For>
        </div>
      </section>

      {/* 802.1Q VLAN */}
      <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
        <div class="mb-3 flex items-center justify-between">
          <h2 class="text-sm font-semibold text-slate-200">802.1Q VLAN 子接口</h2>
          <button
            type="button"
            onClick={addVlan}
            class="rounded-lg border border-slate-700 bg-slate-800 px-3 py-1 text-xs hover:bg-slate-700"
          >
            + 添加 VLAN
          </button>
        </div>
        <div class="space-y-2">
          <For each={props.draft.vlans}>
            {(vlan, idx) => (
              <div class="grid items-center gap-2 rounded-lg border border-slate-800 bg-slate-950/60 p-3 sm:grid-cols-7">
                <input
                  type="text"
                  value={vlan.parent}
                  placeholder="父接口 (如 eth1)"
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.vlans[idx()].parent = e.currentTarget.value;
                    props.onChange(next);
                  }}
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs"
                />
                <input
                  type="number"
                  value={vlan.id}
                  min={1}
                  max={4094}
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.vlans[idx()].id = Number(e.currentTarget.value) || 1;
                    props.onChange(next);
                  }}
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs"
                />
                <select
                  value={vlan.role}
                  onChange={(e) => {
                    const next = structuredClone(props.draft);
                    next.vlans[idx()].role = e.currentTarget.value as InterfaceRole;
                    props.onChange(next);
                  }}
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1 text-xs"
                >
                  <option value="lan">LAN</option>
                  <option value="wan">WAN</option>
                  <option value="disabled">Disabled</option>
                </select>
                <input
                  type="text"
                  value={vlan.cidr ?? ""}
                  placeholder="可选 IPv4 CIDR"
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.vlans[idx()].cidr = e.currentTarget.value || null;
                    props.onChange(next);
                  }}
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs"
                />
                <label class="flex items-center gap-1.5 text-xs text-slate-300">
                  <input
                    type="checkbox"
                    checked={vlan.ipv6_ra_enabled}
                    onChange={(e) => {
                      const next = structuredClone(props.draft);
                      next.vlans[idx()].ipv6_ra_enabled = e.currentTarget.checked;
                      props.onChange(next);
                    }}
                  />
                  <span>IPv6 /64 RA</span>
                </label>
                <input
                  type="number"
                  min={0}
                  max={255}
                  value={vlan.ipv6_subnet_id ?? ""}
                  placeholder="IPv6 子网号"
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    const val = e.currentTarget.value.trim();
                    next.vlans[idx()].ipv6_subnet_id = val === "" ? null : Number(val);
                    props.onChange(next);
                  }}
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs"
                />
                <button
                  type="button"
                  onClick={() => {
                    const next = structuredClone(props.draft);
                    next.vlans.splice(idx(), 1);
                    props.onChange(next);
                  }}
                  class="text-right text-xs text-red-400 hover:text-red-300"
                >
                  删除
                </button>
              </div>
            )}
          </For>
        </div>
      </section>

      {/* MAC Pinning */}
      <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
        <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
          <div>
            <h2 class="text-sm font-semibold text-slate-200">
              网卡 MAC 固化命名（systemd.link / udev）
            </h2>
            <p class="text-xs text-slate-400">防止热插拔或内核枚举顺序变化导致网口名漂移。</p>
          </div>
          <div class="flex gap-2">
            <Show when={props.interfaces.length > 0}>
              <button
                type="button"
                onClick={() => {
                  const first = props.interfaces.find(
                    (i) =>
                      i.mac &&
                      !props.draft.interface_pins.some(
                        (p) => p.mac.toLowerCase() === i.mac?.toLowerCase(),
                      ),
                  );
                  if (first?.mac) {
                    addPin(first.mac, first.name);
                  } else {
                    addPin();
                  }
                }}
                class="rounded-lg border border-slate-700 bg-slate-800 px-3 py-1 text-xs hover:bg-slate-700"
              >
                + 从主机网卡导入 / 新增
              </button>
            </Show>
          </div>
        </div>
        <div class="space-y-2">
          <For each={props.draft.interface_pins}>
            {(pin, idx) => (
              <div class="grid items-center gap-2 rounded-lg border border-slate-800 bg-slate-950/60 p-3 sm:grid-cols-3">
                <input
                  type="text"
                  value={pin.mac}
                  placeholder="aa:bb:cc:dd:ee:ff"
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.interface_pins[idx()].mac = e.currentTarget.value;
                    props.onChange(next);
                  }}
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs"
                />
                <input
                  type="text"
                  value={pin.name}
                  placeholder="固定名称 (如 wan0)"
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.interface_pins[idx()].name = e.currentTarget.value;
                    props.onChange(next);
                  }}
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono text-xs"
                />
                <button
                  type="button"
                  onClick={() => {
                    const next = structuredClone(props.draft);
                    next.interface_pins.splice(idx(), 1);
                    props.onChange(next);
                  }}
                  class="text-right text-xs text-red-400 hover:text-red-300"
                >
                  删除
                </button>
              </div>
            )}
          </For>
        </div>
      </section>

      {/* Static Routes & PBR */}
      <section class="grid gap-4 md:grid-cols-2">
        <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
          <div class="mb-3 flex items-center justify-between">
            <h2 class="text-sm font-semibold text-slate-200">静态路由表</h2>
            <button
              type="button"
              onClick={addRoute}
              class="rounded-lg border border-slate-700 bg-slate-800 px-2.5 py-1 text-xs hover:bg-slate-700"
            >
              + 新增路由
            </button>
          </div>
          <div class="space-y-2">
            <For each={props.draft.routes}>
              {(route, idx) => (
                <div class="space-y-2 rounded-lg border border-slate-800 bg-slate-950/60 p-3 text-xs">
                  <div class="grid grid-cols-2 gap-2">
                    <input
                      type="text"
                      value={route.destination}
                      placeholder="目标网段"
                      onInput={(e) => {
                        const next = structuredClone(props.draft);
                        next.routes[idx()].destination = e.currentTarget.value;
                        props.onChange(next);
                      }}
                      class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono"
                    />
                    <input
                      type="text"
                      value={route.gateway ?? ""}
                      placeholder="下一跳网关"
                      onInput={(e) => {
                        const next = structuredClone(props.draft);
                        next.routes[idx()].gateway = e.currentTarget.value || null;
                        props.onChange(next);
                      }}
                      class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono"
                    />
                  </div>
                  <div class="flex items-center justify-between gap-2">
                    <input
                      type="number"
                      value={route.table_id}
                      placeholder="Table ID"
                      onInput={(e) => {
                        const next = structuredClone(props.draft);
                        next.routes[idx()].table_id = Number(e.currentTarget.value) || 254;
                        props.onChange(next);
                      }}
                      class="w-24 rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono"
                    />
                    <input
                      type="text"
                      value={route.device ?? ""}
                      placeholder="出接口"
                      onInput={(e) => {
                        const next = structuredClone(props.draft);
                        next.routes[idx()].device = e.currentTarget.value || null;
                        props.onChange(next);
                      }}
                      class="flex-1 rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono"
                    />
                    <button
                      type="button"
                      onClick={() => {
                        const next = structuredClone(props.draft);
                        next.routes.splice(idx(), 1);
                        props.onChange(next);
                      }}
                      class="text-red-400 hover:text-red-300"
                    >
                      删除
                    </button>
                  </div>
                </div>
              )}
            </For>
          </div>
        </div>

        <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
          <div class="mb-3 flex items-center justify-between">
            <h2 class="text-sm font-semibold text-slate-200">策略路由规则 (PBR)</h2>
            <button
              type="button"
              onClick={addPolicyRule}
              class="rounded-lg border border-slate-700 bg-slate-800 px-2.5 py-1 text-xs hover:bg-slate-700"
            >
              + 新增规则
            </button>
          </div>
          <div class="space-y-2">
            <For each={props.draft.policy_rules}>
              {(rule, idx) => (
                <div class="space-y-2 rounded-lg border border-slate-800 bg-slate-950/60 p-3 text-xs">
                  <div class="grid grid-cols-3 gap-2">
                    <input
                      type="number"
                      value={rule.priority}
                      placeholder="优先级"
                      onInput={(e) => {
                        const next = structuredClone(props.draft);
                        next.policy_rules[idx()].priority = Number(e.currentTarget.value) || 1000;
                        props.onChange(next);
                      }}
                      class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono"
                    />
                    <input
                      type="number"
                      value={rule.table_id}
                      placeholder="Table ID"
                      onInput={(e) => {
                        const next = structuredClone(props.draft);
                        next.policy_rules[idx()].table_id = Number(e.currentTarget.value) || 100;
                        props.onChange(next);
                      }}
                      class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono"
                    />
                    <input
                      type="text"
                      value={rule.iif ?? ""}
                      placeholder="入接口 (可选)"
                      onInput={(e) => {
                        const next = structuredClone(props.draft);
                        next.policy_rules[idx()].iif = e.currentTarget.value || null;
                        props.onChange(next);
                      }}
                      class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono"
                    />
                  </div>
                  <div class="flex items-center gap-2">
                    <input
                      type="text"
                      value={rule.src ?? ""}
                      placeholder="源网段 (可选)"
                      onInput={(e) => {
                        const next = structuredClone(props.draft);
                        next.policy_rules[idx()].src = e.currentTarget.value || null;
                        props.onChange(next);
                      }}
                      class="flex-1 rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono"
                    />
                    <button
                      type="button"
                      onClick={() => {
                        const next = structuredClone(props.draft);
                        next.policy_rules.splice(idx(), 1);
                        props.onChange(next);
                      }}
                      class="text-red-400 hover:text-red-300"
                    >
                      删除
                    </button>
                  </div>
                </div>
              )}
            </For>
          </div>
        </div>
      </section>
    </div>
  );
}

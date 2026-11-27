import { For, Show, createResource, createSignal } from "solid-js";
import { t } from "../i18n";
import { NicTuningSection } from "./NicTuningSection";
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
import type { IptvConfig } from "../types/IptvConfig";
import type { Ipv6LanMode } from "../types/Ipv6LanMode";
import type { LanConfig } from "../types/LanConfig";
import type { MdnsConfig } from "../types/MdnsConfig";
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

  function updateMdns(patch: Partial<MdnsConfig>) {
    const next = structuredClone(props.draft);
    next.mdns = { ...next.mdns, ...patch };
    props.onChange(next);
  }

  function updateIptv(patch: Partial<IptvConfig>) {
    const next = structuredClone(props.draft);
    next.iptv = { ...next.iptv, ...patch };
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
        return "border border-emerald-500/20 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400";
      case "degraded":
        return "border border-amber-500/20 bg-amber-500/10 text-amber-700 dark:text-amber-400";
      case "down":
        return "border border-rose-500/20 bg-rose-500/10 text-rose-700 dark:text-rose-600 dark:text-rose-400";
      default:
        return "border border-zinc-500/20 bg-zinc-500/10 text-zinc-600 dark:text-zinc-400";
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
      mac_override: null,
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
    <div class="space-y-5 sm:space-y-6 animate-view-enter">
      {/* Drag & Drop + Checkbox Bridge Port Assigner */}
      <section class="rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-colors hover:border-[#d4d4d8] dark:hover:border-[#3a3a3a]">
        <div class="mb-3">
          <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
            {t("netPortAssignerTitle")}
          </h2>
          <p class="mt-0.5 text-xs text-[#666666] dark:text-[#a1a1aa]">
            {t("netPortAssignerDesc")}
          </p>
        </div>

        <div class="grid gap-4 md:grid-cols-2">
          <div
            onDragOver={(e) => e.preventDefault()}
            onDrop={() => dropToUnassigned()}
            class="rounded-xl border border-dashed border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] p-3"
          >
            <div class="mb-2 text-xs font-medium text-[#666666] dark:text-[#a1a1aa]">
              {t("netUnassignedPool")}
            </div>
            <div class="flex flex-wrap gap-2">
              <For each={unassignedIfaces()}>
                {(name) => (
                  <div
                    draggable
                    onDragStart={() => setDraggedIface(name)}
                    class="cursor-grab rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] px-3 py-1.5 font-mono text-xs text-[#171717] dark:text-[#ededed] active:cursor-grabbing"
                  >
                    {name}
                  </div>
                )}
              </For>
              <For each={props.draft.wans}>
                {(wan) => (
                  <div class="rounded-lg border border-[#0070f3]/30 bg-[#0070f3]/10 px-3 py-1.5 font-mono text-xs text-[#0070f3] dark:text-sky-400">
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
                class="rounded-xl border border-sky-500/25 bg-sky-500/5 p-3"
              >
                <div class="mb-2 flex items-center justify-between text-xs">
                  <span class="font-mono font-semibold text-sky-600 dark:text-sky-400">
                    {lan.bridge} ({lan.name})
                  </span>
                  <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netDropHere")}</span>
                </div>
                <div class="mb-2 flex flex-wrap gap-2">
                  <For each={lan.ports}>
                    {(port) => (
                      <div
                        draggable
                        onDragStart={() => setDraggedIface(port)}
                        class="flex cursor-grab items-center gap-1.5 rounded-lg border border-sky-500/25 bg-sky-500/10 px-2.5 py-1 font-mono text-xs text-sky-700 dark:text-sky-600 dark:text-sky-400"
                      >
                        <span>{port}</span>
                        <button
                          type="button"
                          onClick={() => togglePortInLan(lanIdx(), port)}
                          class="text-sky-600 dark:text-sky-400 hover:text-[#171717] dark:hover:text-white bg-transparent border-0 cursor-pointer"
                          title={t("netRemoveFromBridge")}
                        >
                          ×
                        </button>
                      </div>
                    )}
                  </For>
                </div>
                <div class="flex flex-wrap gap-2 border-t border-[#eaeaea] dark:border-[#262626] pt-2">
                  <For each={allPhysicalNames()}>
                    {(iface) => (
                      <label class="flex items-center gap-1.5 text-xs text-[#171717] dark:text-[#ededed]">
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
      <section class="rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-colors hover:border-[#d4d4d8] dark:hover:border-[#3a3a3a]">
        <div class="mb-3 flex items-center justify-between">
          <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">{t("netWanTitle")}</h2>
          <button
            type="button"
            onClick={addWan}
            class="rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#111111] hover:bg-[#f4f4f5] dark:hover:bg-[#1a1a1a] px-3 py-1.5 text-xs font-medium text-[#171717] dark:text-[#ededed] cursor-pointer transition-colors"
          >
            {t("netAddWan")}
          </button>
        </div>

        <div class="space-y-4">
          <For each={props.draft.wans}>
            {(wan, idx) => {
              const runtime = () => props.wanStates.find((s) => s.wan_name === wan.name);
              return (
                <div class="rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] p-4">
                  <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
                    <div class="flex items-center gap-2">
                      <span class="font-mono text-sm font-semibold text-[#0070f3] dark:text-sky-400">
                        {wan.name}
                      </span>
                      <Show when={runtime()}>
                        {(st) => (
                          <span class="rounded border border-emerald-500/20 bg-emerald-500/10 px-2 py-0.5 text-[11px] text-emerald-700 dark:text-emerald-400">
                            {st().state} · {st().ip ?? t("netNoIp")}
                          </span>
                        )}
                      </Show>
                    </div>
                    <button
                      type="button"
                      onClick={() => removeWan(idx())}
                      class="text-xs text-rose-600 dark:text-rose-600 dark:text-rose-400 hover:text-rose-700 dark:hover:text-rose-300 bg-transparent border-0 cursor-pointer"
                    >
                      删除
                    </button>
                  </div>

                  <div class="grid gap-3 sm:grid-cols-3">
                    <label class="block text-xs">
                      <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netName")}</span>
                      <input
                        type="text"
                        value={wan.name}
                        onInput={(e) => updateWan(idx(), { name: e.currentTarget.value })}
                        class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 text-xs"
                      />
                    </label>
                    <label class="block text-xs">
                      <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netDevice")}</span>
                      <input
                        type="text"
                        value={wan.device}
                        onInput={(e) => updateWan(idx(), { device: e.currentTarget.value })}
                        class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 font-mono text-xs"
                      />
                    </label>
                    <label class="block text-xs">
                      <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netProto")}</span>
                      <select
                        value={wan.proto}
                        onChange={(e) =>
                          updateWan(idx(), { proto: e.currentTarget.value as WanProto })
                        }
                        class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 text-xs"
                      >
                        <option value="dhcp">DHCP</option>
                        <option value="pppoe">PPPoE</option>
                        <option value="static">Static</option>
                      </select>
                    </label>
                    <label class="block text-xs">
                      <span class="text-[#666666] dark:text-[#a1a1aa]">MTU</span>
                      <input
                        type="number"
                        value={wan.mtu}
                        onInput={(e) =>
                          updateWan(idx(), { mtu: Number(e.currentTarget.value) || 1500 })
                        }
                        class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 font-mono text-xs"
                      />
                    </label>
                    <label class="block text-xs">
                      <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netMetric")}</span>
                      <input
                        type="number"
                        value={wan.metric}
                        onInput={(e) =>
                          updateWan(idx(), { metric: Number(e.currentTarget.value) || 100 })
                        }
                        class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 font-mono text-xs"
                      />
                    </label>
                    <label class="block text-xs">
                      <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netWeight")}</span>
                      <input
                        type="number"
                        min="1"
                        value={wan.weight}
                        onInput={(e) =>
                          updateWan(idx(), {
                            weight: Math.max(1, Number(e.currentTarget.value) || 1),
                          })
                        }
                        class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 font-mono text-xs"
                      />
                    </label>
                    <label class="block text-xs">
                      <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netTableId")}</span>
                      <input
                        type="number"
                        value={wan.table_id ?? ""}
                        placeholder={`${t("netTableAuto")} (${101 + idx()})`}
                        onInput={(e) => {
                          const raw = e.currentTarget.value.trim();
                          updateWan(idx(), { table_id: raw ? Number(raw) : null });
                        }}
                        class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 font-mono text-xs"
                      />
                    </label>
                    <div class="flex flex-wrap items-center gap-3 pt-5 text-xs text-[#171717] dark:text-[#ededed]">
                      <label class="flex items-center gap-2">
                        <input
                          type="checkbox"
                          checked={wan.health_check_enabled}
                          onChange={(e) =>
                            updateWan(idx(), { health_check_enabled: e.currentTarget.checked })
                          }
                        />
                        {t("netHealthProbe")}
                      </label>
                      <label class="flex items-center gap-2">
                        <input
                          type="checkbox"
                          checked={wan.ipv6_pd}
                          onChange={(e) => updateWan(idx(), { ipv6_pd: e.currentTarget.checked })}
                        />
                        {t("netEnableIpv6Pd")}
                      </label>
                      <Show when={wan.ipv6_pd}>
                        <select
                          value={wan.ipv6_pd_prefix_len}
                          onChange={(e) =>
                            updateWan(idx(), {
                              ipv6_pd_prefix_len: Number(e.currentTarget.value) || 60,
                            })
                          }
                          class="rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono text-xs"
                          title="申请的 IPv6-PD 前缀长度提示"
                        >
                          <option value={56}>/56 (256 个 /64 子网)</option>
                          <option value={60}>/60 (16 个 /64 子网)</option>
                          <option value={64}>/64 (单子网)</option>
                          <option value={48}>/48 (企业级)</option>
                        </select>
                      </Show>
                    </div>
                    <label class="block text-xs">
                      <span class="text-[#666666] dark:text-[#a1a1aa]">MAC 地址克隆 (可选)</span>
                      <input
                        type="text"
                        placeholder="留空使用硬件 MAC (如 aa:bb:cc:dd:ee:ff)"
                        value={wan.mac_override ?? ""}
                        onInput={(e) => {
                          const val = e.currentTarget.value.trim();
                          updateWan(idx(), { mac_override: val === "" ? null : val });
                        }}
                        class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 font-mono text-xs"
                      />
                    </label>
                  </div>

                  <Show when={runtime()?.ipv6_pd_prefix}>
                    {(pd) => (
                      <div class="mt-3 rounded-lg border border-sky-500/25 bg-sky-500/10 p-2.5 text-xs">
                        <div class="flex flex-wrap items-center justify-between gap-2">
                          <span class="font-mono text-sky-600 dark:text-sky-400">
                            {t("netPdPrefixAcquired")}: {pd()}
                          </span>
                          <span class="font-mono text-[#666666] dark:text-[#a1a1aa]">
                            WAN IPv6: {runtime()?.ipv6_addr ?? "-"}
                          </span>
                        </div>
                        <Show when={(runtime()?.ipv6_slices.length ?? 0) > 0}>
                          <div class="mt-2 flex flex-wrap gap-1.5">
                            <For each={runtime()?.ipv6_slices}>
                              {(sl) => (
                                <span class="rounded border border-sky-500/25 bg-sky-500/10 px-2 py-0.5 font-mono text-[11px] text-sky-700 dark:text-sky-300">
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
                        <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netStaticCidr")}</span>
                        <input
                          type="text"
                          value={wan.static_cidr ?? ""}
                          onInput={(e) => updateWan(idx(), { static_cidr: e.currentTarget.value })}
                          class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 font-mono text-xs"
                        />
                      </label>
                      <label class="block text-xs">
                        <span class="text-[#666666] dark:text-[#a1a1aa]">{t("setupGateway")}</span>
                        <input
                          type="text"
                          value={wan.gateway ?? ""}
                          onInput={(e) =>
                            updateWan(idx(), { gateway: e.currentTarget.value || null })
                          }
                          class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 font-mono text-xs"
                        />
                      </label>
                    </div>
                  </Show>

                  <Show when={wan.proto === "pppoe"}>
                    <div class="mt-3 grid gap-3 sm:grid-cols-2">
                      <label class="block text-xs">
                        <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netPppoeUser")}</span>
                        <input
                          type="text"
                          value={wan.pppoe_username ?? ""}
                          onInput={(e) =>
                            updateWan(idx(), { pppoe_username: e.currentTarget.value })
                          }
                          class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 text-xs"
                        />
                      </label>
                      <label class="block text-xs">
                        <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netPppoePass")}</span>
                        <input
                          type="password"
                          value={wan.pppoe_password ?? ""}
                          onInput={(e) =>
                            updateWan(idx(), { pppoe_password: e.currentTarget.value })
                          }
                          class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 text-xs"
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
      <section class="rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-colors hover:border-[#d4d4d8] dark:hover:border-[#3a3a3a]">
        <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
          <div>
            <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
              {t("netMultiWanTitle")}
            </h2>
            <p class="mt-0.5 text-xs text-[#666666] dark:text-[#a1a1aa]">
              基于独立路由表、nftables ct mark 策略路由与 ICMP/HTTP
              {t("netHealthProbe")}，支持主备切换、按权重分流及故障切换精确清理 conntrack。
            </p>
          </div>
          <div class="flex items-center gap-3">
            <label class="flex items-center gap-2 text-xs text-[#171717] dark:text-[#ededed]">
              <input
                type="checkbox"
                checked={props.draft.multi_wan.enabled}
                onChange={(e) => updateMultiWan({ enabled: e.currentTarget.checked })}
              />
              {t("netMultiWanEnable")}
            </label>
            <button
              type="button"
              disabled={probing()}
              onClick={() => void runProbeNow()}
              class="rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-[#171717] hover:bg-[#383838] dark:bg-[#ededed] dark:hover:bg-[#d4d4d8] px-3 py-1.5 text-xs font-medium text-white dark:text-[#0a0a0a] cursor-pointer transition-colors disabled:opacity-50"
            >
              {probing() ? t("netProbing") : t("netProbeNow")}
            </button>
            <button
              type="button"
              onClick={() => void refetchMultiWan()}
              class="rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#111111] hover:bg-[#f4f4f5] dark:hover:bg-[#1a1a1a] px-2.5 py-1.5 text-xs font-medium text-[#171717] dark:text-[#ededed] cursor-pointer transition-colors"
            >
              刷新
            </button>
          </div>
        </div>

        <div class="grid gap-3 sm:grid-cols-4">
          <label class="block text-xs">
            <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netScheduleMode")}</span>
            <select
              value={props.draft.multi_wan.mode}
              onChange={(e) => updateMultiWan({ mode: e.currentTarget.value as MultiWanMode })}
              class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 text-xs"
            >
              <option value="failover">{t("netModeFailover")}</option>
              <option value="load_balance">{t("netModeLoadBalance")}</option>
            </select>
          </label>
          <label class="block text-xs">
            <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netProbeProto")}</span>
            <select
              value={props.draft.multi_wan.probe_protocol}
              onChange={(e) =>
                updateMultiWan({ probe_protocol: e.currentTarget.value as ProbeProtocol })
              }
              class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 text-xs"
            >
              <option value="icmp">ICMP (Ping)</option>
              <option value="http">HTTP (TCP/HEAD)</option>
              <option value="icmp_and_http">ICMP + HTTP 混合</option>
            </select>
          </label>
          <label class="block text-xs">
            <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netProbeInterval")}</span>
            <input
              type="number"
              min="1"
              value={props.draft.multi_wan.probe_interval_secs}
              onInput={(e) =>
                updateMultiWan({
                  probe_interval_secs: Math.max(1, Number(e.currentTarget.value) || 5),
                })
              }
              class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 font-mono text-xs"
            />
          </label>
          <label class="block text-xs">
            <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netProbeTimeout")}</span>
            <input
              type="number"
              min="200"
              value={props.draft.multi_wan.probe_timeout_ms}
              onInput={(e) =>
                updateMultiWan({
                  probe_timeout_ms: Math.max(200, Number(e.currentTarget.value) || 2000),
                })
              }
              class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 font-mono text-xs"
            />
          </label>
          <label class="block text-xs sm:col-span-2">
            <span class="text-[#666666] dark:text-[#a1a1aa]">
              全局探测目标 (逗号分隔 IP 或 URL)
            </span>
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
              class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 font-mono text-xs"
            />
          </label>
          <label class="block text-xs">
            <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netThresholds")}</span>
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
                class="w-1/2 rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1.5 font-mono text-xs"
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
                class="w-1/2 rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1.5 font-mono text-xs"
              />
            </div>
          </label>
          <div class="flex flex-col justify-end gap-1.5 text-xs text-[#171717] dark:text-[#ededed]">
            <label class="flex items-center gap-2">
              <input
                type="checkbox"
                checked={props.draft.multi_wan.sticky_sessions}
                onChange={(e) => updateMultiWan({ sticky_sessions: e.currentTarget.checked })}
              />
              {t("netStickySessions")}
            </label>
            <label class="flex items-center gap-2">
              <input
                type="checkbox"
                checked={props.draft.multi_wan.flush_conntrack_on_failover}
                onChange={(e) =>
                  updateMultiWan({ flush_conntrack_on_failover: e.currentTarget.checked })
                }
              />
              {t("netFlushConntrack")}
            </label>
          </div>
        </div>

        <Show when={multiWanStatus()}>
          {(mw) => (
            <div class="mt-4 rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] p-3 text-xs">
              <div class="mb-2 flex flex-wrap items-center justify-between gap-2 text-[#666666] dark:text-[#a1a1aa]">
                <span>
                  {t("netActiveUplinks")}:{" "}
                  <span class="font-mono text-emerald-700 dark:text-emerald-400">
                    {mw().active_wans.length > 0
                      ? mw().active_wans.join(", ")
                      : t("netPendingProbe")}
                  </span>
                </span>
                <span class="font-mono">
                  {t("netFailoverCount")}: {mw().failover_count}
                  {mw().last_failover_at
                    ? ` (${t("netLastFailover")}: ${mw().last_failover_at})`
                    : ""}
                </span>
              </div>
              <Show when={mw().wans.length > 0}>
                <div class="grid gap-2 sm:grid-cols-2">
                  <For each={mw().wans}>
                    {(item) => (
                      <div class="flex items-center justify-between rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] px-3 py-2">
                        <div>
                          <div class="flex items-center gap-2">
                            <span class="font-mono font-semibold text-[#171717] dark:text-[#ededed]">
                              {item.wan_name}
                            </span>
                            <span
                              class={`rounded-full px-2 py-0.5 text-[10px] font-medium ${healthBadgeClass(item.health)}`}
                            >
                              {item.health}
                            </span>
                            <Show when={item.active}>
                              <span class="rounded border border-sky-500/20 bg-sky-500/10 px-1.5 py-0.5 text-[10px] text-sky-600 dark:text-sky-400">
                                active
                              </span>
                            </Show>
                          </div>
                          <div class="mt-0.5 font-mono text-[11px] text-[#666666] dark:text-[#a1a1aa]">
                            {item.device} · table {item.table_id} · mark 0x
                            {item.fwmark.toString(16)} · eff_weight {item.effective_weight}/
                            {item.weight}
                          </div>
                        </div>
                        <div class="text-right font-mono text-[11px] text-[#171717] dark:text-[#ededed]">
                          <div>
                            {typeof item.rtt_ms === "number" ? `${item.rtt_ms.toFixed(1)} ms` : "-"}
                          </div>
                          <div class="text-[#8f8f8f] dark:text-[#71717a]">
                            loss {item.packet_loss_pct}%
                          </div>
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
      <section class="rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-colors hover:border-[#d4d4d8] dark:hover:border-[#3a3a3a]">
        <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
          <div>
            <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
              {t("netEbpfTitle")}
            </h2>
            <p class="mt-0.5 text-xs text-[#666666] dark:text-[#a1a1aa]">{t("netEbpfDesc")}</p>
          </div>
          <div class="flex items-center gap-3">
            <label class="flex items-center gap-2 text-xs text-[#171717] dark:text-[#ededed]">
              <input
                type="checkbox"
                checked={props.draft.ebpf.enabled}
                onChange={(e) => updateEbpf({ enabled: e.currentTarget.checked })}
              />
              {t("netEbpfEnable")}
            </label>
            <button
              type="button"
              onClick={addEbpfRule}
              class="rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-[#171717] hover:bg-[#383838] dark:bg-[#ededed] dark:hover:bg-[#d4d4d8] px-3 py-1.5 text-xs font-medium text-white dark:text-[#0a0a0a] cursor-pointer transition-colors"
            >
              {t("netAddDomainRule")}
            </button>
            <button
              type="button"
              onClick={() => void refetchEbpf()}
              class="rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#111111] hover:bg-[#f4f4f5] dark:hover:bg-[#1a1a1a] px-2.5 py-1.5 text-xs font-medium text-[#171717] dark:text-[#ededed] cursor-pointer transition-colors"
            >
              {t("netRefreshStatus")}
            </button>
          </div>
        </div>

        <label class="mb-3 block text-xs">
          <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netEbpfAttachIfaces")}</span>
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
            class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 font-mono text-xs"
          />
        </label>

        <div class="mb-3 grid gap-3 rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] p-3 sm:grid-cols-3">
          <label class="flex items-center gap-2 text-xs text-[#171717] dark:text-[#ededed]">
            <input
              type="checkbox"
              checked={props.draft.ebpf.xdp_guard_enabled}
              onChange={(e) => updateEbpf({ xdp_guard_enabled: e.currentTarget.checked })}
            />
            {t("netXdpGuard")}
          </label>
          <label class="flex items-center gap-2 text-xs text-[#171717] dark:text-[#ededed] sm:col-span-2">
            <input
              type="checkbox"
              checked={props.draft.ebpf.fast_forward_enabled}
              onChange={(e) => updateEbpf({ fast_forward_enabled: e.currentTarget.checked })}
            />
            {t("netFastForward")}
          </label>
          <label class="block text-xs">
            <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netXdpIfaces")}</span>
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
              class="mt-1 w-full rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono text-xs"
            />
          </label>
          <label class="block text-xs">
            <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netXdpSynLimit")}</span>
            <input
              type="number"
              min="0"
              value={props.draft.ebpf.xdp_syn_rate_limit}
              onInput={(e) =>
                updateEbpf({ xdp_syn_rate_limit: Math.max(0, Number(e.currentTarget.value) || 0) })
              }
              class="mt-1 w-full rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono text-xs"
            />
          </label>
          <label class="block text-xs sm:col-span-3">
            <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netXdpBlocklist")}</span>
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
              class="mt-1 w-full rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono text-xs"
            />
          </label>
        </div>

        <div class="space-y-2">
          <For each={props.draft.ebpf.domain_rules}>
            {(rule, idx) => (
              <div class="grid items-center gap-2 rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] p-2.5 sm:grid-cols-4">
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
                  class="rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono text-xs"
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
                  class="rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 text-xs"
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
                  class="rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono text-xs"
                />
                <button
                  type="button"
                  onClick={() => {
                    const next = structuredClone(props.draft);
                    next.ebpf.domain_rules.splice(idx(), 1);
                    props.onChange(next);
                  }}
                  class="rounded border border-rose-500/25 bg-rose-500/10 px-2 py-1 text-xs font-medium text-rose-600 dark:text-rose-600 dark:text-rose-400 hover:bg-rose-500/20 cursor-pointer"
                >
                  删除
                </button>
              </div>
            )}
          </For>
        </div>

        <Show when={ebpfStatus()}>
          {(st) => (
            <div class="mt-3 rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] p-3 text-xs">
              <div class="flex flex-wrap items-center justify-between gap-2 text-[#666666] dark:text-[#a1a1aa]">
                <span>
                  {t("netMountedIfaces")}:{" "}
                  <span class="font-mono text-[#171717] dark:text-[#ededed]">
                    {st().attached_interfaces.length > 0
                      ? st().attached_interfaces.join(", ")
                      : t("netNotMounted")}
                  </span>
                </span>
                <span class="font-mono">
                  IPv4 LPM: {st().active_lpm_entries} · IPv6 LPM: {st().active_lpm_v6_entries} ·
                  {t("netFastFwdPkts")}: {st().fast_forward_packets} · {t("netXdpMounted")}:{" "}
                  {st().xdp_attached_interfaces.join(", ") || t("netNotMounted")} ·{" "}
                  {t("netXdpDrops")} {st().xdp_blocklist_drops} / SYN {st().xdp_syn_drops}
                </span>
              </div>
              <Show when={st().last_error}>
                {(err) => (
                  <div class="mt-1 font-mono text-rose-600 dark:text-rose-400">
                    {t("netErrorPrefix")}: {err()}
                  </div>
                )}
              </Show>
              <Show when={st().stats.length > 0}>
                <div class="mt-2 overflow-x-auto">
                  <table class="w-full text-left font-mono text-[11px]">
                    <thead class="text-[#8f8f8f] dark:text-[#71717a]">
                      <tr>
                        <th class="py-1">{t("netColClientIp")}</th>
                        <th class="py-1">{t("netColTagDomain")}</th>
                        <th class="py-1">{t("netColTargetWan")}</th>
                        <th class="py-1 text-right">{t("netColTx")}</th>
                        <th class="py-1 text-right">{t("netColRx")}</th>
                      </tr>
                    </thead>
                    <tbody class="divide-y divide-[#eaeaea] dark:divide-[#262626] text-[#171717] dark:text-[#ededed]">
                      <For each={st().stats.slice(0, 20)}>
                        {(row) => (
                          <tr>
                            <td class="py-1">{row.client_ip}</td>
                            <td class="py-1">
                              #{row.tag_id} {row.domain_label ?? t("netUnclassified")}
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
      <section class="rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-colors hover:border-[#d4d4d8] dark:hover:border-[#3a3a3a]">
        <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
          <div>
            <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
              {t("netDdnsAcmeTitle")}
            </h2>
            <p class="mt-0.5 text-xs text-[#666666] dark:text-[#a1a1aa]">{t("netDdnsAcmeDesc")}</p>
          </div>
          <div class="flex items-center gap-2">
            <button
              type="button"
              onClick={() => void triggerDdnsSync().then(setDdnsStatus)}
              class="rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-[#171717] hover:bg-[#383838] dark:bg-[#ededed] dark:hover:bg-[#d4d4d8] px-3 py-1.5 text-xs font-medium text-white dark:text-[#0a0a0a] cursor-pointer transition-colors"
            >
              {t("netSyncDdnsNow")}
            </button>
            <button
              type="button"
              onClick={() => void triggerTlsRenew().then(setTlsStatus)}
              class="rounded-lg border border-emerald-500/25 bg-emerald-500/10 hover:border border-emerald-500/20 bg-emerald-500/10 px-3 py-1.5 text-xs font-medium text-emerald-700 dark:text-emerald-400 cursor-pointer transition-colors"
            >
              {t("netRenewCert")}
            </button>
          </div>
        </div>

        <div class="grid gap-4 sm:grid-cols-2">
          <div class="space-y-2 rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] p-3">
            <div class="flex items-center justify-between">
              <span class="text-xs font-semibold text-[#171717] dark:text-[#ededed]">
                DDNS 配置
              </span>
              <label class="flex items-center gap-2 text-xs text-[#171717] dark:text-[#ededed]">
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
                class="rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 text-xs"
              >
                <option value="cloudflare">Cloudflare</option>
                <option value="dyn_dns2">DynDNS2</option>
                <option value="webhook">Webhook</option>
              </select>
              <input
                type="text"
                value={props.draft.ddns.domain}
                placeholder={t("netDomainPlaceholder")}
                onInput={(e) => updateDdns({ domain: e.currentTarget.value })}
                class="rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono text-xs"
              />
              <input
                type="text"
                value={props.draft.ddns.token ?? ""}
                placeholder={t("netTokenPlaceholder")}
                onInput={(e) => updateDdns({ token: e.currentTarget.value || null })}
                class="rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono text-xs"
              />
              <input
                type="text"
                value={props.draft.ddns.zone_id ?? ""}
                placeholder="Zone ID (Cloudflare)"
                onInput={(e) => updateDdns({ zone_id: e.currentTarget.value || null })}
                class="rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono text-xs"
              />
            </div>
            <Show when={ddnsStatus()}>
              {(ds) => (
                <div class="font-mono text-[11px] text-[#666666] dark:text-[#a1a1aa]">
                  {t("netLastIp")}: {ds().last_ip ?? "-"} · {t("netLastSync")}:{" "}
                  {ds().last_synced_at ?? "-"}
                  {ds().last_error ? ` · ${t("netErrorPrefix")}: ${ds().last_error}` : ""}
                </div>
              )}
            </Show>
          </div>

          <div class="space-y-2 rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] p-3">
            <div class="flex items-center justify-between">
              <span class="text-xs font-semibold text-[#171717] dark:text-[#ededed]">
                ACME 动态 TLS
              </span>
              <label class="flex items-center gap-2 text-xs text-[#171717] dark:text-[#ededed]">
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
                placeholder={t("netCertDomainPlaceholder")}
                onInput={(e) => updateAcme({ domain: e.currentTarget.value })}
                class="rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono text-xs"
              />
              <input
                type="email"
                value={props.draft.acme.email}
                placeholder={t("netCertEmailPlaceholder")}
                onInput={(e) => updateAcme({ email: e.currentTarget.value })}
                class="rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono text-xs"
              />
              <select
                value={props.draft.acme.challenge_type}
                onChange={(e) =>
                  updateAcme({ challenge_type: e.currentTarget.value as AcmeChallengeType })
                }
                class="rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 text-xs"
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
                class="rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono text-xs"
              />
            </div>
            <Show when={tlsStatus()}>
              {(ts) => (
                <div class="font-mono text-[11px] text-[#666666] dark:text-[#a1a1aa]">
                  {t("netCertPath")}: {ts().cert_path ?? t("netCertSelfSigned")} ·{" "}
                  {t("netCertExpires")}: {ts().expires_at ?? "-"}
                  {ts().last_error ? ` · ${t("netErrorPrefix")}: ${ts().last_error}` : ""}
                </div>
              )}
            </Show>
          </div>
        </div>
      </section>

      {/* LAN Bridge Parameters */}
      <section class="rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-colors hover:border-[#d4d4d8] dark:hover:border-[#3a3a3a]">
        <h2 class="mb-3.5 text-sm font-medium text-[#171717] dark:text-[#ededed]">
          {t("netLanTitle")}
        </h2>
        <div class="space-y-4">
          <For each={props.draft.lans}>
            {(lan, idx) => (
              <div class="grid gap-3 rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] p-4 sm:grid-cols-3">
                <label class="block text-xs">
                  <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netBridgeName")}</span>
                  <input
                    type="text"
                    value={lan.bridge}
                    onInput={(e) => updateLan(idx(), { bridge: e.currentTarget.value })}
                    class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 font-mono text-xs"
                  />
                </label>
                <label class="block text-xs">
                  <span class="text-[#666666] dark:text-[#a1a1aa]">LAN CIDR</span>
                  <input
                    type="text"
                    value={lan.cidr}
                    onInput={(e) => updateLan(idx(), { cidr: e.currentTarget.value })}
                    class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 font-mono text-xs"
                  />
                </label>
                <label class="block text-xs">
                  <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netLocalDomain")}</span>
                  <input
                    type="text"
                    value={lan.domain}
                    onInput={(e) => updateLan(idx(), { domain: e.currentTarget.value })}
                    class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 font-mono text-xs"
                  />
                </label>
                <label class="block text-xs">
                  <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netDhcpStart")}</span>
                  <input
                    type="number"
                    value={lan.dhcp_start_offset}
                    onInput={(e) =>
                      updateLan(idx(), {
                        dhcp_start_offset: Number(e.currentTarget.value) || 100,
                      })
                    }
                    class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 font-mono text-xs"
                  />
                </label>
                <label class="block text-xs">
                  <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netDhcpLimit")}</span>
                  <input
                    type="number"
                    value={lan.dhcp_limit}
                    onInput={(e) =>
                      updateLan(idx(), { dhcp_limit: Number(e.currentTarget.value) || 150 })
                    }
                    class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 font-mono text-xs"
                  />
                </label>
                <label class="flex items-center gap-2 pt-5 text-xs text-[#171717] dark:text-[#ededed]">
                  <input
                    type="checkbox"
                    checked={lan.dhcp_enabled}
                    onChange={(e) => updateLan(idx(), { dhcp_enabled: e.currentTarget.checked })}
                  />
                  {t("netEnableDhcp")}
                </label>
                <label class="block text-xs">
                  <span class="text-[#666666] dark:text-[#a1a1aa]">IPv6 分配模式</span>
                  <select
                    value={lan.ipv6_mode}
                    onChange={(e) => {
                      const mode = e.currentTarget.value as Ipv6LanMode;
                      updateLan(idx(), {
                        ipv6_mode: mode,
                        ipv6_ra_enabled: mode === "server",
                      });
                    }}
                    class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 text-xs"
                  >
                    <option value="server">PD 子网委派 (Server / SLAAC)</option>
                    <option value="relay">IPv6 中继模式 (NDP Proxy / Relay)</option>
                    <option value="disabled">禁用 IPv6</option>
                  </select>
                </label>
                <Show when={lan.ipv6_mode === "relay"}>
                  <label class="block text-xs">
                    <span class="text-[#666666] dark:text-[#a1a1aa]">中继上游 WAN</span>
                    <select
                      value={lan.ipv6_relay_wan ?? ""}
                      onChange={(e) => {
                        const val = e.currentTarget.value.trim();
                        updateLan(idx(), { ipv6_relay_wan: val === "" ? null : val });
                      }}
                      class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 font-mono text-xs"
                    >
                      <option value="">自动 (主 WAN)</option>
                      <For each={props.draft.wans}>
                        {(w) => (
                          <option value={w.name}>
                            {w.name} ({w.device})
                          </option>
                        )}
                      </For>
                    </select>
                  </label>
                </Show>
                <Show when={lan.ipv6_mode === "server"}>
                  <label class="flex items-center gap-2 pt-5 text-xs text-[#171717] dark:text-[#ededed]">
                    <input
                      type="checkbox"
                      checked={lan.ipv6_ra_enabled}
                      onChange={(e) =>
                        updateLan(idx(), { ipv6_ra_enabled: e.currentTarget.checked })
                      }
                    />
                    {t("netEnableRa")}
                  </label>
                </Show>
                <label class="block text-xs">
                  <span class="text-[#666666] dark:text-[#a1a1aa]">
                    IPv6-PD /64 子网序号 (可选 SLA ID)
                  </span>
                  <input
                    type="number"
                    min={0}
                    max={255}
                    placeholder={t("netAutoAssign")}
                    value={lan.ipv6_subnet_id ?? ""}
                    onInput={(e) => {
                      const val = e.currentTarget.value.trim();
                      updateLan(idx(), {
                        ipv6_subnet_id: val === "" ? null : Number(val),
                      });
                    }}
                    class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 font-mono text-xs"
                  />
                </label>
                <label class="block text-xs">
                  <span class="text-[#666666] dark:text-[#a1a1aa]">
                    IPv6 ULA 静态前缀 (可选, 如 fd00:10::1/64)
                  </span>
                  <input
                    type="text"
                    placeholder={t("netIpv6UlaPlaceholder")}
                    value={lan.ipv6_ula_cidr ?? ""}
                    onInput={(e) =>
                      updateLan(idx(), { ipv6_ula_cidr: e.currentTarget.value.trim() || null })
                    }
                    class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 font-mono text-xs"
                  />
                </label>
                <label class="flex items-center gap-2 pt-5 text-xs text-[#171717] dark:text-[#ededed]">
                  <input
                    type="checkbox"
                    checked={lan.stp_enabled}
                    onChange={(e) => updateLan(idx(), { stp_enabled: e.currentTarget.checked })}
                  />
                  {t("netEnableStp")}
                </label>
                <label class="block text-xs">
                  <span class="text-[#666666] dark:text-[#a1a1aa]">
                    STP Forward Delay (cs, 0=默认)
                  </span>
                  <input
                    type="number"
                    min={0}
                    value={lan.forward_delay_cs}
                    onInput={(e) =>
                      updateLan(idx(), {
                        forward_delay_cs: Math.max(0, Number(e.currentTarget.value) || 0),
                      })
                    }
                    class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 font-mono text-xs"
                  />
                </label>
              </div>
            )}
          </For>
        </div>
      </section>

      {/* Link Aggregation (Bonding / LACP) */}
      <section class="rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-colors hover:border-[#d4d4d8] dark:hover:border-[#3a3a3a]">
        <div class="mb-3 flex items-center justify-between">
          <div>
            <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
              {t("netBondTitle")}
            </h2>
            <p class="mt-0.5 text-xs text-[#666666] dark:text-[#a1a1aa]">{t("netBondDesc")}</p>
          </div>
          <button
            type="button"
            onClick={addBond}
            class="rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#111111] hover:bg-[#f4f4f5] dark:hover:bg-[#1a1a1a] px-3 py-1.5 text-xs font-medium text-[#171717] dark:text-[#ededed] cursor-pointer transition-colors"
          >
            {t("netAddBond")}
          </button>
        </div>
        <div class="space-y-3">
          <For
            each={props.draft.bonds}
            fallback={<p class="text-xs text-[#8f8f8f] dark:text-[#71717a]">{t("netNoBonds")}</p>}
          >
            {(bond, idx) => (
              <div class="grid gap-3 rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] p-3 sm:grid-cols-4">
                <label class="block text-xs">
                  <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netBondName")}</span>
                  <input
                    type="text"
                    value={bond.name}
                    placeholder="例如: bond0"
                    onInput={(e) => updateBond(idx(), { name: e.currentTarget.value.trim() })}
                    class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 font-mono text-xs"
                  />
                </label>
                <label class="block text-xs">
                  <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netBondMode")}</span>
                  <select
                    value={bond.mode}
                    onChange={(e) => updateBond(idx(), { mode: e.currentTarget.value as BondMode })}
                    class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 text-xs"
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
                  <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netHashPolicy")}</span>
                  <select
                    value={bond.xmit_hash_policy}
                    onChange={(e) =>
                      updateBond(idx(), {
                        xmit_hash_policy: e.currentTarget.value as BondXmitHashPolicy,
                      })
                    }
                    class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 text-xs"
                  >
                    <option value="layer2+3">layer2+3</option>
                    <option value="layer3+4">layer3+4</option>
                    <option value="layer2">layer2</option>
                    <option value="encap2+3">encap2+3</option>
                    <option value="encap3+4">encap3+4</option>
                  </select>
                </label>
                <label class="block text-xs">
                  <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netLacpRate")}</span>
                  <select
                    value={bond.lacp_rate}
                    onChange={(e) =>
                      updateBond(idx(), { lacp_rate: e.currentTarget.value as BondLacpRate })
                    }
                    class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 text-xs"
                  >
                    <option value="slow">slow (30s)</option>
                    <option value="fast">fast (1s)</option>
                  </select>
                </label>
                <label class="block text-xs sm:col-span-2">
                  <span class="text-[#666666] dark:text-[#a1a1aa]">
                    成员接口 (逗号分隔，如 eth1, eth2)
                  </span>
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
                    class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2.5 py-1.5 font-mono text-xs"
                  />
                </label>
                <label class="block text-xs">
                  <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netMiimonMtu")}</span>
                  <div class="mt-1 flex gap-2">
                    <input
                      type="number"
                      min={10}
                      value={bond.miimon_ms}
                      onInput={(e) =>
                        updateBond(idx(), { miimon_ms: Number(e.currentTarget.value) || 100 })
                      }
                      class="w-1/2 rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1.5 font-mono text-xs"
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
                      class="w-1/2 rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1.5 font-mono text-xs"
                    />
                  </div>
                </label>
                <div class="flex items-end justify-between gap-2">
                  <label class="block flex-1 text-xs">
                    <span class="text-[#666666] dark:text-[#a1a1aa]">{t("netBondPrimary")}</span>
                    <input
                      type="text"
                      placeholder="eth1"
                      value={bond.primary ?? ""}
                      onInput={(e) =>
                        updateBond(idx(), { primary: e.currentTarget.value.trim() || null })
                      }
                      class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1.5 font-mono text-xs"
                    />
                  </label>
                  <button
                    type="button"
                    onClick={() => removeBond(idx())}
                    class="rounded-lg border border-rose-500/25 bg-rose-500/10 px-2.5 py-1.5 text-xs font-medium text-rose-600 dark:text-rose-600 dark:text-rose-400 hover:bg-rose-500/20 cursor-pointer"
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
      <section class="rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-colors hover:border-[#d4d4d8] dark:hover:border-[#3a3a3a]">
        <div class="mb-3 flex items-center justify-between">
          <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
            {t("netVlanTitle")}
          </h2>
          <button
            type="button"
            onClick={addVlan}
            class="rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#111111] hover:bg-[#f4f4f5] dark:hover:bg-[#1a1a1a] px-3 py-1.5 text-xs font-medium text-[#171717] dark:text-[#ededed] cursor-pointer transition-colors"
          >
            {t("netAddVlan")}
          </button>
        </div>
        <div class="space-y-2">
          <For each={props.draft.vlans}>
            {(vlan, idx) => (
              <div class="grid items-center gap-2 rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] p-3 sm:grid-cols-7">
                <input
                  type="text"
                  value={vlan.parent}
                  placeholder={t("netVlanParentPlaceholder")}
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.vlans[idx()].parent = e.currentTarget.value;
                    props.onChange(next);
                  }}
                  class="rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono text-xs"
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
                  class="rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono text-xs"
                />
                <select
                  value={vlan.role}
                  onChange={(e) => {
                    const next = structuredClone(props.draft);
                    next.vlans[idx()].role = e.currentTarget.value as InterfaceRole;
                    props.onChange(next);
                  }}
                  class="rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 text-xs"
                >
                  <option value="lan">LAN</option>
                  <option value="wan">WAN</option>
                  <option value="disabled">Disabled</option>
                </select>
                <input
                  type="text"
                  value={vlan.cidr ?? ""}
                  placeholder={t("netVlanCidrPlaceholder")}
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.vlans[idx()].cidr = e.currentTarget.value || null;
                    props.onChange(next);
                  }}
                  class="rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono text-xs"
                />
                <label class="flex items-center gap-1.5 text-xs text-[#171717] dark:text-[#ededed]">
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
                  placeholder={t("netVlanSubnetPlaceholder")}
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    const val = e.currentTarget.value.trim();
                    next.vlans[idx()].ipv6_subnet_id = val === "" ? null : Number(val);
                    props.onChange(next);
                  }}
                  class="rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono text-xs"
                />
                <button
                  type="button"
                  onClick={() => {
                    const next = structuredClone(props.draft);
                    next.vlans.splice(idx(), 1);
                    props.onChange(next);
                  }}
                  class="text-right text-xs text-rose-600 dark:text-rose-600 dark:text-rose-400 hover:text-rose-700 dark:hover:text-rose-300 bg-transparent border-0 cursor-pointer"
                >
                  删除
                </button>
              </div>
            )}
          </For>
        </div>
      </section>

      {/* MAC Pinning */}
      <section class="rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-colors hover:border-[#d4d4d8] dark:hover:border-[#3a3a3a]">
        <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
          <div>
            <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
              {t("netMacPinTitle")}
            </h2>
            <p class="text-xs text-[#666666] dark:text-[#a1a1aa]">{t("netMacPinDesc")}</p>
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
                class="rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#111111] hover:bg-[#f4f4f5] dark:hover:bg-[#1a1a1a] px-3 py-1.5 text-xs font-medium text-[#171717] dark:text-[#ededed] cursor-pointer transition-colors"
              >
                {t("netMacPinAdd")}
              </button>
            </Show>
          </div>
        </div>
        <div class="space-y-2">
          <For each={props.draft.interface_pins}>
            {(pin, idx) => (
              <div class="grid items-center gap-2 rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] p-3 sm:grid-cols-3">
                <input
                  type="text"
                  value={pin.mac}
                  placeholder="aa:bb:cc:dd:ee:ff"
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.interface_pins[idx()].mac = e.currentTarget.value;
                    props.onChange(next);
                  }}
                  class="rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono text-xs"
                />
                <input
                  type="text"
                  value={pin.name}
                  placeholder={t("netPinNamePlaceholder")}
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.interface_pins[idx()].name = e.currentTarget.value;
                    props.onChange(next);
                  }}
                  class="rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono text-xs"
                />
                <button
                  type="button"
                  onClick={() => {
                    const next = structuredClone(props.draft);
                    next.interface_pins.splice(idx(), 1);
                    props.onChange(next);
                  }}
                  class="text-right text-xs text-rose-600 dark:text-rose-600 dark:text-rose-400 hover:text-rose-700 dark:hover:text-rose-300 bg-transparent border-0 cursor-pointer"
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
        <div class="rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-colors hover:border-[#d4d4d8] dark:hover:border-[#3a3a3a]">
          <div class="mb-3 flex items-center justify-between">
            <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
              {t("netStaticRoutesTitle")}
            </h2>
            <button
              type="button"
              onClick={addRoute}
              class="rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#111111] hover:bg-[#f4f4f5] dark:hover:bg-[#1a1a1a] px-2.5 py-1.5 text-xs font-medium text-[#171717] dark:text-[#ededed] cursor-pointer transition-colors"
            >
              {t("netAddRoute")}
            </button>
          </div>
          <div class="space-y-2">
            <For each={props.draft.routes}>
              {(route, idx) => (
                <div class="space-y-2 rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] p-3 text-xs">
                  <div class="grid grid-cols-2 gap-2">
                    <input
                      type="text"
                      value={route.destination}
                      placeholder={t("netRouteDstPlaceholder")}
                      onInput={(e) => {
                        const next = structuredClone(props.draft);
                        next.routes[idx()].destination = e.currentTarget.value;
                        props.onChange(next);
                      }}
                      class="rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono"
                    />
                    <input
                      type="text"
                      value={route.gateway ?? ""}
                      placeholder={t("netRouteGwPlaceholder")}
                      onInput={(e) => {
                        const next = structuredClone(props.draft);
                        next.routes[idx()].gateway = e.currentTarget.value || null;
                        props.onChange(next);
                      }}
                      class="rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono"
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
                      class="w-24 rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono"
                    />
                    <input
                      type="text"
                      value={route.device ?? ""}
                      placeholder={t("netRouteDevPlaceholder")}
                      onInput={(e) => {
                        const next = structuredClone(props.draft);
                        next.routes[idx()].device = e.currentTarget.value || null;
                        props.onChange(next);
                      }}
                      class="flex-1 rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono"
                    />
                    <button
                      type="button"
                      onClick={() => {
                        const next = structuredClone(props.draft);
                        next.routes.splice(idx(), 1);
                        props.onChange(next);
                      }}
                      class="text-rose-600 dark:text-rose-600 dark:text-rose-400 hover:text-rose-700 dark:hover:text-rose-300 bg-transparent border-0 cursor-pointer"
                    >
                      删除
                    </button>
                  </div>
                </div>
              )}
            </For>
          </div>
        </div>

        <div class="rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-colors hover:border-[#d4d4d8] dark:hover:border-[#3a3a3a]">
          <div class="mb-3 flex items-center justify-between">
            <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
              {t("netPbrTitle")}
            </h2>
            <button
              type="button"
              onClick={addPolicyRule}
              class="rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#111111] hover:bg-[#f4f4f5] dark:hover:bg-[#1a1a1a] px-2.5 py-1.5 text-xs font-medium text-[#171717] dark:text-[#ededed] cursor-pointer transition-colors"
            >
              {t("netAddRule")}
            </button>
          </div>
          <div class="space-y-2">
            <For each={props.draft.policy_rules}>
              {(rule, idx) => (
                <div class="space-y-2 rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] p-3 text-xs">
                  <div class="grid grid-cols-3 gap-2">
                    <input
                      type="number"
                      value={rule.priority}
                      placeholder={t("netPbrPriorityPlaceholder")}
                      onInput={(e) => {
                        const next = structuredClone(props.draft);
                        next.policy_rules[idx()].priority = Number(e.currentTarget.value) || 1000;
                        props.onChange(next);
                      }}
                      class="rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono"
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
                      class="rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono"
                    />
                    <input
                      type="text"
                      value={rule.iif ?? ""}
                      placeholder={t("netPbrIifPlaceholder")}
                      onInput={(e) => {
                        const next = structuredClone(props.draft);
                        next.policy_rules[idx()].iif = e.currentTarget.value || null;
                        props.onChange(next);
                      }}
                      class="rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono"
                    />
                  </div>
                  <div class="flex items-center gap-2">
                    <input
                      type="text"
                      value={rule.src ?? ""}
                      placeholder={t("netPbrSrcPlaceholder")}
                      onInput={(e) => {
                        const next = structuredClone(props.draft);
                        next.policy_rules[idx()].src = e.currentTarget.value || null;
                        props.onChange(next);
                      }}
                      class="flex-1 rounded border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] px-2 py-1 font-mono"
                    />
                    <button
                      type="button"
                      onClick={() => {
                        const next = structuredClone(props.draft);
                        next.policy_rules.splice(idx(), 1);
                        props.onChange(next);
                      }}
                      class="text-rose-600 dark:text-rose-600 dark:text-rose-400 hover:text-rose-700 dark:hover:text-rose-300 bg-transparent border-0 cursor-pointer"
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

      <section class="rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 space-y-4">
        <div class="flex flex-wrap items-center justify-between gap-2">
          <div>
            <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
              跨 VLAN 组播发现 (mDNS / Bonjour)
            </h2>
            <p class="mt-0.5 text-xs text-[#666666] dark:text-[#a1a1aa]">
              通过 Avahi 反射器跨网段转发 AirPlay、Chromecast 与局域网打印机发现报文 (UDP 5353)
            </p>
          </div>
          <label class="flex items-center gap-2 text-xs text-[#171717] dark:text-[#ededed]">
            <input
              type="checkbox"
              checked={props.draft.mdns.enabled}
              onChange={(e) => updateMdns({ enabled: e.currentTarget.checked })}
            />
            启用 mDNS 反射
          </label>
        </div>

        <Show when={props.draft.mdns.enabled}>
          <div class="grid gap-3 sm:grid-cols-3 text-xs">
            <label class="block sm:col-span-2">
              <span class="text-[#666666] dark:text-[#a1a1aa]">
                参与反射的接口 (逗号分隔，留空自动包含全部 LAN 网桥与 LAN VLAN)
              </span>
              <input
                type="text"
                placeholder="br-lan, eth1.20"
                value={props.draft.mdns.interfaces.join(", ")}
                onInput={(e) =>
                  updateMdns({
                    interfaces: e.currentTarget.value
                      .split(",")
                      .map((s) => s.trim())
                      .filter(Boolean),
                  })
                }
                class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] px-2.5 py-1.5 font-mono text-xs text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed]"
              />
            </label>
            <div class="flex flex-col justify-end gap-2">
              <label class="flex items-center gap-2 text-xs text-[#171717] dark:text-[#ededed]">
                <input
                  type="checkbox"
                  checked={props.draft.mdns.publish_workstation}
                  onChange={(e) => updateMdns({ publish_workstation: e.currentTarget.checked })}
                />
                广播路由器主机名 (.local)
              </label>
              <label class="flex items-center gap-2 text-xs text-[#171717] dark:text-[#ededed]">
                <input
                  type="checkbox"
                  checked={props.draft.mdns.allow_point_to_point}
                  onChange={(e) => updateMdns({ allow_point_to_point: e.currentTarget.checked })}
                />
                允许点对点接口反射
              </label>
            </div>
          </div>
        </Show>
      </section>

      <section class="rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 space-y-4">
        <div class="flex flex-wrap items-center justify-between gap-2">
          <div>
            <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
              IPTV 组播支持 (IGMP Proxy + udpxy)
            </h2>
            <p class="mt-0.5 text-xs text-[#666666] dark:text-[#a1a1aa]">
              转发运营商 IPTV 组播流至内网，并可选开启 udpxy 将组播转为 HTTP 单播供 Wi-Fi 终端播放
            </p>
          </div>
          <label class="flex items-center gap-2 text-xs text-[#171717] dark:text-[#ededed]">
            <input
              type="checkbox"
              checked={props.draft.iptv.enabled}
              onChange={(e) => updateIptv({ enabled: e.currentTarget.checked })}
            />
            启用 IPTV 组播
          </label>
        </div>

        <Show when={props.draft.iptv.enabled}>
          <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-4 text-xs">
            <label class="block">
              <span class="text-[#666666] dark:text-[#a1a1aa]">上游组播接口 (WAN / VLAN)</span>
              <input
                type="text"
                placeholder="eth0.85"
                value={props.draft.iptv.upstream_interface}
                onInput={(e) => updateIptv({ upstream_interface: e.currentTarget.value.trim() })}
                class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] px-2.5 py-1.5 font-mono text-xs text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed]"
              />
            </label>
            <label class="block">
              <span class="text-[#666666] dark:text-[#a1a1aa]">下游 LAN 接口 (留空=全部网桥)</span>
              <input
                type="text"
                placeholder="br-lan"
                value={props.draft.iptv.downstream_interfaces.join(", ")}
                onInput={(e) =>
                  updateIptv({
                    downstream_interfaces: e.currentTarget.value
                      .split(",")
                      .map((s) => s.trim())
                      .filter(Boolean),
                  })
                }
                class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] px-2.5 py-1.5 font-mono text-xs text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed]"
              />
            </label>
            <label class="block sm:col-span-2">
              <span class="text-[#666666] dark:text-[#a1a1aa]">允许组播源网段 (altnet CIDR)</span>
              <input
                type="text"
                placeholder="10.0.0.0/8, 224.0.0.0/4"
                value={props.draft.iptv.altnet_cidrs.join(", ")}
                onInput={(e) =>
                  updateIptv({
                    altnet_cidrs: e.currentTarget.value
                      .split(",")
                      .map((s) => s.trim())
                      .filter(Boolean),
                  })
                }
                class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] px-2.5 py-1.5 font-mono text-xs text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed]"
              />
            </label>
          </div>

          <div class="grid gap-3 sm:grid-cols-4 text-xs items-end">
            <label class="flex items-center gap-2 text-xs text-[#171717] dark:text-[#ededed]">
              <input
                type="checkbox"
                checked={props.draft.iptv.igmp_snooping}
                onChange={(e) => updateIptv({ igmp_snooping: e.currentTarget.checked })}
              />
              网桥 IGMP Snooping
            </label>
            <label class="flex items-center gap-2 text-xs text-[#171717] dark:text-[#ededed]">
              <input
                type="checkbox"
                checked={props.draft.iptv.udpxy_enabled}
                onChange={(e) => updateIptv({ udpxy_enabled: e.currentTarget.checked })}
              />
              启用 udpxy 单播转发
            </label>
            <Show when={props.draft.iptv.udpxy_enabled}>
              <label class="block">
                <span class="text-[#666666] dark:text-[#a1a1aa]">udpxy 监听端口</span>
                <input
                  type="number"
                  min={1}
                  max={65535}
                  value={props.draft.iptv.udpxy_port}
                  onInput={(e) => updateIptv({ udpxy_port: Number(e.currentTarget.value) || 4022 })}
                  class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] px-2.5 py-1.5 font-mono text-xs text-[#171717] dark:text-[#ededed]"
                />
              </label>
              <label class="block">
                <span class="text-[#666666] dark:text-[#a1a1aa]">最大并发客户端</span>
                <input
                  type="number"
                  min={1}
                  max={256}
                  value={props.draft.iptv.udpxy_max_clients}
                  onInput={(e) =>
                    updateIptv({ udpxy_max_clients: Number(e.currentTarget.value) || 16 })
                  }
                  class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] px-2.5 py-1.5 font-mono text-xs text-[#171717] dark:text-[#ededed]"
                />
              </label>
            </Show>
          </div>
        </Show>
      </section>

      <NicTuningSection
        draft={props.draft}
        interfaces={props.interfaces}
        onChange={props.onChange}
      />
    </div>
  );
}

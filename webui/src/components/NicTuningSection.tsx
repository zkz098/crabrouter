import { For, Show, createSignal } from "solid-js";
import { getInterfaceCapabilities } from "../api";
import type { HostInterfaceSnapshot } from "../types/HostInterfaceSnapshot";
import type { NicCapabilitiesSnapshot } from "../types/NicCapabilitiesSnapshot";
import type { NicTuningConfig } from "../types/NicTuningConfig";
import type { NicTuningProfile } from "../types/NicTuningProfile";
import type { OffloadFeatureState } from "../types/OffloadFeatureState";
import type { RouterConfig } from "../types/RouterConfig";
import type { RpsMode } from "../types/RpsMode";

interface Props {
  draft: RouterConfig;
  interfaces: HostInterfaceSnapshot[];
  onChange: (next: RouterConfig) => void;
}

type TriState = "inherit" | "on" | "off";

function toTriState(val: boolean | null): TriState {
  if (val === null) {
    return "inherit";
  }
  return val ? "on" : "off";
}

function fromTriState(val: string): boolean | null {
  if (val === "on") {
    return true;
  }
  if (val === "off") {
    return false;
  }
  return null;
}

const INPUT_CLS =
  "mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] px-2.5 py-1.5 text-xs text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] transition-colors";

export function NicTuningSection(props: Props) {
  const [capsByIface, setCapsByIface] = createSignal<Record<string, NicCapabilitiesSnapshot>>({});
  const [probingIface, setProbingIface] = createSignal<string | null>(null);
  const [probeErr, setProbeErr] = createSignal<string | null>(null);

  const candidateIfaces = () => {
    const set = new Set<string>();
    for (const iface of props.interfaces) {
      if (iface.name !== "lo") {
        set.add(iface.name);
      }
    }
    for (const wan of props.draft.wans) {
      set.add(wan.device);
    }
    for (const lan of props.draft.lans) {
      for (const port of lan.ports) {
        set.add(port);
      }
    }
    return [...set];
  };

  function addPolicy() {
    const next = structuredClone(props.draft);
    const configured = new Set(next.nic_tuning.map((n) => n.interface));
    const defaultIface =
      candidateIfaces().find((name) => !configured.has(name)) ?? candidateIfaces()[0] ?? "eth0";
    next.nic_tuning.push({
      interface: defaultIface,
      enabled: true,
      profile: "low_latency",
      gro: null,
      gso: null,
      tso: null,
      lro: null,
      rx_checksum: null,
      tx_checksum: null,
      pause_rx: null,
      pause_tx: null,
      rx_ring: null,
      tx_ring: null,
      rps_mode: "off",
      rps_mask: null,
      xps_enabled: false,
      rfs_flow_entries: null,
    });
    props.onChange(next);
  }

  function updateItem(idx: number, patch: Partial<NicTuningConfig>) {
    const next = structuredClone(props.draft);
    const cur = next.nic_tuning[idx];
    if (!cur) {
      return;
    }
    next.nic_tuning[idx] = { ...cur, ...patch };
    props.onChange(next);
  }

  function removeItem(idx: number) {
    const next = structuredClone(props.draft);
    next.nic_tuning.splice(idx, 1);
    props.onChange(next);
  }

  async function probeIface(iface: string) {
    setProbeErr(null);
    setProbingIface(iface);
    try {
      const snap = await getInterfaceCapabilities(iface);
      setCapsByIface((prev) => ({ ...prev, [iface]: snap }));
    } catch (err) {
      setProbeErr(`${iface}: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setProbingIface(null);
    }
  }

  function formatOffloadBadge(state: OffloadFeatureState | null | undefined): string {
    if (!state) {
      return "";
    }
    const base = state.enabled ? "当前: on" : "当前: off";
    return state.fixed ? `${base} [fixed]` : base;
  }

  return (
    <section class="rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-colors hover:border-[#d4d4d8] dark:hover:border-[#3a3a3a]">
      <div class="mb-3.5 flex flex-wrap items-center justify-between gap-2">
        <div>
          <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
            网口驱动级调优 (ethtool / RPS / XPS / RFS)
          </h2>
          <p class="mt-0.5 text-xs text-[#666666] dark:text-[#a1a1aa]">
            配置网卡硬件 Offload、流控、Ring 缓冲与多核软中断亲和性；支持能力探测与预设模板
          </p>
        </div>
        <button
          type="button"
          onClick={addPolicy}
          class="rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#111111] hover:bg-[#f4f4f5] dark:hover:bg-[#1a1a1a] px-3 py-1.5 text-xs font-medium text-[#171717] dark:text-[#ededed] cursor-pointer transition-all active:scale-[0.97]"
        >
          添加网口调优
        </button>
      </div>

      <Show when={probeErr()}>
        {(msg) => (
          <div class="mb-3 rounded-lg border border-rose-500/30 bg-rose-500/10 px-3 py-2 text-xs text-rose-700 dark:text-rose-400">
            探测失败：{msg()}
          </div>
        )}
      </Show>

      <Show
        when={props.draft.nic_tuning.length > 0}
        fallback={
          <p class="text-xs text-[#666666] dark:text-[#a1a1aa]">
            尚未配置网口驱动级调优策略（点击右上角“添加网口调优”开始配置）。
          </p>
        }
      >
        <div class="space-y-4">
          <For each={props.draft.nic_tuning}>
            {(item, idx) => {
              const snap = () => capsByIface()[item.interface];
              const offloadEntries = (): Array<{
                key: "tso" | "gso" | "gro" | "lro" | "rx_checksum" | "tx_checksum";
                label: string;
                value: boolean | null;
                probed: OffloadFeatureState | null | undefined;
              }> => [
                { key: "tso", label: "TSO", value: item.tso, probed: snap()?.offloads.tso },
                { key: "gso", label: "GSO", value: item.gso, probed: snap()?.offloads.gso },
                { key: "gro", label: "GRO", value: item.gro, probed: snap()?.offloads.gro },
                {
                  key: "lro",
                  label: "LRO (路由建议关闭)",
                  value: item.lro,
                  probed: snap()?.offloads.lro,
                },
                {
                  key: "rx_checksum",
                  label: "RX Checksum",
                  value: item.rx_checksum,
                  probed: snap()?.offloads.rx_checksum,
                },
                {
                  key: "tx_checksum",
                  label: "TX Checksum",
                  value: item.tx_checksum,
                  probed: snap()?.offloads.tx_checksum,
                },
              ];

              return (
                <div class="space-y-3 rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] p-3.5 text-xs">
                  <div class="grid items-end gap-3 sm:grid-cols-5">
                    <label class="flex items-center gap-2 pb-1.5 font-medium text-[#171717] dark:text-[#ededed]">
                      <input
                        type="checkbox"
                        checked={item.enabled}
                        onChange={(e) => updateItem(idx(), { enabled: e.currentTarget.checked })}
                      />
                      启用策略
                    </label>
                    <label class="block">
                      <span class="text-[#666666] dark:text-[#a1a1aa]">网口名称</span>
                      <input
                        type="text"
                        list="nic-tuning-ifaces"
                        value={item.interface}
                        onInput={(e) =>
                          updateItem(idx(), { interface: e.currentTarget.value.trim() })
                        }
                        class={`${INPUT_CLS} font-mono`}
                      />
                    </label>
                    <label class="block">
                      <span class="text-[#666666] dark:text-[#a1a1aa]">调优预设模板</span>
                      <select
                        value={item.profile}
                        onChange={(e) =>
                          updateItem(idx(), {
                            profile: e.currentTarget.value as NicTuningProfile,
                          })
                        }
                        class={INPUT_CLS}
                      >
                        <option value="default">默认 (仅应用显式项)</option>
                        <option value="low_latency">低延迟 (关 LRO/流控 + 开 RPS/XPS)</option>
                        <option value="high_throughput">
                          高吞吐 (开 TSO/GSO/GRO + 关 LRO + RPS/XPS)
                        </option>
                        <option value="custom">自定义 (Custom)</option>
                      </select>
                    </label>
                    <button
                      type="button"
                      disabled={probingIface() === item.interface || item.interface.trim() === ""}
                      onClick={() => void probeIface(item.interface)}
                      class="rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#1a1a1a] hover:bg-[#f4f4f5] dark:hover:bg-[#222222] px-3 py-1.5 text-xs font-medium text-[#171717] dark:text-[#ededed] cursor-pointer disabled:opacity-50 transition-colors"
                    >
                      {probingIface() === item.interface ? "探测中..." : "探测硬件能力"}
                    </button>
                    <div class="pb-1 text-right">
                      <button
                        type="button"
                        onClick={() => removeItem(idx())}
                        class="text-xs font-medium text-rose-600 dark:text-rose-400 hover:text-rose-700 dark:hover:text-rose-300 bg-transparent border-0 cursor-pointer"
                      >
                        删除
                      </button>
                    </div>
                  </div>

                  <Show when={snap()}>
                    {(cap) => (
                      <div class="rounded-lg border border-sky-500/25 bg-sky-500/10 p-2.5 text-xs text-sky-900 dark:text-sky-200">
                        <div class="flex flex-wrap gap-x-4 gap-y-1 font-mono">
                          <span>驱动: {cap().driver ?? "未知"}</span>
                          <span>版本: {cap().driver_version ?? "-"}</span>
                          <span>ethtool: {cap().ethtool_available ? "可用" : "未安装"}</span>
                          <span>
                            队列: RX {cap().rx_queues} / TX {cap().tx_queues}
                          </span>
                          <span>CPU 核数: {cap().cpu_cores}</span>
                          <span>
                            Ring(RX/TX): {cap().rx_ring_current ?? "-"}/{cap().rx_ring_max ?? "-"} ·{" "}
                            {cap().tx_ring_current ?? "-"}/{cap().tx_ring_max ?? "-"}
                          </span>
                          <span>
                            Pause(RX/TX):{" "}
                            {cap().pause_rx === null ? "-" : cap().pause_rx ? "on" : "off"} /{" "}
                            {cap().pause_tx === null ? "-" : cap().pause_tx ? "on" : "off"}
                          </span>
                          <span>
                            当前 RPS/XPS/RFS: {cap().current_rps_cpus ?? "-"} /{" "}
                            {cap().current_xps_cpus ?? "-"} / {cap().current_rfs_entries ?? "-"}
                          </span>
                        </div>
                      </div>
                    )}
                  </Show>

                  <div class="grid gap-3 sm:grid-cols-3 lg:grid-cols-6">
                    <For each={offloadEntries()}>
                      {(feat) => (
                        <label class="block">
                          <span class="flex items-center justify-between gap-1 text-[#666666] dark:text-[#a1a1aa]">
                            <span>{feat.label}</span>
                            <Show when={feat.probed}>
                              <span class="font-mono text-[10px]">
                                {formatOffloadBadge(feat.probed)}
                              </span>
                            </Show>
                          </span>
                          <select
                            disabled={feat.probed?.fixed === true}
                            value={toTriState(feat.value)}
                            onChange={(e) =>
                              updateItem(idx(), {
                                [feat.key]: fromTriState(e.currentTarget.value),
                              } as Partial<NicTuningConfig>)
                            }
                            class={`${INPUT_CLS} disabled:opacity-50`}
                          >
                            <option value="inherit">继承预设</option>
                            <option value="on">开启 (on)</option>
                            <option value="off">关闭 (off)</option>
                          </select>
                        </label>
                      )}
                    </For>
                  </div>

                  <div class="grid gap-3 sm:grid-cols-4">
                    <label class="block">
                      <span class="text-[#666666] dark:text-[#a1a1aa]">RX 流控 (Pause)</span>
                      <select
                        value={toTriState(item.pause_rx)}
                        onChange={(e) =>
                          updateItem(idx(), { pause_rx: fromTriState(e.currentTarget.value) })
                        }
                        class={INPUT_CLS}
                      >
                        <option value="inherit">继承预设</option>
                        <option value="on">开启 (on)</option>
                        <option value="off">关闭 (off)</option>
                      </select>
                    </label>
                    <label class="block">
                      <span class="text-[#666666] dark:text-[#a1a1aa]">TX 流控 (Pause)</span>
                      <select
                        value={toTriState(item.pause_tx)}
                        onChange={(e) =>
                          updateItem(idx(), { pause_tx: fromTriState(e.currentTarget.value) })
                        }
                        class={INPUT_CLS}
                      >
                        <option value="inherit">继承预设</option>
                        <option value="on">开启 (on)</option>
                        <option value="off">关闭 (off)</option>
                      </select>
                    </label>
                    <label class="block">
                      <span class="text-[#666666] dark:text-[#a1a1aa]">
                        RX Ring{" "}
                        {snap()?.rx_ring_max !== null && snap()?.rx_ring_max !== undefined
                          ? `(上限 ${snap()?.rx_ring_max})`
                          : "(64-16384)"}
                      </span>
                      <input
                        type="number"
                        min={64}
                        max={16384}
                        placeholder="保持默认"
                        value={item.rx_ring ?? ""}
                        onInput={(e) => {
                          const raw = e.currentTarget.value.trim();
                          updateItem(idx(), { rx_ring: raw === "" ? null : Number(raw) });
                        }}
                        class={`${INPUT_CLS} font-mono`}
                      />
                    </label>
                    <label class="block">
                      <span class="text-[#666666] dark:text-[#a1a1aa]">
                        TX Ring{" "}
                        {snap()?.tx_ring_max !== null && snap()?.tx_ring_max !== undefined
                          ? `(上限 ${snap()?.tx_ring_max})`
                          : "(64-16384)"}
                      </span>
                      <input
                        type="number"
                        min={64}
                        max={16384}
                        placeholder="保持默认"
                        value={item.tx_ring ?? ""}
                        onInput={(e) => {
                          const raw = e.currentTarget.value.trim();
                          updateItem(idx(), { tx_ring: raw === "" ? null : Number(raw) });
                        }}
                        class={`${INPUT_CLS} font-mono`}
                      />
                    </label>
                  </div>

                  <div class="grid items-end gap-3 sm:grid-cols-4">
                    <label class="block">
                      <span class="text-[#666666] dark:text-[#a1a1aa]">RPS 多核接收调度</span>
                      <select
                        value={item.rps_mode}
                        onChange={(e) =>
                          updateItem(idx(), { rps_mode: e.currentTarget.value as RpsMode })
                        }
                        class={INPUT_CLS}
                      >
                        <option value="off">跟随预设 / 关闭</option>
                        <option value="auto">自动 (单队列时均衡分核)</option>
                        <option value="custom">自定义 CPU 掩码</option>
                      </select>
                    </label>
                    <Show when={item.rps_mode === "custom"}>
                      <label class="block">
                        <span class="text-[#666666] dark:text-[#a1a1aa]">RPS CPU 掩码 (Hex)</span>
                        <input
                          type="text"
                          placeholder="如 0e 或 ff"
                          value={item.rps_mask ?? ""}
                          onInput={(e) => {
                            const val = e.currentTarget.value.trim();
                            updateItem(idx(), { rps_mask: val === "" ? null : val });
                          }}
                          class={`${INPUT_CLS} font-mono`}
                        />
                      </label>
                    </Show>
                    <label class="flex items-center gap-2 pb-1.5 text-[#171717] dark:text-[#ededed]">
                      <input
                        type="checkbox"
                        checked={item.xps_enabled}
                        onChange={(e) =>
                          updateItem(idx(), { xps_enabled: e.currentTarget.checked })
                        }
                      />
                      启用 XPS (发送队列绑核)
                    </label>
                    <label class="block">
                      <span class="text-[#666666] dark:text-[#a1a1aa]">
                        RFS 流表大小 (rps_sock_flow_entries)
                      </span>
                      <input
                        type="number"
                        min={0}
                        max={262144}
                        placeholder="如 32768"
                        value={item.rfs_flow_entries ?? ""}
                        onInput={(e) => {
                          const raw = e.currentTarget.value.trim();
                          updateItem(idx(), {
                            rfs_flow_entries: raw === "" ? null : Number(raw),
                          });
                        }}
                        class={`${INPUT_CLS} font-mono`}
                      />
                    </label>
                  </div>
                </div>
              );
            }}
          </For>
        </div>
      </Show>

      <datalist id="nic-tuning-ifaces">
        <For each={candidateIfaces()}>{(name) => <option value={name} />}</For>
      </datalist>
    </section>
  );
}

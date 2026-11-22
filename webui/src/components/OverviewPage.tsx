import { For, Show, createSignal } from "solid-js";
import type { ConfigSnapshotSummary } from "../types/ConfigSnapshotSummary";
import type { CrabDnsAggregateStatus } from "../types/CrabDnsAggregateStatus";
import type { HardwareHealthSnapshot } from "../types/HardwareHealthSnapshot";
import type { HostInterfaceSnapshot } from "../types/HostInterfaceSnapshot";
import type { RenderedConfigsPreview } from "../types/RenderedConfigsPreview";
import type { RouterConfig } from "../types/RouterConfig";
import type { SystemMetricsSnapshot } from "../types/SystemMetricsSnapshot";
import type { SystemdUnitStatus } from "../types/SystemdUnitStatus";
import type { WanRuntimeState } from "../types/WanRuntimeState";

type PreviewTab =
  | "nftables"
  | "tc_qos"
  | "crabdns"
  | "dnsmasq"
  | "wg_quick"
  | "dhcpcd"
  | "udev"
  | "systemd_link"
  | "pppd";

interface Props {
  config: RouterConfig;
  interfaces: HostInterfaceSnapshot[];
  wanStates: WanRuntimeState[];
  serviceStatuses: SystemdUnitStatus[];
  dnsStatus: CrabDnsAggregateStatus | undefined;
  hardware: HardwareHealthSnapshot | undefined;
  metrics: SystemMetricsSnapshot | null;
  preview: RenderedConfigsPreview | undefined;
  history: ConfigSnapshotSummary[];
  onChangeConfig: (next: RouterConfig) => void;
  onRestoreHistory: (gen: number) => void;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let idx = 0;
  while (value >= 1024 && idx < units.length - 1) {
    value /= 1024;
    idx += 1;
  }
  return `${value.toFixed(1)} ${units[idx]}`;
}

export function OverviewPage(props: Props) {
  const [tab, setTab] = createSignal<PreviewTab>("nftables");

  const previewText = () => {
    const p = props.preview;
    if (!p) {
      return "加载中...";
    }
    switch (tab()) {
      case "nftables":
        return p.nftables;
      case "tc_qos":
        return p.tc_qos;
      case "crabdns":
        return p.crabdns;
      case "dnsmasq":
        return p.dnsmasq;
      case "wg_quick":
        return p.wg_quick;
      case "dhcpcd":
        return p.dhcpcd;
      case "udev":
        return p.udev_rules;
      case "systemd_link":
        return p.systemd_links.length > 0
          ? p.systemd_links.map((f) => `# === ${f.name} ===\n${f.content}`).join("\n")
          : "# 未配置 MAC 固化规则";
      case "pppd":
        return p.pppd_peers.length > 0
          ? p.pppd_peers.map((f) => `# === ${f.name} ===\n${f.content}`).join("\n")
          : "# 未配置 PPPoE WAN";
    }
  };

  const memUsedPct = () => {
    const m = props.metrics;
    if (!m || m.memory_total_bytes === 0) {
      return 0;
    }
    return Math.round((m.memory_used_bytes / m.memory_total_bytes) * 100);
  };

  const rateFor = (name: string) => props.metrics?.interfaces.find((i) => i.name === name);

  return (
    <div class="space-y-5 sm:space-y-6 animate-view-enter">
      <Show when={(props.hardware?.alerts.length ?? 0) > 0}>
        <section class="space-y-2">
          <For each={props.hardware?.alerts}>
            {(alert) => (
              <div
                class={`flex items-center justify-between rounded-xl border px-4 py-2.5 text-xs ${
                  alert.severity === "critical"
                    ? "border-rose-500/25 bg-rose-500/10 text-rose-700 dark:text-rose-300"
                    : "border-amber-500/25 bg-amber-500/10 text-amber-700 dark:text-amber-300"
                }`}
              >
                <div>
                  <span class="font-mono font-semibold">[{alert.source}]</span>{" "}
                  <span>{alert.message}</span>
                </div>
                <span class="rounded-full border border-current/20 px-2 py-0.5 font-mono text-[10px] uppercase">
                  {alert.severity}
                </span>
              </div>
            )}
          </For>
        </section>
      </Show>

      {/* Real-time WebSocket Telemetry KPI Cards */}
      <section class="grid grid-cols-2 lg:grid-cols-4 gap-3 sm:gap-4">
        <div class="animate-card-enter rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-all duration-200 hover:border-[#d4d4d8] dark:hover:border-[#404040]">
          <div class="flex items-center justify-between gap-2">
            <span class="text-xs font-medium text-[#666666] dark:text-[#a1a1aa]">CPU 使用率</span>
            <div class="w-8 h-8 rounded-lg flex items-center justify-center shrink-0 text-sky-600 dark:text-sky-400 bg-sky-500/10">
              <span class="i-ri-cpu-line text-base" />
            </div>
          </div>
          <div class="mt-2.5 text-2xl font-semibold tracking-tight tabular-nums text-[#171717] dark:text-[#ededed]">
            {props.metrics ? `${props.metrics.cpu_usage_pct.toFixed(1)}%` : "-"}
          </div>
          <div class="mt-1 text-xs text-[#666666] dark:text-[#a1a1aa] tabular-nums">
            1m 负载: {props.metrics ? props.metrics.load_avg_1m.toFixed(2) : "-"}
            {props.metrics?.temperature_celsius
              ? ` · ${props.metrics.temperature_celsius.toFixed(1)}°C`
              : ""}
          </div>
        </div>

        <div
          class="animate-card-enter rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-all duration-200 hover:border-[#d4d4d8] dark:hover:border-[#404040]"
          style={{ "animation-delay": "60ms" }}
        >
          <div class="flex items-center justify-between gap-2">
            <span class="text-xs font-medium text-[#666666] dark:text-[#a1a1aa]">内存占用</span>
            <div class="w-8 h-8 rounded-lg flex items-center justify-center shrink-0 text-amber-600 dark:text-amber-400 bg-amber-500/10">
              <span class="i-ri-ram-line text-base" />
            </div>
          </div>
          <div class="mt-2.5 text-2xl font-semibold tracking-tight tabular-nums text-[#171717] dark:text-[#ededed]">
            {memUsedPct()}%
          </div>
          <div class="mt-1 text-xs text-[#666666] dark:text-[#a1a1aa] tabular-nums truncate">
            {props.metrics
              ? `${formatBytes(props.metrics.memory_used_bytes)} / ${formatBytes(props.metrics.memory_total_bytes)}`
              : "等待遥测..."}
          </div>
        </div>

        <div
          class="animate-card-enter rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-all duration-200 hover:border-[#d4d4d8] dark:hover:border-[#404040]"
          style={{ "animation-delay": "120ms" }}
        >
          <div class="flex items-center justify-between gap-2">
            <span class="text-xs font-medium text-[#666666] dark:text-[#a1a1aa]">
              前台 DNS (CrabDNS)
            </span>
            <div class="w-8 h-8 rounded-lg flex items-center justify-center shrink-0 text-emerald-600 dark:text-emerald-400 bg-emerald-500/10">
              <span class="i-ri-shield-check-line text-base" />
            </div>
          </div>
          <div class="mt-2.5 flex items-center justify-between gap-2">
            <span class="font-mono text-2xl font-semibold tracking-tight tabular-nums text-[#171717] dark:text-[#ededed]">
              :{props.config.dns.crabdns_port}
            </span>
            <a
              href="/crabdns/"
              target="_blank"
              rel="noreferrer"
              class="rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] hover:bg-[#f4f4f5] dark:hover:bg-[#1a1a1a] px-2 py-1 text-[11px] font-medium text-[#171717] dark:text-[#ededed] transition-colors whitespace-nowrap"
            >
              面板 →
            </a>
          </div>
          <div class="mt-1 text-xs text-[#666666] dark:text-[#a1a1aa] truncate">
            {props.dnsStatus?.reachable
              ? `运行正常 (127.0.0.1:${props.dnsStatus.web_port})`
              : `后端 :${props.config.dns.crabdns_web_port} 未运行`}
          </div>
        </div>

        <div
          class="animate-card-enter rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-all duration-200 hover:border-[#d4d4d8] dark:hover:border-[#404040]"
          style={{ "animation-delay": "180ms" }}
        >
          <div class="flex items-center justify-between gap-2">
            <span class="text-xs font-medium text-[#666666] dark:text-[#a1a1aa]">
              DHCP & 内网解析
            </span>
            <div class="w-8 h-8 rounded-lg flex items-center justify-center shrink-0 text-violet-600 dark:text-violet-400 bg-violet-500/10">
              <span class="i-ri-server-line text-base" />
            </div>
          </div>
          <div class="mt-2.5 font-mono text-2xl font-semibold tracking-tight tabular-nums text-[#171717] dark:text-[#ededed]">
            :{props.config.dns.dnsmasq_dns_port}
          </div>
          <div class="mt-1 text-xs text-[#666666] dark:text-[#a1a1aa] tabular-nums">
            cache-size=0 · 回滚超时 {props.config.system.commit_confirm_timeout_secs}s
          </div>
        </div>
      </section>

      {/* Systemd D-Bus Service Status & Management Security */}
      <section class="grid gap-4 md:grid-cols-2">
        <div class="rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-colors hover:border-[#d4d4d8] dark:hover:border-[#3a3a3a]">
          <h2 class="mb-3.5 text-sm font-medium text-[#171717] dark:text-[#ededed]">
            系统服务状态 (systemd D-Bus)
          </h2>
          <div class="grid gap-2 sm:grid-cols-2">
            <For each={props.serviceStatuses}>
              {(unit) => (
                <div class="flex items-center justify-between rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] px-3 py-2 text-xs">
                  <span class="font-mono text-[#171717] dark:text-[#ededed] truncate pr-2">
                    {unit.unit}
                  </span>
                  <span
                    class={`shrink-0 rounded-full px-2 py-0.5 text-[11px] font-medium border ${
                      unit.active_state === "active"
                        ? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-400 border-emerald-500/20"
                        : "bg-zinc-500/10 text-zinc-600 dark:text-zinc-400 border-zinc-500/20"
                    }`}
                  >
                    {unit.active_state} ({unit.sub_state})
                  </span>
                </div>
              )}
            </For>
          </div>
        </div>

        <div class="rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-colors hover:border-[#d4d4d8] dark:hover:border-[#3a3a3a]">
          <h2 class="mb-3.5 text-sm font-medium text-[#171717] dark:text-[#ededed]">
            管理面安全与 HTTPS 设置
          </h2>
          <div class="grid gap-3 text-xs sm:grid-cols-2">
            <label class="flex items-center gap-2 text-[#171717] dark:text-[#ededed]">
              <input
                type="checkbox"
                checked={props.config.system.web_lan_only}
                onChange={(e) => {
                  const next = structuredClone(props.config);
                  next.system.web_lan_only = e.currentTarget.checked;
                  props.onChangeConfig(next);
                }}
              />
              仅监听 LAN 管理网段与回环
            </label>
            <label class="flex items-center gap-2 text-[#171717] dark:text-[#ededed]">
              <input
                type="checkbox"
                checked={props.config.system.web_https_enabled}
                onChange={(e) => {
                  const next = structuredClone(props.config);
                  next.system.web_https_enabled = e.currentTarget.checked;
                  props.onChangeConfig(next);
                }}
              />
              启用 HTTPS (rustls)
            </label>
            <label class="block">
              <span class="text-[#666666] dark:text-[#a1a1aa]">HTTPS 监听地址</span>
              <input
                type="text"
                value={props.config.system.web_https_listen}
                onInput={(e) => {
                  const next = structuredClone(props.config);
                  next.system.web_https_listen = e.currentTarget.value;
                  props.onChangeConfig(next);
                }}
                class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#000000] px-2.5 py-1.5 font-mono text-xs text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed]"
              />
            </label>
            <label class="block">
              <span class="text-[#666666] dark:text-[#a1a1aa]">Commit-Confirm 超时 (秒)</span>
              <input
                type="number"
                min={5}
                value={props.config.system.commit_confirm_timeout_secs}
                onInput={(e) => {
                  const next = structuredClone(props.config);
                  next.system.commit_confirm_timeout_secs = Math.max(
                    5,
                    Number(e.currentTarget.value) || 60,
                  );
                  props.onChangeConfig(next);
                }}
                class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#000000] px-2.5 py-1.5 font-mono text-xs text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed]"
              />
            </label>
          </div>
        </div>
      </section>

      <Show when={props.wanStates.length > 0}>
        <section class="rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-colors hover:border-[#d4d4d8] dark:hover:border-[#3a3a3a]">
          <h2 class="mb-3.5 text-sm font-medium text-[#171717] dark:text-[#ededed]">
            WAN 实时拨号与租约状态
          </h2>
          <div class="grid gap-3 md:grid-cols-2">
            <For each={props.wanStates}>
              {(w) => (
                <div class="rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] p-3.5 text-xs">
                  <div class="flex items-center justify-between">
                    <span class="font-mono font-semibold text-[#171717] dark:text-[#ededed]">
                      {w.wan_name} ({w.device} · {w.proto.toUpperCase()})
                    </span>
                    <span class="rounded-full border border-emerald-500/20 bg-emerald-500/10 px-2 py-0.5 text-[11px] font-medium text-emerald-700 dark:text-emerald-400">
                      {w.state}
                    </span>
                  </div>
                  <div class="mt-2 grid grid-cols-2 gap-1 text-[#666666] dark:text-[#a1a1aa] font-mono">
                    <div>IPv4: {w.ip ?? "-"}</div>
                    <div>网关: {w.gateway ?? "-"}</div>
                    <div>IPv6: {w.ipv6_addr ?? "-"}</div>
                    <div>IPv6-PD: {w.ipv6_pd_prefix ?? "-"}</div>
                  </div>
                  <Show when={w.ipv6_slices.length > 0}>
                    <div class="mt-2.5 flex flex-wrap gap-1.5 border-t border-[#eaeaea] dark:border-[#262626] pt-2">
                      <For each={w.ipv6_slices}>
                        {(sl) => (
                          <span class="rounded-md border border-sky-500/25 bg-sky-500/10 px-2 py-0.5 font-mono text-[11px] text-sky-700 dark:text-sky-400">
                            {sl.interface} → {sl.prefix_cidr}
                          </span>
                        )}
                      </For>
                    </div>
                  </Show>
                </div>
              )}
            </For>
          </div>
        </section>
      </Show>

      <section class="rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-colors hover:border-[#d4d4d8] dark:hover:border-[#3a3a3a]">
        <h2 class="mb-3.5 text-sm font-medium text-[#171717] dark:text-[#ededed]">
          主机网络接口与实时速率
        </h2>
        <div class="overflow-x-auto">
          <table class="w-full text-left text-xs">
            <thead class="border-b border-[#eaeaea] dark:border-[#262626] text-[#666666] dark:text-[#a1a1aa]">
              <tr>
                <th class="py-2 pr-4 font-medium">接口</th>
                <th class="py-2 pr-4 font-medium">状态</th>
                <th class="py-2 pr-4 font-medium">MAC</th>
                <th class="py-2 pr-4 font-medium">速率</th>
                <th class="py-2 pr-4 font-medium">RX 实时 / 累计</th>
                <th class="py-2 font-medium">TX 实时 / 累计</th>
              </tr>
            </thead>
            <tbody class="divide-y divide-[#eaeaea] dark:divide-[#262626]">
              <For each={props.interfaces}>
                {(iface) => {
                  const rate = () => rateFor(iface.name);
                  return (
                    <tr class="hover:bg-[#fafafa] dark:hover:bg-[#111111]/60 transition-colors">
                      <td class="py-2.5 pr-4 font-mono font-medium text-[#171717] dark:text-[#ededed]">
                        {iface.name}
                      </td>
                      <td class="py-2.5 pr-4">
                        <span
                          class={`rounded-full border px-2 py-0.5 text-[11px] font-medium ${
                            iface.oper_state === "up"
                              ? "border-emerald-500/20 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400"
                              : "border-zinc-500/20 bg-zinc-500/10 text-zinc-600 dark:text-zinc-400"
                          }`}
                        >
                          {iface.oper_state}
                        </span>
                      </td>
                      <td class="py-2.5 pr-4 font-mono text-[#666666] dark:text-[#a1a1aa]">
                        {iface.mac ?? "-"}
                      </td>
                      <td class="py-2.5 pr-4 tabular-nums text-[#666666] dark:text-[#a1a1aa]">
                        {iface.speed_mbps ? `${iface.speed_mbps} Mbps` : "-"}
                      </td>
                      <td class="py-2.5 pr-4 font-mono tabular-nums">
                        <span class="text-sky-600 dark:text-sky-400">
                          {rate() ? `${formatBytes(rate()!.rx_bytes_per_sec)}/s` : "-"}
                        </span>{" "}
                        <span class="text-[#8f8f8f] dark:text-[#71717a]">
                          ({formatBytes(iface.rx_bytes)})
                        </span>
                      </td>
                      <td class="py-2.5 font-mono tabular-nums">
                        <span class="text-emerald-600 dark:text-emerald-400">
                          {rate() ? `${formatBytes(rate()!.tx_bytes_per_sec)}/s` : "-"}
                        </span>{" "}
                        <span class="text-[#8f8f8f] dark:text-[#71717a]">
                          ({formatBytes(iface.tx_bytes)})
                        </span>
                      </td>
                    </tr>
                  );
                }}
              </For>
            </tbody>
          </table>
        </div>
      </section>

      <section class="rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-colors hover:border-[#d4d4d8] dark:hover:border-[#3a3a3a]">
        <div class="mb-3.5 flex flex-wrap items-center justify-between gap-2">
          <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">下发配置实时预览</h2>
          <div class="flex flex-wrap gap-1 rounded-xl border border-[#eaeaea] dark:border-[#222222] bg-[#f4f4f5] dark:bg-[#111111] p-1 text-xs">
            <For
              each={
                [
                  "nftables",
                  "tc_qos",
                  "crabdns",
                  "dnsmasq",
                  "wg_quick",
                  "dhcpcd",
                  "udev",
                  "systemd_link",
                  "pppd",
                ] as PreviewTab[]
              }
            >
              {(item) => (
                <button
                  type="button"
                  onClick={() => setTab(item)}
                  class={`rounded-lg px-2.5 py-1 text-xs font-medium cursor-pointer border-0 transition-all ${
                    tab() === item
                      ? "bg-white dark:bg-[#1f1f1f] text-[#171717] dark:text-white shadow-sm ring-1 ring-black/5 dark:ring-white/10"
                      : "bg-transparent text-[#666666] dark:text-[#8f8f8f] hover:text-[#171717] dark:hover:text-[#ededed]"
                  }`}
                >
                  {item}
                </button>
              )}
            </For>
          </div>
        </div>
        <pre class="max-h-80 overflow-auto rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#000000] p-4 font-mono text-xs leading-relaxed text-[#171717] dark:text-[#ededed]">
          {previewText()}
        </pre>
      </section>

      <section class="rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-colors hover:border-[#d4d4d8] dark:hover:border-[#3a3a3a]">
        <h2 class="mb-3.5 text-sm font-medium text-[#171717] dark:text-[#ededed]">
          配置版本快照（redb 持久化历史）
        </h2>
        <Show
          when={props.history.length > 0}
          fallback={
            <p class="text-xs text-[#666666] dark:text-[#a1a1aa]">暂无已确认的变更快照。</p>
          }
        >
          <div class="space-y-2">
            <For each={props.history}>
              {(snap) => (
                <div class="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] px-3.5 py-2.5 text-xs">
                  <div>
                    <span class="font-mono font-semibold text-[#0070f3] dark:text-sky-400">
                      Gen #{snap.generation}
                    </span>
                    <span class="ml-2 text-[#8f8f8f] dark:text-[#71717a]">{snap.created_at}</span>
                    <div class="mt-0.5 text-[#171717] dark:text-[#ededed]">{snap.summary}</div>
                  </div>
                  <button
                    type="button"
                    onClick={() => props.onRestoreHistory(snap.generation)}
                    class="rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#1a1a1a] hover:bg-[#f4f4f5] dark:hover:bg-[#222222] px-3 py-1 text-xs font-medium text-[#171717] dark:text-[#ededed] cursor-pointer transition-colors"
                  >
                    回滚到此版本
                  </button>
                </div>
              )}
            </For>
          </div>
        </Show>
      </section>
    </div>
  );
}

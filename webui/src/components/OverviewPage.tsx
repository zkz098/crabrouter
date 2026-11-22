import { For, Show, createSignal } from "solid-js";
import type { ConfigSnapshotSummary } from "../types/ConfigSnapshotSummary";
import type { CrabDnsAggregateStatus } from "../types/CrabDnsAggregateStatus";
import type { HostInterfaceSnapshot } from "../types/HostInterfaceSnapshot";
import type { RenderedConfigsPreview } from "../types/RenderedConfigsPreview";
import type { RouterConfig } from "../types/RouterConfig";
import type { SystemMetricsSnapshot } from "../types/SystemMetricsSnapshot";
import type { SystemdUnitStatus } from "../types/SystemdUnitStatus";
import type { WanRuntimeState } from "../types/WanRuntimeState";

type PreviewTab =
  | "nftables"
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
    <div class="space-y-6">
      {/* Real-time WebSocket Telemetry Cards */}
      <section class="grid gap-4 md:grid-cols-4">
        <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
          <div class="text-xs text-slate-400">CPU 实时使用率 (WS)</div>
          <div class="mt-1 font-mono text-xl font-semibold text-orange-400">
            {props.metrics ? `${props.metrics.cpu_usage_pct.toFixed(1)}%` : "-"}
          </div>
          <div class="mt-1 text-xs text-slate-400">
            1m 负载: {props.metrics ? props.metrics.load_avg_1m.toFixed(2) : "-"}
            {props.metrics?.temperature_celsius
              ? ` · ${props.metrics.temperature_celsius.toFixed(1)}°C`
              : ""}
          </div>
        </div>

        <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
          <div class="text-xs text-slate-400">内存占用</div>
          <div class="mt-1 font-mono text-xl font-semibold text-sky-400">{memUsedPct()}%</div>
          <div class="mt-1 text-xs text-slate-400">
            {props.metrics
              ? `${formatBytes(props.metrics.memory_used_bytes)} / ${formatBytes(props.metrics.memory_total_bytes)}`
              : "等待遥测..."}
          </div>
        </div>

        <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
          <div class="text-xs text-slate-400">前台 DNS (CrabDNS)</div>
          <div class="mt-1 flex items-center justify-between">
            <span class="font-mono text-xl font-semibold text-orange-400">
              :{props.config.dns.crabdns_port}
            </span>
            <a
              href="/crabdns/"
              target="_blank"
              rel="noreferrer"
              class="rounded-md border border-orange-500/40 bg-orange-500/10 px-2 py-0.5 text-[11px] text-orange-300 hover:bg-orange-500/20"
            >
              打开面板代理 →
            </a>
          </div>
          <div class="mt-1 text-xs text-slate-400">
            {props.dnsStatus?.reachable
              ? `状态正常 (127.0.0.1:${props.dnsStatus.web_port})`
              : `后端 :${props.config.dns.crabdns_web_port} 未运行（干跑/未启动）`}
          </div>
        </div>

        <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
          <div class="text-xs text-slate-400">DHCP & 内网解析 (dnsmasq)</div>
          <div class="mt-1 font-mono text-xl font-semibold text-emerald-400">
            127.0.0.1:{props.config.dns.dnsmasq_dns_port}
          </div>
          <div class="mt-1 text-xs text-slate-400">
            cache-size=0 · 防失联超时 {props.config.system.commit_confirm_timeout_secs}s
          </div>
        </div>
      </section>

      {/* Systemd D-Bus Service Status & CrabDNS Aggregation */}
      <section class="grid gap-4 md:grid-cols-2">
        <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
          <h2 class="mb-3 text-sm font-semibold text-slate-200">
            系统服务状态 (systemd D-Bus / zbus)
          </h2>
          <div class="grid gap-2 sm:grid-cols-2">
            <For each={props.serviceStatuses}>
              {(unit) => (
                <div class="flex items-center justify-between rounded-lg border border-slate-800 bg-slate-950/60 px-3 py-2 text-xs">
                  <span class="font-mono text-slate-200">{unit.unit}</span>
                  <span
                    class={
                      unit.active_state === "active"
                        ? "rounded bg-emerald-500/20 px-2 py-0.5 text-emerald-300"
                        : "rounded bg-slate-700 px-2 py-0.5 text-slate-300"
                    }
                  >
                    {unit.active_state} ({unit.sub_state})
                  </span>
                </div>
              )}
            </For>
          </div>
        </div>

        <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
          <h2 class="mb-3 text-sm font-semibold text-slate-200">管理面安全与 HTTPS 设置</h2>
          <div class="grid gap-3 text-xs sm:grid-cols-2">
            <label class="flex items-center gap-2 text-slate-300">
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
            <label class="flex items-center gap-2 text-slate-300">
              <input
                type="checkbox"
                checked={props.config.system.web_https_enabled}
                onChange={(e) => {
                  const next = structuredClone(props.config);
                  next.system.web_https_enabled = e.currentTarget.checked;
                  props.onChangeConfig(next);
                }}
              />
              启用 HTTPS (rustls 自签/自定义证书)
            </label>
            <label class="block">
              <span class="text-slate-400">HTTPS 监听地址</span>
              <input
                type="text"
                value={props.config.system.web_https_listen}
                onInput={(e) => {
                  const next = structuredClone(props.config);
                  next.system.web_https_listen = e.currentTarget.value;
                  props.onChangeConfig(next);
                }}
                class="mt-1 w-full rounded border border-slate-700 bg-slate-950 px-2 py-1 font-mono"
              />
            </label>
            <label class="block">
              <span class="text-slate-400">Commit-Confirm 超时 (秒)</span>
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
                class="mt-1 w-full rounded border border-slate-700 bg-slate-950 px-2 py-1 font-mono"
              />
            </label>
          </div>
        </div>
      </section>

      <Show when={props.wanStates.length > 0}>
        <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
          <h2 class="mb-3 text-sm font-semibold text-slate-200">WAN 实时拨号与租约状态</h2>
          <div class="grid gap-3 md:grid-cols-2">
            <For each={props.wanStates}>
              {(w) => (
                <div class="rounded-lg border border-slate-800 bg-slate-950/70 p-3 text-xs">
                  <div class="flex items-center justify-between">
                    <span class="font-mono font-semibold text-orange-300">
                      {w.wan_name} ({w.device} · {w.proto.toUpperCase()})
                    </span>
                    <span class="rounded bg-emerald-500/20 px-2 py-0.5 text-emerald-300">
                      {w.state}
                    </span>
                  </div>
                  <div class="mt-2 grid grid-cols-2 gap-1 text-slate-400">
                    <div>IPv4: {w.ip ?? "-"}</div>
                    <div>网关: {w.gateway ?? "-"}</div>
                    <div>IPv6: {w.ipv6_addr ?? "-"}</div>
                    <div>IPv6-PD: {w.ipv6_pd_prefix ?? "-"}</div>
                  </div>
                  <Show when={w.ipv6_slices.length > 0}>
                    <div class="mt-2 flex flex-wrap gap-1.5 border-t border-slate-800/80 pt-2">
                      <For each={w.ipv6_slices}>
                        {(sl) => (
                          <span class="rounded border border-sky-500/40 bg-sky-500/10 px-2 py-0.5 font-mono text-[11px] text-sky-200">
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

      <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
        <h2 class="mb-3 text-sm font-semibold text-slate-200">主机网络接口与实时速率</h2>
        <div class="overflow-x-auto">
          <table class="w-full text-left text-xs">
            <thead class="border-b border-slate-800 text-slate-400">
              <tr>
                <th class="py-2 pr-4">接口</th>
                <th class="py-2 pr-4">状态</th>
                <th class="py-2 pr-4">MAC</th>
                <th class="py-2 pr-4">速率</th>
                <th class="py-2 pr-4">RX 实时 / 累计</th>
                <th class="py-2">TX 实时 / 累计</th>
              </tr>
            </thead>
            <tbody class="divide-y divide-slate-800/70">
              <For each={props.interfaces}>
                {(iface) => {
                  const rate = () => rateFor(iface.name);
                  return (
                    <tr>
                      <td class="py-2 pr-4 font-mono font-medium text-slate-100">{iface.name}</td>
                      <td class="py-2 pr-4">
                        <span
                          class={
                            iface.oper_state === "up"
                              ? "rounded bg-emerald-500/20 px-2 py-0.5 text-emerald-300"
                              : "rounded bg-slate-700 px-2 py-0.5 text-slate-300"
                          }
                        >
                          {iface.oper_state}
                        </span>
                      </td>
                      <td class="py-2 pr-4 font-mono text-slate-400">{iface.mac ?? "-"}</td>
                      <td class="py-2 pr-4 text-slate-300">
                        {iface.speed_mbps ? `${iface.speed_mbps} Mbps` : "-"}
                      </td>
                      <td class="py-2 pr-4 font-mono text-slate-300">
                        <span class="text-sky-300">
                          {rate() ? `${formatBytes(rate()!.rx_bytes_per_sec)}/s` : "-"}
                        </span>{" "}
                        <span class="text-slate-500">({formatBytes(iface.rx_bytes)})</span>
                      </td>
                      <td class="py-2 font-mono text-slate-300">
                        <span class="text-orange-300">
                          {rate() ? `${formatBytes(rate()!.tx_bytes_per_sec)}/s` : "-"}
                        </span>{" "}
                        <span class="text-slate-500">({formatBytes(iface.tx_bytes)})</span>
                      </td>
                    </tr>
                  );
                }}
              </For>
            </tbody>
          </table>
        </div>
      </section>

      <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
        <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
          <h2 class="text-sm font-semibold text-slate-200">下发配置实时预览</h2>
          <div class="flex flex-wrap gap-1 rounded-lg bg-slate-950 p-1 text-xs">
            <For
              each={
                [
                  "nftables",
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
                  class={
                    tab() === item
                      ? "rounded-md bg-slate-800 px-2.5 py-1 font-medium text-orange-400"
                      : "rounded-md px-2.5 py-1 text-slate-400 hover:text-slate-200"
                  }
                >
                  {item}
                </button>
              )}
            </For>
          </div>
        </div>
        <pre class="max-h-80 overflow-auto rounded-lg border border-slate-800 bg-slate-950 p-4 font-mono text-xs leading-relaxed text-slate-300">
          {previewText()}
        </pre>
      </section>

      <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
        <h2 class="mb-3 text-sm font-semibold text-slate-200">配置版本快照（redb 持久化历史）</h2>
        <Show
          when={props.history.length > 0}
          fallback={<p class="text-xs text-slate-400">暂无已确认的变更快照。</p>}
        >
          <div class="space-y-2">
            <For each={props.history}>
              {(snap) => (
                <div class="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-slate-800 bg-slate-950/60 px-3 py-2 text-xs">
                  <div>
                    <span class="font-mono font-semibold text-orange-300">
                      Gen #{snap.generation}
                    </span>
                    <span class="ml-2 text-slate-400">{snap.created_at}</span>
                    <div class="mt-0.5 text-slate-300">{snap.summary}</div>
                  </div>
                  <button
                    type="button"
                    onClick={() => props.onRestoreHistory(snap.generation)}
                    class="rounded-md border border-slate-700 bg-slate-800 px-3 py-1 text-xs hover:bg-slate-700"
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

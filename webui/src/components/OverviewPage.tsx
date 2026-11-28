import { For, Show, createResource, createSignal } from "solid-js";
import { getDailyTraffic } from "../api";
import { t } from "../i18n";
import type { PageTab } from "../nav";
import type { CrabDnsAggregateStatus } from "../types/CrabDnsAggregateStatus";
import type { HardwareHealthSnapshot } from "../types/HardwareHealthSnapshot";
import type { HostInterfaceSnapshot } from "../types/HostInterfaceSnapshot";
import type { RouterConfig } from "../types/RouterConfig";
import type { SystemMetricsSnapshot } from "../types/SystemMetricsSnapshot";
import type { WanRuntimeState } from "../types/WanRuntimeState";
import { PageHeader } from "./ui/ArgonPrimitives";

interface Props {
  config: RouterConfig;
  interfaces: HostInterfaceSnapshot[];
  wanStates: WanRuntimeState[];
  dnsStatus: CrabDnsAggregateStatus | undefined;
  hardware: HardwareHealthSnapshot | undefined;
  metrics: SystemMetricsSnapshot | null;
  onNavigate: (tab: PageTab) => void;
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
  const [guideOpen, setGuideOpen] = createSignal(true);
  const [dailyTraffic] = createResource(() => getDailyTraffic());

  const memUsedPct = () => {
    const m = props.metrics;
    if (!m || m.memory_total_bytes === 0) {
      return 0;
    }
    return Math.round((m.memory_used_bytes / m.memory_total_bytes) * 100);
  };

  const cpuPct = () => (props.metrics ? Math.min(100, Math.round(props.metrics.cpu_usage_pct)) : 0);

  const rateFor = (name: string) => props.metrics?.interfaces.find((i) => i.name === name);

  const checklist = () => {
    const wanUp =
      props.wanStates.some((w) => w.state.toLowerCase() === "up" || Boolean(w.ip)) ||
      props.config.wans.length > 0;
    const lanReady = props.config.lans.some((l) => l.dhcp_enabled);
    const dnsReady = Boolean(props.dnsStatus?.reachable);
    const secReady = props.config.system.web_lan_only || props.config.system.web_https_enabled;
    const fwReady = props.config.firewall.zones.length >= 2;

    return [
      {
        key: "wan",
        ok: wanUp,
        title: t("guideWanTitle"),
        desc: t("guideWanDesc"),
        target: "interfaces" as PageTab,
      },
      {
        key: "lan",
        ok: lanReady,
        title: t("guideLanTitle"),
        desc: t("guideLanDesc"),
        target: "interfaces" as PageTab,
      },
      {
        key: "dns",
        ok: dnsReady,
        title: t("guideDnsTitle"),
        desc: t("guideDnsDesc"),
        target: "dns" as PageTab,
      },
      {
        key: "sec",
        ok: secReady,
        title: t("guideSecTitle"),
        desc: t("guideSecDesc"),
        target: "system" as PageTab,
      },
      {
        key: "fw",
        ok: fwReady,
        title: t("guideFwTitle"),
        desc: t("guideFwDesc"),
        target: "firewall" as PageTab,
      },
    ];
  };

  const completedCount = () => checklist().filter((c) => c.ok).length;

  return (
    <div class="space-y-5 animate-view-enter">
      <PageHeader
        title={t("navOverview")}
        subtitle={t("descOverview")}
        actions={
          <button
            type="button"
            onClick={() => props.onNavigate("services")}
            class="rounded-lg border border-[#e9ecef] dark:border-[#232a3b] bg-white dark:bg-[#131722] hover:bg-[#f8f9fe] dark:hover:bg-[#1a1f2e] px-3 py-1.5 text-xs font-medium text-[#172b4d] dark:text-[#eef1f6] cursor-pointer transition-colors flex items-center gap-1.5"
          >
            <span class="i-ri-server-line text-sm text-[#5e72e4]" />
            <span>{t("navServices")}</span>
          </button>
        }
      />

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

      {/* Guided Router Setup & Health Checklist */}
      <section class="rounded-xl border border-[#5e72e4]/25 bg-white dark:bg-[#131722] argon-card overflow-hidden">
        <div class="px-4 sm:px-5 py-3.5 flex flex-wrap items-center justify-between gap-2 bg-[#5e72e4]/5 border-b border-[#e9ecef] dark:border-[#232a3b]">
          <div class="flex items-center gap-2.5">
            <div class="w-7 h-7 rounded-lg bg-[#5e72e4] text-white flex items-center justify-center shrink-0">
              <span class="i-ri-compass-discover-line text-sm" />
            </div>
            <div>
              <div class="text-xs sm:text-sm font-semibold text-[#172b4d] dark:text-[#eef1f6]">
                {t("ovGuideTitle")}
              </div>
              <div class="text-[11px] text-[#525f7f] dark:text-[#a3adc2]">
                {t("ovGuideSubtitle")}
              </div>
            </div>
          </div>
          <div class="flex items-center gap-2.5">
            <span class="rounded-full bg-[#5e72e4]/12 text-[#5e72e4] dark:text-[#8090f7] px-2.5 py-0.5 text-[11px] font-semibold font-mono">
              {completedCount()} / {checklist().length}
            </span>
            <button
              type="button"
              onClick={() => setGuideOpen((v) => !v)}
              class="rounded-lg border border-[#e9ecef] dark:border-[#232a3b] bg-white dark:bg-[#1a1f2e] px-2.5 py-1 text-[11px] font-medium text-[#525f7f] dark:text-[#a3adc2] cursor-pointer"
            >
              {guideOpen() ? t("ovGuideCollapse") : t("ovGuideExpand")}
            </button>
          </div>
        </div>

        <Show when={guideOpen()}>
          <div class="p-4 sm:p-5 grid gap-2.5 sm:grid-cols-2 lg:grid-cols-5">
            <For each={checklist()}>
              {(item) => (
                <button
                  type="button"
                  onClick={() => props.onNavigate(item.target)}
                  class="group text-left rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-[#f8f9fe] dark:bg-[#1a1f2e]/70 hover:border-[#5e72e4]/50 p-3 transition-all cursor-pointer flex flex-col justify-between gap-2"
                >
                  <div class="flex items-center justify-between gap-2">
                    <span
                      class={`inline-flex items-center gap-1 text-[11px] font-semibold ${
                        item.ok
                          ? "text-emerald-600 dark:text-emerald-400"
                          : "text-amber-600 dark:text-amber-400"
                      }`}
                    >
                      <span
                        class={item.ok ? "i-ri-checkbox-circle-fill" : "i-ri-error-warning-fill"}
                      />
                      <span>{item.ok ? t("ovGuideDone") : t("ovGuideAction")}</span>
                    </span>
                    <span class="i-ri-arrow-right-s-line text-sm text-[#8898aa] group-hover:text-[#5e72e4] transition-transform group-hover:translate-x-0.5" />
                  </div>
                  <div>
                    <div class="text-xs font-semibold text-[#172b4d] dark:text-[#eef1f6]">
                      {item.title}
                    </div>
                    <div class="mt-0.5 text-[11px] text-[#525f7f] dark:text-[#a3adc2] line-clamp-2">
                      {item.desc}
                    </div>
                  </div>
                </button>
              )}
            </For>
          </div>
        </Show>
      </section>

      {/* Real-time WebSocket Telemetry KPI Cards */}
      <section class="grid grid-cols-2 lg:grid-cols-4 gap-3 sm:gap-4">
        <div class="animate-card-enter rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-white dark:bg-[#131722] p-4 sm:p-5 argon-card">
          <div class="flex items-center justify-between gap-2">
            <span class="text-xs font-medium text-[#525f7f] dark:text-[#a3adc2]">
              {t("ovCpuUsage")}
            </span>
            <div class="w-8 h-8 rounded-lg flex items-center justify-center shrink-0 text-sky-600 dark:text-sky-400 bg-sky-500/10">
              <span class="i-ri-cpu-line text-base" />
            </div>
          </div>
          <div class="mt-2.5 text-2xl font-semibold tracking-tight tabular-nums text-[#172b4d] dark:text-[#eef1f6]">
            {props.metrics ? `${props.metrics.cpu_usage_pct.toFixed(1)}%` : "-"}
          </div>
          <div class="mt-2 h-1.5 w-full rounded-full bg-[#e9ecef] dark:bg-[#232a3b] overflow-hidden">
            <div
              class="h-full rounded-full bg-sky-500 transition-all duration-300"
              style={{ width: `${cpuPct()}%` }}
            />
          </div>
          <div class="mt-2 text-[11px] text-[#8898aa] dark:text-[#6c7893] tabular-nums">
            {t("ovLoad1m")}: {props.metrics ? props.metrics.load_avg_1m.toFixed(2) : "-"}
            {props.metrics?.temperature_celsius
              ? ` · ${props.metrics.temperature_celsius.toFixed(1)}°C`
              : ""}
          </div>
        </div>

        <div
          class="animate-card-enter rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-white dark:bg-[#131722] p-4 sm:p-5 argon-card"
          style={{ "animation-delay": "50ms" }}
        >
          <div class="flex items-center justify-between gap-2">
            <span class="text-xs font-medium text-[#525f7f] dark:text-[#a3adc2]">
              {t("ovMemoryUsage")}
            </span>
            <div class="w-8 h-8 rounded-lg flex items-center justify-center shrink-0 text-amber-600 dark:text-amber-400 bg-amber-500/10">
              <span class="i-ri-ram-line text-base" />
            </div>
          </div>
          <div class="mt-2.5 text-2xl font-semibold tracking-tight tabular-nums text-[#172b4d] dark:text-[#eef1f6]">
            {memUsedPct()}%
          </div>
          <div class="mt-2 h-1.5 w-full rounded-full bg-[#e9ecef] dark:bg-[#232a3b] overflow-hidden">
            <div
              class="h-full rounded-full bg-amber-500 transition-all duration-300"
              style={{ width: `${memUsedPct()}%` }}
            />
          </div>
          <div class="mt-2 text-[11px] text-[#8898aa] dark:text-[#6c7893] tabular-nums truncate">
            {props.metrics
              ? `${formatBytes(props.metrics.memory_used_bytes)} / ${formatBytes(props.metrics.memory_total_bytes)}`
              : t("ovWaitingTelemetry")}
          </div>
        </div>

        <div
          class="animate-card-enter rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-white dark:bg-[#131722] p-4 sm:p-5 argon-card"
          style={{ "animation-delay": "100ms" }}
        >
          <div class="flex items-center justify-between gap-2">
            <span class="text-xs font-medium text-[#525f7f] dark:text-[#a3adc2]">
              {t("ovFrontendDns")}
            </span>
            <div class="w-8 h-8 rounded-lg flex items-center justify-center shrink-0 text-emerald-600 dark:text-emerald-400 bg-emerald-500/10">
              <span class="i-ri-shield-check-line text-base" />
            </div>
          </div>
          <div class="mt-2.5 flex items-center justify-between gap-2">
            <span class="font-mono text-2xl font-semibold tracking-tight tabular-nums text-[#172b4d] dark:text-[#eef1f6]">
              :{props.config.dns.crabdns_port}
            </span>
            <a
              href="/crabdns/"
              target="_blank"
              rel="noreferrer"
              class="rounded-lg border border-[#e9ecef] dark:border-[#232a3b] bg-[#f8f9fe] dark:bg-[#1a1f2e] hover:bg-[#eef1f6] px-2 py-1 text-[11px] font-medium text-[#5e72e4] dark:text-[#8090f7] transition-colors whitespace-nowrap"
            >
              {t("ovOpenPanel")}
            </a>
          </div>
          <div class="mt-3 text-[11px] text-[#8898aa] dark:text-[#6c7893] truncate">
            {props.dnsStatus?.reachable
              ? `${t("ovDnsRunning")} (127.0.0.1:${props.dnsStatus.web_port})`
              : `${t("ovDnsNotRunning")} (:${props.config.dns.crabdns_web_port})`}
          </div>
        </div>

        <div
          class="animate-card-enter rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-white dark:bg-[#131722] p-4 sm:p-5 argon-card"
          style={{ "animation-delay": "150ms" }}
        >
          <div class="flex items-center justify-between gap-2">
            <span class="text-xs font-medium text-[#525f7f] dark:text-[#a3adc2]">
              {t("ovDhcpLocalDns")}
            </span>
            <div class="w-8 h-8 rounded-lg flex items-center justify-center shrink-0 text-[#5e72e4] dark:text-[#8090f7] bg-[#5e72e4]/10">
              <span class="i-ri-server-line text-base" />
            </div>
          </div>
          <div class="mt-2.5 font-mono text-2xl font-semibold tracking-tight tabular-nums text-[#172b4d] dark:text-[#eef1f6]">
            :{props.config.dns.dnsmasq_dns_port}
          </div>
          <div class="mt-3 text-[11px] text-[#8898aa] dark:text-[#6c7893] tabular-nums">
            {t("ovRollbackTimeout")} {props.config.system.commit_confirm_timeout_secs}s
          </div>
        </div>
      </section>

      {/* WAN & LAN Summary */}
      <section class="grid gap-4 lg:grid-cols-2">
        <div class="rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-white dark:bg-[#131722] p-4 sm:p-5 argon-card">
          <div class="mb-3.5 flex items-center justify-between">
            <h3 class="text-sm font-semibold text-[#172b4d] dark:text-[#eef1f6]">
              {t("ovWanStatus")}
            </h3>
            <button
              type="button"
              onClick={() => props.onNavigate("interfaces")}
              class="text-xs font-medium text-[#5e72e4] dark:text-[#8090f7] bg-transparent border-0 cursor-pointer hover:underline"
            >
              {t("ovConfigureLink")} →
            </button>
          </div>
          <Show
            when={props.wanStates.length > 0}
            fallback={
              <p class="text-xs text-[#8898aa] dark:text-[#6c7893]">{t("ovNoWanRuntime")}</p>
            }
          >
            <div class="space-y-2.5">
              <For each={props.wanStates}>
                {(w) => (
                  <div class="rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-[#f8f9fe] dark:bg-[#1a1f2e] p-3 text-xs">
                    <div class="flex items-center justify-between">
                      <span class="font-mono font-semibold text-[#172b4d] dark:text-[#eef1f6]">
                        {w.wan_name} ({w.device} · {w.proto.toUpperCase()})
                      </span>
                      <span class="rounded-full border border-emerald-500/20 bg-emerald-500/10 px-2 py-0.5 text-[11px] font-medium text-emerald-700 dark:text-emerald-400">
                        {w.state}
                      </span>
                    </div>
                    <div class="mt-2 grid grid-cols-2 gap-1 text-[#525f7f] dark:text-[#a3adc2] font-mono text-[11px]">
                      <div>IPv4: {w.ip ?? "-"}</div>
                      <div>
                        {t("ovGateway")}: {w.gateway ?? "-"}
                      </div>
                      <div>IPv6: {w.ipv6_addr ?? "-"}</div>
                      <div>IPv6-PD: {w.ipv6_pd_prefix ?? "-"}</div>
                    </div>
                  </div>
                )}
              </For>
            </div>
          </Show>
        </div>

        {/* Quick Shortcuts */}
        <div class="rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-white dark:bg-[#131722] p-4 sm:p-5 argon-card">
          <h3 class="mb-3.5 text-sm font-semibold text-[#172b4d] dark:text-[#eef1f6]">
            {t("ovQuickActions")}
          </h3>
          <div class="grid grid-cols-2 gap-2.5">
            <button
              type="button"
              onClick={() => props.onNavigate("clients")}
              class="rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-[#f8f9fe] dark:bg-[#1a1f2e] hover:border-[#5e72e4]/40 p-3 text-left cursor-pointer transition-all"
            >
              <span class="i-ri-computer-line text-lg text-[#5e72e4]" />
              <div class="mt-1.5 text-xs font-semibold text-[#172b4d] dark:text-[#eef1f6]">
                {t("navClients")}
              </div>
              <div class="text-[11px] text-[#8898aa] dark:text-[#6c7893]">
                {t("ovActionClientsDesc")}
              </div>
            </button>
            <button
              type="button"
              onClick={() => props.onNavigate("firewall")}
              class="rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-[#f8f9fe] dark:bg-[#1a1f2e] hover:border-[#5e72e4]/40 p-3 text-left cursor-pointer transition-all"
            >
              <span class="i-ri-shield-flash-line text-lg text-emerald-500" />
              <div class="mt-1.5 text-xs font-semibold text-[#172b4d] dark:text-[#eef1f6]">
                {t("navFirewall")}
              </div>
              <div class="text-[11px] text-[#8898aa] dark:text-[#6c7893]">
                {t("ovActionFirewallDesc")}
              </div>
            </button>
            <button
              type="button"
              onClick={() => props.onNavigate("diagnostics")}
              class="rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-[#f8f9fe] dark:bg-[#1a1f2e] hover:border-[#5e72e4]/40 p-3 text-left cursor-pointer transition-all"
            >
              <span class="i-ri-terminal-box-line text-lg text-sky-500" />
              <div class="mt-1.5 text-xs font-semibold text-[#172b4d] dark:text-[#eef1f6]">
                {t("navDiagnostics")}
              </div>
              <div class="text-[11px] text-[#8898aa] dark:text-[#6c7893]">
                {t("ovActionDiagDesc")}
              </div>
            </button>
            <button
              type="button"
              onClick={() => props.onNavigate("ota")}
              class="rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-[#f8f9fe] dark:bg-[#1a1f2e] hover:border-[#5e72e4]/40 p-3 text-left cursor-pointer transition-all"
            >
              <span class="i-ri-download-cloud-2-line text-lg text-amber-500" />
              <div class="mt-1.5 text-xs font-semibold text-[#172b4d] dark:text-[#eef1f6]">
                {t("navOta")}
              </div>
              <div class="text-[11px] text-[#8898aa] dark:text-[#6c7893]">
                {t("ovActionOtaDesc")}
              </div>
            </button>
          </div>
        </div>
      </section>

      {/* Host Physical Interfaces */}
      <section class="rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-white dark:bg-[#131722] p-4 sm:p-5 argon-card">
        <div class="mb-3.5 flex items-center justify-between">
          <h3 class="text-sm font-semibold text-[#172b4d] dark:text-[#eef1f6]">
            {t("ovHostInterfaces")}
          </h3>
          <button
            type="button"
            onClick={() => props.onNavigate("conntrack")}
            class="text-xs font-medium text-[#5e72e4] dark:text-[#8090f7] bg-transparent border-0 cursor-pointer hover:underline"
          >
            {t("navConntrack")} →
          </button>
        </div>
        <div class="overflow-x-auto">
          <table class="w-full text-left text-xs">
            <thead class="border-b border-[#e9ecef] dark:border-[#232a3b] text-[#525f7f] dark:text-[#a3adc2]">
              <tr>
                <th class="py-2 pr-4 font-medium">{t("ovColInterface")}</th>
                <th class="py-2 pr-4 font-medium">{t("ovColState")}</th>
                <th class="py-2 pr-4 font-medium">MAC</th>
                <th class="py-2 pr-4 font-medium">{t("ovColSpeed")}</th>
                <th class="py-2 pr-4 font-medium">{t("ovColRx")}</th>
                <th class="py-2 font-medium">{t("ovColTx")}</th>
              </tr>
            </thead>
            <tbody class="divide-y divide-[#e9ecef] dark:divide-[#232a3b]">
              <For each={props.interfaces}>
                {(iface) => {
                  const rate = () => rateFor(iface.name);
                  return (
                    <tr class="hover:bg-[#f8f9fe] dark:hover:bg-[#1a1f2e]/60 transition-colors">
                      <td class="py-2.5 pr-4 font-mono font-medium text-[#172b4d] dark:text-[#eef1f6]">
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
                      <td class="py-2.5 pr-4 font-mono text-[#525f7f] dark:text-[#a3adc2]">
                        {iface.mac ?? "-"}
                      </td>
                      <td class="py-2.5 pr-4 tabular-nums text-[#525f7f] dark:text-[#a3adc2]">
                        {iface.speed_mbps ? `${iface.speed_mbps} Mbps` : "-"}
                      </td>
                      <td class="py-2.5 pr-4 font-mono tabular-nums">
                        <span class="text-sky-600 dark:text-sky-400">
                          {rate() ? `${formatBytes(rate()!.rx_bytes_per_sec)}/s` : "-"}
                        </span>{" "}
                        <span class="text-[#8898aa] dark:text-[#6c7893]">
                          ({formatBytes(iface.rx_bytes)})
                        </span>
                      </td>
                      <td class="py-2.5 font-mono tabular-nums">
                        <span class="text-emerald-600 dark:text-emerald-400">
                          {rate() ? `${formatBytes(rate()!.tx_bytes_per_sec)}/s` : "-"}
                        </span>{" "}
                        <span class="text-[#8898aa] dark:text-[#6c7893]">
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

      <section class="rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-white dark:bg-[#131722] p-4 sm:p-5 argon-card">
        <h3 class="mb-3.5 text-sm font-semibold text-[#172b4d] dark:text-[#eef1f6]">
          每日流量统计报表 (持久化)
        </h3>
        <Show
          when={(dailyTraffic()?.length ?? 0) > 0}
          fallback={
            <p class="text-xs text-[#525f7f] dark:text-[#a3adc2]">
              暂无历史流量记录，守护进程每 30 秒自动累计一次接口流量。
            </p>
          }
        >
          <div class="overflow-x-auto">
            <table class="w-full text-left text-xs">
              <thead>
                <tr class="border-b border-[#e9ecef] dark:border-[#232a3b] text-[#8898aa] dark:text-[#6c7893]">
                  <th class="pb-2 pr-4 font-medium">日期</th>
                  <th class="pb-2 pr-4 font-medium">接口</th>
                  <th class="pb-2 pr-4 font-medium">下行 (RX)</th>
                  <th class="pb-2 pr-4 font-medium">上行 (TX)</th>
                  <th class="pb-2 font-medium">总计</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-[#e9ecef] dark:divide-[#232a3b]">
                <For each={dailyTraffic()}>
                  {(row) => (
                    <tr class="hover:bg-[#f8f9fe] dark:hover:bg-[#1a1f2e]/60 transition-colors">
                      <td class="py-2 pr-4 font-mono text-[#172b4d] dark:text-[#eef1f6]">
                        {row.date}
                      </td>
                      <td class="py-2 pr-4 font-mono text-[#5e72e4] dark:text-[#8090f7]">
                        {row.interface}
                      </td>
                      <td class="py-2 pr-4 font-mono tabular-nums text-sky-600 dark:text-sky-400">
                        {formatBytes(row.rx_bytes)}
                      </td>
                      <td class="py-2 pr-4 font-mono tabular-nums text-emerald-600 dark:text-emerald-400">
                        {formatBytes(row.tx_bytes)}
                      </td>
                      <td class="py-2 font-mono tabular-nums text-[#525f7f] dark:text-[#a3adc2]">
                        {formatBytes(row.rx_bytes + row.tx_bytes)}
                      </td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
        </Show>
      </section>
    </div>
  );
}

import { For, createSignal } from "solid-js";
import { t } from "../i18n";
import type { RenderedConfigsPreview } from "../types/RenderedConfigsPreview";
import type { SystemdUnitStatus } from "../types/SystemdUnitStatus";
import { GuideBanner, PageHeader } from "./ui/ArgonPrimitives";

type PreviewTab =
  | "nftables"
  | "tc_qos"
  | "crabdns"
  | "dnsmasq"
  | "wg_quick"
  | "miniupnpd"
  | "ndppd"
  | "nic_tuning"
  | "dhcpcd"
  | "udev"
  | "systemd_link"
  | "pppd";

interface Props {
  serviceStatuses: SystemdUnitStatus[];
  preview: RenderedConfigsPreview | undefined;
}

export function ServicesPreviewPage(props: Props) {
  const [tab, setTab] = createSignal<PreviewTab>("nftables");

  const previewText = () => {
    const p = props.preview;
    if (!p) {
      return t("commonLoading");
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
      case "miniupnpd":
        return p.miniupnpd;
      case "ndppd":
        return p.ndppd;
      case "nic_tuning":
        return p.nic_tuning;
      case "dhcpcd":
        return p.dhcpcd;
      case "udev":
        return p.udev_rules;
      case "systemd_link":
        return p.systemd_links.length > 0
          ? p.systemd_links.map((f) => `# === ${f.name} ===\n${f.content}`).join("\n")
          : "# No MAC pinning rules configured";
      case "pppd":
        return p.pppd_peers.length > 0
          ? p.pppd_peers.map((f) => `# === ${f.name} ===\n${f.content}`).join("\n")
          : "# No PPPoE WAN configured";
    }
  };

  return (
    <div class="space-y-5 animate-view-enter">
      <PageHeader title={t("navServices")} subtitle={t("descServices")} />

      <GuideBanner
        title={t("svcGuideTitle")}
        description={t("svcGuideDesc")}
        variant="info"
        dismissible
      />

      <section class="rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-white dark:bg-[#131722] p-4 sm:p-5 argon-card">
        <h3 class="mb-3.5 text-sm font-semibold text-[#172b4d] dark:text-[#eef1f6]">
          {t("ovSystemServices")}
        </h3>
        <div class="grid gap-2.5 sm:grid-cols-2 lg:grid-cols-3">
          <For each={props.serviceStatuses}>
            {(unit) => (
              <div class="flex items-center justify-between rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-[#f8f9fe] dark:bg-[#1a1f2e] px-3.5 py-2.5 text-xs">
                <span class="font-mono text-[#172b4d] dark:text-[#eef1f6] truncate pr-2">
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
      </section>

      <section class="rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-white dark:bg-[#131722] p-4 sm:p-5 argon-card">
        <div class="mb-3.5 flex flex-wrap items-center justify-between gap-2">
          <h3 class="text-sm font-semibold text-[#172b4d] dark:text-[#eef1f6]">
            {t("ovConfigPreview")}
          </h3>
          <div class="flex flex-wrap gap-1 rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-[#f8f9fe] dark:bg-[#1a1f2e] p-1 text-xs">
            <For
              each={
                [
                  "nftables",
                  "tc_qos",
                  "crabdns",
                  "dnsmasq",
                  "wg_quick",
                  "miniupnpd",
                  "ndppd",
                  "nic_tuning",
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
                      ? "bg-[#5e72e4] text-white shadow-sm"
                      : "bg-transparent text-[#525f7f] dark:text-[#a3adc2] hover:text-[#172b4d] dark:hover:text-[#eef1f6]"
                  }`}
                >
                  {item}
                </button>
              )}
            </For>
          </div>
        </div>
        <pre class="max-h-96 overflow-auto rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-[#f8f9fe] dark:bg-[#0b0e14] p-4 font-mono text-xs leading-relaxed text-[#172b4d] dark:text-[#eef1f6]">
          {previewText()}
        </pre>
      </section>
    </div>
  );
}

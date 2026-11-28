import { For, Show, createResource, createSignal } from "solid-js";
import { getCrowdSecStatus, getUpnpLeases, triggerCrowdSecSync } from "../api";
import { t } from "../i18n";
import { GuideBanner, PageHeader, SubTabs } from "./ui/ArgonPrimitives";
import type { CrowdSecConfig } from "../types/CrowdSecConfig";
import type { FirewallAction } from "../types/FirewallAction";
import type { Ipv6NatMode } from "../types/Ipv6NatMode";
import type { RouterConfig } from "../types/RouterConfig";
import type { TransportProto } from "../types/TransportProto";
import type { UpnpConfig } from "../types/UpnpConfig";

interface Props {
  draft: RouterConfig;
  onChange: (next: RouterConfig) => void;
}

const SECTION_CLS =
  "rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-colors hover:border-[#d4d4d8] dark:hover:border-[#3a3a3a]";
const ROW_CLS =
  "rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] p-3 text-xs";
const INPUT_CLS =
  "w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] px-2.5 py-1.5 text-xs text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] transition-colors";
const BTN_OUTLINE =
  "rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#111111] hover:bg-[#f4f4f5] dark:hover:bg-[#1a1a1a] px-3 py-1.5 text-xs font-medium text-[#171717] dark:text-[#ededed] cursor-pointer transition-all active:scale-[0.97]";
const BTN_DELETE =
  "text-xs font-medium text-rose-600 dark:text-rose-400 hover:text-rose-700 dark:hover:text-rose-300 bg-transparent border-0 cursor-pointer";

export function FirewallPage(props: Props) {
  const [fwTab, setFwTab] = createSignal<"zones" | "forward" | "crowdsec">("zones");
  const zoneNames = () => props.draft.firewall.zones.map((z) => z.name);
  const [upnpLeases, { refetch: refetchUpnpLeases }] = createResource(getUpnpLeases);
  const [crowdsecStatus, { mutate: setCrowdsecStatus }] = createResource(getCrowdSecStatus);
  const [syncingCs, setSyncingCs] = createSignal(false);

  function updateCrowdSec(patch: Partial<CrowdSecConfig>) {
    const next = structuredClone(props.draft);
    next.crowdsec = { ...next.crowdsec, ...patch };
    props.onChange(next);
  }

  async function syncCrowdSecNow() {
    setSyncingCs(true);
    try {
      const res = await triggerCrowdSecSync();
      setCrowdsecStatus(res);
    } finally {
      setSyncingCs(false);
    }
  }

  function updateUpnp(patch: Partial<UpnpConfig>) {
    const next = structuredClone(props.draft);
    next.upnp = { ...next.upnp, ...patch };
    props.onChange(next);
  }

  function addZone() {
    const next = structuredClone(props.draft);
    next.firewall.zones.push({
      name: `dmz${next.firewall.zones.length + 1}`,
      interfaces: [],
      input: "drop",
      output: "accept",
      forward: "drop",
      masquerade: false,
    });
    props.onChange(next);
  }

  function addAclRule() {
    const next = structuredClone(props.draft);
    next.firewall.rules.push({
      name: `rule-${next.firewall.rules.length + 1}`,
      enabled: true,
      src_zone: zoneNames()[0] ?? "wan",
      dst_zone: null,
      proto: "tcp",
      src_ip: null,
      dst_ip: null,
      dst_port: "443",
      action: "accept",
    });
    props.onChange(next);
  }

  function formatPortRange(start: number, end: number | null): string {
    return end && end > start ? `${start}-${end}` : String(start);
  }

  function parsePortRange(raw: string): { start: number; end: number | null } {
    const trimmed = raw.trim();
    const parts = trimmed.split("-");
    if (parts.length === 2) {
      const start = Math.max(1, Math.min(65535, Number(parts[0]) || 1));
      const end = Math.max(start, Math.min(65535, Number(parts[1]) || start));
      return { start, end: end > start ? end : null };
    }
    const start = Math.max(1, Math.min(65535, Number(trimmed) || 1));
    return { start, end: null };
  }

  function addPortForward() {
    const next = structuredClone(props.draft);
    next.firewall.port_forwards.push({
      name: `fwd-${next.firewall.port_forwards.length + 1}`,
      enabled: true,
      proto: "tcp",
      src_zone: "wan",
      external_port: 8443,
      external_port_end: null,
      dest_ip: "192.168.10.50",
      internal_port: 443,
      internal_port_end: null,
      reflection: true,
    });
    props.onChange(next);
  }

  return (
    <div class="space-y-5 animate-view-enter">
      <PageHeader title={t("navFirewall")} subtitle={t("descFirewall")} />
      <GuideBanner
        title={t("fwGuideTitle")}
        description={t("fwGuideDesc")}
        variant="info"
        dismissible
      />
      <SubTabs
        active={fwTab()}
        onChange={setFwTab}
        items={[
          { key: "zones", label: t("fwSubTabZones"), icon: "i-ri-shield-line" },
          { key: "forward", label: t("fwSubTabForward"), icon: "i-ri-arrow-left-right-line" },
          { key: "crowdsec", label: t("fwSubTabCrowdSec"), icon: "i-ri-radar-line" },
        ]}
      />

      <Show when={fwTab() === "zones"}>
        <section class={SECTION_CLS}>
          <h2 class="mb-3.5 text-sm font-medium text-[#171717] dark:text-[#ededed]">
            {t("fwGlobalTitle")}
          </h2>
          <div class="grid gap-4 sm:grid-cols-5">
            <label class="flex items-center gap-2 text-xs text-[#171717] dark:text-[#ededed]">
              <input
                type="checkbox"
                checked={props.draft.firewall.flowtable_offload}
                onChange={(e) => {
                  const next = structuredClone(props.draft);
                  next.firewall.flowtable_offload = e.currentTarget.checked;
                  props.onChange(next);
                }}
              />
              {t("fwFlowtable")}
            </label>
            <label class="flex items-center gap-2 text-xs text-[#171717] dark:text-[#ededed]">
              <input
                type="checkbox"
                checked={props.draft.firewall.mss_clamping}
                onChange={(e) => {
                  const next = structuredClone(props.draft);
                  next.firewall.mss_clamping = e.currentTarget.checked;
                  props.onChange(next);
                }}
              />
              {t("fwMssClamp")}
            </label>
            <label class="flex items-center gap-2 text-xs text-[#171717] dark:text-[#ededed]">
              <input
                type="checkbox"
                checked={props.draft.firewall.nat_loopback}
                onChange={(e) => {
                  const next = structuredClone(props.draft);
                  next.firewall.nat_loopback = e.currentTarget.checked;
                  props.onChange(next);
                }}
              />
              NAT 回流 (Hairpin NAT)
            </label>
            <label class="flex items-center gap-2 text-xs text-[#171717] dark:text-[#ededed]">
              <input
                type="checkbox"
                checked={props.draft.firewall.syn_flood_protect}
                onChange={(e) => {
                  const next = structuredClone(props.draft);
                  next.firewall.syn_flood_protect = e.currentTarget.checked;
                  props.onChange(next);
                }}
              />
              {t("fwSynFlood")}
            </label>
            <label class="block text-xs">
              <span class="text-[#666666] dark:text-[#a1a1aa]">{t("fwSynRate")}</span>
              <input
                type="number"
                min={1}
                value={props.draft.firewall.syn_flood_rate}
                onInput={(e) => {
                  const next = structuredClone(props.draft);
                  next.firewall.syn_flood_rate = Math.max(1, Number(e.currentTarget.value) || 25);
                  props.onChange(next);
                }}
                class={`mt-1 ${INPUT_CLS} font-mono`}
              />
            </label>
          </div>

          <div class="mt-4 grid gap-3 border-t border-[#eaeaea] pt-3.5 dark:border-[#262626] sm:grid-cols-3">
            <label class="block text-xs">
              <span class="text-[#666666] dark:text-[#a1a1aa]">IPv6 NAT 模式 (NAT66 / NPTv6)</span>
              <select
                value={props.draft.firewall.ipv6_nat_mode}
                onChange={(e) => {
                  const next = structuredClone(props.draft);
                  next.firewall.ipv6_nat_mode = e.currentTarget.value as Ipv6NatMode;
                  props.onChange(next);
                }}
                class={`mt-1 ${INPUT_CLS}`}
              >
                <option value="disabled">禁用 (纯路由)</option>
                <option value="masquerade">NAT66 (Masquerade)</option>
                <option value="nptv6">NPTv6 (1:1 前缀转换)</option>
              </select>
            </label>
            <Show when={props.draft.firewall.ipv6_nat_mode === "nptv6"}>
              <label class="block text-xs">
                <span class="text-[#666666] dark:text-[#a1a1aa]">NPTv6 内网前缀 (ULA)</span>
                <input
                  type="text"
                  placeholder="fd00:10::/64"
                  value={props.draft.firewall.nptv6_internal_prefix ?? ""}
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.firewall.nptv6_internal_prefix = e.currentTarget.value || null;
                    props.onChange(next);
                  }}
                  class={`mt-1 ${INPUT_CLS} font-mono`}
                />
              </label>
              <label class="block text-xs">
                <span class="text-[#666666] dark:text-[#a1a1aa]">NPTv6 外网前缀 (WAN)</span>
                <input
                  type="text"
                  placeholder="2001:db8:1::/64"
                  value={props.draft.firewall.nptv6_external_prefix ?? ""}
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.firewall.nptv6_external_prefix = e.currentTarget.value || null;
                    props.onChange(next);
                  }}
                  class={`mt-1 ${INPUT_CLS} font-mono`}
                />
              </label>
            </Show>
          </div>
        </section>

        {/* Firewall Zones */}
        <section class={SECTION_CLS}>
          <div class="mb-3.5 flex items-center justify-between gap-2">
            <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
              {t("fwZonesTitle")}
            </h2>
            <button type="button" onClick={addZone} class={BTN_OUTLINE}>
              {t("fwAddZone")}
            </button>
          </div>
          <div class="space-y-2.5">
            <For each={props.draft.firewall.zones}>
              {(zone, idx) => (
                <div class={`grid items-end gap-3 sm:grid-cols-7 ${ROW_CLS}`}>
                  <label class="block">
                    <span class="text-[#666666] dark:text-[#a1a1aa]">{t("fwZoneName")}</span>
                    <input
                      type="text"
                      value={zone.name}
                      onInput={(e) => {
                        const next = structuredClone(props.draft);
                        next.firewall.zones[idx()].name = e.currentTarget.value;
                        props.onChange(next);
                      }}
                      class={`mt-1 ${INPUT_CLS} font-mono`}
                    />
                  </label>
                  <label class="block sm:col-span-2">
                    <span class="text-[#666666] dark:text-[#a1a1aa]">{t("fwZoneIfaces")}</span>
                    <input
                      type="text"
                      value={zone.interfaces.join(", ")}
                      onInput={(e) => {
                        const next = structuredClone(props.draft);
                        next.firewall.zones[idx()].interfaces = e.currentTarget.value
                          .split(",")
                          .map((s) => s.trim())
                          .filter(Boolean);
                        props.onChange(next);
                      }}
                      class={`mt-1 ${INPUT_CLS} font-mono`}
                    />
                  </label>
                  <label class="block">
                    <span class="text-[#666666] dark:text-[#a1a1aa]">Input</span>
                    <select
                      value={zone.input}
                      onChange={(e) => {
                        const next = structuredClone(props.draft);
                        next.firewall.zones[idx()].input = e.currentTarget.value as FirewallAction;
                        props.onChange(next);
                      }}
                      class={`mt-1 ${INPUT_CLS}`}
                    >
                      <option value="accept">accept</option>
                      <option value="drop">drop</option>
                      <option value="reject">reject</option>
                    </select>
                  </label>
                  <label class="block">
                    <span class="text-[#666666] dark:text-[#a1a1aa]">Forward</span>
                    <select
                      value={zone.forward}
                      onChange={(e) => {
                        const next = structuredClone(props.draft);
                        next.firewall.zones[idx()].forward = e.currentTarget
                          .value as FirewallAction;
                        props.onChange(next);
                      }}
                      class={`mt-1 ${INPUT_CLS}`}
                    >
                      <option value="accept">accept</option>
                      <option value="drop">drop</option>
                      <option value="reject">reject</option>
                    </select>
                  </label>
                  <label class="flex items-center gap-1.5 pb-1.5 text-[#171717] dark:text-[#ededed]">
                    <input
                      type="checkbox"
                      checked={zone.masquerade}
                      onChange={(e) => {
                        const next = structuredClone(props.draft);
                        next.firewall.zones[idx()].masquerade = e.currentTarget.checked;
                        props.onChange(next);
                      }}
                    />
                    {t("fwNatMasq")}
                  </label>
                  <button
                    type="button"
                    onClick={() => {
                      const next = structuredClone(props.draft);
                      next.firewall.zones.splice(idx(), 1);
                      props.onChange(next);
                    }}
                    class={`pb-1.5 text-right ${BTN_DELETE}`}
                  >
                    {t("commonDelete")}
                  </button>
                </div>
              )}
            </For>
          </div>
        </section>

        {/* Custom ACL Rules */}
        <section class={SECTION_CLS}>
          <div class="mb-3.5 flex flex-wrap items-center justify-between gap-2">
            <div>
              <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
                {t("fwAclTitle")}
              </h2>
              <p class="mt-0.5 text-xs text-[#666666] dark:text-[#a1a1aa]">{t("fwAclDesc")}</p>
            </div>
            <button type="button" onClick={addAclRule} class={BTN_OUTLINE}>
              {t("fwAddAcl")}
            </button>
          </div>
          <div class="space-y-2.5">
            <For each={props.draft.firewall.rules}>
              {(rule, idx) => (
                <div class={`grid items-center gap-2 sm:grid-cols-8 ${ROW_CLS}`}>
                  <label class="flex items-center gap-1.5">
                    <input
                      type="checkbox"
                      checked={rule.enabled}
                      onChange={(e) => {
                        const next = structuredClone(props.draft);
                        next.firewall.rules[idx()].enabled = e.currentTarget.checked;
                        props.onChange(next);
                      }}
                    />
                    <input
                      type="text"
                      value={rule.name}
                      onInput={(e) => {
                        const next = structuredClone(props.draft);
                        next.firewall.rules[idx()].name = e.currentTarget.value;
                        props.onChange(next);
                      }}
                      class={INPUT_CLS}
                    />
                  </label>
                  <select
                    value={rule.src_zone}
                    onChange={(e) => {
                      const next = structuredClone(props.draft);
                      next.firewall.rules[idx()].src_zone = e.currentTarget.value;
                      props.onChange(next);
                    }}
                    class={INPUT_CLS}
                  >
                    <For each={zoneNames()}>
                      {(z) => (
                        <option value={z}>
                          {t("fwSrcPrefix")} {z}
                        </option>
                      )}
                    </For>
                  </select>
                  <select
                    value={rule.dst_zone ?? ""}
                    onChange={(e) => {
                      const next = structuredClone(props.draft);
                      next.firewall.rules[idx()].dst_zone = e.currentTarget.value || null;
                      props.onChange(next);
                    }}
                    class={INPUT_CLS}
                  >
                    <option value="">{t("fwLocalInput")}</option>
                    <For each={zoneNames()}>
                      {(z) => (
                        <option value={z}>
                          {t("fwDstPrefix")} {z}
                        </option>
                      )}
                    </For>
                  </select>
                  <select
                    value={rule.proto}
                    onChange={(e) => {
                      const next = structuredClone(props.draft);
                      next.firewall.rules[idx()].proto = e.currentTarget.value as TransportProto;
                      props.onChange(next);
                    }}
                    class={INPUT_CLS}
                  >
                    <option value="tcp">TCP</option>
                    <option value="udp">UDP</option>
                    <option value="tcp_udp">TCP+UDP</option>
                    <option value="icmp">ICMP</option>
                    <option value="any">ANY</option>
                  </select>
                  <input
                    type="text"
                    value={rule.src_ip ?? ""}
                    placeholder="源 IP / CIDR / ::1234/::ffff:ffff:ffff:ffff"
                    onInput={(e) => {
                      const next = structuredClone(props.draft);
                      next.firewall.rules[idx()].src_ip = e.currentTarget.value || null;
                      props.onChange(next);
                    }}
                    class={`${INPUT_CLS} font-mono`}
                  />
                  <input
                    type="text"
                    value={rule.dst_port ?? ""}
                    placeholder={t("fwDstPortPlaceholder")}
                    onInput={(e) => {
                      const next = structuredClone(props.draft);
                      next.firewall.rules[idx()].dst_port = e.currentTarget.value || null;
                      props.onChange(next);
                    }}
                    class={`${INPUT_CLS} font-mono`}
                  />
                  <select
                    value={rule.action}
                    onChange={(e) => {
                      const next = structuredClone(props.draft);
                      next.firewall.rules[idx()].action = e.currentTarget.value as FirewallAction;
                      props.onChange(next);
                    }}
                    class={INPUT_CLS}
                  >
                    <option value="accept">{t("fwActionAccept")}</option>
                    <option value="drop">{t("fwActionDrop")}</option>
                    <option value="reject">{t("fwActionReject")}</option>
                  </select>
                  <button
                    type="button"
                    onClick={() => {
                      const next = structuredClone(props.draft);
                      next.firewall.rules.splice(idx(), 1);
                      props.onChange(next);
                    }}
                    class={`text-right ${BTN_DELETE}`}
                  >
                    {t("commonDelete")}
                  </button>
                </div>
              )}
            </For>
          </div>
        </section>
      </Show>

      <Show when={fwTab() === "forward"}>
        {/* Port Forwarding */}
        <section class={SECTION_CLS}>
          <div class="mb-3.5 flex items-center justify-between gap-2">
            <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
              {t("fwPortFwdTitle")}
            </h2>
            <button type="button" onClick={addPortForward} class={BTN_OUTLINE}>
              {t("fwAddPortFwd")}
            </button>
          </div>
          <div class="space-y-2.5">
            <For each={props.draft.firewall.port_forwards}>
              {(fwd, idx) => (
                <div class={`grid items-center gap-2 sm:grid-cols-8 ${ROW_CLS}`}>
                  <label class="flex items-center gap-1.5">
                    <input
                      type="checkbox"
                      checked={fwd.enabled}
                      onChange={(e) => {
                        const next = structuredClone(props.draft);
                        next.firewall.port_forwards[idx()].enabled = e.currentTarget.checked;
                        props.onChange(next);
                      }}
                    />
                    <input
                      type="text"
                      value={fwd.name}
                      onInput={(e) => {
                        const next = structuredClone(props.draft);
                        next.firewall.port_forwards[idx()].name = e.currentTarget.value;
                        props.onChange(next);
                      }}
                      class={INPUT_CLS}
                    />
                  </label>
                  <select
                    value={fwd.proto}
                    onChange={(e) => {
                      const next = structuredClone(props.draft);
                      next.firewall.port_forwards[idx()].proto = e.currentTarget
                        .value as TransportProto;
                      props.onChange(next);
                    }}
                    class={INPUT_CLS}
                  >
                    <option value="tcp">TCP</option>
                    <option value="udp">UDP</option>
                    <option value="tcp_udp">TCP+UDP</option>
                  </select>
                  <select
                    value={fwd.src_zone}
                    onChange={(e) => {
                      const next = structuredClone(props.draft);
                      next.firewall.port_forwards[idx()].src_zone = e.currentTarget.value;
                      props.onChange(next);
                    }}
                    class={INPUT_CLS}
                  >
                    <For each={zoneNames()}>{(z) => <option value={z}>{z}</option>}</For>
                  </select>
                  <input
                    type="text"
                    placeholder="8443 或 8000-8100"
                    value={formatPortRange(fwd.external_port, fwd.external_port_end)}
                    onChange={(e) => {
                      const next = structuredClone(props.draft);
                      const { start, end } = parsePortRange(e.currentTarget.value);
                      next.firewall.port_forwards[idx()].external_port = start;
                      next.firewall.port_forwards[idx()].external_port_end = end;
                      props.onChange(next);
                    }}
                    class={`${INPUT_CLS} font-mono`}
                  />
                  <input
                    type="text"
                    value={fwd.dest_ip}
                    onInput={(e) => {
                      const next = structuredClone(props.draft);
                      next.firewall.port_forwards[idx()].dest_ip = e.currentTarget.value;
                      props.onChange(next);
                    }}
                    class={`${INPUT_CLS} font-mono`}
                  />
                  <input
                    type="text"
                    placeholder="443 或 8000-8100"
                    value={formatPortRange(fwd.internal_port, fwd.internal_port_end)}
                    onChange={(e) => {
                      const next = structuredClone(props.draft);
                      const { start, end } = parsePortRange(e.currentTarget.value);
                      next.firewall.port_forwards[idx()].internal_port = start;
                      next.firewall.port_forwards[idx()].internal_port_end = end;
                      props.onChange(next);
                    }}
                    class={`${INPUT_CLS} font-mono`}
                  />
                  <label class="flex items-center gap-1.5 text-[#171717] dark:text-[#ededed]">
                    <input
                      type="checkbox"
                      checked={fwd.reflection}
                      onChange={(e) => {
                        const next = structuredClone(props.draft);
                        next.firewall.port_forwards[idx()].reflection = e.currentTarget.checked;
                        props.onChange(next);
                      }}
                    />
                    回流
                  </label>
                  <button
                    type="button"
                    onClick={() => {
                      const next = structuredClone(props.draft);
                      next.firewall.port_forwards.splice(idx(), 1);
                      props.onChange(next);
                    }}
                    class={`text-right ${BTN_DELETE}`}
                  >
                    {t("commonDelete")}
                  </button>
                </div>
              )}
            </For>
          </div>
        </section>

        {/* UPnP / NAT-PMP / PCP */}
        <section class={SECTION_CLS}>
          <div class="mb-3.5 flex flex-wrap items-center justify-between gap-2">
            <div>
              <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
                UPnP IGD / NAT-PMP / PCP
              </h2>
              <p class="mt-0.5 text-xs text-[#666666] dark:text-[#a1a1aa]">
                允许内网游戏主机与 P2P 应用通过 miniupnpd (nftables) 自动申请动态端口映射
              </p>
            </div>
            <label class="flex items-center gap-2 text-xs font-medium text-[#171717] dark:text-[#ededed]">
              <input
                type="checkbox"
                checked={props.draft.upnp.enabled}
                onChange={(e) => updateUpnp({ enabled: e.currentTarget.checked })}
              />
              启用 UPnP / NAT-PMP / PCP
            </label>
          </div>

          <Show when={props.draft.upnp.enabled}>
            <div class="space-y-4">
              <div class="grid gap-3 sm:grid-cols-3">
                <label class="flex items-center gap-2 text-xs text-[#171717] dark:text-[#ededed]">
                  <input
                    type="checkbox"
                    checked={props.draft.upnp.enable_upnp}
                    onChange={(e) => updateUpnp({ enable_upnp: e.currentTarget.checked })}
                  />
                  启用 UPnP IGD (SSDP/SOAP)
                </label>
                <label class="flex items-center gap-2 text-xs text-[#171717] dark:text-[#ededed]">
                  <input
                    type="checkbox"
                    checked={props.draft.upnp.enable_natpmp}
                    onChange={(e) => updateUpnp({ enable_natpmp: e.currentTarget.checked })}
                  />
                  启用 NAT-PMP / PCP (UDP 5351)
                </label>
                <label class="flex items-center gap-2 text-xs text-[#171717] dark:text-[#ededed]">
                  <input
                    type="checkbox"
                    checked={props.draft.upnp.secure_mode}
                    onChange={(e) => updateUpnp({ secure_mode: e.currentTarget.checked })}
                  />
                  安全模式 (仅允许映射到请求者自身 IP)
                </label>
              </div>

              <div class="grid gap-3 sm:grid-cols-4">
                <label class="block text-xs">
                  <span class="text-[#666666] dark:text-[#a1a1aa]">外部起始端口</span>
                  <input
                    type="number"
                    min={1}
                    max={65535}
                    value={props.draft.upnp.min_ext_port}
                    onInput={(e) =>
                      updateUpnp({
                        min_ext_port: Math.max(1, Number(e.currentTarget.value) || 1024),
                      })
                    }
                    class={`mt-1 ${INPUT_CLS} font-mono`}
                  />
                </label>
                <label class="block text-xs">
                  <span class="text-[#666666] dark:text-[#a1a1aa]">外部结束端口</span>
                  <input
                    type="number"
                    min={1}
                    max={65535}
                    value={props.draft.upnp.max_ext_port}
                    onInput={(e) =>
                      updateUpnp({
                        max_ext_port: Math.min(65535, Number(e.currentTarget.value) || 65535),
                      })
                    }
                    class={`mt-1 ${INPUT_CLS} font-mono`}
                  />
                </label>
                <label class="block text-xs">
                  <span class="text-[#666666] dark:text-[#a1a1aa]">最大租约时长 (秒)</span>
                  <input
                    type="number"
                    min={60}
                    value={props.draft.upnp.lease_duration_secs}
                    onInput={(e) =>
                      updateUpnp({
                        lease_duration_secs: Math.max(60, Number(e.currentTarget.value) || 3600),
                      })
                    }
                    class={`mt-1 ${INPUT_CLS} font-mono`}
                  />
                </label>
                <label class="block text-xs">
                  <span class="text-[#666666] dark:text-[#a1a1aa]">指定 WAN 接口 (可选)</span>
                  <input
                    type="text"
                    placeholder="自动 (主 WAN / ppp0)"
                    value={props.draft.upnp.wan_interface ?? ""}
                    onInput={(e) => {
                      const val = e.currentTarget.value.trim();
                      updateUpnp({ wan_interface: val === "" ? null : val });
                    }}
                    class={`mt-1 ${INPUT_CLS} font-mono`}
                  />
                </label>
              </div>

              <div class="rounded-lg border border-[#eaeaea] dark:border-[#262626] p-3">
                <div class="mb-2 flex items-center justify-between">
                  <span class="text-xs font-medium text-[#171717] dark:text-[#ededed]">
                    活跃动态映射 ({upnpLeases()?.length ?? 0})
                  </span>
                  <button
                    type="button"
                    onClick={() => void refetchUpnpLeases()}
                    class={BTN_OUTLINE}
                  >
                    刷新
                  </button>
                </div>
                <Show
                  when={(upnpLeases()?.length ?? 0) > 0}
                  fallback={
                    <p class="text-xs text-[#666666] dark:text-[#a1a1aa]">暂无活跃 UPnP/PCP 映射</p>
                  }
                >
                  <div class="overflow-x-auto">
                    <table class="w-full text-left text-xs font-mono">
                      <thead class="text-[#666666] dark:text-[#a1a1aa]">
                        <tr>
                          <th class="py-1 pr-3">协议</th>
                          <th class="py-1 pr-3">外部端口</th>
                          <th class="py-1 pr-3">内网目标</th>
                          <th class="py-1">描述</th>
                        </tr>
                      </thead>
                      <tbody>
                        <For each={upnpLeases() ?? []}>
                          {(entry) => (
                            <tr class="border-t border-[#eaeaea] dark:border-[#262626]">
                              <td class="py-1 pr-3">{entry.proto}</td>
                              <td class="py-1 pr-3">{entry.external_port}</td>
                              <td class="py-1 pr-3">
                                {entry.internal_ip}:{entry.internal_port}
                              </td>
                              <td class="py-1 font-sans">{entry.description}</td>
                            </tr>
                          )}
                        </For>
                      </tbody>
                    </table>
                  </div>
                </Show>
              </div>
            </div>
          </Show>
        </section>
      </Show>

      <Show when={fwTab() === "crowdsec"}>
        <section class={SECTION_CLS}>
          <div class="mb-3.5 flex flex-wrap items-center justify-between gap-2">
            <div>
              <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
                CrowdSec 智能威胁防御 (LAPI + nftables / XDP)
              </h2>
              <p class="mt-0.5 text-xs text-[#666666] dark:text-[#a1a1aa]">
                基于日志行为分析与协同威胁情报自动拦截暴力破解、端口扫描及恶意僵尸网络 IP
              </p>
            </div>
            <label class="flex items-center gap-2 text-xs text-[#171717] dark:text-[#ededed]">
              <input
                type="checkbox"
                checked={props.draft.crowdsec.enabled}
                onChange={(e) => updateCrowdSec({ enabled: e.currentTarget.checked })}
              />
              启用 CrowdSec
            </label>
          </div>

          <Show when={props.draft.crowdsec.enabled}>
            <div class="space-y-4">
              <div class="grid gap-3 sm:grid-cols-3 text-xs">
                <label class="flex items-center gap-2 text-[#171717] dark:text-[#ededed]">
                  <input
                    type="checkbox"
                    checked={props.draft.crowdsec.bouncer_enabled}
                    onChange={(e) => updateCrowdSec({ bouncer_enabled: e.currentTarget.checked })}
                  />
                  启用 nftables Bouncer 封禁集合
                </label>
                <label class="flex items-center gap-2 text-[#171717] dark:text-[#ededed]">
                  <input
                    type="checkbox"
                    checked={props.draft.crowdsec.sync_to_xdp}
                    onChange={(e) => updateCrowdSec({ sync_to_xdp: e.currentTarget.checked })}
                  />
                  同步封禁列表至 eBPF XDP 驱动层
                </label>
                <label class="block">
                  <span class="text-[#666666] dark:text-[#a1a1aa]">决策同步周期 (秒)</span>
                  <input
                    type="number"
                    min={5}
                    value={props.draft.crowdsec.update_frequency_secs}
                    onInput={(e) =>
                      updateCrowdSec({
                        update_frequency_secs: Math.max(5, Number(e.currentTarget.value) || 10),
                      })
                    }
                    class={`mt-1 ${INPUT_CLS} font-mono`}
                  />
                </label>
              </div>

              <div class="grid gap-3 sm:grid-cols-2 text-xs">
                <label class="block">
                  <span class="text-[#666666] dark:text-[#a1a1aa]">LAPI 监听地址</span>
                  <input
                    type="text"
                    value={props.draft.crowdsec.lapi_listen}
                    onInput={(e) => updateCrowdSec({ lapi_listen: e.currentTarget.value.trim() })}
                    class={`mt-1 ${INPUT_CLS} font-mono`}
                  />
                </label>
                <label class="block">
                  <span class="text-[#666666] dark:text-[#a1a1aa]">
                    白名单 IP / CIDR (逗号分隔)
                  </span>
                  <input
                    type="text"
                    placeholder="192.168.10.0/24"
                    value={props.draft.crowdsec.whitelist_cidrs.join(", ")}
                    onInput={(e) =>
                      updateCrowdSec({
                        whitelist_cidrs: e.currentTarget.value
                          .split(",")
                          .map((s) => s.trim())
                          .filter(Boolean),
                      })
                    }
                    class={`mt-1 ${INPUT_CLS} font-mono`}
                  />
                </label>
              </div>

              <div class="rounded-lg border border-[#eaeaea] dark:border-[#262626] p-3">
                <div class="mb-2 flex items-center justify-between">
                  <span class="text-xs font-medium text-[#171717] dark:text-[#ededed]">
                    活跃封禁决策 ({crowdsecStatus()?.active_decisions ?? 0})
                  </span>
                  <button
                    type="button"
                    disabled={syncingCs()}
                    onClick={() => void syncCrowdSecNow()}
                    class={BTN_OUTLINE}
                  >
                    {syncingCs() ? "同步中..." : "立即同步"}
                  </button>
                </div>
                <Show
                  when={(crowdsecStatus()?.decisions.length ?? 0) > 0}
                  fallback={
                    <p class="text-xs text-[#666666] dark:text-[#a1a1aa]">
                      暂无活跃 CrowdSec 封禁决策
                    </p>
                  }
                >
                  <div class="overflow-x-auto">
                    <table class="w-full text-left text-xs font-mono">
                      <thead class="text-[#666666] dark:text-[#a1a1aa]">
                        <tr>
                          <th class="py-1 pr-3">IP / CIDR</th>
                          <th class="py-1 pr-3">动作</th>
                          <th class="py-1 pr-3">触发场景</th>
                          <th class="py-1">剩余时长</th>
                        </tr>
                      </thead>
                      <tbody>
                        <For each={crowdsecStatus()?.decisions ?? []}>
                          {(dec) => (
                            <tr class="border-t border-[#eaeaea] dark:border-[#262626]">
                              <td class="py-1 pr-3">{dec.value}</td>
                              <td class="py-1 pr-3">{dec.decision_type}</td>
                              <td class="py-1 pr-3 font-sans">{dec.scenario}</td>
                              <td class="py-1">{dec.duration}</td>
                            </tr>
                          )}
                        </For>
                      </tbody>
                    </table>
                  </div>
                </Show>
              </div>
            </div>
          </Show>
        </section>
      </Show>
    </div>
  );
}

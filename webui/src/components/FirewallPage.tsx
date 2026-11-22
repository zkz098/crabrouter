import { For } from "solid-js";
import type { FirewallAction } from "../types/FirewallAction";
import type { RouterConfig } from "../types/RouterConfig";
import type { TransportProto } from "../types/TransportProto";

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
  const zoneNames = () => props.draft.firewall.zones.map((z) => z.name);

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

  function addPortForward() {
    const next = structuredClone(props.draft);
    next.firewall.port_forwards.push({
      name: `fwd-${next.firewall.port_forwards.length + 1}`,
      enabled: true,
      proto: "tcp",
      src_zone: "wan",
      external_port: 8443,
      dest_ip: "192.168.10.50",
      internal_port: 443,
    });
    props.onChange(next);
  }

  return (
    <div class="space-y-5 sm:space-y-6 animate-view-enter">
      <section class={SECTION_CLS}>
        <h2 class="mb-3.5 text-sm font-medium text-[#171717] dark:text-[#ededed]">
          全局防火墙与转发加速参数
        </h2>
        <div class="grid gap-4 sm:grid-cols-4">
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
            启用 nftables Flowtable 加速
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
            启用 TCP MSS 钳制 (PPPoE)
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
            启用 SYN Flood 限速防护
          </label>
          <label class="block text-xs">
            <span class="text-[#666666] dark:text-[#a1a1aa]">SYN 限速 (pkt/s)</span>
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
      </section>

      {/* Firewall Zones */}
      <section class={SECTION_CLS}>
        <div class="mb-3.5 flex items-center justify-between gap-2">
          <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
            安全区域 (Firewall Zones)
          </h2>
          <button type="button" onClick={addZone} class={BTN_OUTLINE}>
            + 新增 Zone
          </button>
        </div>
        <div class="space-y-2.5">
          <For each={props.draft.firewall.zones}>
            {(zone, idx) => (
              <div class={`grid items-end gap-3 sm:grid-cols-7 ${ROW_CLS}`}>
                <label class="block">
                  <span class="text-[#666666] dark:text-[#a1a1aa]">Zone 名</span>
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
                  <span class="text-[#666666] dark:text-[#a1a1aa]">绑定接口 (逗号分隔)</span>
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
                      next.firewall.zones[idx()].forward = e.currentTarget.value as FirewallAction;
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
                  NAT 伪装
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
                  删除
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
              自定义访问控制规则 (ACL)
            </h2>
            <p class="mt-0.5 text-xs text-[#666666] dark:text-[#a1a1aa]">
              目标 Zone 为空时作用于路由器本机 Input；指定目标 Zone 时作用于 Zone 间 Forward。
            </p>
          </div>
          <button type="button" onClick={addAclRule} class={BTN_OUTLINE}>
            + 新增 ACL 规则
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
                  <For each={zoneNames()}>{(z) => <option value={z}>源: {z}</option>}</For>
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
                  <option value="">本机 (Input)</option>
                  <For each={zoneNames()}>{(z) => <option value={z}>目标: {z}</option>}</For>
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
                  placeholder="源 IP/CIDR"
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
                  placeholder="端口 (如 443)"
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
                  <option value="accept">放行 (accept)</option>
                  <option value="drop">丢弃 (drop)</option>
                  <option value="reject">拒绝 (reject)</option>
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
                  删除
                </button>
              </div>
            )}
          </For>
        </div>
      </section>

      {/* Port Forwarding */}
      <section class={SECTION_CLS}>
        <div class="mb-3.5 flex items-center justify-between gap-2">
          <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">端口转发 (DNAT)</h2>
          <button type="button" onClick={addPortForward} class={BTN_OUTLINE}>
            + 新增转发
          </button>
        </div>
        <div class="space-y-2.5">
          <For each={props.draft.firewall.port_forwards}>
            {(fwd, idx) => (
              <div class={`grid items-center gap-2 sm:grid-cols-7 ${ROW_CLS}`}>
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
                  type="number"
                  value={fwd.external_port}
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.firewall.port_forwards[idx()].external_port =
                      Number(e.currentTarget.value) || 1;
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
                  type="number"
                  value={fwd.internal_port}
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.firewall.port_forwards[idx()].internal_port =
                      Number(e.currentTarget.value) || 1;
                    props.onChange(next);
                  }}
                  class={`${INPUT_CLS} font-mono`}
                />
                <button
                  type="button"
                  onClick={() => {
                    const next = structuredClone(props.draft);
                    next.firewall.port_forwards.splice(idx(), 1);
                    props.onChange(next);
                  }}
                  class={`text-right ${BTN_DELETE}`}
                >
                  删除
                </button>
              </div>
            )}
          </For>
        </div>
      </section>
    </div>
  );
}

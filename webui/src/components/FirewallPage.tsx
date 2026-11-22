import { For } from "solid-js";
import type { FirewallAction } from "../types/FirewallAction";
import type { RouterConfig } from "../types/RouterConfig";
import type { TransportProto } from "../types/TransportProto";

interface Props {
  draft: RouterConfig;
  onChange: (next: RouterConfig) => void;
}

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
    <div class="space-y-6">
      <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
        <h2 class="mb-3 text-sm font-semibold text-slate-200">全局防火墙与转发加速参数</h2>
        <div class="grid gap-4 sm:grid-cols-4">
          <label class="flex items-center gap-2 text-xs text-slate-300">
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
          <label class="flex items-center gap-2 text-xs text-slate-300">
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
          <label class="flex items-center gap-2 text-xs text-slate-300">
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
            <span class="text-slate-400">SYN 限速 (pkt/s)</span>
            <input
              type="number"
              min={1}
              value={props.draft.firewall.syn_flood_rate}
              onInput={(e) => {
                const next = structuredClone(props.draft);
                next.firewall.syn_flood_rate = Math.max(1, Number(e.currentTarget.value) || 25);
                props.onChange(next);
              }}
              class="mt-1 w-full rounded border border-slate-700 bg-slate-950 px-2.5 py-1 font-mono text-xs"
            />
          </label>
        </div>
      </section>

      {/* Firewall Zones */}
      <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
        <div class="mb-3 flex items-center justify-between">
          <h2 class="text-sm font-semibold text-slate-200">安全区域 (Firewall Zones)</h2>
          <button
            type="button"
            onClick={addZone}
            class="rounded-lg border border-slate-700 bg-slate-800 px-3 py-1 text-xs hover:bg-slate-700"
          >
            + 新增 Zone
          </button>
        </div>
        <div class="space-y-3">
          <For each={props.draft.firewall.zones}>
            {(zone, idx) => (
              <div class="grid items-end gap-3 rounded-xl border border-slate-800 bg-slate-950/60 p-3 text-xs sm:grid-cols-7">
                <label class="block">
                  <span class="text-slate-400">Zone 名</span>
                  <input
                    type="text"
                    value={zone.name}
                    onInput={(e) => {
                      const next = structuredClone(props.draft);
                      next.firewall.zones[idx()].name = e.currentTarget.value;
                      props.onChange(next);
                    }}
                    class="mt-1 w-full rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono"
                  />
                </label>
                <label class="block sm:col-span-2">
                  <span class="text-slate-400">绑定接口 (逗号分隔)</span>
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
                    class="mt-1 w-full rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono"
                  />
                </label>
                <label class="block">
                  <span class="text-slate-400">Input</span>
                  <select
                    value={zone.input}
                    onChange={(e) => {
                      const next = structuredClone(props.draft);
                      next.firewall.zones[idx()].input = e.currentTarget.value as FirewallAction;
                      props.onChange(next);
                    }}
                    class="mt-1 w-full rounded border border-slate-700 bg-slate-900 px-2 py-1"
                  >
                    <option value="accept">accept</option>
                    <option value="drop">drop</option>
                    <option value="reject">reject</option>
                  </select>
                </label>
                <label class="block">
                  <span class="text-slate-400">Forward</span>
                  <select
                    value={zone.forward}
                    onChange={(e) => {
                      const next = structuredClone(props.draft);
                      next.firewall.zones[idx()].forward = e.currentTarget.value as FirewallAction;
                      props.onChange(next);
                    }}
                    class="mt-1 w-full rounded border border-slate-700 bg-slate-900 px-2 py-1"
                  >
                    <option value="accept">accept</option>
                    <option value="drop">drop</option>
                    <option value="reject">reject</option>
                  </select>
                </label>
                <label class="flex items-center gap-1.5 pb-1 text-slate-300">
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
                  class="pb-1 text-right text-red-400 hover:text-red-300"
                >
                  删除
                </button>
              </div>
            )}
          </For>
        </div>
      </section>

      {/* Custom ACL Rules */}
      <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
        <div class="mb-3 flex items-center justify-between">
          <div>
            <h2 class="text-sm font-semibold text-slate-200">自定义访问控制规则 (ACL)</h2>
            <p class="text-xs text-slate-400">
              目标 Zone 为空时作用于路由器本机 Input；指定目标 Zone 时作用于 Zone 间 Forward。
            </p>
          </div>
          <button
            type="button"
            onClick={addAclRule}
            class="rounded-lg border border-slate-700 bg-slate-800 px-3 py-1 text-xs hover:bg-slate-700"
          >
            + 新增 ACL 规则
          </button>
        </div>
        <div class="space-y-2">
          <For each={props.draft.firewall.rules}>
            {(rule, idx) => (
              <div class="grid items-center gap-2 rounded-lg border border-slate-800 bg-slate-950/60 p-3 text-xs sm:grid-cols-8">
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
                    class="w-full rounded border border-slate-700 bg-slate-900 px-2 py-1"
                  />
                </label>
                <select
                  value={rule.src_zone}
                  onChange={(e) => {
                    const next = structuredClone(props.draft);
                    next.firewall.rules[idx()].src_zone = e.currentTarget.value;
                    props.onChange(next);
                  }}
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1"
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
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1"
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
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1"
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
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono"
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
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono"
                />
                <select
                  value={rule.action}
                  onChange={(e) => {
                    const next = structuredClone(props.draft);
                    next.firewall.rules[idx()].action = e.currentTarget.value as FirewallAction;
                    props.onChange(next);
                  }}
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1"
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
                  class="text-right text-red-400 hover:text-red-300"
                >
                  删除
                </button>
              </div>
            )}
          </For>
        </div>
      </section>

      {/* Port Forwarding */}
      <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
        <div class="mb-3 flex items-center justify-between">
          <h2 class="text-sm font-semibold text-slate-200">端口转发 (DNAT)</h2>
          <button
            type="button"
            onClick={addPortForward}
            class="rounded-lg border border-slate-700 bg-slate-800 px-3 py-1 text-xs hover:bg-slate-700"
          >
            + 新增转发
          </button>
        </div>
        <div class="space-y-2">
          <For each={props.draft.firewall.port_forwards}>
            {(fwd, idx) => (
              <div class="grid items-center gap-2 rounded-lg border border-slate-800 bg-slate-950/60 p-3 text-xs sm:grid-cols-7">
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
                    class="w-full rounded border border-slate-700 bg-slate-900 px-2 py-1"
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
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1"
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
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1"
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
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono"
                />
                <input
                  type="text"
                  value={fwd.dest_ip}
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.firewall.port_forwards[idx()].dest_ip = e.currentTarget.value;
                    props.onChange(next);
                  }}
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono"
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
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono"
                />
                <button
                  type="button"
                  onClick={() => {
                    const next = structuredClone(props.draft);
                    next.firewall.port_forwards.splice(idx(), 1);
                    props.onChange(next);
                  }}
                  class="text-right text-red-400 hover:text-red-300"
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

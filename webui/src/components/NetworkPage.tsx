import { For, Show, createSignal } from "solid-js";
import type { HostInterfaceSnapshot } from "../types/HostInterfaceSnapshot";
import type { InterfaceRole } from "../types/InterfaceRole";
import type { LanConfig } from "../types/LanConfig";
import type { RouterConfig } from "../types/RouterConfig";
import type { WanConfig } from "../types/WanConfig";
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
      metric: 100 + next.wans.length * 10,
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

  function addVlan() {
    const next = structuredClone(props.draft);
    next.vlans.push({
      parent: allPhysicalNames()[0] ?? "eth1",
      id: 20,
      role: "lan",
      cidr: "192.168.20.1/24",
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
                    <label class="flex items-center gap-2 pt-5 text-xs text-slate-300">
                      <input
                        type="checkbox"
                        checked={wan.ipv6_pd}
                        onChange={(e) => updateWan(idx(), { ipv6_pd: e.currentTarget.checked })}
                      />
                      启用 IPv6-PD 前缀请求
                    </label>
                  </div>

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
                  启用 dnsmasq DHCP 服务
                </label>
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
              <div class="grid items-center gap-2 rounded-lg border border-slate-800 bg-slate-950/60 p-3 sm:grid-cols-5">
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
                  placeholder="可选 CIDR"
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.vlans[idx()].cidr = e.currentTarget.value || null;
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

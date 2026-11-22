import { For, Show, createSignal } from "solid-js";
import { initSetup } from "../api";
import type { HostInterfaceSnapshot } from "../types/HostInterfaceSnapshot";
import type { LanConfig } from "../types/LanConfig";
import type { RouterConfig } from "../types/RouterConfig";
import type { WanConfig } from "../types/WanConfig";
import type { WanProto } from "../types/WanProto";

interface Props {
  interfaces: HostInterfaceSnapshot[];
  defaultConfig: RouterConfig;
  onCompleted: () => void;
}

export function SetupWizard(props: Props) {
  const defaultWan = props.defaultConfig.wans[0];
  const defaultLan = props.defaultConfig.lans[0];

  const [step, setStep] = createSignal(1);
  const [username, setUsername] = createSignal("admin");
  const [password, setPassword] = createSignal("");
  const [confirmPassword, setConfirmPassword] = createSignal("");
  const [hostname, setHostname] = createSignal(props.defaultConfig.system.hostname);
  const [timezone, setTimezone] = createSignal(props.defaultConfig.system.timezone);

  const [wanDevice, setWanDevice] = createSignal(
    props.interfaces[0]?.name ?? defaultWan?.device ?? "eth0",
  );
  const [wanProto, setWanProto] = createSignal<WanProto>(defaultWan?.proto ?? "dhcp");
  const [pppoeUser, setPppoeUser] = createSignal("");
  const [pppoePass, setPppoePass] = createSignal("");
  const [staticCidr, setStaticCidr] = createSignal("192.168.1.2/24");
  const [gateway, setGateway] = createSignal("192.168.1.1");

  const [lanPorts, setLanPorts] = createSignal<string[]>(
    props.interfaces.length > 1
      ? props.interfaces.slice(1).map((i) => i.name)
      : (defaultLan?.ports ?? ["eth1", "eth2"]),
  );
  const [lanCidr, setLanCidr] = createSignal(defaultLan?.cidr ?? "192.168.10.1/24");
  const [lanDomain, setLanDomain] = createSignal(defaultLan?.domain ?? "lan");

  const [submitting, setSubmitting] = createSignal(false);
  const [err, setErr] = createSignal<string | null>(null);

  const availableNames = () => {
    const names = props.interfaces.map((i) => i.name);
    return names.length > 0 ? names : ["eth0", "eth1", "eth2", "eth3"];
  };

  function toggleLanPort(name: string) {
    if (name === wanDevice()) {
      return;
    }
    setLanPorts((prev) => (prev.includes(name) ? prev.filter((p) => p !== name) : [...prev, name]));
  }

  function selectWan(name: string) {
    setWanDevice(name);
    setLanPorts((prev) => prev.filter((p) => p !== name));
  }

  async function handleFinish() {
    setErr(null);
    if (password().length < 6) {
      setErr("管理员密码至少需要 6 位字符");
      setStep(1);
      return;
    }
    if (password() !== confirmPassword()) {
      setErr("两次输入的密码不一致");
      setStep(1);
      return;
    }

    const wan: WanConfig = {
      name: "wan",
      device: wanDevice(),
      proto: wanProto(),
      static_cidr: wanProto() === "static" ? staticCidr() : null,
      gateway: wanProto() === "static" ? gateway() : null,
      pppoe_username: wanProto() === "pppoe" ? pppoeUser() : null,
      pppoe_password: wanProto() === "pppoe" ? pppoePass() : null,
      mtu: wanProto() === "pppoe" ? 1492 : 1500,
      ipv6_pd: true,
      ipv6_pd_prefix_len: 60,
      metric: 100,
    };

    const lan: LanConfig = {
      name: "lan",
      bridge: defaultLan?.bridge ?? "br-lan",
      ports: lanPorts(),
      cidr: lanCidr(),
      domain: lanDomain(),
      dhcp_enabled: true,
      dhcp_start_offset: 100,
      dhcp_limit: 150,
      dhcp_lease_secs: 43200,
      static_leases: [],
      ipv6_ra_enabled: true,
      ipv6_subnet_id: null,
      ipv6_ula_cidr: null,
    };

    setSubmitting(true);
    try {
      await initSetup({
        admin_username: username(),
        admin_password: password(),
        hostname: hostname(),
        timezone: timezone(),
        wan,
        lan,
      });
      props.onCompleted();
    } catch (error) {
      setErr(error instanceof Error ? error.message : String(error));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div class="flex min-h-screen items-center justify-center bg-slate-950 p-6 text-slate-100">
      <div class="w-full max-w-2xl rounded-2xl border border-slate-800 bg-slate-900/90 p-6 shadow-2xl">
        <div class="flex items-center justify-between border-b border-slate-800 pb-4">
          <div class="flex items-center gap-3">
            <span class="i-ri-router-line text-2xl text-orange-400" />
            <div>
              <h1 class="text-lg font-semibold">CrabRouter 首次初始化向导</h1>
              <p class="text-xs text-slate-400">
                步骤 {step()} / 4 · 设置管理员凭据、WAN/LAN 端口映射与拨号参数
              </p>
            </div>
          </div>
          <div class="flex gap-1.5">
            <For each={[1, 2, 3, 4]}>
              {(s) => (
                <div
                  class={
                    step() >= s
                      ? "h-2 w-8 rounded-full bg-orange-500"
                      : "h-2 w-8 rounded-full bg-slate-800"
                  }
                />
              )}
            </For>
          </div>
        </div>

        <Show when={err()}>
          {(msg) => (
            <div class="mt-4 rounded-xl border border-red-500/40 bg-red-950/50 px-4 py-3 text-xs text-red-200">
              {msg()}
            </div>
          )}
        </Show>

        <div class="mt-5 space-y-4">
          <Show when={step() === 1}>
            <div class="grid gap-4 sm:grid-cols-2">
              <label class="block text-xs">
                <span class="text-slate-400">路由器主机名</span>
                <input
                  type="text"
                  value={hostname()}
                  onInput={(e) => setHostname(e.currentTarget.value)}
                  class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-sm"
                />
              </label>
              <label class="block text-xs">
                <span class="text-slate-400">系统时区</span>
                <input
                  type="text"
                  value={timezone()}
                  onInput={(e) => setTimezone(e.currentTarget.value)}
                  class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-sm"
                />
              </label>
              <label class="block text-xs sm:col-span-2">
                <span class="text-slate-400">管理员用户名</span>
                <input
                  type="text"
                  value={username()}
                  onInput={(e) => setUsername(e.currentTarget.value)}
                  class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-sm"
                />
              </label>
              <label class="block text-xs">
                <span class="text-slate-400">管理员密码（至少 6 位）</span>
                <input
                  type="password"
                  value={password()}
                  onInput={(e) => setPassword(e.currentTarget.value)}
                  class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-sm"
                />
              </label>
              <label class="block text-xs">
                <span class="text-slate-400">确认密码</span>
                <input
                  type="password"
                  value={confirmPassword()}
                  onInput={(e) => setConfirmPassword(e.currentTarget.value)}
                  class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-sm"
                />
              </label>
            </div>
          </Show>

          <Show when={step() === 2}>
            <div class="space-y-4">
              <div>
                <h2 class="text-sm font-medium text-slate-200">选择 WAN 外网接口</h2>
                <div class="mt-2 grid gap-2 sm:grid-cols-3">
                  <For each={availableNames()}>
                    {(iface) => (
                      <button
                        type="button"
                        onClick={() => selectWan(iface)}
                        class={
                          wanDevice() === iface
                            ? "rounded-xl border border-orange-500 bg-orange-500/15 px-3 py-2.5 text-left text-xs font-medium text-orange-300"
                            : "rounded-xl border border-slate-800 bg-slate-950 px-3 py-2.5 text-left text-xs text-slate-300 hover:border-slate-700"
                        }
                      >
                        <div class="font-mono font-semibold">{iface}</div>
                        <div class="mt-0.5 text-[11px] text-slate-400">
                          {wanDevice() === iface ? "已选为 WAN" : "点击设为 WAN"}
                        </div>
                      </button>
                    )}
                  </For>
                </div>
              </div>

              <div>
                <h2 class="text-sm font-medium text-slate-200">选择 LAN 网桥 (br-lan) 成员口</h2>
                <div class="mt-2 grid gap-2 sm:grid-cols-3">
                  <For each={availableNames()}>
                    {(iface) => {
                      const isWan = () => wanDevice() === iface;
                      const isLan = () => lanPorts().includes(iface);
                      return (
                        <button
                          type="button"
                          disabled={isWan()}
                          onClick={() => toggleLanPort(iface)}
                          class={
                            isWan()
                              ? "cursor-not-allowed rounded-xl border border-slate-800/60 bg-slate-950/40 px-3 py-2.5 text-left text-xs text-slate-600"
                              : isLan()
                                ? "rounded-xl border border-sky-500 bg-sky-500/15 px-3 py-2.5 text-left text-xs font-medium text-sky-300"
                                : "rounded-xl border border-slate-800 bg-slate-950 px-3 py-2.5 text-left text-xs text-slate-300 hover:border-slate-700"
                          }
                        >
                          <div class="font-mono font-semibold">{iface}</div>
                          <div class="mt-0.5 text-[11px] text-slate-400">
                            {isWan() ? "已分配给 WAN" : isLan() ? "属于 br-lan" : "未分配"}
                          </div>
                        </button>
                      );
                    }}
                  </For>
                </div>
              </div>
            </div>
          </Show>

          <Show when={step() === 3}>
            <div class="space-y-4">
              <div>
                <span class="text-xs text-slate-400">WAN 接入协议</span>
                <div class="mt-1.5 grid grid-cols-3 gap-2">
                  <For each={["dhcp", "pppoe", "static"] as WanProto[]}>
                    {(p) => (
                      <button
                        type="button"
                        onClick={() => setWanProto(p)}
                        class={
                          wanProto() === p
                            ? "rounded-lg border border-orange-500 bg-orange-500/15 py-2 text-xs font-semibold uppercase text-orange-300"
                            : "rounded-lg border border-slate-800 bg-slate-950 py-2 text-xs uppercase text-slate-300"
                        }
                      >
                        {p}
                      </button>
                    )}
                  </For>
                </div>
              </div>

              <Show when={wanProto() === "pppoe"}>
                <div class="grid gap-3 sm:grid-cols-2">
                  <label class="block text-xs">
                    <span class="text-slate-400">宽带拨号账号</span>
                    <input
                      type="text"
                      value={pppoeUser()}
                      onInput={(e) => setPppoeUser(e.currentTarget.value)}
                      class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-sm"
                    />
                  </label>
                  <label class="block text-xs">
                    <span class="text-slate-400">宽带拨号密码</span>
                    <input
                      type="password"
                      value={pppoePass()}
                      onInput={(e) => setPppoePass(e.currentTarget.value)}
                      class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-sm"
                    />
                  </label>
                </div>
              </Show>

              <Show when={wanProto() === "static"}>
                <div class="grid gap-3 sm:grid-cols-2">
                  <label class="block text-xs">
                    <span class="text-slate-400">WAN IP/掩码 (CIDR)</span>
                    <input
                      type="text"
                      value={staticCidr()}
                      onInput={(e) => setStaticCidr(e.currentTarget.value)}
                      class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 font-mono text-sm"
                    />
                  </label>
                  <label class="block text-xs">
                    <span class="text-slate-400">默认网关</span>
                    <input
                      type="text"
                      value={gateway()}
                      onInput={(e) => setGateway(e.currentTarget.value)}
                      class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 font-mono text-sm"
                    />
                  </label>
                </div>
              </Show>

              <div class="grid gap-3 sm:grid-cols-2">
                <label class="block text-xs">
                  <span class="text-slate-400">LAN 管理 IP/网段 (CIDR)</span>
                  <input
                    type="text"
                    value={lanCidr()}
                    onInput={(e) => setLanCidr(e.currentTarget.value)}
                    class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 font-mono text-sm"
                  />
                </label>
                <label class="block text-xs">
                  <span class="text-slate-400">内网域名后缀</span>
                  <input
                    type="text"
                    value={lanDomain()}
                    onInput={(e) => setLanDomain(e.currentTarget.value)}
                    class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 font-mono text-sm"
                  />
                </label>
              </div>
            </div>
          </Show>

          <Show when={step() === 4}>
            <div class="space-y-2 rounded-xl border border-slate-800 bg-slate-950/70 p-4 text-xs">
              <div class="flex justify-between py-1">
                <span class="text-slate-400">主机名 / 管理员</span>
                <span class="font-mono">
                  {hostname()} ({username()})
                </span>
              </div>
              <div class="flex justify-between py-1">
                <span class="text-slate-400">WAN 接口与协议</span>
                <span class="font-mono text-orange-300">
                  {wanDevice()} ({wanProto().toUpperCase()})
                </span>
              </div>
              <div class="flex justify-between py-1">
                <span class="text-slate-400">LAN 网桥成员</span>
                <span class="font-mono text-sky-300">
                  br-lan ← [{lanPorts().join(", ") || "无"}]
                </span>
              </div>
              <div class="flex justify-between py-1">
                <span class="text-slate-400">LAN 网段 / 本地域</span>
                <span class="font-mono">
                  {lanCidr()} (.{lanDomain()})
                </span>
              </div>
            </div>
          </Show>
        </div>

        <div class="mt-6 flex justify-between border-t border-slate-800 pt-4">
          <button
            type="button"
            disabled={step() === 1 || submitting()}
            onClick={() => setStep((s) => Math.max(1, s - 1))}
            class="rounded-lg border border-slate-700 bg-slate-800 px-4 py-2 text-xs font-medium text-slate-200 hover:bg-slate-700 disabled:opacity-40"
          >
            上一步
          </button>
          <Show
            when={step() < 4}
            fallback={
              <button
                type="button"
                disabled={submitting()}
                onClick={() => void handleFinish()}
                class="rounded-lg bg-orange-600 px-5 py-2 text-xs font-semibold text-white hover:bg-orange-500 disabled:opacity-50"
              >
                {submitting() ? "正在初始化..." : "完成初始化并应用"}
              </button>
            }
          >
            <button
              type="button"
              onClick={() => setStep((s) => Math.min(4, s + 1))}
              class="rounded-lg bg-orange-600 px-5 py-2 text-xs font-semibold text-white hover:bg-orange-500"
            >
              下一步
            </button>
          </Show>
        </div>
      </div>
    </div>
  );
}

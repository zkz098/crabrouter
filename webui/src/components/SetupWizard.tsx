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

const INPUT_CLS =
  "mt-1.5 w-full h-9 bg-[#fafafa] dark:bg-[#000000] border border-[#eaeaea] dark:border-[#262626] rounded-lg px-3 text-sm text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] transition-colors";

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
      weight: 1,
      table_id: null,
      health_check_enabled: true,
      probe_targets: [],
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
      stp_enabled: false,
      forward_delay_cs: 0,
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
    <div class="min-h-screen vercel-grid-bg flex items-center justify-center px-4 py-8 bg-[#fafafa] dark:bg-black text-[#171717] dark:text-[#ededed]">
      <div class="w-full max-w-2xl rounded-2xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-6 sm:p-7 shadow-sm animate-card-enter">
        <div class="flex items-center justify-between border-b border-[#eaeaea] dark:border-[#262626] pb-4">
          <div class="flex items-center gap-3">
            <div class="w-9 h-9 rounded-xl bg-[#171717] dark:bg-white text-white dark:text-[#0a0a0a] flex items-center justify-center shrink-0">
              <span class="i-ri-router-fill text-lg" />
            </div>
            <div>
              <h1 class="text-base font-semibold tracking-tight">CrabRouter 首次初始化向导</h1>
              <p class="text-xs text-[#666666] dark:text-[#a1a1aa]">
                步骤 {step()} / 4 · 设置管理员凭据、WAN/LAN 端口映射与拨号参数
              </p>
            </div>
          </div>
          <div class="flex gap-1.5">
            <For each={[1, 2, 3, 4]}>
              {(s) => (
                <div
                  class={`h-1.5 w-7 rounded-full transition-colors ${
                    step() >= s
                      ? "bg-[#171717] dark:bg-[#ededed]"
                      : "bg-[#eaeaea] dark:bg-[#262626]"
                  }`}
                />
              )}
            </For>
          </div>
        </div>

        <Show when={err()}>
          {(msg) => (
            <div class="mt-4 rounded-lg border border-rose-500/20 bg-rose-500/10 px-3 py-2 text-xs text-rose-600 dark:text-rose-400 flex items-center gap-1.5">
              <span class="i-ri-error-warning-line shrink-0" />
              <span>{msg()}</span>
            </div>
          )}
        </Show>

        <div class="mt-5 space-y-4">
          <Show when={step() === 1}>
            <div class="grid gap-4 sm:grid-cols-2">
              <label class="block text-xs">
                <span class="font-medium text-[#171717] dark:text-[#ededed]">路由器主机名</span>
                <input
                  type="text"
                  value={hostname()}
                  onInput={(e) => setHostname(e.currentTarget.value)}
                  class={INPUT_CLS}
                />
              </label>
              <label class="block text-xs">
                <span class="font-medium text-[#171717] dark:text-[#ededed]">系统时区</span>
                <input
                  type="text"
                  value={timezone()}
                  onInput={(e) => setTimezone(e.currentTarget.value)}
                  class={INPUT_CLS}
                />
              </label>
              <label class="block text-xs sm:col-span-2">
                <span class="font-medium text-[#171717] dark:text-[#ededed]">管理员用户名</span>
                <input
                  type="text"
                  value={username()}
                  onInput={(e) => setUsername(e.currentTarget.value)}
                  class={INPUT_CLS}
                />
              </label>
              <label class="block text-xs">
                <span class="font-medium text-[#171717] dark:text-[#ededed]">
                  管理员密码（至少 6 位）
                </span>
                <input
                  type="password"
                  value={password()}
                  onInput={(e) => setPassword(e.currentTarget.value)}
                  class={INPUT_CLS}
                />
              </label>
              <label class="block text-xs">
                <span class="font-medium text-[#171717] dark:text-[#ededed]">确认密码</span>
                <input
                  type="password"
                  value={confirmPassword()}
                  onInput={(e) => setConfirmPassword(e.currentTarget.value)}
                  class={INPUT_CLS}
                />
              </label>
            </div>
          </Show>

          <Show when={step() === 2}>
            <div class="space-y-4">
              <div>
                <h2 class="text-xs font-medium text-[#171717] dark:text-[#ededed]">
                  选择 WAN 外网接口
                </h2>
                <div class="mt-2 grid gap-2 sm:grid-cols-3">
                  <For each={availableNames()}>
                    {(iface) => (
                      <button
                        type="button"
                        onClick={() => selectWan(iface)}
                        class={`rounded-xl border px-3 py-2.5 text-left text-xs cursor-pointer transition-all ${
                          wanDevice() === iface
                            ? "border-[#0070f3] bg-[#0070f3]/10 text-[#0070f3] dark:text-sky-400 font-medium"
                            : "border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] text-[#171717] dark:text-[#ededed] hover:border-[#d4d4d8] dark:hover:border-[#404040]"
                        }`}
                      >
                        <div class="font-mono font-semibold">{iface}</div>
                        <div class="mt-0.5 text-[11px] text-[#666666] dark:text-[#a1a1aa]">
                          {wanDevice() === iface ? "已选为 WAN" : "点击设为 WAN"}
                        </div>
                      </button>
                    )}
                  </For>
                </div>
              </div>

              <div>
                <h2 class="text-xs font-medium text-[#171717] dark:text-[#ededed]">
                  选择 LAN 网桥 (br-lan) 成员口
                </h2>
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
                          class={`rounded-xl border px-3 py-2.5 text-left text-xs transition-all ${
                            isWan()
                              ? "cursor-not-allowed border-[#eaeaea] dark:border-[#262626] bg-[#fafafa]/50 dark:bg-[#111111]/40 text-[#8f8f8f] dark:text-[#71717a] opacity-60"
                              : isLan()
                                ? "cursor-pointer border-emerald-500/40 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400 font-medium"
                                : "cursor-pointer border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] text-[#171717] dark:text-[#ededed] hover:border-[#d4d4d8] dark:hover:border-[#404040]"
                          }`}
                        >
                          <div class="font-mono font-semibold">{iface}</div>
                          <div class="mt-0.5 text-[11px] text-[#666666] dark:text-[#a1a1aa]">
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
                <span class="text-xs font-medium text-[#171717] dark:text-[#ededed]">
                  WAN 接入协议
                </span>
                <div class="mt-1.5 grid grid-cols-3 gap-2">
                  <For each={["dhcp", "pppoe", "static"] as WanProto[]}>
                    {(p) => (
                      <button
                        type="button"
                        onClick={() => setWanProto(p)}
                        class={`rounded-lg border py-2 text-xs font-medium uppercase cursor-pointer transition-all ${
                          wanProto() === p
                            ? "border-[#171717] dark:border-[#ededed] bg-[#171717] dark:bg-[#ededed] text-white dark:text-[#0a0a0a]"
                            : "border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] text-[#666666] dark:text-[#a1a1aa] hover:text-[#171717] dark:hover:text-[#ededed]"
                        }`}
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
                    <span class="font-medium text-[#171717] dark:text-[#ededed]">宽带拨号账号</span>
                    <input
                      type="text"
                      value={pppoeUser()}
                      onInput={(e) => setPppoeUser(e.currentTarget.value)}
                      class={INPUT_CLS}
                    />
                  </label>
                  <label class="block text-xs">
                    <span class="font-medium text-[#171717] dark:text-[#ededed]">宽带拨号密码</span>
                    <input
                      type="password"
                      value={pppoePass()}
                      onInput={(e) => setPppoePass(e.currentTarget.value)}
                      class={INPUT_CLS}
                    />
                  </label>
                </div>
              </Show>

              <Show when={wanProto() === "static"}>
                <div class="grid gap-3 sm:grid-cols-2">
                  <label class="block text-xs">
                    <span class="font-medium text-[#171717] dark:text-[#ededed]">
                      WAN IP/掩码 (CIDR)
                    </span>
                    <input
                      type="text"
                      value={staticCidr()}
                      onInput={(e) => setStaticCidr(e.currentTarget.value)}
                      class={`${INPUT_CLS} font-mono`}
                    />
                  </label>
                  <label class="block text-xs">
                    <span class="font-medium text-[#171717] dark:text-[#ededed]">默认网关</span>
                    <input
                      type="text"
                      value={gateway()}
                      onInput={(e) => setGateway(e.currentTarget.value)}
                      class={`${INPUT_CLS} font-mono`}
                    />
                  </label>
                </div>
              </Show>

              <div class="grid gap-3 sm:grid-cols-2">
                <label class="block text-xs">
                  <span class="font-medium text-[#171717] dark:text-[#ededed]">
                    LAN 管理 IP/网段 (CIDR)
                  </span>
                  <input
                    type="text"
                    value={lanCidr()}
                    onInput={(e) => setLanCidr(e.currentTarget.value)}
                    class={`${INPUT_CLS} font-mono`}
                  />
                </label>
                <label class="block text-xs">
                  <span class="font-medium text-[#171717] dark:text-[#ededed]">内网域名后缀</span>
                  <input
                    type="text"
                    value={lanDomain()}
                    onInput={(e) => setLanDomain(e.currentTarget.value)}
                    class={`${INPUT_CLS} font-mono`}
                  />
                </label>
              </div>
            </div>
          </Show>

          <Show when={step() === 4}>
            <div class="space-y-2.5 rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] p-4 text-xs">
              <div class="flex justify-between py-1">
                <span class="text-[#666666] dark:text-[#a1a1aa]">主机名 / 管理员</span>
                <span class="font-mono font-medium">
                  {hostname()} ({username()})
                </span>
              </div>
              <div class="flex justify-between py-1">
                <span class="text-[#666666] dark:text-[#a1a1aa]">WAN 接口与协议</span>
                <span class="font-mono font-medium text-[#0070f3] dark:text-sky-400">
                  {wanDevice()} ({wanProto().toUpperCase()})
                </span>
              </div>
              <div class="flex justify-between py-1">
                <span class="text-[#666666] dark:text-[#a1a1aa]">LAN 网桥成员</span>
                <span class="font-mono font-medium text-emerald-600 dark:text-emerald-400">
                  br-lan ← [{lanPorts().join(", ") || "无"}]
                </span>
              </div>
              <div class="flex justify-between py-1">
                <span class="text-[#666666] dark:text-[#a1a1aa]">LAN 网段 / 本地域</span>
                <span class="font-mono font-medium">
                  {lanCidr()} (.{lanDomain()})
                </span>
              </div>
            </div>
          </Show>
        </div>

        <div class="mt-6 flex justify-between border-t border-[#eaeaea] dark:border-[#262626] pt-4">
          <button
            type="button"
            disabled={step() === 1 || submitting()}
            onClick={() => setStep((s) => Math.max(1, s - 1))}
            class="h-9 rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#111111] hover:bg-[#f4f4f5] dark:hover:bg-[#1a1a1a] px-4 text-xs font-medium text-[#171717] dark:text-[#ededed] cursor-pointer transition-all disabled:opacity-40"
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
                class="h-9 rounded-lg bg-[#171717] hover:bg-[#383838] dark:bg-[#ededed] dark:hover:bg-[#d4d4d8] px-5 text-xs font-medium text-white dark:text-[#0a0a0a] border-0 cursor-pointer transition-all active:scale-[0.98] disabled:opacity-50"
              >
                {submitting() ? "正在初始化..." : "完成初始化并应用"}
              </button>
            }
          >
            <button
              type="button"
              onClick={() => setStep((s) => Math.min(4, s + 1))}
              class="h-9 rounded-lg bg-[#171717] hover:bg-[#383838] dark:bg-[#ededed] dark:hover:bg-[#d4d4d8] px-5 text-xs font-medium text-white dark:text-[#0a0a0a] border-0 cursor-pointer transition-all active:scale-[0.98]"
            >
              下一步
            </button>
          </Show>
        </div>
      </div>
    </div>
  );
}

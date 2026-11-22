import {
  For,
  Match,
  Show,
  Switch,
  createEffect,
  createResource,
  createSignal,
  onCleanup,
} from "solid-js";
import {
  beginCommit,
  confirmCommit,
  getClients,
  getConfig,
  getConfigHistory,
  getDnsStatus,
  getInterfaces,
  getPreview,
  getServiceStatuses,
  getSetupStatus,
  getStatus,
  getWanStatus,
  logout,
  openMetricsWebSocket,
  restoreConfigSnapshot,
  rollbackCommit,
} from "./api";
import { ClientsWgPage } from "./components/ClientsWgPage";
import { CommitConfirmModal } from "./components/CommitConfirmModal";
import { DiagnosticsPage } from "./components/DiagnosticsPage";
import { FirewallPage } from "./components/FirewallPage";
import { LoginView } from "./components/LoginView";
import { NetworkPage } from "./components/NetworkPage";
import { OverviewPage } from "./components/OverviewPage";
import { SetupWizard } from "./components/SetupWizard";
import type { RouterConfig } from "./types/RouterConfig";
import type { SystemMetricsSnapshot } from "./types/SystemMetricsSnapshot";

type PageTab = "overview" | "network" | "firewall" | "clients" | "diagnostics";

const TABS: { id: PageTab; label: string; icon: string }[] = [
  { id: "overview", label: "总览与预览", icon: "i-ri-dashboard-line" },
  { id: "network", label: "接口 / WAN / VLAN", icon: "i-ri-node-tree" },
  { id: "firewall", label: "防火墙 / NAT", icon: "i-ri-shield-line" },
  { id: "clients", label: "终端 / DHCP / VPN", icon: "i-ri-device-line" },
  { id: "diagnostics", label: "诊断工具箱", icon: "i-ri-terminal-box-line" },
];

export default function App() {
  const [setupStatus, { refetch: refetchSetup }] = createResource(getSetupStatus);
  const [interfaces, { refetch: refetchIfaces }] = createResource(getInterfaces);

  const isReady = () => Boolean(setupStatus()?.initialized && setupStatus()?.authenticated);

  const [status, { refetch: refetchStatus }] = createResource(isReady, getStatus);
  const [config, { refetch: refetchConfig }] = createResource(isReady, getConfig);
  const [preview, { refetch: refetchPreview }] = createResource(isReady, getPreview);
  const [wanStates, { refetch: refetchWanStates }] = createResource(isReady, getWanStatus);
  const [serviceStatuses, { refetch: refetchServices }] = createResource(
    isReady,
    getServiceStatuses,
  );
  const [dnsStatus, { refetch: refetchDnsStatus }] = createResource(isReady, getDnsStatus);
  const [clients, { refetch: refetchClients }] = createResource(isReady, getClients);
  const [history, { refetch: refetchHistory }] = createResource(isReady, getConfigHistory);

  const [metrics, setMetrics] = createSignal<SystemMetricsSnapshot | null>(null);
  const [tab, setTab] = createSignal<PageTab>("overview");
  const [draft, setDraft] = createSignal<RouterConfig | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [errorMsg, setErrorMsg] = createSignal<string | null>(null);

  createEffect(() => {
    if (!isReady()) {
      setMetrics(null);
      return;
    }
    const ws = openMetricsWebSocket();
    ws.onmessage = (evt) => {
      try {
        const snap = JSON.parse(String(evt.data)) as SystemMetricsSnapshot;
        setMetrics(snap);
      } catch {
        // ignore malformed telemetry frames
      }
    };
    onCleanup(() => ws.close());
  });

  createEffect(() => {
    const cfg = config();
    if (cfg && !draft()) {
      setDraft(structuredClone(cfg));
    }
  });

  const isDirty = () => {
    const a = config();
    const b = draft();
    if (!a || !b) {
      return false;
    }
    return JSON.stringify(a) !== JSON.stringify(b);
  };

  async function refreshAll() {
    await Promise.all([
      refetchStatus(),
      refetchIfaces(),
      refetchConfig(),
      refetchPreview(),
      refetchWanStates(),
      refetchServices(),
      refetchDnsStatus(),
      refetchClients(),
      refetchHistory(),
    ]);
    const latest = config();
    if (latest) {
      setDraft(structuredClone(latest));
    }
  }

  async function handleBeginCommit() {
    const next = draft();
    if (!next) {
      return;
    }
    setErrorMsg(null);
    setBusy(true);
    try {
      await beginCommit(next);
      await refetchStatus();
      await refetchPreview();
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  async function handleConfirm() {
    setErrorMsg(null);
    setBusy(true);
    try {
      await confirmCommit();
      await refreshAll();
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  async function handleRollback() {
    setErrorMsg(null);
    setBusy(true);
    try {
      await rollbackCommit();
      await refreshAll();
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  async function handleRestoreHistory(gen: number) {
    setErrorMsg(null);
    setBusy(true);
    try {
      await restoreConfigSnapshot(gen);
      await refetchStatus();
      await refetchPreview();
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  async function handleLogout() {
    await logout();
    setDraft(null);
    await refetchSetup();
  }

  const [uninitConfig] = createResource(() => setupStatus()?.initialized === false, getConfig);

  return (
    <Switch
      fallback={
        <div class="flex min-h-screen items-center justify-center bg-slate-950 text-xs text-slate-400">
          正在连接 CrabRouter 控制面...
        </div>
      }
    >
      <Match when={setupStatus() && !setupStatus()!.initialized && uninitConfig()}>
        {(defaultCfg) => (
          <SetupWizard
            interfaces={interfaces() ?? []}
            defaultConfig={defaultCfg()}
            onCompleted={() => {
              void Promise.resolve(refetchSetup()).then(() => void refreshAll());
            }}
          />
        )}
      </Match>

      <Match when={setupStatus()?.initialized && !setupStatus()?.authenticated}>
        <LoginView
          onSuccess={() => {
            void Promise.resolve(refetchSetup()).then(() => void refreshAll());
          }}
        />
      </Match>

      <Match when={isReady() && draft()}>
        {(activeDraft) => (
          <div class="min-h-screen bg-slate-950 text-slate-100">
            <header class="border-b border-slate-800 bg-slate-900/80 backdrop-blur">
              <div class="mx-auto flex max-w-6xl flex-wrap items-center justify-between gap-3 px-6 py-4">
                <div class="flex items-center gap-3">
                  <span class="i-ri-router-line text-2xl text-orange-400" />
                  <div>
                    <h1 class="text-lg font-semibold tracking-tight">
                      {config()?.system.hostname ?? "CrabRouter"}
                    </h1>
                    <p class="text-xs text-slate-400">
                      Debian 13 (Trixie) · Rust Control Plane · redb State Store
                    </p>
                  </div>
                </div>
                <div class="flex items-center gap-2 text-xs text-slate-300">
                  <span class="rounded-full bg-slate-800 px-3 py-1">
                    Gen #{status()?.active_generation ?? "-"}
                  </span>
                  <button
                    type="button"
                    onClick={() => void refreshAll()}
                    class="flex items-center gap-1 rounded-lg border border-slate-700 bg-slate-800 px-3 py-1.5 hover:bg-slate-700"
                  >
                    <span class="i-ri-refresh-line" />
                    刷新
                  </button>
                  <button
                    type="button"
                    onClick={() => void handleLogout()}
                    class="rounded-lg border border-slate-700 bg-slate-800 px-3 py-1.5 hover:bg-slate-700"
                  >
                    退出
                  </button>
                </div>
              </div>

              <nav class="mx-auto flex max-w-6xl gap-1 overflow-x-auto px-6 pb-2">
                <For each={TABS}>
                  {(item) => (
                    <button
                      type="button"
                      onClick={() => setTab(item.id)}
                      class={
                        tab() === item.id
                          ? "flex items-center gap-1.5 rounded-lg bg-slate-800 px-3.5 py-2 text-xs font-semibold text-orange-400"
                          : "flex items-center gap-1.5 rounded-lg px-3.5 py-2 text-xs text-slate-400 hover:bg-slate-800/50 hover:text-slate-200"
                      }
                    >
                      <span class={item.icon} />
                      {item.label}
                    </button>
                  )}
                </For>
              </nav>
            </header>

            <main class="mx-auto max-w-6xl space-y-6 px-6 py-6">
              <Show when={errorMsg()}>
                {(msg) => (
                  <div class="rounded-xl border border-red-500/40 bg-red-950/50 px-4 py-3 text-sm text-red-200">
                    {msg()}
                  </div>
                )}
              </Show>

              <Show when={isDirty() && !status()?.pending}>
                <div class="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-orange-500/40 bg-orange-950/30 px-4 py-3">
                  <div class="text-xs text-orange-200">
                    当前草稿有尚未下发的修改。点击「应用变更」将通过 Commit-Confirm 安全窗口下发。
                  </div>
                  <div class="flex gap-2">
                    <button
                      type="button"
                      disabled={busy()}
                      onClick={() => {
                        const base = config();
                        if (base) {
                          setDraft(structuredClone(base));
                        }
                      }}
                      class="rounded-lg border border-slate-700 bg-slate-800 px-3 py-1.5 text-xs hover:bg-slate-700"
                    >
                      放弃草稿
                    </button>
                    <button
                      type="button"
                      disabled={busy()}
                      onClick={() => void handleBeginCommit()}
                      class="rounded-lg bg-orange-600 px-4 py-1.5 text-xs font-semibold text-white hover:bg-orange-500 disabled:opacity-50"
                    >
                      {busy() ? "校验并下发中..." : "应用变更 (Commit-Confirm)"}
                    </button>
                  </div>
                </div>
              </Show>

              <Switch>
                <Match when={tab() === "overview"}>
                  <OverviewPage
                    config={activeDraft()}
                    interfaces={interfaces() ?? []}
                    wanStates={wanStates() ?? []}
                    serviceStatuses={serviceStatuses() ?? []}
                    dnsStatus={dnsStatus()}
                    metrics={metrics()}
                    preview={preview()}
                    history={history() ?? []}
                    onChangeConfig={(next) => setDraft(next)}
                    onRestoreHistory={(gen) => void handleRestoreHistory(gen)}
                  />
                </Match>
                <Match when={tab() === "network"}>
                  <NetworkPage
                    draft={activeDraft()}
                    interfaces={interfaces() ?? []}
                    wanStates={wanStates() ?? []}
                    onChange={(next) => setDraft(next)}
                  />
                </Match>
                <Match when={tab() === "firewall"}>
                  <FirewallPage draft={activeDraft()} onChange={(next) => setDraft(next)} />
                </Match>
                <Match when={tab() === "clients"}>
                  <ClientsWgPage
                    draft={activeDraft()}
                    clients={clients() ?? []}
                    onRefreshClients={() => void refetchClients()}
                    onChange={(next) => setDraft(next)}
                  />
                </Match>
                <Match when={tab() === "diagnostics"}>
                  <DiagnosticsPage />
                </Match>
              </Switch>
            </main>

            <CommitConfirmModal
              pending={status()?.pending}
              timeoutSecs={activeDraft().system.commit_confirm_timeout_secs}
              busy={busy()}
              onConfirm={() => void handleConfirm()}
              onRollback={() => void handleRollback()}
            />
          </div>
        )}
      </Match>
    </Switch>
  );
}

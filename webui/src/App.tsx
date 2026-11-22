import {
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
  getConntrack,
  getDnsStatus,
  getHardwareHealth,
  getInterfaces,
  getOtaStatus,
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
import { ConntrackQosPage } from "./components/ConntrackQosPage";
import { DiagnosticsPage } from "./components/DiagnosticsPage";
import { FirewallPage } from "./components/FirewallPage";
import { Header, type PageTab } from "./components/Header";
import { LoginView } from "./components/LoginView";
import { MobileNav } from "./components/MobileNav";
import { NetworkPage } from "./components/NetworkPage";
import { OtaPage } from "./components/OtaPage";
import { OverviewPage } from "./components/OverviewPage";
import { SetupWizard } from "./components/SetupWizard";
import { t } from "./i18n";
import "./theme";
import type { RouterConfig } from "./types/RouterConfig";
import type { SystemMetricsSnapshot } from "./types/SystemMetricsSnapshot";

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
  const [conntrack, { refetch: refetchConntrack }] = createResource(isReady, getConntrack);
  const [history, { refetch: refetchHistory }] = createResource(isReady, getConfigHistory);
  const [otaStatus, { refetch: refetchOta }] = createResource(isReady, getOtaStatus);
  const [hardwareHealth, { refetch: refetchHardware }] = createResource(isReady, getHardwareHealth);

  const [metrics, setMetrics] = createSignal<SystemMetricsSnapshot | null>(null);
  const [wsConnected, setWsConnected] = createSignal(false);
  const [tab, setTab] = createSignal<PageTab>("overview");
  const [draft, setDraft] = createSignal<RouterConfig | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [errorMsg, setErrorMsg] = createSignal<string | null>(null);

  createEffect(() => {
    if (!isReady()) {
      setMetrics(null);
      setWsConnected(false);
      return;
    }
    const ws = openMetricsWebSocket();
    ws.onopen = () => setWsConnected(true);
    ws.onclose = () => setWsConnected(false);
    ws.onerror = () => setWsConnected(false);
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
      refetchConntrack(),
      refetchHistory(),
      refetchOta(),
      refetchHardware(),
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
        <div class="min-h-screen vercel-grid-bg flex items-center justify-center bg-[#fafafa] dark:bg-black text-xs text-[#666666] dark:text-[#a1a1aa]">
          {t("connectingControlPlane")}
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
          <div class="min-h-screen vercel-grid-bg bg-[#fafafa] dark:bg-black text-[#171717] dark:text-[#ededed] transition-colors">
            <Header
              hostname={config()?.system.hostname ?? "CrabRouter"}
              generation={status()?.active_generation ?? "-"}
              wsConnected={wsConnected()}
              tab={tab()}
              onSelectTab={setTab}
              onRefresh={() => void refreshAll()}
              onLogout={() => void handleLogout()}
            />

            <main class="max-w-6xl mx-auto px-4 sm:px-6 pt-5 pb-24 md:py-7 space-y-6">
              <Show when={errorMsg()}>
                {(msg) => (
                  <div class="rounded-xl border border-rose-500/25 bg-rose-500/10 px-4 py-3 text-xs text-rose-600 dark:text-rose-400 flex items-center gap-2 animate-toast-in">
                    <span class="i-ri-error-warning-line text-sm shrink-0" />
                    <span>{msg()}</span>
                  </div>
                )}
              </Show>

              <Show when={isDirty() && !status()?.pending}>
                <div class="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-amber-500/30 bg-amber-500/10 px-4 py-3 animate-toast-in">
                  <div class="flex items-center gap-2 text-xs text-amber-800 dark:text-amber-200">
                    <span class="i-ri-edit-circle-line text-sm text-amber-600 dark:text-amber-400 shrink-0" />
                    <span>{t("dirtyDraftBanner")}</span>
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
                      class="rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#111111] hover:bg-[#f4f4f5] dark:hover:bg-[#1a1a1a] px-3 py-1.5 text-xs font-medium text-[#171717] dark:text-[#ededed] cursor-pointer transition-all active:scale-[0.97]"
                    >
                      {t("discardDraft")}
                    </button>
                    <button
                      type="button"
                      disabled={busy()}
                      onClick={() => void handleBeginCommit()}
                      class="rounded-lg bg-[#171717] hover:bg-[#383838] dark:bg-[#ededed] dark:hover:bg-[#d4d4d8] px-4 py-1.5 text-xs font-medium text-white dark:text-[#0a0a0a] border-0 cursor-pointer transition-all active:scale-[0.97] disabled:opacity-50"
                    >
                      {busy() ? t("applyingChanges") : t("applyChanges")}
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
                    hardware={hardwareHealth()}
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
                <Match when={tab() === "qos_conntrack"}>
                  <ConntrackQosPage
                    draft={activeDraft()}
                    entries={conntrack() ?? []}
                    onRefresh={() => void refetchConntrack()}
                    onChange={(next) => setDraft(next)}
                  />
                </Match>
                <Match when={tab() === "clients"}>
                  <ClientsWgPage
                    draft={activeDraft()}
                    clients={clients() ?? []}
                    onRefreshClients={() => void refetchClients()}
                    onChange={(next) => setDraft(next)}
                  />
                </Match>
                <Match when={tab() === "ota"}>
                  <OtaPage
                    draft={activeDraft()}
                    otaStatus={otaStatus()}
                    onChange={(next) => setDraft(next)}
                    onRefreshOta={() => void refetchOta()}
                  />
                </Match>
                <Match when={tab() === "diagnostics"}>
                  <DiagnosticsPage
                    hardware={hardwareHealth()}
                    onRefreshHardware={() => void refetchHardware()}
                  />
                </Match>
              </Switch>
            </main>

            <MobileNav tab={tab()} onSelectTab={setTab} />

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

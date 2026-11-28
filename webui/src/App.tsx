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
import { ExtensionsPage } from "./components/ExtensionsPage";
import { FirewallPage } from "./components/FirewallPage";
import { LoginView } from "./components/LoginView";
import { NetworkPage } from "./components/NetworkPage";
import { OtaPage } from "./components/OtaPage";
import { OverviewPage } from "./components/OverviewPage";
import { ServicesPreviewPage } from "./components/ServicesPreviewPage";
import { SetupWizard } from "./components/SetupWizard";
import { SidebarNav } from "./components/SidebarNav";
import { SystemSettingsPage } from "./components/SystemSettingsPage";
import { TopBar } from "./components/TopBar";
import { t } from "./i18n";
import type { PageTab } from "./nav";
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
  const [mobileMenuOpen, setMobileMenuOpen] = createSignal(false);
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
          <div class="min-h-screen argon-bg text-[#172b4d] dark:text-[#eef1f6] flex transition-colors">
            <SidebarNav
              hostname={config()?.system.hostname ?? "CrabRouter"}
              generation={status()?.active_generation ?? "-"}
              wsConnected={wsConnected()}
              tab={tab()}
              mobileOpen={mobileMenuOpen()}
              onCloseMobile={() => setMobileMenuOpen(false)}
              onSelectTab={setTab}
              onLogout={() => void handleLogout()}
            />

            <div class="flex-1 min-w-0 flex flex-col">
              <TopBar
                tab={tab()}
                wsConnected={wsConnected()}
                isDirty={isDirty()}
                pending={Boolean(status()?.pending)}
                busy={busy()}
                onOpenMobileMenu={() => setMobileMenuOpen(true)}
                onRefresh={() => void refreshAll()}
                onDiscard={() => {
                  const base = config();
                  if (base) {
                    setDraft(structuredClone(base));
                  }
                }}
                onApply={() => void handleBeginCommit()}
              />

              <main class="flex-1 w-full max-w-6xl mx-auto px-4 sm:px-6 py-5 sm:py-6 space-y-5">
                <Show when={errorMsg()}>
                  {(msg) => (
                    <div class="rounded-xl border border-rose-500/25 bg-rose-500/10 px-4 py-3 text-xs text-rose-600 dark:text-rose-400 flex items-center gap-2 animate-toast-in">
                      <span class="i-ri-error-warning-line text-sm shrink-0" />
                      <span>{msg()}</span>
                    </div>
                  )}
                </Show>

                <Show when={isDirty() && !status()?.pending}>
                  <div class="sm:hidden flex flex-wrap items-center justify-between gap-3 rounded-xl border border-amber-500/30 bg-amber-500/10 px-4 py-3 animate-toast-in">
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
                        class="rounded-lg border border-[#e9ecef] dark:border-[#232a3b] bg-white dark:bg-[#131722] px-3 py-1.5 text-xs font-medium text-[#172b4d] dark:text-[#eef1f6] cursor-pointer"
                      >
                        {t("discardDraft")}
                      </button>
                      <button
                        type="button"
                        disabled={busy()}
                        onClick={() => void handleBeginCommit()}
                        class="rounded-lg bg-[#5e72e4] hover:bg-[#4c60d4] px-4 py-1.5 text-xs font-medium text-white border-0 cursor-pointer disabled:opacity-50"
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
                      dnsStatus={dnsStatus()}
                      hardware={hardwareHealth()}
                      metrics={metrics()}
                      onNavigate={setTab}
                    />
                  </Match>
                  <Match when={tab() === "conntrack"}>
                    <ConntrackQosPage
                      mode="conntrack"
                      draft={activeDraft()}
                      interfaces={interfaces() ?? []}
                      entries={conntrack() ?? []}
                      onRefresh={() => void refetchConntrack()}
                      onChange={(next) => setDraft(next)}
                    />
                  </Match>
                  <Match when={tab() === "services"}>
                    <ServicesPreviewPage
                      serviceStatuses={serviceStatuses() ?? []}
                      preview={preview()}
                    />
                  </Match>
                  <Match when={tab() === "system"}>
                    <SystemSettingsPage
                      config={activeDraft()}
                      history={history() ?? []}
                      onChangeConfig={(next) => setDraft(next)}
                      onRestoreHistory={(gen) => void handleRestoreHistory(gen)}
                    />
                  </Match>
                  <Match when={tab() === "interfaces"}>
                    <NetworkPage
                      section="interfaces"
                      draft={activeDraft()}
                      interfaces={interfaces() ?? []}
                      wanStates={wanStates() ?? []}
                      onChange={(next) => setDraft(next)}
                    />
                  </Match>
                  <Match when={tab() === "routing"}>
                    <NetworkPage
                      section="routing"
                      draft={activeDraft()}
                      interfaces={interfaces() ?? []}
                      wanStates={wanStates() ?? []}
                      onChange={(next) => setDraft(next)}
                    />
                  </Match>
                  <Match when={tab() === "dns"}>
                    <NetworkPage
                      section="dns"
                      draft={activeDraft()}
                      interfaces={interfaces() ?? []}
                      wanStates={wanStates() ?? []}
                      onChange={(next) => setDraft(next)}
                    />
                  </Match>
                  <Match when={tab() === "qos"}>
                    <ConntrackQosPage
                      mode="qos"
                      draft={activeDraft()}
                      interfaces={interfaces() ?? []}
                      entries={conntrack() ?? []}
                      onRefresh={() => void refetchConntrack()}
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
                  <Match when={tab() === "extensions"}>
                    <ExtensionsPage
                      draft={activeDraft()}
                      onEnableContainers={() => {
                        const next = structuredClone(activeDraft());
                        next.containers.enabled = true;
                        setDraft(next);
                      }}
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
            </div>

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

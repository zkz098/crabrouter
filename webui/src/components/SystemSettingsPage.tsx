import { For, Show } from "solid-js";
import { t } from "../i18n";
import type { ConfigSnapshotSummary } from "../types/ConfigSnapshotSummary";
import type { RouterConfig } from "../types/RouterConfig";
import { GuideBanner, PageHeader } from "./ui/ArgonPrimitives";

interface Props {
  config: RouterConfig;
  history: ConfigSnapshotSummary[];
  onChangeConfig: (next: RouterConfig) => void;
  onRestoreHistory: (gen: number) => void;
}

export function SystemSettingsPage(props: Props) {
  return (
    <div class="space-y-5 animate-view-enter">
      <PageHeader title={t("navSystemSettings")} subtitle={t("descSystemSettings")} />

      <GuideBanner
        title={t("sysGuideTitle")}
        description={t("sysGuideDesc")}
        variant="tip"
        dismissible
      />

      <section class="rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-white dark:bg-[#131722] p-4 sm:p-5 argon-card space-y-4">
        <h3 class="text-sm font-semibold text-[#172b4d] dark:text-[#eef1f6]">
          {t("sysIdentityTitle")}
        </h3>
        <div class="grid gap-3 text-xs sm:grid-cols-2">
          <label class="block">
            <span class="text-[#525f7f] dark:text-[#a3adc2]">{t("setupHostname")}</span>
            <input
              type="text"
              value={props.config.system.hostname}
              onInput={(e) => {
                const next = structuredClone(props.config);
                next.system.hostname = e.currentTarget.value;
                props.onChangeConfig(next);
              }}
              class="mt-1 w-full rounded-lg border border-[#e9ecef] dark:border-[#232a3b] bg-[#f8f9fe] dark:bg-[#0b0e14] px-2.5 py-1.5 font-mono text-xs text-[#172b4d] dark:text-[#eef1f6] outline-none focus:border-[#5e72e4]"
            />
          </label>
          <label class="block">
            <span class="text-[#525f7f] dark:text-[#a3adc2]">{t("setupTimezone")}</span>
            <input
              type="text"
              value={props.config.system.timezone}
              onInput={(e) => {
                const next = structuredClone(props.config);
                next.system.timezone = e.currentTarget.value;
                props.onChangeConfig(next);
              }}
              class="mt-1 w-full rounded-lg border border-[#e9ecef] dark:border-[#232a3b] bg-[#f8f9fe] dark:bg-[#0b0e14] px-2.5 py-1.5 font-mono text-xs text-[#172b4d] dark:text-[#eef1f6] outline-none focus:border-[#5e72e4]"
            />
          </label>
        </div>
      </section>

      <section class="rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-white dark:bg-[#131722] p-4 sm:p-5 argon-card space-y-4">
        <h3 class="text-sm font-semibold text-[#172b4d] dark:text-[#eef1f6]">
          {t("ovMgmtSecurity")}
        </h3>
        <div class="grid gap-3 text-xs sm:grid-cols-2">
          <label class="flex items-center gap-2 text-[#172b4d] dark:text-[#eef1f6]">
            <input
              type="checkbox"
              checked={props.config.system.web_lan_only}
              onChange={(e) => {
                const next = structuredClone(props.config);
                next.system.web_lan_only = e.currentTarget.checked;
                props.onChangeConfig(next);
              }}
            />
            {t("ovWebLanOnly")}
          </label>
          <label class="flex items-center gap-2 text-[#172b4d] dark:text-[#eef1f6]">
            <input
              type="checkbox"
              checked={props.config.system.web_https_enabled}
              onChange={(e) => {
                const next = structuredClone(props.config);
                next.system.web_https_enabled = e.currentTarget.checked;
                props.onChangeConfig(next);
              }}
            />
            {t("ovWebHttps")}
          </label>
          <label class="block">
            <span class="text-[#525f7f] dark:text-[#a3adc2]">{t("ovHttpsListen")}</span>
            <input
              type="text"
              value={props.config.system.web_https_listen}
              onInput={(e) => {
                const next = structuredClone(props.config);
                next.system.web_https_listen = e.currentTarget.value;
                props.onChangeConfig(next);
              }}
              class="mt-1 w-full rounded-lg border border-[#e9ecef] dark:border-[#232a3b] bg-[#f8f9fe] dark:bg-[#0b0e14] px-2.5 py-1.5 font-mono text-xs text-[#172b4d] dark:text-[#eef1f6] outline-none focus:border-[#5e72e4]"
            />
          </label>
          <label class="block">
            <span class="text-[#525f7f] dark:text-[#a3adc2]">{t("ovCommitTimeout")}</span>
            <input
              type="number"
              min={5}
              value={props.config.system.commit_confirm_timeout_secs}
              onInput={(e) => {
                const next = structuredClone(props.config);
                next.system.commit_confirm_timeout_secs = Math.max(
                  5,
                  Number(e.currentTarget.value) || 60,
                );
                props.onChangeConfig(next);
              }}
              class="mt-1 w-full rounded-lg border border-[#e9ecef] dark:border-[#232a3b] bg-[#f8f9fe] dark:bg-[#0b0e14] px-2.5 py-1.5 font-mono text-xs text-[#172b4d] dark:text-[#eef1f6] outline-none focus:border-[#5e72e4]"
            />
          </label>
        </div>
      </section>

      <section class="rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-white dark:bg-[#131722] p-4 sm:p-5 argon-card">
        <h3 class="mb-3.5 text-sm font-semibold text-[#172b4d] dark:text-[#eef1f6]">
          {t("ovConfigHistory")}
        </h3>
        <Show
          when={props.history.length > 0}
          fallback={<p class="text-xs text-[#525f7f] dark:text-[#a3adc2]">{t("ovNoHistory")}</p>}
        >
          <div class="space-y-2">
            <For each={props.history}>
              {(snap) => (
                <div class="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-[#f8f9fe] dark:bg-[#1a1f2e] px-3.5 py-2.5 text-xs">
                  <div>
                    <span class="font-mono font-semibold text-[#5e72e4] dark:text-[#8090f7]">
                      Gen #{snap.generation}
                    </span>
                    <span class="ml-2 text-[#8898aa] dark:text-[#6c7893]">{snap.created_at}</span>
                    <div class="mt-0.5 text-[#172b4d] dark:text-[#eef1f6]">{snap.summary}</div>
                  </div>
                  <button
                    type="button"
                    onClick={() => props.onRestoreHistory(snap.generation)}
                    class="rounded-lg border border-[#e9ecef] dark:border-[#232a3b] bg-white dark:bg-[#131722] hover:bg-[#f4f5f7] px-3 py-1 text-xs font-medium text-[#172b4d] dark:text-[#eef1f6] cursor-pointer transition-colors"
                  >
                    {t("ovRestoreSnapshot")}
                  </button>
                </div>
              )}
            </For>
          </div>
        </Show>
      </section>
    </div>
  );
}

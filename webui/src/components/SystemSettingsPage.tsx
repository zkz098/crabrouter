import { For, Show, createSignal } from "solid-js";
import { exportConfigBackup, importConfigBackup } from "../api";
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
  const [backupMsg, setBackupMsg] = createSignal<string | null>(null);
  const [backupBusy, setBackupBusy] = createSignal(false);

  const handleExport = async () => {
    setBackupBusy(true);
    setBackupMsg(null);
    try {
      const tomlText = await exportConfigBackup();
      const blob = new Blob([tomlText], { type: "application/toml;charset=utf-8" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `crabrouter-config-${props.config.system.hostname || "backup"}.toml`;
      a.click();
      URL.revokeObjectURL(url);
      setBackupMsg("已导出配置文件 (crabrouter-config.toml)");
    } catch (err) {
      setBackupMsg(`导出失败: ${String(err)}`);
    } finally {
      setBackupBusy(false);
    }
  };

  const handleImportFile = async (e: Event) => {
    const input = e.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) {
      return;
    }
    setBackupBusy(true);
    setBackupMsg(null);
    try {
      const text = await file.text();
      await importConfigBackup(text);
      setBackupMsg("已提交导入配置，请在顶部提示栏确认提交 (Commit-Confirm)");
    } catch (err) {
      setBackupMsg(`导入失败: ${String(err)}`);
    } finally {
      input.value = "";
      setBackupBusy(false);
    }
  };

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

      <section class="rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-white dark:bg-[#131722] p-4 sm:p-5 argon-card space-y-4">
        <h3 class="text-sm font-semibold text-[#172b4d] dark:text-[#eef1f6]">
          NTP 时间同步与局域网校时 (chrony)
        </h3>
        <div class="grid gap-3 text-xs sm:grid-cols-2">
          <label class="block sm:col-span-2">
            <span class="text-[#525f7f] dark:text-[#a3adc2]">上游 NTP 服务器 (逗号分隔)</span>
            <input
              type="text"
              value={props.config.system.ntp_servers.join(", ")}
              onInput={(e) => {
                const next = structuredClone(props.config);
                next.system.ntp_servers = e.currentTarget.value
                  .split(",")
                  .map((s) => s.trim())
                  .filter(Boolean);
                props.onChangeConfig(next);
              }}
              class="mt-1 w-full rounded-lg border border-[#e9ecef] dark:border-[#232a3b] bg-[#f8f9fe] dark:bg-[#0b0e14] px-2.5 py-1.5 font-mono text-xs text-[#172b4d] dark:text-[#eef1f6] outline-none focus:border-[#5e72e4]"
            />
          </label>
          <label class="flex items-center gap-2 text-[#172b4d] dark:text-[#eef1f6]">
            <input
              type="checkbox"
              checked={props.config.system.ntp_server_enabled}
              onChange={(e) => {
                const next = structuredClone(props.config);
                next.system.ntp_server_enabled = e.currentTarget.checked;
                props.onChangeConfig(next);
              }}
            />
            开启局域网 NTP 校时服务 (允许 LAN / VLAN 客户端同步时间)
          </label>
          <label class="flex items-center gap-2 text-[#172b4d] dark:text-[#eef1f6]">
            <input
              type="checkbox"
              checked={props.config.dns.use_isp_dns}
              onChange={(e) => {
                const next = structuredClone(props.config);
                next.dns.use_isp_dns = e.currentTarget.checked;
                props.onChangeConfig(next);
              }}
            />
            合并运营商下发 DNS 到 crabdns 上游 (use_isp_dns)
          </label>
        </div>
      </section>

      <section class="rounded-xl border border-[#e9ecef] dark:border-[#232a3b] bg-white dark:bg-[#131722] p-4 sm:p-5 argon-card space-y-3">
        <h3 class="text-sm font-semibold text-[#172b4d] dark:text-[#eef1f6]">
          配置备份与恢复 (TOML)
        </h3>
        <div class="flex flex-wrap items-center gap-3 text-xs">
          <button
            type="button"
            disabled={backupBusy()}
            onClick={handleExport}
            class="rounded-lg border border-[#5e72e4] bg-[#5e72e4] hover:bg-[#4c60d4] px-3.5 py-1.5 font-medium text-white cursor-pointer disabled:opacity-60 transition-colors"
          >
            导出配置备份 (.toml)
          </button>
          <label class="rounded-lg border border-[#e9ecef] dark:border-[#232a3b] bg-[#f8f9fe] dark:bg-[#1a1f2e] hover:bg-[#eef0f6] dark:hover:bg-[#232a3b] px-3.5 py-1.5 font-medium text-[#172b4d] dark:text-[#eef1f6] cursor-pointer transition-colors">
            <span>导入并应用配置</span>
            <input
              type="file"
              accept=".toml,text/plain"
              disabled={backupBusy()}
              onChange={handleImportFile}
              class="hidden"
            />
          </label>
        </div>
        <Show when={backupMsg()}>
          <p class="text-xs text-[#5e72e4] dark:text-[#8090f7]">{backupMsg()}</p>
        </Show>
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

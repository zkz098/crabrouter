import { For, Show, createResource, createSignal } from "solid-js";
import {
  deleteSysextImage,
  getAppCatalog,
  getInstalledApps,
  getSysextStatus,
  installApp,
  refreshSysext,
  startApp,
  stopApp,
  uninstallApp,
  unmergeSysext,
  uploadSysextImage,
} from "../api";
import type { AppCatalogEntry } from "../types/AppCatalogEntry";
import type { RouterConfig } from "../types/RouterConfig";
import { t } from "../i18n";
import { GuideBanner, PageHeader } from "./ui/ArgonPrimitives";

interface Props {
  draft: RouterConfig;
  onEnableContainers: () => void;
}

const CARD_CLS =
  "rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 space-y-4";
const BTN_PRIMARY =
  "rounded-lg bg-[#171717] hover:bg-[#383838] dark:bg-[#ededed] dark:hover:bg-[#d4d4d8] px-3 py-1.5 text-xs font-medium text-white dark:text-[#0a0a0a] border-0 cursor-pointer transition-all disabled:opacity-50";
const BTN_OUTLINE =
  "rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#111111] hover:bg-[#f4f4f5] dark:hover:bg-[#1a1a1a] px-3 py-1.5 text-xs font-medium text-[#171717] dark:text-[#ededed] cursor-pointer transition-all disabled:opacity-50";

export function ExtensionsPage(props: Props) {
  const [catalog] = createResource(getAppCatalog);
  const [installed, { refetch: refetchInstalled }] = createResource(getInstalledApps);
  const [sysext, { refetch: refetchSysext }] = createResource(getSysextStatus);

  const [activeApp, setActiveApp] = createSignal<AppCatalogEntry | null>(null);
  const [settingsMap, setSettingsMap] = createSignal<Record<string, string>>({});
  const [busyMsg, setBusyMsg] = createSignal<string | null>(null);
  const [errorMsg, setErrorMsg] = createSignal<string | null>(null);

  function openInstallForm(entry: AppCatalogEntry) {
    const defaults: Record<string, string> = {};
    for (const f of entry.fields) {
      defaults[f.key] = f.default_value;
    }
    setSettingsMap(defaults);
    setActiveApp(entry);
    setErrorMsg(null);
  }

  async function handleInstall() {
    const entry = activeApp();
    if (!entry) {
      return;
    }
    setBusyMsg(`正在安装 ${entry.name}...`);
    setErrorMsg(null);
    try {
      await installApp({
        app_id: entry.id,
        settings: settingsMap(),
        start_immediately: true,
      });
      setActiveApp(null);
      await refetchInstalled();
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : String(err));
    } finally {
      setBusyMsg(null);
    }
  }

  async function handleAppAction(id: string, action: "start" | "stop" | "uninstall") {
    setBusyMsg(`正在执行 ${action}...`);
    setErrorMsg(null);
    try {
      if (action === "start") {
        await startApp(id);
      } else if (action === "stop") {
        await stopApp(id);
      } else {
        await uninstallApp(id);
      }
      await refetchInstalled();
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : String(err));
    } finally {
      setBusyMsg(null);
    }
  }

  async function handleSysextUpload(file: File | undefined) {
    if (!file) {
      return;
    }
    setBusyMsg(`正在上传扩展镜像 ${file.name}...`);
    setErrorMsg(null);
    try {
      await uploadSysextImage(file);
      await refetchSysext();
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : String(err));
    } finally {
      setBusyMsg(null);
    }
  }

  async function handleSysextOp(op: "refresh" | "unmerge" | string) {
    setBusyMsg(`正在执行 sysext 操作...`);
    setErrorMsg(null);
    try {
      if (op === "refresh") {
        await refreshSysext();
      } else if (op === "unmerge") {
        await unmergeSysext();
      } else {
        await deleteSysextImage(op);
      }
      await refetchSysext();
    } catch (err) {
      setErrorMsg(err instanceof Error ? err.message : String(err));
    } finally {
      setBusyMsg(null);
    }
  }

  const installedById = () => {
    const map = new Map<string, NonNullable<ReturnType<typeof installed>>[number]>();
    for (const item of installed() ?? []) {
      map.set(item.app_id, item);
    }
    return map;
  };

  return (
    <div class="space-y-5 animate-view-enter">
      <PageHeader title={t("navExtensions")} subtitle={t("descExtensions")} />
      <GuideBanner
        title={t("extGuideTitle")}
        description={t("extGuideDesc")}
        variant="info"
        dismissible
      />
      <Show when={errorMsg()}>
        <div class="rounded-xl border border-rose-500/30 bg-rose-500/10 px-4 py-3 text-xs text-rose-600 dark:text-rose-400">
          {errorMsg()}
        </div>
      </Show>
      <Show when={busyMsg()}>
        <div class="rounded-xl border border-sky-500/30 bg-sky-500/10 px-4 py-2.5 text-xs text-sky-700 dark:text-sky-300">
          {busyMsg()}
        </div>
      </Show>

      {/* 1. 容器化插件商店 (App Store) */}
      <section class={CARD_CLS}>
        <div class="flex flex-wrap items-center justify-between gap-2">
          <div>
            <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
              容器化插件商店 (Podman App Store)
            </h2>
            <p class="mt-0.5 text-xs text-[#666666] dark:text-[#a1a1aa]">
              基于 Podman 运行用户态网络与存储服务，数据持久化在 /var，OTA 升级零损耗保留
            </p>
          </div>
          <Show when={!props.draft.containers.enabled}>
            <button type="button" onClick={props.onEnableContainers} class={BTN_PRIMARY}>
              启用容器运行时
            </button>
          </Show>
        </div>

        <div class="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          <For each={catalog() ?? []}>
            {(entry) => {
              const inst = () => installedById().get(entry.id);
              return (
                <div class="flex flex-col justify-between rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] p-3.5 text-xs space-y-3">
                  <div>
                    <div class="flex items-center justify-between gap-2">
                      <span class="font-medium text-[#171717] dark:text-[#ededed]">
                        {entry.name}
                      </span>
                      <span class="rounded bg-[#eaeaea] dark:bg-[#262626] px-1.5 py-0.5 font-mono text-[10px] text-[#666666] dark:text-[#a1a1aa]">
                        {entry.category}
                      </span>
                    </div>
                    <p class="mt-1.5 text-[#666666] dark:text-[#a1a1aa] leading-relaxed">
                      {entry.summary}
                    </p>
                  </div>

                  <Show
                    when={inst()}
                    fallback={
                      <button
                        type="button"
                        disabled={!props.draft.containers.enabled || Boolean(busyMsg())}
                        onClick={() => openInstallForm(entry)}
                        class={BTN_PRIMARY}
                      >
                        安装插件
                      </button>
                    }
                  >
                    {(app) => (
                      <div class="space-y-2 border-t border-[#eaeaea] dark:border-[#262626] pt-2.5">
                        <div class="flex items-center justify-between text-[11px] font-mono">
                          <span
                            class={
                              app().state === "running"
                                ? "text-emerald-600 dark:text-emerald-400"
                                : "text-amber-600 dark:text-amber-400"
                            }
                          >
                            ● {app().state}
                          </span>
                          <Show when={app().web_port}>
                            {(port) => (
                              <a
                                href={`http://${window.location.hostname}:${port()}`}
                                target="_blank"
                                rel="noreferrer"
                                class="text-sky-600 dark:text-sky-400 hover:underline"
                              >
                                打开 :{port()}
                              </a>
                            )}
                          </Show>
                        </div>
                        <div class="flex flex-wrap gap-1.5">
                          <Show
                            when={app().state === "running"}
                            fallback={
                              <button
                                type="button"
                                onClick={() => void handleAppAction(entry.id, "start")}
                                class={BTN_OUTLINE}
                              >
                                启动
                              </button>
                            }
                          >
                            <button
                              type="button"
                              onClick={() => void handleAppAction(entry.id, "stop")}
                              class={BTN_OUTLINE}
                            >
                              停止
                            </button>
                          </Show>
                          <button
                            type="button"
                            onClick={() => void handleAppAction(entry.id, "uninstall")}
                            class="rounded-lg border border-rose-500/30 bg-rose-500/10 px-2.5 py-1 text-xs text-rose-600 dark:text-rose-400 cursor-pointer"
                          >
                            卸载
                          </button>
                        </div>
                      </div>
                    )}
                  </Show>
                </div>
              );
            }}
          </For>
        </div>

        <Show when={activeApp()}>
          {(entry) => (
            <div class="rounded-xl border border-[#171717]/20 dark:border-white/20 bg-[#fafafa] dark:bg-[#111111] p-4 space-y-3 text-xs">
              <div class="flex items-center justify-between">
                <span class="font-medium text-[#171717] dark:text-[#ededed]">
                  配置并安装：{entry().name}
                </span>
                <button type="button" onClick={() => setActiveApp(null)} class={BTN_OUTLINE}>
                  取消
                </button>
              </div>
              <div class="grid gap-3 sm:grid-cols-2">
                <For each={entry().fields}>
                  {(f) => (
                    <label class="block">
                      <span class="text-[#666666] dark:text-[#a1a1aa]">
                        {f.label} {f.required ? "*" : ""}
                      </span>
                      <input
                        type={f.field_type === "password" ? "password" : "text"}
                        placeholder={f.placeholder}
                        value={settingsMap()[f.key] ?? f.default_value}
                        onInput={(e) =>
                          setSettingsMap((prev) => ({
                            ...prev,
                            [f.key]: e.currentTarget.value,
                          }))
                        }
                        class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] px-2.5 py-1.5 font-mono text-xs text-[#171717] dark:text-[#ededed]"
                      />
                    </label>
                  )}
                </For>
              </div>
              <div class="flex justify-end">
                <button
                  type="button"
                  disabled={Boolean(busyMsg())}
                  onClick={() => void handleInstall()}
                  class={BTN_PRIMARY}
                >
                  确认安装并启动
                </button>
              </div>
            </div>
          )}
        </Show>
      </section>

      {/* 2. systemd-sysext 系统与内核驱动扩展 */}
      <section class={CARD_CLS}>
        <div class="flex flex-wrap items-center justify-between gap-2">
          <div>
            <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
              系统与驱动扩展 (systemd-sysext)
            </h2>
            <p class="mt-0.5 text-xs text-[#666666] dark:text-[#a1a1aa]">
              将只读 .raw 扩展镜像叠加到 /usr，用于补充内核模块 (kmod) 与系统工具，不破坏 A/B 根分区
            </p>
          </div>
          <div class="flex flex-wrap items-center gap-2">
            <label class={BTN_OUTLINE}>
              上传 .raw 扩展
              <input
                type="file"
                accept=".raw"
                class="hidden"
                onChange={(e) => void handleSysextUpload(e.currentTarget.files?.[0])}
              />
            </label>
            <button
              type="button"
              onClick={() => void handleSysextOp("refresh")}
              class={BTN_PRIMARY}
            >
              刷新挂载 (Refresh)
            </button>
            <button
              type="button"
              onClick={() => void handleSysextOp("unmerge")}
              class={BTN_OUTLINE}
            >
              全部卸载 (Unmerge)
            </button>
          </div>
        </div>

        <Show
          when={(sysext()?.extensions.length ?? 0) > 0}
          fallback={
            <p class="text-xs text-[#666666] dark:text-[#a1a1aa]">
              当前 {sysext()?.extensions_dir ?? "/var/lib/extensions"} 目录下暂无 .raw 扩展镜像
            </p>
          }
        >
          <div class="overflow-x-auto">
            <table class="w-full text-left text-xs font-mono">
              <thead class="text-[#666666] dark:text-[#a1a1aa]">
                <tr>
                  <th class="py-1.5 pr-3">镜像名称</th>
                  <th class="py-1.5 pr-3">状态</th>
                  <th class="py-1.5 pr-3">大小</th>
                  <th class="py-1.5 pr-3">SHA-256</th>
                  <th class="py-1.5 text-right">操作</th>
                </tr>
              </thead>
              <tbody>
                <For each={sysext()?.extensions ?? []}>
                  {(ext) => (
                    <tr class="border-t border-[#eaeaea] dark:border-[#262626]">
                      <td class="py-2 pr-3">{ext.filename}</td>
                      <td class="py-2 pr-3">
                        <span
                          class={
                            ext.active ? "text-emerald-600 dark:text-emerald-400" : "text-zinc-500"
                          }
                        >
                          {ext.active ? "已激活" : "未挂载"}
                        </span>
                      </td>
                      <td class="py-2 pr-3">{(ext.size_bytes / 1024 / 1024).toFixed(2)} MiB</td>
                      <td class="py-2 pr-3 text-[#666666] dark:text-[#a1a1aa]">
                        {ext.sha256.slice(0, 12)}
                      </td>
                      <td class="py-2 text-right">
                        <button
                          type="button"
                          onClick={() => void handleSysextOp(ext.filename)}
                          class="text-rose-600 dark:text-rose-400 hover:underline bg-transparent border-0 cursor-pointer text-xs"
                        >
                          删除
                        </button>
                      </td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
        </Show>
      </section>
    </div>
  );
}

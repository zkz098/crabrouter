import { For, Show, createSignal } from "solid-js";
import {
  applyOtaUpdate,
  blessOtaBoot,
  checkOtaUpdates,
  rebootSystem,
  uploadOtaImage,
} from "../api";
import type { OtaCommandResult } from "../types/OtaCommandResult";
import type { OtaStatus } from "../types/OtaStatus";
import type { RouterConfig } from "../types/RouterConfig";

interface Props {
  draft: RouterConfig;
  otaStatus: OtaStatus | undefined;
  onChange: (next: RouterConfig) => void;
  onRefreshOta: () => void;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let idx = 0;
  while (value >= 1024 && idx < units.length - 1) {
    value /= 1024;
    idx += 1;
  }
  return `${value.toFixed(1)} ${units[idx]}`;
}

export function OtaPage(props: Props) {
  const [sourceUrl, setSourceUrl] = createSignal(props.draft.system.ota_update_url);
  const [verifySig, setVerifySig] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [result, setResult] = createSignal<OtaCommandResult | null>(null);
  const [confirmReboot, setConfirmReboot] = createSignal(false);

  async function runAction(fn: () => Promise<OtaCommandResult>) {
    setBusy(true);
    try {
      const res = await fn();
      setResult(res);
      props.onRefreshOta();
    } catch (err) {
      setResult({
        ok: false,
        action: "ota",
        message: err instanceof Error ? err.message : String(err),
        output: "",
        reboot_required: false,
      });
    } finally {
      setBusy(false);
    }
  }

  async function handleFileUpload(files: FileList | null) {
    const file = files?.[0];
    if (!file) {
      return;
    }
    setBusy(true);
    try {
      const staged = await uploadOtaImage(file);
      setResult({
        ok: true,
        action: "upload",
        message: `镜像 ${staged.filename} 已上传并校验 SHA256`,
        output: `sha256: ${staged.sha256} (${formatBytes(staged.size_bytes)})`,
        reboot_required: false,
      });
      props.onRefreshOta();
    } catch (err) {
      setResult({
        ok: false,
        action: "upload",
        message: err instanceof Error ? err.message : String(err),
        output: "",
        reboot_required: false,
      });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div class="space-y-6">
      {/* A/B Slot & Boot Assessment Cards */}
      <section class="grid gap-4 md:grid-cols-3">
        <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
          <div class="text-xs text-slate-400">当前活跃槽位 (A/B Root)</div>
          <div class="mt-1 font-mono text-xl font-semibold text-orange-400">
            Slot {props.otaStatus?.active_slot ?? "未知"}
          </div>
          <div class="mt-1 text-xs text-slate-400">
            系统版本: {props.otaStatus?.current_version ?? "dev/unreleased"}
          </div>
        </div>

        <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
          <div class="text-xs text-slate-400">systemd-boot 启动评估</div>
          <div class="mt-1 flex items-center justify-between">
            <span class="font-mono text-xl font-semibold text-emerald-400">
              {props.otaStatus?.boot_assessment ?? "-"}
            </span>
            <button
              type="button"
              disabled={busy()}
              onClick={() => void runAction(() => blessOtaBoot())}
              class="rounded border border-emerald-500/40 bg-emerald-500/10 px-2.5 py-1 text-xs text-emerald-300 hover:bg-emerald-500/20 disabled:opacity-50"
            >
              确认槽位健康
            </button>
          </div>
          <div class="mt-1 text-xs text-slate-400">升级后若连续 3 次未通过健康检查将自动回滚</div>
        </div>

        <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
          <div class="text-xs text-slate-400">槽位切换与重启</div>
          <div class="mt-2 flex items-center gap-2">
            <Show
              when={confirmReboot()}
              fallback={
                <button
                  type="button"
                  disabled={busy()}
                  onClick={() => setConfirmReboot(true)}
                  class="rounded-lg bg-amber-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-amber-500 disabled:opacity-50"
                >
                  重启并切换新槽位
                </button>
              }
            >
              <button
                type="button"
                disabled={busy()}
                onClick={() => {
                  setConfirmReboot(false);
                  void runAction(() => rebootSystem());
                }}
                class="rounded-lg bg-red-600 px-3 py-1.5 text-xs font-semibold text-white hover:bg-red-500"
              >
                确认立即重启
              </button>
              <button
                type="button"
                onClick={() => setConfirmReboot(false)}
                class="rounded-lg border border-slate-700 bg-slate-800 px-3 py-1.5 text-xs hover:bg-slate-700"
              >
                取消
              </button>
            </Show>
          </div>
          <div class="mt-2 text-xs text-slate-400">通过 systemd-logind D-Bus 触发平滑重启</div>
        </div>
      </section>

      {/* Remote OTA Feed */}
      <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
        <h2 class="mb-3 text-sm font-semibold text-slate-200">
          在线 OTA 拉取升级 (systemd-sysupdate)
        </h2>
        <div class="flex flex-wrap items-end gap-3 text-xs">
          <label class="flex-1 min-w-64 block">
            <span class="text-slate-400">OTA 镜像源 URL</span>
            <input
              type="text"
              value={sourceUrl()}
              onInput={(e) => {
                const val = e.currentTarget.value;
                setSourceUrl(val);
                const next = structuredClone(props.draft);
                next.system.ota_update_url = val;
                props.onChange(next);
              }}
              class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 font-mono text-xs"
            />
          </label>
          <label class="flex items-center gap-2 pb-2 text-slate-300">
            <input
              type="checkbox"
              checked={verifySig()}
              onChange={(e) => setVerifySig(e.currentTarget.checked)}
            />
            校验签名 (SHA256SUMS.gpg)
          </label>
          <button
            type="button"
            disabled={busy()}
            onClick={() => void runAction(() => checkOtaUpdates())}
            class="rounded-lg border border-slate-700 bg-slate-800 px-3 py-2 font-medium hover:bg-slate-700 disabled:opacity-50"
          >
            检查更新
          </button>
          <button
            type="button"
            disabled={busy()}
            onClick={() =>
              void runAction(() =>
                applyOtaUpdate({
                  source_url: sourceUrl(),
                  staged_file: null,
                  verify_signature: verifySig(),
                }),
              )
            }
            class="rounded-lg bg-orange-600 px-4 py-2 font-semibold text-white hover:bg-orange-500 disabled:opacity-50"
          >
            拉取并写入备用槽位
          </button>
        </div>
      </section>

      {/* Local Upload & Staged Images */}
      <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
        <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
          <div>
            <h2 class="text-sm font-semibold text-slate-200">本地上传 OTA 镜像升级</h2>
            <p class="text-xs text-slate-400">
              支持上传 Debian 13 `mkosi` 产出的 `.raw` / `.raw.xz` / `.efi` 固件镜像。
            </p>
          </div>
          <label class="cursor-pointer rounded-lg bg-sky-600 px-4 py-2 text-xs font-medium text-white hover:bg-sky-500">
            选择固件文件上传
            <input
              type="file"
              accept=".raw,.xz,.zst,.efi,.img"
              disabled={busy()}
              onChange={(e) => void handleFileUpload(e.currentTarget.files)}
              class="hidden"
            />
          </label>
        </div>

        <Show
          when={(props.otaStatus?.staged_images.length ?? 0) > 0}
          fallback={<p class="text-xs text-slate-400">暂未上传本地固件镜像。</p>}
        >
          <div class="space-y-2">
            <For each={props.otaStatus?.staged_images ?? []}>
              {(img) => (
                <div class="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-slate-800 bg-slate-950/60 px-3 py-2 text-xs">
                  <div>
                    <div class="font-mono font-semibold text-slate-100">{img.filename}</div>
                    <div class="mt-0.5 font-mono text-[11px] text-slate-400">
                      {formatBytes(img.size_bytes)} · SHA256: {img.sha256.slice(0, 24)}...
                    </div>
                  </div>
                  <button
                    type="button"
                    disabled={busy()}
                    onClick={() =>
                      void runAction(() =>
                        applyOtaUpdate({
                          source_url: null,
                          staged_file: img.filename,
                          verify_signature: verifySig(),
                        }),
                      )
                    }
                    class="rounded-lg bg-orange-600 px-3 py-1.5 font-medium text-white hover:bg-orange-500 disabled:opacity-50"
                  >
                    写入备用槽位
                  </button>
                </div>
              )}
            </For>
          </div>
        </Show>
      </section>

      <Show when={result()}>
        {(res) => (
          <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
            <div class="mb-2 flex items-center justify-between text-xs">
              <span
                class={res().ok ? "font-semibold text-emerald-300" : "font-semibold text-red-300"}
              >
                [{res().action}] {res().message}
              </span>
              <Show when={res().reboot_required}>
                <span class="rounded bg-amber-500/20 px-2 py-0.5 text-amber-300">需要重启生效</span>
              </Show>
            </div>
            <Show when={res().output}>
              <pre class="max-h-64 overflow-auto rounded-lg border border-slate-800 bg-slate-950 p-3 font-mono text-xs text-slate-300">
                {res().output}
              </pre>
            </Show>
          </section>
        )}
      </Show>
    </div>
  );
}

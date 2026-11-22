import { For, Show, createSignal } from "solid-js";
import {
  applyOtaUpdate,
  blessOtaBoot,
  checkOtaUpdates,
  rebootSystem,
  uploadOtaImage,
} from "../api";
import { t } from "../i18n";
import type { OtaCommandResult } from "../types/OtaCommandResult";
import type { OtaStatus } from "../types/OtaStatus";
import type { RouterConfig } from "../types/RouterConfig";

interface Props {
  draft: RouterConfig;
  otaStatus: OtaStatus | undefined;
  onChange: (next: RouterConfig) => void;
  onRefreshOta: () => void;
}

const SECTION_CLS =
  "rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-colors hover:border-[#d4d4d8] dark:hover:border-[#3a3a3a]";
const BTN_OUTLINE =
  "rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#111111] hover:bg-[#f4f4f5] dark:hover:bg-[#1a1a1a] px-3 py-1.5 text-xs font-medium text-[#171717] dark:text-[#ededed] cursor-pointer transition-all active:scale-[0.97] disabled:opacity-50";
const BTN_PRIMARY =
  "rounded-lg bg-[#171717] hover:bg-[#383838] dark:bg-[#ededed] dark:hover:bg-[#d4d4d8] px-4 py-1.5 text-xs font-medium text-white dark:text-[#0a0a0a] border-0 cursor-pointer transition-all active:scale-[0.97] disabled:opacity-50";

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
        message: t("otaUploadedMsg").replace("{name}", staged.filename),
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
    <div class="space-y-5 sm:space-y-6 animate-view-enter">
      {/* A/B Slot & Boot Assessment Cards */}
      <section class="grid gap-4 md:grid-cols-3">
        <div class={`${SECTION_CLS} animate-card-enter`}>
          <div class="flex items-center justify-between gap-2">
            <span class="text-xs font-medium text-[#666666] dark:text-[#a1a1aa]">
              {t("otaActiveSlot")}
            </span>
            <div class="w-8 h-8 rounded-lg flex items-center justify-center shrink-0 text-sky-600 dark:text-sky-400 bg-sky-500/10">
              <span class="i-ri-hard-drive-2-line text-base" />
            </div>
          </div>
          <div class="mt-2.5 font-mono text-2xl font-semibold tracking-tight text-[#171717] dark:text-[#ededed]">
            Slot {props.otaStatus?.active_slot ?? t("otaUnknown")}
          </div>
          <div class="mt-1 text-xs text-[#666666] dark:text-[#a1a1aa]">
            {t("otaSystemVersion")}: {props.otaStatus?.current_version ?? "dev/unreleased"}
          </div>
        </div>

        <div class={`${SECTION_CLS} animate-card-enter`} style={{ "animation-delay": "60ms" }}>
          <div class="flex items-center justify-between gap-2">
            <span class="text-xs font-medium text-[#666666] dark:text-[#a1a1aa]">
              {t("otaBootAssess")}
            </span>
            <button
              type="button"
              disabled={busy()}
              onClick={() => void runAction(() => blessOtaBoot())}
              class="rounded-lg border border-emerald-500/25 bg-emerald-500/10 hover:bg-emerald-500/20 px-2.5 py-1 text-[11px] font-medium text-emerald-700 dark:text-emerald-400 cursor-pointer transition-colors disabled:opacity-50"
            >
              {t("otaBlessBoot")}
            </button>
          </div>
          <div class="mt-2.5 font-mono text-2xl font-semibold tracking-tight text-emerald-600 dark:text-emerald-400">
            {props.otaStatus?.boot_assessment ?? "-"}
          </div>
          <div class="mt-1 text-xs text-[#666666] dark:text-[#a1a1aa]">{t("otaBootDesc")}</div>
        </div>

        <div class={`${SECTION_CLS} animate-card-enter`} style={{ "animation-delay": "120ms" }}>
          <div class="flex items-center justify-between gap-2">
            <span class="text-xs font-medium text-[#666666] dark:text-[#a1a1aa]">
              {t("otaSlotSwitch")}
            </span>
            <div class="w-8 h-8 rounded-lg flex items-center justify-center shrink-0 text-amber-600 dark:text-amber-400 bg-amber-500/10">
              <span class="i-ri-restart-line text-base" />
            </div>
          </div>
          <div class="mt-3 flex items-center gap-2">
            <Show
              when={confirmReboot()}
              fallback={
                <button
                  type="button"
                  disabled={busy()}
                  onClick={() => setConfirmReboot(true)}
                  class="rounded-lg border border-amber-500/30 bg-amber-500/10 hover:bg-amber-500/20 px-3 py-1.5 text-xs font-medium text-amber-700 dark:text-amber-300 cursor-pointer transition-colors disabled:opacity-50"
                >
                  {t("otaRebootSwitch")}
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
                class="rounded-lg bg-rose-600 hover:bg-rose-500 px-3 py-1.5 text-xs font-medium text-white border-0 cursor-pointer transition-colors"
              >
                {t("otaConfirmReboot")}
              </button>
              <button type="button" onClick={() => setConfirmReboot(false)} class={BTN_OUTLINE}>
                {t("commonCancel")}
              </button>
            </Show>
          </div>
          <div class="mt-2 text-xs text-[#666666] dark:text-[#a1a1aa]">{t("otaRebootDesc")}</div>
        </div>
      </section>

      {/* Remote OTA Feed */}
      <section class={SECTION_CLS}>
        <h2 class="mb-3.5 text-sm font-medium text-[#171717] dark:text-[#ededed]">
          {t("otaRemoteTitle")}
        </h2>
        <div class="flex flex-wrap items-end gap-3 text-xs">
          <label class="flex-1 min-w-64 block">
            <span class="text-[#666666] dark:text-[#a1a1aa]">{t("otaFeedUrl")}</span>
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
              class="mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#000000] px-3 py-2 font-mono text-xs text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed]"
            />
          </label>
          <label class="flex items-center gap-2 pb-2 text-[#171717] dark:text-[#ededed]">
            <input
              type="checkbox"
              checked={verifySig()}
              onChange={(e) => setVerifySig(e.currentTarget.checked)}
            />
            {t("otaVerifySig")}
          </label>
          <button
            type="button"
            disabled={busy()}
            onClick={() => void runAction(() => checkOtaUpdates())}
            class={BTN_OUTLINE}
          >
            {t("otaCheckUpdates")}
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
            class={BTN_PRIMARY}
          >
            {t("otaPullApply")}
          </button>
        </div>
      </section>

      {/* Local Upload & Staged Images */}
      <section class={SECTION_CLS}>
        <div class="mb-3.5 flex flex-wrap items-center justify-between gap-2">
          <div>
            <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
              {t("otaLocalTitle")}
            </h2>
            <p class="mt-0.5 text-xs text-[#666666] dark:text-[#a1a1aa]">{t("otaLocalDesc")}</p>
          </div>
          <label class={BTN_PRIMARY}>
            {t("otaSelectFile")}
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
          fallback={<p class="text-xs text-[#666666] dark:text-[#a1a1aa]">{t("otaNoStaged")}</p>}
        >
          <div class="space-y-2">
            <For each={props.otaStatus?.staged_images ?? []}>
              {(img) => (
                <div class="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] px-3.5 py-2.5 text-xs">
                  <div>
                    <div class="font-mono font-semibold text-[#171717] dark:text-[#ededed]">
                      {img.filename}
                    </div>
                    <div class="mt-0.5 font-mono text-[11px] text-[#666666] dark:text-[#a1a1aa]">
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
                    class={BTN_PRIMARY}
                  >
                    {t("otaWriteStandby")}
                  </button>
                </div>
              )}
            </For>
          </div>
        </Show>
      </section>

      <Show when={result()}>
        {(res) => (
          <section class={SECTION_CLS}>
            <div class="mb-2.5 flex items-center justify-between text-xs">
              <span
                class={`font-medium ${
                  res().ok
                    ? "text-emerald-600 dark:text-emerald-400"
                    : "text-rose-600 dark:text-rose-400"
                }`}
              >
                [{res().action}] {res().message}
              </span>
              <Show when={res().reboot_required}>
                <span class="rounded-full border border-amber-500/20 bg-amber-500/10 px-2 py-0.5 text-[11px] font-medium text-amber-700 dark:text-amber-400">
                  {t("otaRebootRequired")}
                </span>
              </Show>
            </div>
            <Show when={res().output}>
              <pre class="max-h-64 overflow-auto rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#000000] p-3 font-mono text-xs text-[#171717] dark:text-[#ededed]">
                {res().output}
              </pre>
            </Show>
          </section>
        )}
      </Show>
    </div>
  );
}

import { Show, createEffect, createSignal, onCleanup } from "solid-js";
import type { PendingCommit } from "../types/PendingCommit";

interface Props {
  pending: PendingCommit | null | undefined;
  timeoutSecs: number;
  busy: boolean;
  onConfirm: () => void;
  onRollback: () => void;
}

export function CommitConfirmModal(props: Props) {
  const [remainingSecs, setRemainingSecs] = createSignal(0);

  createEffect(() => {
    const p = props.pending;
    if (!p) {
      setRemainingSecs(0);
      return;
    }

    const update = () => {
      const ms = Date.parse(p.expires_at) - Date.now();
      setRemainingSecs(Math.max(0, Math.ceil(ms / 1000)));
    };
    update();
    const timer = window.setInterval(update, 250);
    onCleanup(() => window.clearInterval(timer));
  });

  const progressPct = () => {
    const total = Math.max(props.timeoutSecs, 5);
    return Math.min(100, Math.max(0, (remainingSecs() / total) * 100));
  };

  return (
    <Show when={props.pending}>
      {(pending) => (
        <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 dark:bg-black/80 p-4 backdrop-blur-sm">
          <div class="w-full max-w-md rounded-2xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-6 shadow-2xl animate-card-enter">
            <div class="flex items-start gap-3">
              <div class="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-amber-500/10 text-amber-600 dark:text-amber-400 border border-amber-500/20">
                <span class="i-ri-shield-check-line text-xl" />
              </div>
              <div class="flex-1">
                <h3 class="text-base font-semibold tracking-tight text-[#171717] dark:text-[#ededed]">
                  Commit-Confirm 防失联确认 (Gen #{pending().generation})
                </h3>
                <p class="mt-1 text-xs leading-relaxed text-[#666666] dark:text-[#a1a1aa]">
                  新网络与防火墙配置已生效。请确认当前管理连接正常，否则系统将在倒计时结束后自动回滚到上一份已确认配置。
                </p>
              </div>
            </div>

            <div class="mt-5 rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] p-4">
              <div class="flex items-baseline justify-between">
                <span class="text-xs text-[#666666] dark:text-[#a1a1aa]">自动回滚剩余时间</span>
                <span class="font-mono text-2xl font-semibold tabular-nums text-amber-600 dark:text-amber-400">
                  {remainingSecs()}s
                </span>
              </div>
              <div class="mt-2.5 h-2 w-full overflow-hidden rounded-full bg-[#eaeaea] dark:bg-[#262626]">
                <div
                  class="h-full rounded-full bg-amber-500 transition-all duration-200"
                  style={{ width: `${progressPct()}%` }}
                />
              </div>
            </div>

            <div class="mt-5 flex justify-end gap-2">
              <button
                type="button"
                disabled={props.busy}
                onClick={() => props.onRollback()}
                class="rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#111111] hover:bg-[#f4f4f5] dark:hover:bg-[#1a1a1a] px-4 py-2 text-xs font-medium text-[#171717] dark:text-[#ededed] cursor-pointer transition-all active:scale-[0.97] disabled:opacity-50"
              >
                立即回滚
              </button>
              <button
                type="button"
                disabled={props.busy}
                onClick={() => props.onConfirm()}
                class="rounded-lg bg-[#171717] hover:bg-[#383838] dark:bg-[#ededed] dark:hover:bg-[#d4d4d8] px-4 py-2 text-xs font-medium text-white dark:text-[#0a0a0a] border-0 cursor-pointer transition-all active:scale-[0.97] disabled:opacity-50"
              >
                连接正常，确认保存
              </button>
            </div>
          </div>
        </div>
      )}
    </Show>
  );
}

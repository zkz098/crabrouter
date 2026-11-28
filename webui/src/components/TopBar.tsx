import { Show } from "solid-js";
import { t } from "../i18n";
import { type PageTab, findNavMeta } from "../nav";

interface TopBarProps {
  tab: PageTab;
  wsConnected: boolean;
  isDirty: boolean;
  pending: boolean;
  busy: boolean;
  onOpenMobileMenu: () => void;
  onRefresh: () => void;
  onDiscard: () => void;
  onApply: () => void;
}

export function TopBar(props: TopBarProps) {
  const meta = () => findNavMeta(props.tab);

  return (
    <header class="sticky top-0 z-30 h-15 border-b border-[#e9ecef] dark:border-[#232a3b] bg-white/85 dark:bg-[#131722]/85 backdrop-blur-xl px-4 sm:px-6 flex items-center justify-between gap-3">
      <div class="flex items-center gap-3 min-w-0">
        <button
          type="button"
          onClick={() => props.onOpenMobileMenu()}
          class="md:hidden h-9 w-9 rounded-lg border border-[#e9ecef] dark:border-[#232a3b] bg-[#f8f9fe] dark:bg-[#1a1f2e] text-[#172b4d] dark:text-[#eef1f6] flex items-center justify-center cursor-pointer shrink-0"
          aria-label="Menu"
        >
          <span class="i-ri-menu-2-line text-lg" />
        </button>

        <div class="flex items-center gap-1.5 text-xs min-w-0 truncate">
          <span class="hidden sm:inline text-[#8898aa] dark:text-[#6c7893]">
            {meta().group.label()}
          </span>
          <span class="hidden sm:inline text-[#8898aa] dark:text-[#6c7893]">/</span>
          <span class="font-semibold text-[#172b4d] dark:text-[#eef1f6] truncate">
            {meta().item.label()}
          </span>
        </div>
      </div>

      <div class="flex items-center gap-2 shrink-0">
        <Show when={props.isDirty && !props.pending}>
          <div class="hidden sm:flex items-center gap-1.5 rounded-lg border border-amber-500/30 bg-amber-500/10 px-2.5 py-1 text-xs">
            <span class="w-1.5 h-1.5 rounded-full bg-amber-500 animate-pulse" />
            <span class="text-amber-800 dark:text-amber-200 font-medium">{t("topBarUnsaved")}</span>
            <button
              type="button"
              disabled={props.busy}
              onClick={() => props.onDiscard()}
              class="ml-1 rounded px-2 py-0.5 text-[11px] bg-white/70 dark:bg-black/30 hover:bg-white text-[#525f7f] dark:text-[#a3adc2] border border-amber-500/20 cursor-pointer"
            >
              {t("discardDraft")}
            </button>
            <button
              type="button"
              disabled={props.busy}
              onClick={() => props.onApply()}
              class="rounded px-2.5 py-0.5 text-[11px] font-medium bg-[#5e72e4] hover:bg-[#4c60d4] text-white border-0 cursor-pointer disabled:opacity-50"
            >
              {props.busy ? t("applyingChanges") : t("topBarApplyShort")}
            </button>
          </div>
        </Show>

        <span
          class={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[11px] font-medium border ${
            props.wsConnected
              ? "bg-emerald-500/10 text-emerald-700 border-emerald-500/20 dark:text-emerald-400"
              : "bg-zinc-500/10 text-zinc-600 border-zinc-500/20 dark:text-zinc-400"
          }`}
        >
          <span class="relative flex h-1.5 w-1.5">
            <Show when={props.wsConnected}>
              <span class="animate-pulse-ring absolute inline-flex h-full w-full rounded-full bg-emerald-500" />
            </Show>
            <span
              class={`relative inline-flex rounded-full h-1.5 w-1.5 ${
                props.wsConnected ? "bg-emerald-500" : "bg-zinc-400"
              }`}
            />
          </span>
          <span class="hidden sm:inline">
            {props.wsConnected ? t("topBarLive") : t("topBarOffline")}
          </span>
        </span>

        <button
          type="button"
          onClick={() => props.onRefresh()}
          title={t("headerRefreshTitle")}
          class="h-8 px-2.5 rounded-lg text-xs font-medium bg-[#f8f9fe] dark:bg-[#1a1f2e] hover:bg-[#eef1f6] dark:hover:bg-[#232a3b] text-[#172b4d] dark:text-[#eef1f6] border border-[#e9ecef] dark:border-[#232a3b] flex items-center gap-1.5 cursor-pointer transition-colors"
        >
          <span class="i-ri-refresh-line text-sm text-[#5e72e4] dark:text-[#8090f7]" />
          <span class="hidden sm:inline">{t("headerRefresh")}</span>
        </button>
      </div>
    </header>
  );
}

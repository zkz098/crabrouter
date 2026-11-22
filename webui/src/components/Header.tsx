import { For, Show } from "solid-js";
import { theme, toggleTheme } from "../theme";

export type PageTab =
  | "overview"
  | "network"
  | "firewall"
  | "qos_conntrack"
  | "clients"
  | "ota"
  | "diagnostics";

export const NAV_ITEMS: Array<{ key: PageTab; label: string; icon: string }> = [
  { key: "overview", label: "总览", icon: "i-ri-dashboard-3-line" },
  { key: "network", label: "网络", icon: "i-ri-node-tree" },
  { key: "firewall", label: "防火墙", icon: "i-ri-shield-flash-line" },
  { key: "qos_conntrack", label: "流控", icon: "i-ri-speed-up-line" },
  { key: "clients", label: "终端", icon: "i-ri-computer-line" },
  { key: "ota", label: "升级", icon: "i-ri-download-cloud-2-line" },
  { key: "diagnostics", label: "诊断", icon: "i-ri-terminal-box-line" },
];

interface HeaderProps {
  hostname: string;
  generation: number | string;
  wsConnected: boolean;
  tab: PageTab;
  onSelectTab: (tab: PageTab) => void;
  onRefresh: () => void;
  onLogout: () => void;
}

export function Header(props: HeaderProps) {
  return (
    <header class="sticky top-0 z-40 border-b border-[#eaeaea] dark:border-[#262626] bg-white/80 dark:bg-black/80 backdrop-blur-xl transition-colors">
      <div class="max-w-6xl mx-auto px-4 sm:px-6 h-14 flex items-center justify-between gap-3">
        {/* Brand + Live Status */}
        <div class="flex items-center gap-3 min-w-0">
          <div class="w-8 h-8 rounded-lg bg-[#171717] dark:bg-white text-white dark:text-[#0a0a0a] flex items-center justify-center shrink-0 shadow-sm transition-transform duration-200 hover:scale-105">
            <span class="i-ri-router-fill text-base" />
          </div>
          <div class="flex items-center gap-2 min-w-0">
            <h1 class="text-sm sm:text-base font-semibold tracking-tight text-[#171717] dark:text-[#ededed] truncate">
              {props.hostname}
            </h1>
            <span
              class={`inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-[11px] font-medium border transition-colors shrink-0 ${
                props.wsConnected
                  ? "bg-emerald-500/10 text-emerald-700 border-emerald-500/20 dark:bg-emerald-500/10 dark:text-emerald-400 dark:border-emerald-500/25"
                  : "bg-zinc-500/10 text-zinc-600 border-zinc-500/20 dark:bg-zinc-800 dark:text-zinc-400 dark:border-zinc-700"
              }`}
            >
              <span class="relative flex h-1.5 w-1.5">
                <Show when={props.wsConnected}>
                  <span class="animate-pulse-ring absolute inline-flex h-full w-full rounded-full bg-emerald-500" />
                </Show>
                <span
                  class={`relative inline-flex rounded-full h-1.5 w-1.5 ${
                    props.wsConnected ? "bg-emerald-500" : "bg-zinc-400 dark:bg-zinc-500"
                  }`}
                />
              </span>
              <span class="hidden sm:inline font-mono tabular-nums">Gen #{props.generation}</span>
            </span>
          </div>
        </div>

        {/* Desktop Navigation */}
        <nav class="hidden md:flex items-center gap-0.5 p-1 rounded-xl bg-[#f4f4f5] dark:bg-[#111111] border border-[#eaeaea] dark:border-[#222222]">
          <For each={NAV_ITEMS}>
            {(item) => {
              const active = () => props.tab === item.key;
              return (
                <button
                  type="button"
                  onClick={() => props.onSelectTab(item.key)}
                  class={`px-2.5 py-1.5 rounded-lg text-xs font-medium flex items-center gap-1.5 transition-all duration-200 cursor-pointer border-0 active:scale-[0.97] whitespace-nowrap ${
                    active()
                      ? "bg-white dark:bg-[#1f1f1f] text-[#171717] dark:text-white shadow-sm ring-1 ring-black/5 dark:ring-white/10"
                      : "bg-transparent text-[#666666] dark:text-[#8f8f8f] hover:text-[#171717] dark:hover:text-[#ededed]"
                  }`}
                >
                  <span class={`${item.icon} text-sm`} />
                  <span>{item.label}</span>
                </button>
              );
            }}
          </For>
        </nav>

        {/* Right Controls: Refresh + Theme + Logout */}
        <div class="flex items-center gap-2 shrink-0">
          <button
            type="button"
            onClick={() => props.onRefresh()}
            title="刷新全部状态"
            class="h-8 px-2.5 rounded-lg text-xs font-medium bg-white dark:bg-[#111111] hover:bg-[#f4f4f5] dark:hover:bg-[#1a1a1a] text-[#171717] dark:text-[#ededed] border border-[#eaeaea] dark:border-[#262626] flex items-center gap-1.5 cursor-pointer transition-all duration-200 active:scale-[0.96]"
          >
            <span class="i-ri-refresh-line text-sm text-[#666666] dark:text-[#a1a1aa]" />
            <span class="hidden lg:inline">刷新</span>
          </button>

          <button
            type="button"
            onClick={toggleTheme}
            aria-label={theme() === "dark" ? "浅色模式" : "深色模式"}
            title={theme() === "dark" ? "切换浅色模式" : "切换深色模式"}
            class="h-8 px-2.5 rounded-lg text-xs font-medium bg-white dark:bg-[#111111] hover:bg-[#f4f4f5] dark:hover:bg-[#1a1a1a] text-[#171717] dark:text-[#ededed] border border-[#eaeaea] dark:border-[#262626] flex items-center gap-1.5 cursor-pointer transition-all duration-200 active:scale-[0.96]"
          >
            <span
              class={`text-sm transition-transform duration-300 ${
                theme() === "dark" ? "i-ri-sun-line text-amber-400" : "i-ri-moon-line text-zinc-700"
              }`}
            />
            <span class="hidden xl:inline">{theme() === "dark" ? "浅色" : "深色"}</span>
          </button>

          <button
            type="button"
            onClick={() => props.onLogout()}
            title="退出登录"
            class="h-8 px-2.5 rounded-lg text-xs font-medium bg-white dark:bg-[#111111] hover:bg-[#f4f4f5] dark:hover:bg-[#1a1a1a] text-[#171717] dark:text-[#ededed] border border-[#eaeaea] dark:border-[#262626] flex items-center gap-1.5 cursor-pointer transition-all duration-200 active:scale-[0.96]"
          >
            <span class="i-ri-logout-box-r-line text-sm text-[#666666] dark:text-[#a1a1aa]" />
            <span class="hidden xl:inline">退出</span>
          </button>
        </div>
      </div>
    </header>
  );
}

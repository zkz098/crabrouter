import { For, Show, createEffect, createSignal } from "solid-js";
import { t, toggleLocale } from "../i18n";
import { NAV_GROUPS, type NavGroupKey, type PageTab, findNavMeta } from "../nav";
import { theme, toggleTheme } from "../theme";

interface SidebarNavProps {
  hostname: string;
  generation: number | string;
  wsConnected: boolean;
  tab: PageTab;
  mobileOpen: boolean;
  onCloseMobile: () => void;
  onSelectTab: (tab: PageTab) => void;
  onLogout: () => void;
}

export function SidebarNav(props: SidebarNavProps) {
  const [expanded, setExpanded] = createSignal<Record<NavGroupKey, boolean>>({
    status: true,
    system: false,
    network: true,
    security: false,
    tools: false,
  });

  createEffect(() => {
    const { group } = findNavMeta(props.tab);
    setExpanded((prev) => (prev[group.key] ? prev : { ...prev, [group.key]: true }));
  });

  function toggleGroup(key: NavGroupKey) {
    setExpanded((prev) => ({ ...prev, [key]: !prev[key] }));
  }

  function handleSelect(tab: PageTab) {
    props.onSelectTab(tab);
    props.onCloseMobile();
  }

  const sidebarContent = (
    <div class="flex flex-col h-full bg-white dark:bg-[#131722] border-r border-[#e9ecef] dark:border-[#232a3b] select-none">
      {/* Argon Brand Header */}
      <div class="h-15 px-4 flex items-center justify-between border-b border-[#e9ecef] dark:border-[#232a3b] shrink-0">
        <button
          type="button"
          onClick={() => handleSelect("overview")}
          class="flex items-center gap-2.5 min-w-0 bg-transparent border-0 p-0 cursor-pointer text-left"
        >
          <div class="w-8 h-8 rounded-xl bg-gradient-to-br from-[#5e72e4] to-[#825ee4] text-white flex items-center justify-center shrink-0 shadow-sm">
            <span class="i-ri-router-fill text-base" />
          </div>
          <div class="min-w-0">
            <div class="text-sm font-semibold tracking-tight text-[#172b4d] dark:text-[#eef1f6] truncate">
              {props.hostname}
            </div>
            <div class="flex items-center gap-1.5 text-[10px] text-[#8898aa] dark:text-[#6c7893]">
              <span
                class={`inline-block w-1.5 h-1.5 rounded-full ${
                  props.wsConnected ? "bg-emerald-500" : "bg-zinc-400"
                }`}
              />
              <span class="font-mono tabular-nums">Gen #{props.generation}</span>
            </div>
          </div>
        </button>

        <button
          type="button"
          onClick={() => props.onCloseMobile()}
          class="md:hidden p-1.5 rounded-lg text-[#525f7f] dark:text-[#a3adc2] hover:bg-[#f4f5f7] dark:hover:bg-[#1a1f2e] bg-transparent border-0 cursor-pointer"
          aria-label={t("commonClose")}
        >
          <span class="i-ri-close-line text-lg" />
        </button>
      </div>

      {/* Accordion Navigation Groups */}
      <nav class="flex-1 overflow-y-auto px-3 py-3 space-y-1.5">
        <For each={NAV_GROUPS}>
          {(group) => {
            const isOpen = () => expanded()[group.key];
            const hasActive = () => group.items.some((i) => i.key === props.tab);
            return (
              <div class="space-y-0.5">
                <button
                  type="button"
                  onClick={() => toggleGroup(group.key)}
                  class={`w-full flex items-center justify-between px-2.5 py-2 rounded-lg text-xs font-semibold tracking-wide cursor-pointer border-0 transition-colors ${
                    hasActive()
                      ? "text-[#5e72e4] dark:text-[#8090f7] bg-[#5e72e4]/6"
                      : "text-[#525f7f] dark:text-[#a3adc2] hover:bg-[#f8f9fe] dark:hover:bg-[#1a1f2e] bg-transparent"
                  }`}
                >
                  <span class="flex items-center gap-2">
                    <span class={`${group.icon} text-sm opacity-85`} />
                    <span>{group.label()}</span>
                  </span>
                  <span
                    class={`i-ri-arrow-down-s-line text-sm transition-transform duration-200 ${
                      isOpen() ? "rotate-0" : "-rotate-90"
                    }`}
                  />
                </button>

                <Show when={isOpen()}>
                  <div class="pl-2 space-y-0.5">
                    <For each={group.items}>
                      {(item) => {
                        const active = () => props.tab === item.key;
                        return (
                          <button
                            type="button"
                            onClick={() => handleSelect(item.key)}
                            class={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-xs font-medium cursor-pointer border-0 transition-all text-left ${
                              active()
                                ? "bg-[#5e72e4] text-white shadow-sm"
                                : "bg-transparent text-[#525f7f] dark:text-[#a3adc2] hover:bg-[#f4f5f7] dark:hover:bg-[#1a1f2e] hover:text-[#172b4d] dark:hover:text-[#eef1f6]"
                            }`}
                          >
                            <span
                              class={`${item.icon} text-sm shrink-0 ${
                                active() ? "text-white" : "opacity-75"
                              }`}
                            />
                            <span class="truncate">{item.label()}</span>
                          </button>
                        );
                      }}
                    </For>
                  </div>
                </Show>
              </div>
            );
          }}
        </For>
      </nav>

      {/* Sidebar Footer Quick Controls */}
      <div class="p-3 border-t border-[#e9ecef] dark:border-[#232a3b] space-y-2 shrink-0">
        <div class="grid grid-cols-2 gap-1.5">
          <button
            type="button"
            onClick={toggleTheme}
            class="h-8 px-2 rounded-lg text-[11px] font-medium bg-[#f8f9fe] dark:bg-[#1a1f2e] hover:bg-[#eef1f6] dark:hover:bg-[#232a3b] text-[#525f7f] dark:text-[#a3adc2] border border-[#e9ecef] dark:border-[#232a3b] flex items-center justify-center gap-1.5 cursor-pointer transition-colors"
          >
            <span
              class={
                theme() === "dark"
                  ? "i-ri-sun-line text-amber-400"
                  : "i-ri-moon-line text-[#5e72e4]"
              }
            />
            <span>{theme() === "dark" ? t("themeLight") : t("themeDark")}</span>
          </button>
          <button
            type="button"
            onClick={toggleLocale}
            class="h-8 px-2 rounded-lg text-[11px] font-medium bg-[#f8f9fe] dark:bg-[#1a1f2e] hover:bg-[#eef1f6] dark:hover:bg-[#232a3b] text-[#525f7f] dark:text-[#a3adc2] border border-[#e9ecef] dark:border-[#232a3b] flex items-center justify-center gap-1.5 cursor-pointer transition-colors"
          >
            <span class="i-ri-translate-2 text-sm" />
            <span>{t("langToggle")}</span>
          </button>
        </div>
        <button
          type="button"
          onClick={() => props.onLogout()}
          class="w-full h-8 px-3 rounded-lg text-xs font-medium bg-transparent hover:bg-rose-500/10 text-[#525f7f] dark:text-[#a3adc2] hover:text-rose-600 dark:hover:text-rose-400 border border-[#e9ecef] dark:border-[#232a3b] flex items-center justify-center gap-1.5 cursor-pointer transition-colors"
        >
          <span class="i-ri-logout-box-r-line text-sm" />
          <span>{t("logout")}</span>
        </button>
      </div>
    </div>
  );

  return (
    <>
      {/* Desktop Fixed Sidebar */}
      <aside class="hidden md:block w-60 shrink-0 sticky top-0 h-screen z-30">
        {sidebarContent}
      </aside>

      {/* Mobile Slide-Over Drawer */}
      <Show when={props.mobileOpen}>
        <div class="md:hidden fixed inset-0 z-50 flex">
          <div
            class="fixed inset-0 bg-black/45 backdrop-blur-xs"
            onClick={() => props.onCloseMobile()}
          />
          <aside class="relative w-64 max-w-[80vw] h-full z-10 shadow-2xl">{sidebarContent}</aside>
        </div>
      </Show>
    </>
  );
}

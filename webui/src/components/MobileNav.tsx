import { For } from "solid-js";
import { NAV_ITEMS, type PageTab } from "./Header";

interface MobileNavProps {
  tab: PageTab;
  onSelectTab: (tab: PageTab) => void;
}

export function MobileNav(props: MobileNavProps) {
  return (
    <nav
      aria-label="Mobile Navigation"
      class="md:hidden fixed bottom-0 inset-x-0 z-40 border-t border-[#eaeaea] dark:border-[#262626] bg-white/90 dark:bg-[#0a0a0a]/90 backdrop-blur-xl pb-[env(safe-area-inset-bottom)]"
    >
      <div class="grid grid-cols-7 h-15">
        <For each={NAV_ITEMS}>
          {(item) => {
            const active = () => props.tab === item.key;
            return (
              <button
                type="button"
                onClick={() => props.onSelectTab(item.key)}
                class={`relative flex flex-col items-center justify-center gap-1 bg-transparent border-0 cursor-pointer transition-colors duration-200 active:scale-95 ${
                  active()
                    ? "text-[#171717] dark:text-white font-medium"
                    : "text-[#8f8f8f] dark:text-[#71717a]"
                }`}
              >
                <span
                  class={`absolute top-0 left-1/2 -translate-x-1/2 h-0.5 rounded-full transition-all duration-250 ${
                    active()
                      ? "w-6 bg-[#171717] dark:bg-white opacity-100"
                      : "w-0 bg-transparent opacity-0"
                  }`}
                />
                <span class={`${item.icon} text-lg`} />
                <span class="text-[10px] leading-none truncate max-w-full px-0.5">
                  {item.label}
                </span>
              </button>
            );
          }}
        </For>
      </div>
    </nav>
  );
}

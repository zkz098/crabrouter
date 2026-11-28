import { For, type JSX, Show, createSignal } from "solid-js";

interface PageHeaderProps {
  title: string;
  subtitle?: string;
  actions?: JSX.Element;
}

export function PageHeader(props: PageHeaderProps) {
  return (
    <div class="flex flex-wrap items-start justify-between gap-3 pb-1">
      <div class="min-w-0">
        <h2 class="text-lg sm:text-xl font-semibold tracking-tight text-[#172b4d] dark:text-[#eef1f6]">
          {props.title}
        </h2>
        <Show when={props.subtitle}>
          <p class="mt-0.5 text-xs sm:text-[13px] text-[#525f7f] dark:text-[#a3adc2] leading-relaxed">
            {props.subtitle}
          </p>
        </Show>
      </div>
      <Show when={props.actions}>
        <div class="flex items-center gap-2 shrink-0">{props.actions}</div>
      </Show>
    </div>
  );
}

interface GuideBannerProps {
  title: string;
  description: string;
  variant?: "info" | "tip" | "warn";
  actionLabel?: string;
  onAction?: () => void;
  dismissible?: boolean;
}

export function GuideBanner(props: GuideBannerProps) {
  const [dismissed, setDismissed] = createSignal(false);

  const styleMap = () => {
    switch (props.variant) {
      case "warn":
        return "border-amber-500/25 bg-amber-500/8 text-amber-800 dark:text-amber-200";
      case "tip":
        return "border-emerald-500/25 bg-emerald-500/8 text-emerald-800 dark:text-emerald-200";
      default:
        return "border-[#5e72e4]/25 bg-[#5e72e4]/8 text-[#3245b8] dark:text-[#b4c0ff]";
    }
  };

  const iconClass = () => {
    switch (props.variant) {
      case "warn":
        return "i-ri-alert-line text-amber-500";
      case "tip":
        return "i-ri-lightbulb-flash-line text-emerald-500";
      default:
        return "i-ri-information-line text-[#5e72e4] dark:text-[#8090f7]";
    }
  };

  return (
    <Show when={!dismissed()}>
      <div
        class={`rounded-xl border px-4 py-3 flex flex-wrap items-center justify-between gap-3 text-xs ${styleMap()}`}
      >
        <div class="flex items-start gap-2.5 min-w-0 flex-1">
          <span class={`${iconClass()} text-base shrink-0 mt-0.5`} />
          <div class="min-w-0">
            <span class="font-semibold">{props.title}</span>
            <span class="mx-1.5 opacity-50">·</span>
            <span class="opacity-90 leading-relaxed">{props.description}</span>
          </div>
        </div>
        <div class="flex items-center gap-2 shrink-0">
          <Show when={props.actionLabel && props.onAction}>
            <button
              type="button"
              onClick={() => props.onAction?.()}
              class="rounded-lg bg-white/80 dark:bg-white/10 hover:bg-white dark:hover:bg-white/15 border border-current/15 px-2.5 py-1 text-[11px] font-medium cursor-pointer transition-colors"
            >
              {props.actionLabel}
            </button>
          </Show>
          <Show when={props.dismissible}>
            <button
              type="button"
              onClick={() => setDismissed(true)}
              aria-label="Dismiss"
              class="p-1 rounded-md opacity-60 hover:opacity-100 bg-transparent border-0 cursor-pointer text-current"
            >
              <span class="i-ri-close-line text-sm" />
            </button>
          </Show>
        </div>
      </div>
    </Show>
  );
}

export interface SubTabItem<T extends string> {
  key: T;
  label: string;
  icon?: string;
  badge?: string | number;
}

interface SubTabsProps<T extends string> {
  items: SubTabItem<T>[];
  active: T;
  onChange: (key: T) => void;
}

export function SubTabs<T extends string>(props: SubTabsProps<T>) {
  return (
    <div class="flex flex-wrap items-center gap-1.5 p-1.5 rounded-xl bg-white dark:bg-[#131722] border border-[#e9ecef] dark:border-[#232a3b] argon-card">
      <For each={props.items}>
        {(item) => {
          const isActive = () => props.active === item.key;
          return (
            <button
              type="button"
              onClick={() => props.onChange(item.key)}
              class={`px-3 py-1.5 rounded-lg text-xs font-medium flex items-center gap-1.5 transition-all cursor-pointer border-0 whitespace-nowrap ${
                isActive()
                  ? "bg-[#5e72e4] text-white shadow-sm"
                  : "bg-transparent text-[#525f7f] dark:text-[#a3adc2] hover:bg-[#f8f9fe] dark:hover:bg-[#1a1f2e] hover:text-[#172b4d] dark:hover:text-[#eef1f6]"
              }`}
            >
              <Show when={item.icon}>
                <span class={`${item.icon} text-sm`} />
              </Show>
              <span>{item.label}</span>
              <Show when={item.badge !== undefined}>
                <span
                  class={`ml-0.5 rounded-full px-1.5 py-0.2 text-[10px] font-mono ${
                    isActive()
                      ? "bg-white/20 text-white"
                      : "bg-[#f4f5f7] dark:bg-[#232a3b] text-[#525f7f] dark:text-[#a3adc2]"
                  }`}
                >
                  {item.badge}
                </span>
              </Show>
            </button>
          );
        }}
      </For>
    </div>
  );
}

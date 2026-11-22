import { Show, createSignal } from "solid-js";
import { login } from "../api";
import { t, toggleLocale } from "../i18n";

interface Props {
  onSuccess: () => void;
}

export function LoginView(props: Props) {
  const [username, setUsername] = createSignal("admin");
  const [password, setPassword] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [err, setErr] = createSignal<string | null>(null);

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    setErr(null);
    setBusy(true);
    try {
      await login({ username: username(), password: password() });
      props.onSuccess();
    } catch (error) {
      setErr(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div class="min-h-screen vercel-grid-bg flex items-center justify-center px-4 bg-[#fafafa] dark:bg-black text-[#171717] dark:text-[#ededed]">
      <div class="w-full max-w-sm rounded-2xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-6 sm:p-7 shadow-sm space-y-6 animate-card-enter">
        <div class="flex items-center justify-between gap-3">
          <div class="flex items-center gap-3 min-w-0">
            <div class="w-9 h-9 rounded-xl bg-[#171717] dark:bg-white text-white dark:text-[#0a0a0a] flex items-center justify-center shrink-0">
              <span class="i-ri-router-fill text-lg" />
            </div>
            <div class="min-w-0">
              <h1 class="text-base font-semibold tracking-tight">CrabRouter</h1>
              <p class="text-xs text-[#666666] dark:text-[#a1a1aa] truncate">
                {t("loginSubtitle")}
              </p>
            </div>
          </div>
          <button
            type="button"
            onClick={toggleLocale}
            class="h-8 px-2.5 rounded-lg text-xs font-medium bg-white dark:bg-[#111111] hover:bg-[#f4f4f5] dark:hover:bg-[#1a1a1a] text-[#171717] dark:text-[#ededed] border border-[#eaeaea] dark:border-[#262626] flex items-center gap-1 cursor-pointer shrink-0 transition-colors"
          >
            <span class="i-ri-translate-2 text-sm text-[#666666] dark:text-[#a1a1aa]" />
            <span>{t("langToggle")}</span>
          </button>
        </div>

        <form onSubmit={(e) => void submit(e)} class="space-y-4">
          <div>
            <label class="block text-xs font-medium mb-1.5 text-[#171717] dark:text-[#ededed]">
              {t("loginUsername")}
            </label>
            <input
              type="text"
              autocomplete="username"
              value={username()}
              onInput={(e) => setUsername(e.currentTarget.value)}
              class="w-full h-9 bg-[#fafafa] dark:bg-[#000000] border border-[#eaeaea] dark:border-[#262626] rounded-lg px-3 text-sm text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] transition-colors"
              required
            />
          </div>
          <div>
            <label class="block text-xs font-medium mb-1.5 text-[#171717] dark:text-[#ededed]">
              {t("loginPassword")}
            </label>
            <input
              type="password"
              autocomplete="current-password"
              value={password()}
              onInput={(e) => setPassword(e.currentTarget.value)}
              class="w-full h-9 bg-[#fafafa] dark:bg-[#000000] border border-[#eaeaea] dark:border-[#262626] rounded-lg px-3 text-sm text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] transition-colors"
              required
            />
          </div>

          <Show when={err()}>
            {(msg) => (
              <div class="rounded-lg border border-rose-500/20 bg-rose-500/10 text-rose-600 dark:text-rose-400 px-3 py-2 text-xs flex items-center gap-1.5">
                <span class="i-ri-error-warning-line shrink-0" />
                <span>{msg()}</span>
              </div>
            )}
          </Show>

          <button
            type="submit"
            disabled={busy()}
            class="w-full h-9 rounded-lg text-sm font-medium bg-[#171717] hover:bg-[#383838] dark:bg-[#ededed] dark:hover:bg-[#d4d4d8] text-white dark:text-[#0a0a0a] cursor-pointer border-0 transition-all active:scale-[0.98] disabled:opacity-50"
          >
            {busy() ? t("loginSubmitting") : t("loginButton")}
          </button>
        </form>
      </div>
    </div>
  );
}

import { Show, createSignal } from "solid-js";
import { login } from "../api";

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
    <div class="flex min-h-screen items-center justify-center bg-slate-950 p-6 text-slate-100">
      <form
        onSubmit={(e) => void submit(e)}
        class="w-full max-w-sm rounded-2xl border border-slate-800 bg-slate-900/90 p-6 shadow-2xl"
      >
        <div class="flex items-center gap-3">
          <span class="i-ri-router-line text-2xl text-orange-400" />
          <div>
            <h1 class="text-base font-semibold">CrabRouter 管理登录</h1>
            <p class="text-xs text-slate-400">请输入管理员账号与密码</p>
          </div>
        </div>

        <Show when={err()}>
          {(msg) => (
            <div class="mt-4 rounded-lg border border-red-500/40 bg-red-950/50 px-3 py-2 text-xs text-red-200">
              {msg()}
            </div>
          )}
        </Show>

        <div class="mt-4 space-y-3">
          <label class="block text-xs">
            <span class="text-slate-400">用户名</span>
            <input
              type="text"
              value={username()}
              onInput={(e) => setUsername(e.currentTarget.value)}
              class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-sm"
            />
          </label>
          <label class="block text-xs">
            <span class="text-slate-400">密码</span>
            <input
              type="password"
              value={password()}
              onInput={(e) => setPassword(e.currentTarget.value)}
              class="mt-1 w-full rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-sm"
            />
          </label>
        </div>

        <button
          type="submit"
          disabled={busy()}
          class="mt-5 w-full rounded-lg bg-orange-600 py-2 text-xs font-semibold text-white hover:bg-orange-500 disabled:opacity-50"
        >
          {busy() ? "验证中..." : "登录控制台"}
        </button>
      </form>
    </div>
  );
}

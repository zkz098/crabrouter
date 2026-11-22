import { For, Show, createSignal, onCleanup, onMount } from "solid-js";
import { getLogSnapshot, openLogWebSocket, runIperf3, runPing, runTraceroute } from "../api";
import type { DiagCommandResult } from "../types/DiagCommandResult";
import type { LogEvent } from "../types/LogEvent";

export function DiagnosticsPage() {
  const [pingHost, setPingHost] = createSignal("1.1.1.1");
  const [pingCount, setPingCount] = createSignal(4);
  const [traceHost, setTraceHost] = createSignal("1.1.1.1");
  const [iperfServer, setIperfServer] = createSignal("127.0.0.1");
  const [iperfPort, setIperfPort] = createSignal(5201);
  const [iperfDur, setIperfDur] = createSignal(5);
  const [iperfRev, setIperfRev] = createSignal(false);

  const [diagResult, setDiagResult] = createSignal<DiagCommandResult | null>(null);
  const [running, setRunning] = createSignal(false);

  const [logs, setLogs] = createSignal<LogEvent[]>([]);
  const [paused, setPaused] = createSignal(false);
  const [levelFilter, setLevelFilter] = createSignal("ALL");
  const [wsConnected, setWsConnected] = createSignal(false);

  onMount(() => {
    void getLogSnapshot()
      .then((initial) => setLogs(initial))
      .catch(() => {});

    const ws = openLogWebSocket();
    ws.onopen = () => setWsConnected(true);
    ws.onclose = () => setWsConnected(false);
    ws.onmessage = (evt) => {
      if (paused()) {
        return;
      }
      try {
        const entry = JSON.parse(String(evt.data)) as LogEvent;
        setLogs((prev) => [...prev.slice(-249), entry]);
      } catch {
        // ignore malformed frames
      }
    };
    onCleanup(() => ws.close());
  });

  async function execDiag(fn: () => Promise<DiagCommandResult>) {
    setRunning(true);
    try {
      const res = await fn();
      setDiagResult(res);
    } catch (err) {
      setDiagResult({
        ok: false,
        command: "diagnostic",
        exit_code: null,
        output: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setRunning(false);
    }
  }

  const filteredLogs = () => {
    const f = levelFilter();
    if (f === "ALL") {
      return logs();
    }
    return logs().filter((l) => l.level.toUpperCase() === f);
  };

  return (
    <div class="space-y-6">
      <section class="grid gap-4 md:grid-cols-3">
        {/* Ping */}
        <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
          <h3 class="text-sm font-semibold text-slate-200">Ping 连通性测试</h3>
          <div class="mt-3 space-y-2 text-xs">
            <input
              type="text"
              value={pingHost()}
              onInput={(e) => setPingHost(e.currentTarget.value)}
              placeholder="IP 或域名"
              class="w-full rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono"
            />
            <div class="flex items-center gap-2">
              <input
                type="number"
                min={1}
                max={10}
                value={pingCount()}
                onInput={(e) => setPingCount(Number(e.currentTarget.value) || 4)}
                class="w-20 rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono"
              />
              <button
                type="button"
                disabled={running()}
                onClick={() =>
                  void execDiag(() => runPing({ host: pingHost(), count: pingCount() }))
                }
                class="flex-1 rounded-lg bg-orange-600 py-1.5 font-medium text-white hover:bg-orange-500 disabled:opacity-50"
              >
                执行 Ping
              </button>
            </div>
          </div>
        </div>

        {/* Traceroute */}
        <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
          <h3 class="text-sm font-semibold text-slate-200">Traceroute 路由追踪</h3>
          <div class="mt-3 space-y-2 text-xs">
            <input
              type="text"
              value={traceHost()}
              onInput={(e) => setTraceHost(e.currentTarget.value)}
              placeholder="IP 或域名"
              class="w-full rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono"
            />
            <button
              type="button"
              disabled={running()}
              onClick={() => void execDiag(() => runTraceroute({ host: traceHost() }))}
              class="w-full rounded-lg bg-sky-600 py-1.5 font-medium text-white hover:bg-sky-500 disabled:opacity-50"
            >
              追踪路径
            </button>
          </div>
        </div>

        {/* iperf3 */}
        <div class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
          <h3 class="text-sm font-semibold text-slate-200">iperf3 吞吐量测速</h3>
          <div class="mt-3 space-y-2 text-xs">
            <div class="flex gap-2">
              <input
                type="text"
                value={iperfServer()}
                onInput={(e) => setIperfServer(e.currentTarget.value)}
                placeholder="服务端 IP"
                class="flex-1 rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono"
              />
              <input
                type="number"
                value={iperfPort()}
                onInput={(e) => setIperfPort(Number(e.currentTarget.value) || 5201)}
                class="w-20 rounded border border-slate-700 bg-slate-950 px-2 py-1.5 font-mono"
              />
            </div>
            <div class="flex items-center justify-between gap-2">
              <label class="flex items-center gap-1.5 text-slate-300">
                <input
                  type="checkbox"
                  checked={iperfRev()}
                  onChange={(e) => setIperfRev(e.currentTarget.checked)}
                />
                反向 (-R)
              </label>
              <input
                type="number"
                min={1}
                max={30}
                value={iperfDur()}
                onInput={(e) => setIperfDur(Number(e.currentTarget.value) || 5)}
                class="w-16 rounded border border-slate-700 bg-slate-950 px-2 py-1 font-mono"
              />
              <button
                type="button"
                disabled={running()}
                onClick={() =>
                  void execDiag(() =>
                    runIperf3({
                      server: iperfServer(),
                      port: iperfPort(),
                      duration_secs: iperfDur(),
                      reverse: iperfRev(),
                    }),
                  )
                }
                class="rounded-lg bg-emerald-600 px-3 py-1.5 font-medium text-white hover:bg-emerald-500 disabled:opacity-50"
              >
                开始测速
              </button>
            </div>
          </div>
        </div>
      </section>

      <Show when={diagResult()}>
        {(res) => (
          <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
            <div class="mb-2 flex items-center justify-between text-xs">
              <span class="font-mono text-slate-300">$ {res().command}</span>
              <span
                class={
                  res().ok
                    ? "rounded bg-emerald-500/20 px-2 py-0.5 text-emerald-300"
                    : "rounded bg-red-500/20 px-2 py-0.5 text-red-300"
                }
              >
                exit {res().exit_code ?? "err"}
              </span>
            </div>
            <pre class="max-h-72 overflow-auto rounded-lg border border-slate-800 bg-slate-950 p-3 font-mono text-xs text-slate-300">
              {res().output}
            </pre>
          </section>
        )}
      </Show>

      {/* Live WebSocket Log Stream */}
      <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
        <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
          <div class="flex items-center gap-2">
            <h2 class="text-sm font-semibold text-slate-200">实时控制面日志流</h2>
            <span
              class={
                wsConnected()
                  ? "rounded bg-emerald-500/20 px-2 py-0.5 text-[11px] text-emerald-300"
                  : "rounded bg-slate-700 px-2 py-0.5 text-[11px] text-slate-300"
              }
            >
              {wsConnected() ? "WS 已连接" : "WS 未连接"}
            </span>
          </div>
          <div class="flex items-center gap-2 text-xs">
            <select
              value={levelFilter()}
              onChange={(e) => setLevelFilter(e.currentTarget.value)}
              class="rounded border border-slate-700 bg-slate-950 px-2 py-1"
            >
              <option value="ALL">全部级别</option>
              <option value="INFO">INFO</option>
              <option value="WARN">WARN</option>
              <option value="ERROR">ERROR</option>
            </select>
            <button
              type="button"
              onClick={() => setPaused((p) => !p)}
              class="rounded border border-slate-700 bg-slate-800 px-2.5 py-1 hover:bg-slate-700"
            >
              {paused() ? "恢复滚动" : "暂停"}
            </button>
            <button
              type="button"
              onClick={() => setLogs([])}
              class="rounded border border-slate-700 bg-slate-800 px-2.5 py-1 hover:bg-slate-700"
            >
              清屏
            </button>
          </div>
        </div>

        <div class="max-h-96 space-y-1 overflow-auto rounded-lg border border-slate-800 bg-slate-950 p-3 font-mono text-xs">
          <For each={filteredLogs()}>
            {(log) => (
              <div class="flex gap-2 leading-relaxed">
                <span class="shrink-0 text-slate-500">{log.timestamp.slice(11, 19)}</span>
                <span
                  class={
                    log.level === "ERROR"
                      ? "shrink-0 text-red-400"
                      : log.level === "WARN"
                        ? "shrink-0 text-amber-400"
                        : "shrink-0 text-sky-400"
                  }
                >
                  [{log.level}]
                </span>
                <span class="shrink-0 text-slate-400">{log.target}:</span>
                <span class="break-all text-slate-200">{log.message}</span>
              </div>
            )}
          </For>
        </div>
      </section>
    </div>
  );
}

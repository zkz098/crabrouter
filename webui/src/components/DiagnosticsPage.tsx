import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { For, Show, createSignal, onCleanup, onMount } from "solid-js";
import {
  getLogSnapshot,
  openLogWebSocket,
  openTerminalWebSocket,
  runIperf3,
  runPing,
  runTraceroute,
} from "../api";
import type { DiagCommandResult } from "../types/DiagCommandResult";
import type { HardwareHealthSnapshot } from "../types/HardwareHealthSnapshot";
import type { HealthSeverity } from "../types/HealthSeverity";
import type { LogEvent } from "../types/LogEvent";

interface Props {
  hardware: HardwareHealthSnapshot | undefined;
  onRefreshHardware: () => void;
}

function severityBadgeClass(s: HealthSeverity): string {
  switch (s) {
    case "critical":
      return "rounded bg-red-500/20 px-2 py-0.5 text-[11px] font-semibold text-red-300";
    case "warning":
      return "rounded bg-amber-500/20 px-2 py-0.5 text-[11px] font-semibold text-amber-300";
    default:
      return "rounded bg-emerald-500/20 px-2 py-0.5 text-[11px] font-semibold text-emerald-300";
  }
}

function severityLabel(s: HealthSeverity): string {
  switch (s) {
    case "critical":
      return "CRITICAL 严重告警";
    case "warning":
      return "WARNING 预警";
    default:
      return "HEALTHY 正常";
  }
}

export function DiagnosticsPage(props: Props) {
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

  const [ptyConnected, setPtyConnected] = createSignal(false);
  let termContainerRef: HTMLDivElement | undefined;
  let termInstance: Terminal | undefined;
  let fitAddonInstance: FitAddon | undefined;
  let ptySocket: WebSocket | undefined;

  function connectPtySocket(term: Terminal, fitAddon: FitAddon) {
    ptySocket?.close();
    const ws = openTerminalWebSocket();
    ptySocket = ws;

    ws.onopen = () => {
      setPtyConnected(true);
      fitAddon.fit();
      ws.send(
        JSON.stringify({
          type: "resize",
          cols: term.cols,
          rows: term.rows,
        }),
      );
    };
    ws.onmessage = (evt) => {
      term.write(String(evt.data));
    };
    ws.onclose = () => {
      setPtyConnected(false);
    };
    ws.onerror = () => {
      setPtyConnected(false);
    };
  }

  function reconnectTerminal() {
    if (termInstance && fitAddonInstance) {
      termInstance.clear();
      connectPtySocket(termInstance, fitAddonInstance);
    }
  }

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

    if (termContainerRef) {
      const term = new Terminal({
        cursorBlink: true,
        fontFamily: "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace",
        fontSize: 12,
        rows: 22,
        theme: {
          background: "#020617",
          foreground: "#e2e8f0",
          cursor: "#fb923c",
        },
      });
      const fitAddon = new FitAddon();
      term.loadAddon(fitAddon);
      term.open(termContainerRef);
      fitAddon.fit();

      termInstance = term;
      fitAddonInstance = fitAddon;

      const dataDisp = term.onData((data) => {
        if (ptySocket && ptySocket.readyState === WebSocket.OPEN) {
          ptySocket.send(JSON.stringify({ type: "input", data }));
        }
      });

      const handleResize = () => {
        fitAddon.fit();
        if (ptySocket && ptySocket.readyState === WebSocket.OPEN) {
          ptySocket.send(
            JSON.stringify({
              type: "resize",
              cols: term.cols,
              rows: term.rows,
            }),
          );
        }
      };
      window.addEventListener("resize", handleResize);
      connectPtySocket(term, fitAddon);

      onCleanup(() => {
        window.removeEventListener("resize", handleResize);
        dataDisp.dispose();
        ptySocket?.close();
        term.dispose();
      });
    }

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
      {/* Section 1: Hardware Health Monitor (lm-sensors + smartctl / nvme-cli) */}
      <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
        <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
          <div class="flex items-center gap-2.5">
            <h2 class="text-sm font-semibold text-slate-200">
              硬件健康监控与异常告警 (lm-sensors · smartctl · nvme)
            </h2>
            <Show when={props.hardware}>
              {(hw) => (
                <span class={severityBadgeClass(hw().overall_severity)}>
                  {severityLabel(hw().overall_severity)}
                </span>
              )}
            </Show>
          </div>
          <button
            type="button"
            onClick={() => props.onRefreshHardware()}
            class="rounded-lg border border-slate-700 bg-slate-800 px-3 py-1 text-xs hover:bg-slate-700"
          >
            刷新传感器与 SMART
          </button>
        </div>

        <Show when={(props.hardware?.alerts.length ?? 0) > 0}>
          <div class="mb-4 space-y-2">
            <For each={props.hardware?.alerts}>
              {(alert) => (
                <div
                  class={
                    alert.severity === "critical"
                      ? "flex items-center justify-between rounded-lg border border-red-500/40 bg-red-950/40 px-3 py-2 text-xs text-red-200"
                      : "flex items-center justify-between rounded-lg border border-amber-500/40 bg-amber-950/40 px-3 py-2 text-xs text-amber-200"
                  }
                >
                  <div>
                    <span class="font-mono font-semibold">[{alert.source}]</span>{" "}
                    <span>{alert.message}</span>
                  </div>
                  <span class={severityBadgeClass(alert.severity)}>
                    {alert.severity.toUpperCase()}
                  </span>
                </div>
              )}
            </For>
          </div>
        </Show>

        <div class="grid gap-4 md:grid-cols-2">
          {/* Thermal Sensors */}
          <div class="rounded-xl border border-slate-800 bg-slate-950/60 p-3">
            <div class="mb-2.5 text-xs font-medium text-slate-400">
              温度传感器 (lm-sensors / hwmon)
            </div>
            <Show
              when={(props.hardware?.sensors.length ?? 0) > 0}
              fallback={<p class="text-xs text-slate-500">未检测到板载温度传感器</p>}
            >
              <div class="grid gap-2 sm:grid-cols-2">
                <For each={props.hardware?.sensors}>
                  {(s) => (
                    <div class="rounded-lg border border-slate-800/90 bg-slate-900/70 p-2.5 text-xs">
                      <div class="flex items-center justify-between">
                        <span class="font-medium text-slate-200">{s.label}</span>
                        <span class={severityBadgeClass(s.severity)}>
                          {s.temp_celsius.toFixed(1)}°C
                        </span>
                      </div>
                      <div class="mt-1 flex items-center justify-between font-mono text-[11px] text-slate-500">
                        <span>{s.chip}</span>
                        <span>
                          {s.high_celsius ? `高:${s.high_celsius.toFixed(0)}°C ` : ""}
                          {s.crit_celsius ? `危:${s.crit_celsius.toFixed(0)}°C` : ""}
                        </span>
                      </div>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </div>

          {/* NVMe / SMART SSD Health */}
          <div class="rounded-xl border border-slate-800 bg-slate-950/60 p-3">
            <div class="mb-2.5 text-xs font-medium text-slate-400">
              SSD / NVMe 磁盘 SMART 健康度 (smartctl / nvme-cli)
            </div>
            <Show
              when={(props.hardware?.disks.length ?? 0) > 0}
              fallback={<p class="text-xs text-slate-500">未检测到支持 SMART 的物理磁盘</p>}
            >
              <div class="space-y-2.5">
                <For each={props.hardware?.disks}>
                  {(disk) => (
                    <div class="rounded-lg border border-slate-800/90 bg-slate-900/70 p-3 text-xs">
                      <div class="flex flex-wrap items-center justify-between gap-2">
                        <div>
                          <span class="font-mono font-semibold text-orange-300">{disk.device}</span>
                          <span class="ml-2 text-slate-200">{disk.model}</span>
                          <span class="ml-1.5 rounded bg-slate-800 px-1.5 py-0.5 font-mono text-[10px] text-slate-400">
                            {disk.protocol}
                          </span>
                        </div>
                        <span class={severityBadgeClass(disk.severity)}>
                          {disk.passed ? "SMART PASSED" : "SMART FAILED"}
                        </span>
                      </div>

                      <div class="mt-2 grid grid-cols-2 gap-1.5 font-mono text-[11px] text-slate-400 sm:grid-cols-3">
                        <div>
                          温度:{" "}
                          <span class="text-slate-200">
                            {disk.temperature_celsius !== null
                              ? `${disk.temperature_celsius.toFixed(1)}°C`
                              : "-"}
                          </span>
                        </div>
                        <div>
                          寿命磨损:{" "}
                          <span class="text-sky-300">
                            {disk.percentage_used !== null ? `${disk.percentage_used}%` : "-"}
                          </span>
                        </div>
                        <div>
                          备用块余量:{" "}
                          <span class="text-emerald-300">
                            {disk.available_spare_pct !== null
                              ? `${disk.available_spare_pct}%`
                              : "-"}
                          </span>
                        </div>
                        <div>
                          通电时长:{" "}
                          <span class="text-slate-200">
                            {disk.power_on_hours !== null ? `${disk.power_on_hours}h` : "-"}
                          </span>
                        </div>
                        <div>
                          异常掉电:{" "}
                          <span class="text-slate-200">{disk.unsafe_shutdowns ?? "-"}</span>
                        </div>
                        <div>
                          介质错误:{" "}
                          <span
                            class={(disk.media_errors ?? 0) > 0 ? "text-red-400" : "text-slate-200"}
                          >
                            {disk.media_errors ?? 0}
                          </span>
                        </div>
                      </div>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </div>
        </div>
      </section>

      {/* Section 2: Interactive Web Terminal (xterm.js + PTY WebSocket) */}
      <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
        <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
          <div class="flex items-center gap-2">
            <h2 class="text-sm font-semibold text-slate-200">
              Web 交互式终端 (xterm.js + PTY WebSocket)
            </h2>
            <span
              class={
                ptyConnected()
                  ? "rounded bg-emerald-500/20 px-2 py-0.5 text-[11px] text-emerald-300"
                  : "rounded bg-slate-700 px-2 py-0.5 text-[11px] text-slate-300"
              }
            >
              {ptyConnected() ? "PTY 已连接" : "PTY 已断开"}
            </span>
          </div>
          <button
            type="button"
            onClick={reconnectTerminal}
            class="rounded-lg border border-slate-700 bg-slate-800 px-3 py-1 text-xs hover:bg-slate-700"
          >
            重连 PTY 会话
          </button>
        </div>
        <div
          ref={(el) => {
            termContainerRef = el;
          }}
          class="overflow-hidden rounded-lg border border-slate-800 bg-slate-950 p-2"
        />
      </section>

      {/* Section 3: Network Diagnostic Tools (Ping / Traceroute / iperf3) */}
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
                class="w-20 rounded border border-slate-700 bg-slate-950 px-2.5 py-1.5 font-mono"
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

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

const SECTION_CLS =
  "rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-colors hover:border-[#d4d4d8] dark:hover:border-[#3a3a3a]";
const INPUT_CLS =
  "w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#000000] px-2.5 py-1.5 font-mono text-xs text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] transition-colors";
const BTN_OUTLINE =
  "rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#111111] hover:bg-[#f4f4f5] dark:hover:bg-[#1a1a1a] px-3 py-1.5 text-xs font-medium text-[#171717] dark:text-[#ededed] cursor-pointer transition-all active:scale-[0.97]";
const BTN_PRIMARY =
  "rounded-lg bg-[#171717] hover:bg-[#383838] dark:bg-[#ededed] dark:hover:bg-[#d4d4d8] px-3 py-1.5 text-xs font-medium text-white dark:text-[#0a0a0a] border-0 cursor-pointer transition-all active:scale-[0.97] disabled:opacity-50";

function severityBadgeClass(s: HealthSeverity): string {
  switch (s) {
    case "critical":
      return "rounded-full border border-rose-500/20 bg-rose-500/10 px-2 py-0.5 text-[11px] font-medium text-rose-700 dark:text-rose-400";
    case "warning":
      return "rounded-full border border-amber-500/20 bg-amber-500/10 px-2 py-0.5 text-[11px] font-medium text-amber-700 dark:text-amber-400";
    default:
      return "rounded-full border border-emerald-500/20 bg-emerald-500/10 px-2 py-0.5 text-[11px] font-medium text-emerald-700 dark:text-emerald-400";
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
        fontFamily:
          '"JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
        fontSize: 12,
        rows: 22,
        theme: {
          background: "#0a0a0a",
          foreground: "#ededed",
          cursor: "#0070f3",
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
    <div class="space-y-5 sm:space-y-6 animate-view-enter">
      {/* Section 1: Hardware Health Monitor */}
      <section class={SECTION_CLS}>
        <div class="mb-3.5 flex flex-wrap items-center justify-between gap-2">
          <div class="flex items-center gap-2.5">
            <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
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
          <button type="button" onClick={() => props.onRefreshHardware()} class={BTN_OUTLINE}>
            刷新传感器与 SMART
          </button>
        </div>

        <Show when={(props.hardware?.alerts.length ?? 0) > 0}>
          <div class="mb-4 space-y-2">
            <For each={props.hardware?.alerts}>
              {(alert) => (
                <div
                  class={`flex items-center justify-between rounded-xl border px-3.5 py-2 text-xs ${
                    alert.severity === "critical"
                      ? "border-rose-500/25 bg-rose-500/10 text-rose-700 dark:text-rose-300"
                      : "border-amber-500/25 bg-amber-500/10 text-amber-700 dark:text-amber-300"
                  }`}
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
          <div class="rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] p-3.5">
            <div class="mb-2.5 text-xs font-medium text-[#666666] dark:text-[#a1a1aa]">
              温度传感器 (lm-sensors / hwmon)
            </div>
            <Show
              when={(props.hardware?.sensors.length ?? 0) > 0}
              fallback={
                <p class="text-xs text-[#8f8f8f] dark:text-[#71717a]">未检测到板载温度传感器</p>
              }
            >
              <div class="grid gap-2 sm:grid-cols-2">
                <For each={props.hardware?.sensors}>
                  {(s) => (
                    <div class="rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-2.5 text-xs">
                      <div class="flex items-center justify-between">
                        <span class="font-medium text-[#171717] dark:text-[#ededed]">
                          {s.label}
                        </span>
                        <span class={severityBadgeClass(s.severity)}>
                          {s.temp_celsius.toFixed(1)}°C
                        </span>
                      </div>
                      <div class="mt-1 flex items-center justify-between font-mono text-[11px] text-[#666666] dark:text-[#a1a1aa]">
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
          <div class="rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] p-3.5">
            <div class="mb-2.5 text-xs font-medium text-[#666666] dark:text-[#a1a1aa]">
              SSD / NVMe 磁盘 SMART 健康度 (smartctl / nvme-cli)
            </div>
            <Show
              when={(props.hardware?.disks.length ?? 0) > 0}
              fallback={
                <p class="text-xs text-[#8f8f8f] dark:text-[#71717a]">
                  未检测到支持 SMART 的物理磁盘
                </p>
              }
            >
              <div class="space-y-2.5">
                <For each={props.hardware?.disks}>
                  {(disk) => (
                    <div class="rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-3 text-xs">
                      <div class="flex flex-wrap items-center justify-between gap-2">
                        <div>
                          <span class="font-mono font-semibold text-[#0070f3] dark:text-sky-400">
                            {disk.device}
                          </span>
                          <span class="ml-2 text-[#171717] dark:text-[#ededed]">{disk.model}</span>
                          <span class="ml-1.5 rounded border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] px-1.5 py-0.5 font-mono text-[10px] text-[#666666] dark:text-[#a1a1aa]">
                            {disk.protocol}
                          </span>
                        </div>
                        <span class={severityBadgeClass(disk.severity)}>
                          {disk.passed ? "SMART PASSED" : "SMART FAILED"}
                        </span>
                      </div>

                      <div class="mt-2 grid grid-cols-2 gap-1.5 font-mono text-[11px] text-[#666666] dark:text-[#a1a1aa] sm:grid-cols-3">
                        <div>
                          温度:{" "}
                          <span class="text-[#171717] dark:text-[#ededed]">
                            {disk.temperature_celsius !== null
                              ? `${disk.temperature_celsius.toFixed(1)}°C`
                              : "-"}
                          </span>
                        </div>
                        <div>
                          寿命磨损:{" "}
                          <span class="text-sky-600 dark:text-sky-400">
                            {disk.percentage_used !== null ? `${disk.percentage_used}%` : "-"}
                          </span>
                        </div>
                        <div>
                          备用块余量:{" "}
                          <span class="text-emerald-600 dark:text-emerald-400">
                            {disk.available_spare_pct !== null
                              ? `${disk.available_spare_pct}%`
                              : "-"}
                          </span>
                        </div>
                        <div>
                          通电时长:{" "}
                          <span class="text-[#171717] dark:text-[#ededed]">
                            {disk.power_on_hours !== null ? `${disk.power_on_hours}h` : "-"}
                          </span>
                        </div>
                        <div>
                          异常掉电:{" "}
                          <span class="text-[#171717] dark:text-[#ededed]">
                            {disk.unsafe_shutdowns ?? "-"}
                          </span>
                        </div>
                        <div>
                          介质错误:{" "}
                          <span
                            class={
                              (disk.media_errors ?? 0) > 0
                                ? "text-rose-600 dark:text-rose-400"
                                : "text-[#171717] dark:text-[#ededed]"
                            }
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

      {/* Section 2: Interactive Web Terminal */}
      <section class={SECTION_CLS}>
        <div class="mb-3.5 flex flex-wrap items-center justify-between gap-2">
          <div class="flex items-center gap-2">
            <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
              Web 交互式终端 (xterm.js + PTY WebSocket)
            </h2>
            <span
              class={`rounded-full border px-2 py-0.5 text-[11px] font-medium ${
                ptyConnected()
                  ? "border-emerald-500/20 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400"
                  : "border-zinc-500/20 bg-zinc-500/10 text-zinc-600 dark:text-zinc-400"
              }`}
            >
              {ptyConnected() ? "PTY 已连接" : "PTY 已断开"}
            </span>
          </div>
          <button type="button" onClick={reconnectTerminal} class={BTN_OUTLINE}>
            重连 PTY 会话
          </button>
        </div>
        <div
          ref={(el) => {
            termContainerRef = el;
          }}
          class="overflow-hidden rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-[#0a0a0a] p-2.5"
        />
      </section>

      {/* Section 3: Network Diagnostic Tools */}
      <section class="grid gap-4 md:grid-cols-3">
        <div class={SECTION_CLS}>
          <h3 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">Ping 连通性测试</h3>
          <div class="mt-3 space-y-2.5 text-xs">
            <input
              type="text"
              value={pingHost()}
              onInput={(e) => setPingHost(e.currentTarget.value)}
              placeholder="IP 或域名"
              class={INPUT_CLS}
            />
            <div class="flex items-center gap-2">
              <input
                type="number"
                min={1}
                max={10}
                value={pingCount()}
                onInput={(e) => setPingCount(Number(e.currentTarget.value) || 4)}
                class="w-20 rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#000000] px-2.5 py-1.5 font-mono text-xs text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed]"
              />
              <button
                type="button"
                disabled={running()}
                onClick={() =>
                  void execDiag(() => runPing({ host: pingHost(), count: pingCount() }))
                }
                class={`flex-1 ${BTN_PRIMARY}`}
              >
                执行 Ping
              </button>
            </div>
          </div>
        </div>

        <div class={SECTION_CLS}>
          <h3 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
            Traceroute 路由追踪
          </h3>
          <div class="mt-3 space-y-2.5 text-xs">
            <input
              type="text"
              value={traceHost()}
              onInput={(e) => setTraceHost(e.currentTarget.value)}
              placeholder="IP 或域名"
              class={INPUT_CLS}
            />
            <button
              type="button"
              disabled={running()}
              onClick={() => void execDiag(() => runTraceroute({ host: traceHost() }))}
              class={`w-full ${BTN_PRIMARY}`}
            >
              追踪路径
            </button>
          </div>
        </div>

        <div class={SECTION_CLS}>
          <h3 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">iperf3 吞吐量测速</h3>
          <div class="mt-3 space-y-2.5 text-xs">
            <div class="flex gap-2">
              <input
                type="text"
                value={iperfServer()}
                onInput={(e) => setIperfServer(e.currentTarget.value)}
                placeholder="服务端 IP"
                class={`flex-1 ${INPUT_CLS}`}
              />
              <input
                type="number"
                value={iperfPort()}
                onInput={(e) => setIperfPort(Number(e.currentTarget.value) || 5201)}
                class="w-20 rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#000000] px-2.5 py-1.5 font-mono text-xs text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed]"
              />
            </div>
            <div class="flex items-center justify-between gap-2">
              <label class="flex items-center gap-1.5 text-[#171717] dark:text-[#ededed]">
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
                class="w-16 rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#000000] px-2 py-1 font-mono text-xs text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed]"
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
                class={BTN_PRIMARY}
              >
                开始测速
              </button>
            </div>
          </div>
        </div>
      </section>

      <Show when={diagResult()}>
        {(res) => (
          <section class={SECTION_CLS}>
            <div class="mb-2.5 flex items-center justify-between text-xs">
              <span class="font-mono text-[#171717] dark:text-[#ededed]">$ {res().command}</span>
              <span
                class={`rounded-full border px-2 py-0.5 font-mono text-[11px] font-medium ${
                  res().ok
                    ? "border-emerald-500/20 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400"
                    : "border-rose-500/20 bg-rose-500/10 text-rose-700 dark:text-rose-400"
                }`}
              >
                exit {res().exit_code ?? "err"}
              </span>
            </div>
            <pre class="max-h-72 overflow-auto rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#000000] p-3.5 font-mono text-xs text-[#171717] dark:text-[#ededed]">
              {res().output}
            </pre>
          </section>
        )}
      </Show>

      {/* Live WebSocket Log Stream */}
      <section class={SECTION_CLS}>
        <div class="mb-3.5 flex flex-wrap items-center justify-between gap-2">
          <div class="flex items-center gap-2">
            <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">实时控制面日志流</h2>
            <span
              class={`rounded-full border px-2 py-0.5 text-[11px] font-medium ${
                wsConnected()
                  ? "border-emerald-500/20 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400"
                  : "border-zinc-500/20 bg-zinc-500/10 text-zinc-600 dark:text-zinc-400"
              }`}
            >
              {wsConnected() ? "WS 已连接" : "WS 未连接"}
            </span>
          </div>
          <div class="flex items-center gap-2 text-xs">
            <select
              value={levelFilter()}
              onChange={(e) => setLevelFilter(e.currentTarget.value)}
              class="rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#000000] px-2.5 py-1.5 text-xs text-[#171717] dark:text-[#ededed] outline-none"
            >
              <option value="ALL">全部级别</option>
              <option value="INFO">INFO</option>
              <option value="WARN">WARN</option>
              <option value="ERROR">ERROR</option>
            </select>
            <button type="button" onClick={() => setPaused((p) => !p)} class={BTN_OUTLINE}>
              {paused() ? "恢复滚动" : "暂停"}
            </button>
            <button type="button" onClick={() => setLogs([])} class={BTN_OUTLINE}>
              清屏
            </button>
          </div>
        </div>

        <div class="max-h-96 space-y-1 overflow-auto rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#000000] p-3.5 font-mono text-xs">
          <For each={filteredLogs()}>
            {(log) => (
              <div class="flex gap-2 leading-relaxed">
                <span class="shrink-0 text-[#8f8f8f] dark:text-[#71717a]">
                  {log.timestamp.slice(11, 19)}
                </span>
                <span
                  class={`shrink-0 ${
                    log.level === "ERROR"
                      ? "text-rose-600 dark:text-rose-400"
                      : log.level === "WARN"
                        ? "text-amber-600 dark:text-amber-400"
                        : "text-sky-600 dark:text-sky-400"
                  }`}
                >
                  [{log.level}]
                </span>
                <span class="shrink-0 text-[#666666] dark:text-[#a1a1aa]">{log.target}:</span>
                <span class="break-all text-[#171717] dark:text-[#ededed]">{log.message}</span>
              </div>
            )}
          </For>
        </div>
      </section>
    </div>
  );
}

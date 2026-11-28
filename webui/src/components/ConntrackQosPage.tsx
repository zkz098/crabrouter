import {
  type SortingState,
  createColumnHelper,
  createPaginatedRowModel,
  createSortedRowModel,
  createTable,
  rowPaginationFeature,
  rowSortingFeature,
  sortFns,
  tableFeatures,
} from "@tanstack/solid-table";
import { For, Show, createEffect, createMemo, createSignal, onCleanup } from "solid-js";
import { killConntrack } from "../api";
import { t } from "../i18n";
import type { CakeDiffserv } from "../types/CakeDiffserv";
import type { ConntrackEntry } from "../types/ConntrackEntry";
import type { HostInterfaceSnapshot } from "../types/HostInterfaceSnapshot";
import type { QdiscAlgo } from "../types/QdiscAlgo";
import type { QosInterfaceConfig } from "../types/QosInterfaceConfig";
import type { RouterConfig } from "../types/RouterConfig";
import { NicTuningSection } from "./NicTuningSection";
import { GuideBanner, PageHeader, SubTabs } from "./ui/ArgonPrimitives";

interface Props {
  mode: "conntrack" | "qos";
  draft: RouterConfig;
  interfaces: HostInterfaceSnapshot[];
  entries: ConntrackEntry[];
  onRefresh: () => void;
  onChange: (next: RouterConfig) => void;
}

const SECTION_CLS =
  "rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-colors hover:border-[#d4d4d8] dark:hover:border-[#3a3a3a]";
const ROW_CLS =
  "rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] p-4";
const INPUT_CLS =
  "mt-1 w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] px-2.5 py-1.5 text-xs text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] transition-colors";
const BTN_OUTLINE =
  "rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#111111] hover:bg-[#f4f4f5] dark:hover:bg-[#1a1a1a] px-3 py-1.5 text-xs font-medium text-[#171717] dark:text-[#ededed] cursor-pointer transition-all active:scale-[0.97] disabled:opacity-40";

const conntrackFeatures = tableFeatures({
  rowSortingFeature,
  rowPaginationFeature,
  sortedRowModel: createSortedRowModel(),
  paginatedRowModel: createPaginatedRowModel(),
  sortFns,
});

const columnHelper = createColumnHelper<typeof conntrackFeatures, ConntrackEntry>();

function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let idx = 0;
  while (value >= 1024 && idx < units.length - 1) {
    value /= 1024;
    idx += 1;
  }
  return `${value.toFixed(1)} ${units[idx]}`;
}

export function ConntrackQosPage(props: Props) {
  const [qosTab, setQosTab] = createSignal<"sqms" | "nic">("sqms");
  const [globalFilter, setGlobalFilter] = createSignal("");
  const [protoFilter, setProtoFilter] = createSignal<string>("all");
  const [familyFilter, setFamilyFilter] = createSignal<string>("all");
  const [autoRefresh, setAutoRefresh] = createSignal(false);
  const [sorting, setSorting] = createSignal<SortingState>([]);
  const [killingKey, setKillingKey] = createSignal<string | null>(null);
  const [actionMsg, setActionMsg] = createSignal<string | null>(null);

  createEffect(() => {
    if (!autoRefresh()) {
      return;
    }
    const timer = window.setInterval(() => {
      props.onRefresh();
    }, 3000);
    onCleanup(() => window.clearInterval(timer));
  });

  function toggleQosEnabled(enabled: boolean) {
    const next = structuredClone(props.draft);
    next.qos.enabled = enabled;
    if (enabled && next.qos.interfaces.length === 0) {
      const defaultIface = next.wans[0]?.device ?? "eth0";
      next.qos.interfaces.push({
        interface: defaultIface,
        enabled: true,
        algorithm: "cake",
        upload_kbps: 50000,
        download_kbps: 500000,
        overhead_bytes: next.wans[0]?.proto === "pppoe" ? 26 : 18,
        nat_aware: true,
        ack_filter: true,
        diffserv: "diffserv4",
      });
    }
    props.onChange(next);
  }

  function addQosInterface() {
    const next = structuredClone(props.draft);
    const candidate =
      next.wans.find((w) => !next.qos.interfaces.some((q) => q.interface === w.device))?.device ??
      next.lans[0]?.bridge ??
      "eth0";
    next.qos.interfaces.push({
      interface: candidate,
      enabled: true,
      algorithm: "cake",
      upload_kbps: 50000,
      download_kbps: 0,
      overhead_bytes: 18,
      nat_aware: true,
      ack_filter: false,
      diffserv: "diffserv4",
    });
    props.onChange(next);
  }

  function updateQosInterface(idx: number, patch: Partial<QosInterfaceConfig>) {
    const next = structuredClone(props.draft);
    const cur = next.qos.interfaces[idx];
    if (!cur) {
      return;
    }
    next.qos.interfaces[idx] = { ...cur, ...patch };
    props.onChange(next);
  }

  function removeQosInterface(idx: number) {
    const next = structuredClone(props.draft);
    next.qos.interfaces.splice(idx, 1);
    props.onChange(next);
  }

  async function handleKillFlow(entry: ConntrackEntry) {
    const key = `${entry.proto}:${entry.src}:${entry.sport ?? 0}->${entry.dst}:${entry.dport ?? 0}`;
    setKillingKey(key);
    setActionMsg(null);
    try {
      await killConntrack({
        family: entry.family,
        proto: entry.proto,
        src: entry.src,
        dst: entry.dst,
        sport: entry.sport,
        dport: entry.dport,
      });
      setActionMsg(
        `${t("ctKilledMsg")} ${entry.proto.toUpperCase()} ${entry.src}${entry.sport ? `:${entry.sport}` : ""} → ${entry.dst}${entry.dport ? `:${entry.dport}` : ""}`,
      );
      props.onRefresh();
    } catch (err) {
      setActionMsg(err instanceof Error ? err.message : String(err));
    } finally {
      setKillingKey(null);
    }
  }

  const filteredData = createMemo(() => {
    const p = protoFilter();
    const f = familyFilter();
    const q = globalFilter().trim().toLowerCase();
    return props.entries.filter((e) => {
      if (p !== "all" && e.proto.toLowerCase() !== p) {
        return false;
      }
      if (f !== "all" && e.family.toLowerCase() !== f) {
        return false;
      }
      if (!q) {
        return true;
      }
      const haystack =
        `${e.family} ${e.proto} ${e.state} ${e.src}:${e.sport ?? ""} ${e.dst}:${e.dport ?? ""}`.toLowerCase();
      return haystack.includes(q);
    });
  });

  const columns = columnHelper.columns([
    columnHelper.accessor((row) => `${row.proto.toUpperCase()} (${row.family})`, {
      id: "proto",
      header: () => t("ctColProto"),
      cell: (info) => {
        const row = info.row.original;
        return (
          <div class="flex items-center gap-1.5">
            <span class="rounded-md border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] px-2 py-0.5 font-mono text-[11px] font-semibold uppercase text-[#171717] dark:text-[#ededed]">
              {row.proto}
            </span>
            <span class="font-mono text-[11px] text-[#666666] dark:text-[#a1a1aa]">
              {row.family}
            </span>
          </div>
        );
      },
    }),
    columnHelper.accessor("state", {
      header: () => t("ctColState"),
      cell: (info) => {
        const row = info.row.original;
        return (
          <div class="flex flex-wrap items-center gap-1">
            <span class="rounded-full border border-emerald-500/20 bg-emerald-500/10 px-2 py-0.5 font-mono text-[11px] text-emerald-700 dark:text-emerald-400">
              {row.state}
            </span>
            <Show when={row.offloaded}>
              <span class="rounded-full border border-sky-500/20 bg-sky-500/10 px-1.5 py-0.5 font-mono text-[10px] text-sky-700 dark:text-sky-400">
                FLOWTABLE
              </span>
            </Show>
          </div>
        );
      },
    }),
    columnHelper.accessor((row) => `${row.src}:${row.sport ?? ""}`, {
      id: "src",
      header: () => t("ctColSrc"),
      cell: (info) => {
        const row = info.row.original;
        return (
          <span class="font-mono text-xs text-[#171717] dark:text-[#ededed]">
            {row.src}
            <Show when={row.sport}>
              {(p) => <span class="text-[#0070f3] dark:text-sky-400">:{p()}</span>}
            </Show>
          </span>
        );
      },
    }),
    columnHelper.accessor((row) => `${row.dst}:${row.dport ?? ""}`, {
      id: "dst",
      header: () => t("ctColDst"),
      cell: (info) => {
        const row = info.row.original;
        return (
          <span class="font-mono text-xs text-[#171717] dark:text-[#ededed]">
            {row.dst}
            <Show when={row.dport}>
              {(p) => <span class="text-amber-600 dark:text-amber-400">:{p()}</span>}
            </Show>
          </span>
        );
      },
    }),
    columnHelper.accessor("orig_bytes", {
      header: () => t("ctColOrig"),
      cell: (info) => {
        const row = info.row.original;
        return (
          <div class="font-mono text-xs tabular-nums">
            <span class="text-emerald-600 dark:text-emerald-400">
              {formatBytes(row.orig_bytes)}
            </span>
            <span class="ml-1 text-[11px] text-[#8f8f8f] dark:text-[#71717a]">
              ({row.orig_packets} pkts)
            </span>
          </div>
        );
      },
    }),
    columnHelper.accessor("reply_bytes", {
      header: () => t("ctColReply"),
      cell: (info) => {
        const row = info.row.original;
        return (
          <div class="font-mono text-xs tabular-nums">
            <span class="text-sky-600 dark:text-sky-400">{formatBytes(row.reply_bytes)}</span>
            <span class="ml-1 text-[11px] text-[#8f8f8f] dark:text-[#71717a]">
              ({row.reply_packets} pkts)
            </span>
          </div>
        );
      },
    }),
    columnHelper.accessor("timeout_secs", {
      header: "TTL",
      cell: (info) => (
        <span class="font-mono text-xs tabular-nums text-[#666666] dark:text-[#a1a1aa]">
          {info.row.original.timeout_secs}s
        </span>
      ),
    }),
    columnHelper.display({
      id: "actions",
      header: () => t("ctColAction"),
      cell: (info) => {
        const row = info.row.original;
        const key = () => `${row.proto}:${row.src}:${row.sport ?? 0}->${row.dst}:${row.dport ?? 0}`;
        return (
          <button
            type="button"
            disabled={killingKey() === key()}
            onClick={() => void handleKillFlow(row)}
            class="rounded-md border border-rose-500/25 bg-rose-500/10 hover:bg-rose-500/20 px-2 py-0.5 text-[11px] font-medium text-rose-600 dark:text-rose-400 cursor-pointer disabled:opacity-40 transition-colors"
          >
            {killingKey() === key() ? t("ctKilling") : t("ctKill")}
          </button>
        );
      },
    }),
  ]);

  const table = createTable({
    features: conntrackFeatures,
    columns,
    get data() {
      return filteredData();
    },
    state: {
      get sorting() {
        return sorting();
      },
    },
    onSortingChange: setSorting,
    initialState: {
      pagination: {
        pageIndex: 0,
        pageSize: 25,
      },
    },
  });

  const totalBytes = () => props.entries.reduce((acc, e) => acc + e.orig_bytes + e.reply_bytes, 0);
  const offloadedCount = () => props.entries.filter((e) => e.offloaded).length;

  return (
    <div class="space-y-5 animate-view-enter">
      <Show when={props.mode === "qos"}>
        <PageHeader title={t("navQos")} subtitle={t("descQos")} />
        <GuideBanner
          title={t("qosGuideTitle")}
          description={t("qosGuideDesc")}
          variant="tip"
          dismissible
        />
        <SubTabs
          active={qosTab()}
          onChange={setQosTab}
          items={[
            { key: "sqms", label: t("qosSubTabSqm"), icon: "i-ri-speed-up-line" },
            { key: "nic", label: t("qosSubTabNic"), icon: "i-ri-cpu-line" },
          ]}
        />
      </Show>

      <Show when={props.mode === "conntrack"}>
        <PageHeader title={t("navConntrack")} subtitle={t("descConntrack")} />
        <GuideBanner
          title={t("ctGuideTitle")}
          description={t("ctGuideDesc")}
          variant="info"
          dismissible
        />
      </Show>

      <Show when={props.mode === "qos" && qosTab() === "sqms"}>
        {/* Section 1: QoS Smart Queue Management */}
        <section class={SECTION_CLS}>
          <div class="mb-3.5 flex flex-wrap items-center justify-between gap-2">
            <div>
              <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
                {t("qosTitle")}
              </h2>
              <p class="mt-0.5 text-xs text-[#666666] dark:text-[#a1a1aa]">{t("qosDesc")}</p>
            </div>
            <div class="flex items-center gap-3">
              <label class="flex items-center gap-2 text-xs font-medium text-[#171717] dark:text-[#ededed]">
                <input
                  type="checkbox"
                  checked={props.draft.qos.enabled}
                  onChange={(e) => toggleQosEnabled(e.currentTarget.checked)}
                />
                {t("qosEnable")}
              </label>
              <button type="button" onClick={addQosInterface} class={BTN_OUTLINE}>
                {t("qosAddIface")}
              </button>
            </div>
          </div>

          <Show
            when={props.draft.qos.interfaces.length > 0}
            fallback={
              <p class="rounded-xl border border-dashed border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111]/60 p-4 text-center text-xs text-[#666666] dark:text-[#a1a1aa]">
                {t("qosEmpty")}
              </p>
            }
          >
            <div class="space-y-3">
              <For each={props.draft.qos.interfaces}>
                {(rule, idx) => (
                  <div class={ROW_CLS}>
                    <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
                      <div class="flex items-center gap-3">
                        <span class="font-mono text-sm font-semibold text-[#0070f3] dark:text-sky-400">
                          {rule.interface}
                        </span>
                        <label class="flex items-center gap-1.5 text-xs text-[#171717] dark:text-[#ededed]">
                          <input
                            type="checkbox"
                            checked={rule.enabled}
                            onChange={(e) =>
                              updateQosInterface(idx(), { enabled: e.currentTarget.checked })
                            }
                          />
                          {t("qosEnableIface")}
                        </label>
                      </div>
                      <button
                        type="button"
                        onClick={() => removeQosInterface(idx())}
                        class="text-xs font-medium text-rose-600 dark:text-rose-400 hover:text-rose-700 dark:hover:text-rose-300 bg-transparent border-0 cursor-pointer"
                      >
                        {t("commonDelete")}
                      </button>
                    </div>

                    <div class="grid gap-3 sm:grid-cols-3 lg:grid-cols-6">
                      <label class="block text-xs">
                        <span class="text-[#666666] dark:text-[#a1a1aa]">{t("qosBindIface")}</span>
                        <input
                          type="text"
                          value={rule.interface}
                          onInput={(e) =>
                            updateQosInterface(idx(), { interface: e.currentTarget.value })
                          }
                          class={`${INPUT_CLS} font-mono`}
                        />
                      </label>

                      <label class="block text-xs">
                        <span class="text-[#666666] dark:text-[#a1a1aa]">{t("qosAlgo")}</span>
                        <select
                          value={rule.algorithm}
                          onChange={(e) =>
                            updateQosInterface(idx(), {
                              algorithm: e.currentTarget.value as QdiscAlgo,
                            })
                          }
                          class={INPUT_CLS}
                        >
                          <option value="cake">{t("qosAlgoCake")}</option>
                          <option value="fq_codel">{t("qosAlgoFqCodel")}</option>
                        </select>
                      </label>

                      <label class="block text-xs">
                        <span class="text-[#666666] dark:text-[#a1a1aa]">{t("qosUploadKbps")}</span>
                        <input
                          type="number"
                          min={0}
                          step={1000}
                          value={rule.upload_kbps}
                          onInput={(e) =>
                            updateQosInterface(idx(), {
                              upload_kbps: Math.max(0, Number(e.currentTarget.value) || 0),
                            })
                          }
                          class={`${INPUT_CLS} font-mono`}
                        />
                      </label>

                      <label class="block text-xs">
                        <span class="text-[#666666] dark:text-[#a1a1aa]">
                          {t("qosDownloadKbps")}
                        </span>
                        <input
                          type="number"
                          min={0}
                          step={1000}
                          value={rule.download_kbps}
                          onInput={(e) =>
                            updateQosInterface(idx(), {
                              download_kbps: Math.max(0, Number(e.currentTarget.value) || 0),
                            })
                          }
                          class={`${INPUT_CLS} font-mono`}
                        />
                      </label>

                      <label class="block text-xs">
                        <span class="text-[#666666] dark:text-[#a1a1aa]">{t("qosOverhead")}</span>
                        <input
                          type="number"
                          min={-64}
                          max={256}
                          value={rule.overhead_bytes}
                          onInput={(e) =>
                            updateQosInterface(idx(), {
                              overhead_bytes: Number(e.currentTarget.value) || 0,
                            })
                          }
                          class={`${INPUT_CLS} font-mono`}
                        />
                      </label>

                      <Show when={rule.algorithm === "cake"}>
                        <label class="block text-xs">
                          <span class="text-[#666666] dark:text-[#a1a1aa]">{t("qosDiffserv")}</span>
                          <select
                            value={rule.diffserv}
                            onChange={(e) =>
                              updateQosInterface(idx(), {
                                diffserv: e.currentTarget.value as CakeDiffserv,
                              })
                            }
                            class={INPUT_CLS}
                          >
                            <option value="diffserv4">{t("qosDiffserv4")}</option>
                            <option value="diffserv3">{t("qosDiffserv3")}</option>
                            <option value="best_effort">{t("qosBestEffort")}</option>
                          </select>
                        </label>
                      </Show>
                    </div>

                    <Show when={rule.algorithm === "cake"}>
                      <div class="mt-3 flex flex-wrap gap-4 border-t border-[#eaeaea] dark:border-[#262626] pt-2.5 text-xs text-[#171717] dark:text-[#ededed]">
                        <label class="flex items-center gap-2">
                          <input
                            type="checkbox"
                            checked={rule.nat_aware}
                            onChange={(e) =>
                              updateQosInterface(idx(), { nat_aware: e.currentTarget.checked })
                            }
                          />
                          {t("qosNatAware")}
                        </label>
                        <label class="flex items-center gap-2">
                          <input
                            type="checkbox"
                            checked={rule.ack_filter}
                            onChange={(e) =>
                              updateQosInterface(idx(), { ack_filter: e.currentTarget.checked })
                            }
                          />
                          {t("qosAckFilter")}
                        </label>
                      </div>
                    </Show>
                  </div>
                )}
              </For>
            </div>
          </Show>
        </section>
      </Show>

      <Show when={props.mode === "qos" && qosTab() === "nic"}>
        <NicTuningSection
          draft={props.draft}
          interfaces={props.interfaces}
          onChange={props.onChange}
        />
      </Show>

      <Show when={props.mode === "conntrack"}>
        {/* Section 2: Real-Time Netlink Conntrack Table */}
        <section class={SECTION_CLS}>
          <div class="mb-4 flex flex-wrap items-center justify-between gap-3">
            <div>
              <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">{t("ctTitle")}</h2>
              <p class="mt-0.5 text-xs text-[#666666] dark:text-[#a1a1aa] tabular-nums">
                {t("ctActiveFlows")}{" "}
                <span class="font-mono font-medium text-[#171717] dark:text-[#ededed]">
                  {props.entries.length}
                </span>{" "}
                ({t("ctFiltered")}{" "}
                <span class="font-mono font-medium text-[#0070f3] dark:text-sky-400">
                  {filteredData().length}
                </span>
                ) · {t("ctOffloaded")}{" "}
                <span class="font-mono font-medium text-emerald-600 dark:text-emerald-400">
                  {offloadedCount()}
                </span>{" "}
                · {t("ctTotalTraffic")}{" "}
                <span class="font-mono font-medium text-[#171717] dark:text-[#ededed]">
                  {formatBytes(totalBytes())}
                </span>
              </p>
            </div>

            <div class="flex flex-wrap items-center gap-2 text-xs">
              <label class="flex items-center gap-1.5 rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] px-2.5 py-1.5 text-[#171717] dark:text-[#ededed]">
                <input
                  type="checkbox"
                  checked={autoRefresh()}
                  onChange={(e) => setAutoRefresh(e.currentTarget.checked)}
                />
                {t("ctAutoRefresh")}
              </label>
              <button type="button" onClick={() => props.onRefresh()} class={BTN_OUTLINE}>
                {t("ctRefreshNow")}
              </button>
            </div>
          </div>

          <Show when={actionMsg()}>
            {(msg) => (
              <div class="mb-3 rounded-lg border border-sky-500/25 bg-sky-500/10 px-3 py-2 text-xs text-sky-700 dark:text-sky-300">
                {msg()}
              </div>
            )}
          </Show>

          <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
            <div class="relative min-w-64 flex-1">
              <span class="i-ri-search-line absolute left-3 top-1/2 -translate-y-1/2 text-[#8f8f8f] dark:text-[#71717a] text-sm pointer-events-none" />
              <input
                type="text"
                value={globalFilter()}
                placeholder={t("ctSearchPlaceholder")}
                onInput={(e) => setGlobalFilter(e.currentTarget.value)}
                class="w-full h-9 rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#000000] pl-9 pr-3 font-mono text-xs text-[#171717] dark:text-[#ededed] placeholder:text-[#8f8f8f] dark:placeholder:text-[#71717a] outline-none focus:border-[#171717] dark:focus:border-[#ededed] transition-colors"
              />
            </div>

            <div class="flex flex-wrap gap-1.5 text-xs">
              <div class="flex items-center gap-0.5 rounded-xl border border-[#eaeaea] dark:border-[#222222] bg-[#f4f4f5] dark:bg-[#111111] p-1">
                <For each={["all", "tcp", "udp", "icmp"]}>
                  {(p) => (
                    <button
                      type="button"
                      onClick={() => setProtoFilter(p)}
                      class={`rounded-lg px-2.5 py-1 text-xs font-medium uppercase cursor-pointer border-0 transition-all ${
                        protoFilter() === p
                          ? "bg-white dark:bg-[#1f1f1f] text-[#171717] dark:text-white shadow-sm ring-1 ring-black/5 dark:ring-white/10"
                          : "bg-transparent text-[#666666] dark:text-[#8f8f8f] hover:text-[#171717] dark:hover:text-[#ededed]"
                      }`}
                    >
                      {p}
                    </button>
                  )}
                </For>
              </div>
              <div class="flex items-center gap-0.5 rounded-xl border border-[#eaeaea] dark:border-[#222222] bg-[#f4f4f5] dark:bg-[#111111] p-1">
                <For each={["all", "ipv4", "ipv6"]}>
                  {(f) => (
                    <button
                      type="button"
                      onClick={() => setFamilyFilter(f)}
                      class={`rounded-lg px-2.5 py-1 text-xs font-medium uppercase cursor-pointer border-0 transition-all ${
                        familyFilter() === f
                          ? "bg-white dark:bg-[#1f1f1f] text-[#171717] dark:text-white shadow-sm ring-1 ring-black/5 dark:ring-white/10"
                          : "bg-transparent text-[#666666] dark:text-[#8f8f8f] hover:text-[#171717] dark:hover:text-[#ededed]"
                      }`}
                    >
                      {f}
                    </button>
                  )}
                </For>
              </div>
            </div>
          </div>

          <div class="overflow-x-auto rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a]">
            <table class="w-full text-left text-xs">
              <thead class="border-b border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] text-[#666666] dark:text-[#a1a1aa]">
                <For each={table.getHeaderGroups()}>
                  {(headerGroup) => (
                    <tr>
                      <For each={headerGroup.headers}>
                        {(header) => (
                          <th
                            onClick={header.column.getToggleSortingHandler()}
                            class="cursor-pointer select-none px-3 py-2.5 font-medium hover:text-[#171717] dark:hover:text-[#ededed]"
                          >
                            <div class="flex items-center gap-1">
                              <Show when={!header.isPlaceholder}>
                                <table.FlexRender header={header} />
                              </Show>
                              <span>
                                {{
                                  asc: "▲",
                                  desc: "▼",
                                }[header.column.getIsSorted() as string] ?? ""}
                              </span>
                            </div>
                          </th>
                        )}
                      </For>
                    </tr>
                  )}
                </For>
              </thead>
              <tbody class="divide-y divide-[#eaeaea] dark:divide-[#262626]">
                <Show
                  when={table.getRowModel().rows.length > 0}
                  fallback={
                    <tr>
                      <td
                        colspan={columns.length}
                        class="py-8 text-center text-[#8f8f8f] dark:text-[#71717a]"
                      >
                        {t("ctEmpty")}
                      </td>
                    </tr>
                  }
                >
                  <For each={table.getRowModel().rows}>
                    {(row) => (
                      <tr class="hover:bg-[#fafafa] dark:hover:bg-[#111111]/60 transition-colors">
                        <For each={row.getAllCells()}>
                          {(cell) => (
                            <td class="px-3 py-2">
                              <table.FlexRender cell={cell} />
                            </td>
                          )}
                        </For>
                      </tr>
                    )}
                  </For>
                </Show>
              </tbody>
            </table>
          </div>

          <div class="mt-3 flex flex-wrap items-center justify-between gap-2 text-xs text-[#666666] dark:text-[#a1a1aa]">
            <div class="tabular-nums">
              {t("ctPageOf")
                .replace("{page}", String(table.atoms.pagination.get().pageIndex + 1))
                .replace("{total}", String(Math.max(1, table.getPageCount())))}
            </div>
            <div class="flex gap-2">
              <button
                type="button"
                disabled={!table.getCanPreviousPage()}
                onClick={() => table.previousPage()}
                class={BTN_OUTLINE}
              >
                {t("ctPrevPage")}
              </button>
              <button
                type="button"
                disabled={!table.getCanNextPage()}
                onClick={() => table.nextPage()}
                class={BTN_OUTLINE}
              >
                {t("ctNextPage")}
              </button>
            </div>
          </div>
        </section>
      </Show>
    </div>
  );
}

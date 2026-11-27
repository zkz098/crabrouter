import type { AppCatalogEntry } from "./types/AppCatalogEntry";
import type { AppInstallRequest } from "./types/AppInstallRequest";
import type { AuthTokenResponse } from "./types/AuthTokenResponse";
import type { CommitStatus } from "./types/CommitStatus";
import type { ConfigSnapshotSummary } from "./types/ConfigSnapshotSummary";
import type { ConntrackEntry } from "./types/ConntrackEntry";
import type { ConntrackKillRequest } from "./types/ConntrackKillRequest";
import type { ConntrackKillResponse } from "./types/ConntrackKillResponse";
import type { ContainerActionResult } from "./types/ContainerActionResult";
import type { CrabDnsAggregateStatus } from "./types/CrabDnsAggregateStatus";
import type { CrowdSecStatus } from "./types/CrowdSecStatus";
import type { DdnsRuntimeStatus } from "./types/DdnsRuntimeStatus";
import type { DhcpLeaseEntry } from "./types/DhcpLeaseEntry";
import type { DiagCommandResult } from "./types/DiagCommandResult";
import type { EbpfRuntimeStatus } from "./types/EbpfRuntimeStatus";
import type { HardwareHealthSnapshot } from "./types/HardwareHealthSnapshot";
import type { HostInterfaceSnapshot } from "./types/HostInterfaceSnapshot";
import type { InstalledApp } from "./types/InstalledApp";
import type { Iperf3Request } from "./types/Iperf3Request";
import type { LogEvent } from "./types/LogEvent";
import type { LoginRequest } from "./types/LoginRequest";
import type { MultiWanRuntimeStatus } from "./types/MultiWanRuntimeStatus";
import type { NicCapabilitiesSnapshot } from "./types/NicCapabilitiesSnapshot";
import type { OtaApplyRequest } from "./types/OtaApplyRequest";
import type { OtaCommandResult } from "./types/OtaCommandResult";
import type { OtaStatus } from "./types/OtaStatus";
import type { PingRequest } from "./types/PingRequest";
import type { RenderedConfigsPreview } from "./types/RenderedConfigsPreview";
import type { RouterConfig } from "./types/RouterConfig";
import type { SetupInitRequest } from "./types/SetupInitRequest";
import type { SetupStatusResponse } from "./types/SetupStatusResponse";
import type { StagedOtaImage } from "./types/StagedOtaImage";
import type { SysextActionResult } from "./types/SysextActionResult";
import type { SysextRuntimeStatus } from "./types/SysextRuntimeStatus";
import type { SystemdUnitStatus } from "./types/SystemdUnitStatus";
import type { TlsRuntimeStatus } from "./types/TlsRuntimeStatus";
import type { TracerouteRequest } from "./types/TracerouteRequest";
import type { UpnpLeaseEntry } from "./types/UpnpLeaseEntry";
import type { WanRuntimeState } from "./types/WanRuntimeState";
import type { WireGuardKeypairResponse } from "./types/WireGuardKeypairResponse";

async function fetchJson<T>(url: string, init?: RequestInit): Promise<T> {
  const res = await fetch(url, {
    credentials: "same-origin",
    ...init,
  });
  if (!res.ok) {
    const text = await res.text();
    try {
      const parsed = JSON.parse(text) as { error?: string };
      if (parsed.error) {
        throw new Error(parsed.error);
      }
    } catch {
      // fall back to raw response text
    }
    throw new Error(text || `HTTP ${res.status}`);
  }
  if (res.status === 204) {
    return undefined as T;
  }
  return (await res.json()) as T;
}

export function getSetupStatus(): Promise<SetupStatusResponse> {
  return fetchJson<SetupStatusResponse>("/api/setup/status");
}

export function initSetup(payload: SetupInitRequest): Promise<AuthTokenResponse> {
  return fetchJson<AuthTokenResponse>("/api/setup/init", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
}

export function login(payload: LoginRequest): Promise<AuthTokenResponse> {
  return fetchJson<AuthTokenResponse>("/api/auth/login", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload),
  });
}

export function logout(): Promise<void> {
  return fetchJson<void>("/api/auth/logout", { method: "POST" });
}

export function getStatus(): Promise<CommitStatus> {
  return fetchJson<CommitStatus>("/api/status");
}

export function getServiceStatuses(): Promise<SystemdUnitStatus[]> {
  return fetchJson<SystemdUnitStatus[]>("/api/services/status");
}

export function getDnsStatus(): Promise<CrabDnsAggregateStatus> {
  return fetchJson<CrabDnsAggregateStatus>("/api/dns/status");
}

export function getInterfaces(): Promise<HostInterfaceSnapshot[]> {
  return fetchJson<HostInterfaceSnapshot[]>("/api/interfaces");
}

export function getInterfaceCapabilities(name: string): Promise<NicCapabilitiesSnapshot> {
  return fetchJson<NicCapabilitiesSnapshot>(
    `/api/interfaces/${encodeURIComponent(name)}/capabilities`,
  );
}

export function getWanStatus(): Promise<WanRuntimeState[]> {
  return fetchJson<WanRuntimeState[]>("/api/wans/status");
}

export function getMultiWanStatus(): Promise<MultiWanRuntimeStatus> {
  return fetchJson<MultiWanRuntimeStatus>("/api/wans/multi-wan/status");
}

export function triggerMultiWanProbe(): Promise<MultiWanRuntimeStatus> {
  return fetchJson<MultiWanRuntimeStatus>("/api/wans/multi-wan/probe", {
    method: "POST",
  });
}

export function getEbpfStatus(): Promise<EbpfRuntimeStatus> {
  return fetchJson<EbpfRuntimeStatus>("/api/ebpf/status");
}

export function getDdnsStatus(): Promise<DdnsRuntimeStatus> {
  return fetchJson<DdnsRuntimeStatus>("/api/ddns/status");
}

export function triggerDdnsSync(): Promise<DdnsRuntimeStatus> {
  return fetchJson<DdnsRuntimeStatus>("/api/ddns/sync", { method: "POST" });
}

export function getTlsStatus(): Promise<TlsRuntimeStatus> {
  return fetchJson<TlsRuntimeStatus>("/api/tls/status");
}

export function triggerTlsRenew(): Promise<TlsRuntimeStatus> {
  return fetchJson<TlsRuntimeStatus>("/api/tls/renew", { method: "POST" });
}

export function getCrowdSecStatus(): Promise<CrowdSecStatus> {
  return fetchJson<CrowdSecStatus>("/api/crowdsec/status");
}

export function triggerCrowdSecSync(): Promise<CrowdSecStatus> {
  return fetchJson<CrowdSecStatus>("/api/crowdsec/sync", { method: "POST" });
}

export function getAppCatalog(): Promise<AppCatalogEntry[]> {
  return fetchJson<AppCatalogEntry[]>("/api/apps/catalog");
}

export function getInstalledApps(): Promise<InstalledApp[]> {
  return fetchJson<InstalledApp[]>("/api/apps/installed");
}

export function installApp(req: AppInstallRequest): Promise<ContainerActionResult> {
  return fetchJson<ContainerActionResult>("/api/apps/install", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export function startApp(id: string): Promise<ContainerActionResult> {
  return fetchJson<ContainerActionResult>(`/api/apps/${encodeURIComponent(id)}/start`, {
    method: "POST",
  });
}

export function stopApp(id: string): Promise<ContainerActionResult> {
  return fetchJson<ContainerActionResult>(`/api/apps/${encodeURIComponent(id)}/stop`, {
    method: "POST",
  });
}

export function uninstallApp(id: string): Promise<ContainerActionResult> {
  return fetchJson<ContainerActionResult>(`/api/apps/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

export function getSysextStatus(): Promise<SysextRuntimeStatus> {
  return fetchJson<SysextRuntimeStatus>("/api/sysext/status");
}

export async function uploadSysextImage(file: File): Promise<SysextActionResult> {
  const form = new FormData();
  form.append("file", file, file.name);
  const res = await fetch("/api/sysext/upload", {
    method: "POST",
    body: form,
    credentials: "same-origin",
  });
  if (!res.ok) {
    throw new Error((await res.text()) || `HTTP ${res.status}`);
  }
  return (await res.json()) as SysextActionResult;
}

export function refreshSysext(): Promise<SysextActionResult> {
  return fetchJson<SysextActionResult>("/api/sysext/refresh", { method: "POST" });
}

export function unmergeSysext(): Promise<SysextActionResult> {
  return fetchJson<SysextActionResult>("/api/sysext/unmerge", { method: "POST" });
}

export function deleteSysextImage(name: string): Promise<SysextActionResult> {
  return fetchJson<SysextActionResult>(`/api/sysext/${encodeURIComponent(name)}`, {
    method: "DELETE",
  });
}

export function getClients(): Promise<DhcpLeaseEntry[]> {
  return fetchJson<DhcpLeaseEntry[]>("/api/clients");
}

export function getUpnpLeases(): Promise<UpnpLeaseEntry[]> {
  return fetchJson<UpnpLeaseEntry[]>("/api/upnp/leases");
}

export function getConntrack(): Promise<ConntrackEntry[]> {
  return fetchJson<ConntrackEntry[]>("/api/conntrack");
}

export function killConntrack(req: ConntrackKillRequest): Promise<ConntrackKillResponse> {
  return fetchJson<ConntrackKillResponse>("/api/conntrack/kill", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });
}

export function getConfig(): Promise<RouterConfig> {
  return fetchJson<RouterConfig>("/api/config");
}

export function getPreview(): Promise<RenderedConfigsPreview> {
  return fetchJson<RenderedConfigsPreview>("/api/config/preview");
}

export function beginCommit(next: RouterConfig): Promise<CommitStatus> {
  return fetchJson<CommitStatus>("/api/config/commit", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(next),
  });
}

export function confirmCommit(): Promise<CommitStatus> {
  return fetchJson<CommitStatus>("/api/config/confirm", { method: "POST" });
}

export function rollbackCommit(): Promise<CommitStatus> {
  return fetchJson<CommitStatus>("/api/config/rollback", { method: "POST" });
}

export function getConfigHistory(): Promise<ConfigSnapshotSummary[]> {
  return fetchJson<ConfigSnapshotSummary[]>("/api/config/history");
}

export function restoreConfigSnapshot(generation: number): Promise<CommitStatus> {
  return fetchJson<CommitStatus>(`/api/config/history/${generation}/restore`, {
    method: "POST",
  });
}

export function generateWireGuardKeypair(): Promise<WireGuardKeypairResponse> {
  return fetchJson<WireGuardKeypairResponse>("/api/wireguard/keypair", {
    method: "POST",
  });
}

export function runPing(req: PingRequest): Promise<DiagCommandResult> {
  return fetchJson<DiagCommandResult>("/api/diag/ping", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });
}

export function runTraceroute(req: TracerouteRequest): Promise<DiagCommandResult> {
  return fetchJson<DiagCommandResult>("/api/diag/traceroute", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });
}

export function runIperf3(req: Iperf3Request): Promise<DiagCommandResult> {
  return fetchJson<DiagCommandResult>("/api/diag/iperf3", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });
}

export function getLogSnapshot(): Promise<LogEvent[]> {
  return fetchJson<LogEvent[]>("/api/diag/logs");
}

export function getHardwareHealth(): Promise<HardwareHealthSnapshot> {
  return fetchJson<HardwareHealthSnapshot>("/api/hardware/health");
}

export function openTerminalWebSocket(): WebSocket {
  const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
  return new WebSocket(`${proto}//${window.location.host}/api/terminal/ws`);
}

export function openLogWebSocket(): WebSocket {
  const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
  return new WebSocket(`${proto}//${window.location.host}/api/diag/logs/ws`);
}

export function openMetricsWebSocket(): WebSocket {
  const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
  return new WebSocket(`${proto}//${window.location.host}/api/metrics/ws`);
}

export function getOtaStatus(): Promise<OtaStatus> {
  return fetchJson<OtaStatus>("/api/ota/status");
}

export function checkOtaUpdates(): Promise<OtaCommandResult> {
  return fetchJson<OtaCommandResult>("/api/ota/check", { method: "POST" });
}

export function uploadOtaImage(file: File): Promise<StagedOtaImage> {
  const form = new FormData();
  form.append("file", file);
  return fetchJson<StagedOtaImage>("/api/ota/upload", {
    method: "POST",
    body: form,
  });
}

export function applyOtaUpdate(req: OtaApplyRequest): Promise<OtaCommandResult> {
  return fetchJson<OtaCommandResult>("/api/ota/apply", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(req),
  });
}

export function blessOtaBoot(): Promise<OtaCommandResult> {
  return fetchJson<OtaCommandResult>("/api/ota/bless", { method: "POST" });
}

export function rebootSystem(): Promise<OtaCommandResult> {
  return fetchJson<OtaCommandResult>("/api/ota/reboot", { method: "POST" });
}

import QRCode from "qrcode";
import { For, Show, createSignal } from "solid-js";
import { generateWireGuardKeypair } from "../api";
import type { DhcpLeaseEntry } from "../types/DhcpLeaseEntry";
import type { RouterConfig } from "../types/RouterConfig";
import type { WireGuardPeer } from "../types/WireGuardPeer";

interface Props {
  draft: RouterConfig;
  clients: DhcpLeaseEntry[];
  onRefreshClients: () => void;
  onChange: (next: RouterConfig) => void;
}

export function ClientsWgPage(props: Props) {
  const [serverPubKey, setServerPubKey] = createSignal("");
  const [qrDataUrl, setQrDataUrl] = createSignal<string | null>(null);
  const [qrConfigText, setQrConfigText] = createSignal<string | null>(null);
  const [qrPeerName, setQrPeerName] = createSignal<string>("");
  const [endpointHost, setEndpointHost] = createSignal(window.location.hostname || "router.lan");
  const [busyKey, setBusyKey] = createSignal(false);

  const primaryLan = () => props.draft.lans[0];

  function addStaticLease(mac = "", ip = "", hostname = "") {
    const next = structuredClone(props.draft);
    const lan = next.lans[0];
    if (!lan) {
      return;
    }
    lan.static_leases.push({
      name: hostname || `host-${lan.static_leases.length + 1}`,
      mac,
      ip,
      hostname: hostname || null,
    });
    props.onChange(next);
  }

  async function generatePeerWithQr() {
    setBusyKey(true);
    try {
      const peerKeys = await generateWireGuardKeypair();
      const next = structuredClone(props.draft);
      let srvPub = serverPubKey();
      if (!srvPub) {
        const srvKeys = await generateWireGuardKeypair();
        srvPub = srvKeys.public_key;
        setServerPubKey(srvPub);
        if (!next.wireguard.private_key) {
          next.wireguard.private_key = srvKeys.private_key;
        }
      }

      const idx = next.wireguard.peers.length + 1;
      const clientIp = `10.10.0.${idx + 1}/32`;
      const peerName = `peer-${idx}`;
      const peer: WireGuardPeer = {
        name: peerName,
        public_key: peerKeys.public_key,
        allowed_ips: [clientIp],
        endpoint: null,
        persistent_keepalive: 25,
      };
      next.wireguard.peers.push(peer);
      props.onChange(next);

      const clientConf = [
        "[Interface]",
        `PrivateKey = ${peerKeys.private_key}`,
        `Address = ${clientIp}`,
        "DNS = 192.168.10.1",
        "",
        "[Peer]",
        `PublicKey = ${srvPub}`,
        `Endpoint = ${endpointHost()}:${next.wireguard.listen_port}`,
        "AllowedIPs = 0.0.0.0/0, ::/0",
        "PersistentKeepalive = 25",
      ].join("\n");

      const dataUrl = await QRCode.toDataURL(clientConf, {
        width: 240,
        margin: 2,
      });
      setQrPeerName(peerName);
      setQrConfigText(clientConf);
      setQrDataUrl(dataUrl);
    } finally {
      setBusyKey(false);
    }
  }

  async function showExistingPeerQr(peer: WireGuardPeer) {
    const srvPub = serverPubKey() || "<SERVER_PUBLIC_KEY>";
    const clientConf = [
      "[Interface]",
      "PrivateKey = <INSERT_CLIENT_PRIVATE_KEY>",
      `Address = ${peer.allowed_ips[0] ?? "10.10.0.2/32"}`,
      "DNS = 192.168.10.1",
      "",
      "[Peer]",
      `PublicKey = ${srvPub}`,
      `Endpoint = ${peer.endpoint ?? `${endpointHost()}:${props.draft.wireguard.listen_port}`}`,
      "AllowedIPs = 0.0.0.0/0, ::/0",
      `PersistentKeepalive = ${peer.persistent_keepalive ?? 25}`,
    ].join("\n");
    const dataUrl = await QRCode.toDataURL(clientConf, { width: 240, margin: 2 });
    setQrPeerName(peer.name);
    setQrConfigText(clientConf);
    setQrDataUrl(dataUrl);
  }

  return (
    <div class="space-y-6">
      {/* Online Clients */}
      <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
        <div class="mb-3 flex items-center justify-between">
          <h2 class="text-sm font-semibold text-slate-200">在线终端列表 (DHCP 租约 & ARP 邻居)</h2>
          <button
            type="button"
            onClick={() => props.onRefreshClients()}
            class="rounded-lg border border-slate-700 bg-slate-800 px-3 py-1 text-xs hover:bg-slate-700"
          >
            刷新列表
          </button>
        </div>
        <Show
          when={props.clients.length > 0}
          fallback={
            <p class="text-xs text-slate-400">当前未读取到活动 DHCP 租约或 ARP 在线终端。</p>
          }
        >
          <div class="overflow-x-auto">
            <table class="w-full text-left text-xs">
              <thead class="border-b border-slate-800 text-slate-400">
                <tr>
                  <th class="py-2 pr-4">IP 地址</th>
                  <th class="py-2 pr-4">MAC 地址</th>
                  <th class="py-2 pr-4">主机名</th>
                  <th class="py-2 pr-4">绑定类型</th>
                  <th class="py-2 text-right">操作</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-slate-800/70">
                <For each={props.clients}>
                  {(client) => (
                    <tr>
                      <td class="py-2 pr-4 font-mono text-slate-100">{client.ip}</td>
                      <td class="py-2 pr-4 font-mono text-slate-400">{client.mac}</td>
                      <td class="py-2 pr-4 text-slate-200">{client.hostname}</td>
                      <td class="py-2 pr-4">
                        <span
                          class={
                            client.is_static
                              ? "rounded bg-sky-500/20 px-2 py-0.5 text-sky-300"
                              : "rounded bg-slate-700 px-2 py-0.5 text-slate-300"
                          }
                        >
                          {client.is_static ? "静态绑定" : "动态租约"}
                        </span>
                      </td>
                      <td class="py-2 text-right">
                        <Show when={!client.is_static}>
                          <button
                            type="button"
                            onClick={() =>
                              addStaticLease(
                                client.mac,
                                client.ip,
                                client.hostname === "-" ? "" : client.hostname,
                              )
                            }
                            class="rounded border border-slate-700 bg-slate-800 px-2 py-0.5 text-xs hover:bg-slate-700"
                          >
                            转为静态
                          </button>
                        </Show>
                      </td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
        </Show>
      </section>

      {/* DHCP Static Leases */}
      <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
        <div class="mb-3 flex items-center justify-between">
          <h2 class="text-sm font-semibold text-slate-200">DHCP 静态地址绑定</h2>
          <button
            type="button"
            onClick={() => addStaticLease("", "192.168.10.50", "")}
            class="rounded-lg border border-slate-700 bg-slate-800 px-3 py-1 text-xs hover:bg-slate-700"
          >
            + 新增静态绑定
          </button>
        </div>
        <div class="space-y-2">
          <For each={primaryLan()?.static_leases ?? []}>
            {(lease, idx) => (
              <div class="grid items-center gap-2 rounded-lg border border-slate-800 bg-slate-950/60 p-3 text-xs sm:grid-cols-5">
                <input
                  type="text"
                  value={lease.name}
                  placeholder="备注名"
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.lans[0].static_leases[idx()].name = e.currentTarget.value;
                    props.onChange(next);
                  }}
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1"
                />
                <input
                  type="text"
                  value={lease.mac}
                  placeholder="aa:bb:cc:dd:ee:ff"
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.lans[0].static_leases[idx()].mac = e.currentTarget.value;
                    props.onChange(next);
                  }}
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono"
                />
                <input
                  type="text"
                  value={lease.ip}
                  placeholder="192.168.10.50"
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.lans[0].static_leases[idx()].ip = e.currentTarget.value;
                    props.onChange(next);
                  }}
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono"
                />
                <input
                  type="text"
                  value={lease.hostname ?? ""}
                  placeholder="主机名 (可选)"
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.lans[0].static_leases[idx()].hostname = e.currentTarget.value || null;
                    props.onChange(next);
                  }}
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono"
                />
                <button
                  type="button"
                  onClick={() => {
                    const next = structuredClone(props.draft);
                    next.lans[0].static_leases.splice(idx(), 1);
                    props.onChange(next);
                  }}
                  class="text-right text-red-400 hover:text-red-300"
                >
                  删除
                </button>
              </div>
            )}
          </For>
        </div>
      </section>

      {/* WireGuard VPN */}
      <section class="rounded-xl border border-slate-800 bg-slate-900/60 p-4">
        <div class="mb-3 flex flex-wrap items-center justify-between gap-2">
          <h2 class="text-sm font-semibold text-slate-200">WireGuard VPN & 客户端二维码</h2>
          <button
            type="button"
            disabled={busyKey()}
            onClick={() => void generatePeerWithQr()}
            class="rounded-lg bg-orange-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-orange-500 disabled:opacity-50"
          >
            {busyKey() ? "生成密钥中..." : "+ 新增 Peer 并生成客户端二维码"}
          </button>
        </div>

        <div class="mb-4 grid gap-3 rounded-xl border border-slate-800 bg-slate-950/60 p-3 text-xs sm:grid-cols-5">
          <label class="flex items-center gap-2 pt-4 text-slate-300">
            <input
              type="checkbox"
              checked={props.draft.wireguard.enabled}
              onChange={(e) => {
                const next = structuredClone(props.draft);
                next.wireguard.enabled = e.currentTarget.checked;
                props.onChange(next);
              }}
            />
            启用 WireGuard
          </label>
          <label class="block">
            <span class="text-slate-400">接口名</span>
            <input
              type="text"
              value={props.draft.wireguard.interface}
              onInput={(e) => {
                const next = structuredClone(props.draft);
                next.wireguard.interface = e.currentTarget.value;
                props.onChange(next);
              }}
              class="mt-1 w-full rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono"
            />
          </label>
          <label class="block">
            <span class="text-slate-400">UDP 监听端口</span>
            <input
              type="number"
              value={props.draft.wireguard.listen_port}
              onInput={(e) => {
                const next = structuredClone(props.draft);
                next.wireguard.listen_port = Number(e.currentTarget.value) || 51820;
                props.onChange(next);
              }}
              class="mt-1 w-full rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono"
            />
          </label>
          <label class="block">
            <span class="text-slate-400">隧道地址 CIDR</span>
            <input
              type="text"
              value={props.draft.wireguard.address_cidr}
              onInput={(e) => {
                const next = structuredClone(props.draft);
                next.wireguard.address_cidr = e.currentTarget.value;
                props.onChange(next);
              }}
              class="mt-1 w-full rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono"
            />
          </label>
          <label class="block">
            <span class="text-slate-400">公网域名/IP (用于二维码)</span>
            <input
              type="text"
              value={endpointHost()}
              onInput={(e) => setEndpointHost(e.currentTarget.value)}
              class="mt-1 w-full rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono"
            />
          </label>
        </div>

        <div class="space-y-2">
          <For each={props.draft.wireguard.peers}>
            {(peer, idx) => (
              <div class="grid items-center gap-2 rounded-lg border border-slate-800 bg-slate-950/60 p-3 text-xs sm:grid-cols-6">
                <input
                  type="text"
                  value={peer.name}
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.wireguard.peers[idx()].name = e.currentTarget.value;
                    props.onChange(next);
                  }}
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1"
                />
                <input
                  type="text"
                  value={peer.public_key}
                  placeholder="Peer PublicKey"
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.wireguard.peers[idx()].public_key = e.currentTarget.value;
                    props.onChange(next);
                  }}
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono sm:col-span-2"
                />
                <input
                  type="text"
                  value={peer.allowed_ips.join(", ")}
                  placeholder="10.10.0.2/32"
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.wireguard.peers[idx()].allowed_ips = e.currentTarget.value
                      .split(",")
                      .map((s) => s.trim())
                      .filter(Boolean);
                    props.onChange(next);
                  }}
                  class="rounded border border-slate-700 bg-slate-900 px-2 py-1 font-mono"
                />
                <button
                  type="button"
                  onClick={() => void showExistingPeerQr(peer)}
                  class="rounded border border-slate-700 bg-slate-800 px-2 py-1 hover:bg-slate-700"
                >
                  查看二维码
                </button>
                <button
                  type="button"
                  onClick={() => {
                    const next = structuredClone(props.draft);
                    next.wireguard.peers.splice(idx(), 1);
                    props.onChange(next);
                  }}
                  class="text-right text-red-400 hover:text-red-300"
                >
                  删除
                </button>
              </div>
            )}
          </For>
        </div>

        <Show when={qrDataUrl()}>
          {(url) => (
            <div class="mt-4 flex flex-wrap items-start gap-4 rounded-xl border border-orange-500/40 bg-slate-950 p-4">
              <img src={url()} alt="WireGuard QR" class="rounded-lg bg-white p-1" />
              <div class="flex-1 space-y-2">
                <div class="flex items-center justify-between">
                  <span class="text-xs font-semibold text-orange-300">
                    客户端配置 ({qrPeerName()})
                  </span>
                  <button
                    type="button"
                    onClick={() => setQrDataUrl(null)}
                    class="text-xs text-slate-400 hover:text-slate-200"
                  >
                    关闭
                  </button>
                </div>
                <pre class="overflow-auto rounded-lg border border-slate-800 bg-slate-900 p-3 font-mono text-xs text-slate-300">
                  {qrConfigText()}
                </pre>
              </div>
            </div>
          )}
        </Show>
      </section>
    </div>
  );
}

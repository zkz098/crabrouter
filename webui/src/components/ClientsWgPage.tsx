import QRCode from "qrcode";
import { For, Show, createSignal } from "solid-js";
import { generateWireGuardKeypair } from "../api";
import { t } from "../i18n";
import type { DhcpLeaseEntry } from "../types/DhcpLeaseEntry";
import type { RouterConfig } from "../types/RouterConfig";
import type { WireGuardPeer } from "../types/WireGuardPeer";

interface Props {
  draft: RouterConfig;
  clients: DhcpLeaseEntry[];
  onRefreshClients: () => void;
  onChange: (next: RouterConfig) => void;
}

const SECTION_CLS =
  "rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#0a0a0a] p-4 sm:p-5 transition-colors hover:border-[#d4d4d8] dark:hover:border-[#3a3a3a]";
const ROW_CLS =
  "rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-[#fafafa] dark:bg-[#111111] p-3 text-xs";
const INPUT_CLS =
  "w-full rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] px-2.5 py-1.5 text-xs text-[#171717] dark:text-[#ededed] outline-none focus:border-[#171717] dark:focus:border-[#ededed] transition-colors";
const BTN_OUTLINE =
  "rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#111111] hover:bg-[#f4f4f5] dark:hover:bg-[#1a1a1a] px-3 py-1.5 text-xs font-medium text-[#171717] dark:text-[#ededed] cursor-pointer transition-all active:scale-[0.97]";
const BTN_PRIMARY =
  "rounded-lg bg-[#171717] hover:bg-[#383838] dark:bg-[#ededed] dark:hover:bg-[#d4d4d8] px-3.5 py-1.5 text-xs font-medium text-white dark:text-[#0a0a0a] border-0 cursor-pointer transition-all active:scale-[0.97] disabled:opacity-50";
const BTN_DELETE =
  "text-xs font-medium text-rose-600 dark:text-rose-400 hover:text-rose-700 dark:hover:text-rose-300 bg-transparent border-0 cursor-pointer";

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
    <div class="space-y-5 sm:space-y-6 animate-view-enter">
      {/* Online Clients */}
      <section class={SECTION_CLS}>
        <div class="mb-3.5 flex items-center justify-between gap-2">
          <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
            {t("clOnlineTitle")}
          </h2>
          <button type="button" onClick={() => props.onRefreshClients()} class={BTN_OUTLINE}>
            {t("clRefreshList")}
          </button>
        </div>
        <Show
          when={props.clients.length > 0}
          fallback={<p class="text-xs text-[#666666] dark:text-[#a1a1aa]">{t("clNoClients")}</p>}
        >
          <div class="overflow-x-auto">
            <table class="w-full text-left text-xs">
              <thead class="border-b border-[#eaeaea] dark:border-[#262626] text-[#666666] dark:text-[#a1a1aa]">
                <tr>
                  <th class="py-2 pr-4 font-medium">{t("clColIp")}</th>
                  <th class="py-2 pr-4 font-medium">{t("clColMac")}</th>
                  <th class="py-2 pr-4 font-medium">{t("clColHostname")}</th>
                  <th class="py-2 pr-4 font-medium">{t("clColLeaseType")}</th>
                  <th class="py-2 text-right font-medium">{t("clColAction")}</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-[#eaeaea] dark:divide-[#262626]">
                <For each={props.clients}>
                  {(client) => (
                    <tr class="hover:bg-[#fafafa] dark:hover:bg-[#111111]/60 transition-colors">
                      <td class="py-2.5 pr-4 font-mono text-[#171717] dark:text-[#ededed]">
                        {client.ip}
                      </td>
                      <td class="py-2.5 pr-4 font-mono text-[#666666] dark:text-[#a1a1aa]">
                        {client.mac}
                      </td>
                      <td class="py-2.5 pr-4 text-[#171717] dark:text-[#ededed]">
                        {client.hostname}
                      </td>
                      <td class="py-2.5 pr-4">
                        <span
                          class={`rounded-full border px-2 py-0.5 text-[11px] font-medium ${
                            client.is_static
                              ? "border-sky-500/20 bg-sky-500/10 text-sky-700 dark:text-sky-400"
                              : "border-zinc-500/20 bg-zinc-500/10 text-zinc-600 dark:text-zinc-400"
                          }`}
                        >
                          {client.is_static ? t("clStatic") : t("clDynamic")}
                        </span>
                      </td>
                      <td class="py-2.5 text-right">
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
                            class="rounded-lg border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#111111] hover:bg-[#f4f4f5] dark:hover:bg-[#1a1a1a] px-2.5 py-1 text-xs font-medium text-[#171717] dark:text-[#ededed] cursor-pointer transition-colors"
                          >
                            {t("clMakeStatic")}
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
      <section class={SECTION_CLS}>
        <div class="mb-3.5 flex items-center justify-between gap-2">
          <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">
            {t("clStaticLeasesTitle")}
          </h2>
          <button
            type="button"
            onClick={() => addStaticLease("", "192.168.10.50", "")}
            class={BTN_OUTLINE}
          >
            {t("clAddStaticLease")}
          </button>
        </div>
        <div class="space-y-2.5">
          <For each={primaryLan()?.static_leases ?? []}>
            {(lease, idx) => (
              <div class={`grid items-center gap-2 sm:grid-cols-5 ${ROW_CLS}`}>
                <input
                  type="text"
                  value={lease.name}
                  placeholder={t("clNotePlaceholder")}
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.lans[0].static_leases[idx()].name = e.currentTarget.value;
                    props.onChange(next);
                  }}
                  class={INPUT_CLS}
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
                  class={`${INPUT_CLS} font-mono`}
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
                  class={`${INPUT_CLS} font-mono`}
                />
                <input
                  type="text"
                  value={lease.hostname ?? ""}
                  placeholder={t("clHostnameOptional")}
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.lans[0].static_leases[idx()].hostname = e.currentTarget.value || null;
                    props.onChange(next);
                  }}
                  class={`${INPUT_CLS} font-mono`}
                />
                <button
                  type="button"
                  onClick={() => {
                    const next = structuredClone(props.draft);
                    next.lans[0].static_leases.splice(idx(), 1);
                    props.onChange(next);
                  }}
                  class={`text-right ${BTN_DELETE}`}
                >
                  {t("commonDelete")}
                </button>
              </div>
            )}
          </For>
        </div>
      </section>

      {/* WireGuard VPN */}
      <section class={SECTION_CLS}>
        <div class="mb-3.5 flex flex-wrap items-center justify-between gap-2">
          <h2 class="text-sm font-medium text-[#171717] dark:text-[#ededed]">{t("clWgTitle")}</h2>
          <button
            type="button"
            disabled={busyKey()}
            onClick={() => void generatePeerWithQr()}
            class={BTN_PRIMARY}
          >
            {busyKey() ? t("clWgGenerating") : t("clWgAddPeer")}
          </button>
        </div>

        <div class={`mb-4 grid gap-3 sm:grid-cols-5 ${ROW_CLS}`}>
          <label class="flex items-center gap-2 pt-4 text-[#171717] dark:text-[#ededed]">
            <input
              type="checkbox"
              checked={props.draft.wireguard.enabled}
              onChange={(e) => {
                const next = structuredClone(props.draft);
                next.wireguard.enabled = e.currentTarget.checked;
                props.onChange(next);
              }}
            />
            {t("clWgEnable")}
          </label>
          <label class="block">
            <span class="text-[#666666] dark:text-[#a1a1aa]">{t("clWgIface")}</span>
            <input
              type="text"
              value={props.draft.wireguard.interface}
              onInput={(e) => {
                const next = structuredClone(props.draft);
                next.wireguard.interface = e.currentTarget.value;
                props.onChange(next);
              }}
              class={`mt-1 ${INPUT_CLS} font-mono`}
            />
          </label>
          <label class="block">
            <span class="text-[#666666] dark:text-[#a1a1aa]">{t("clWgPort")}</span>
            <input
              type="number"
              value={props.draft.wireguard.listen_port}
              onInput={(e) => {
                const next = structuredClone(props.draft);
                next.wireguard.listen_port = Number(e.currentTarget.value) || 51820;
                props.onChange(next);
              }}
              class={`mt-1 ${INPUT_CLS} font-mono`}
            />
          </label>
          <label class="block">
            <span class="text-[#666666] dark:text-[#a1a1aa]">{t("clWgCidr")}</span>
            <input
              type="text"
              value={props.draft.wireguard.address_cidr}
              onInput={(e) => {
                const next = structuredClone(props.draft);
                next.wireguard.address_cidr = e.currentTarget.value;
                props.onChange(next);
              }}
              class={`mt-1 ${INPUT_CLS} font-mono`}
            />
          </label>
          <label class="block">
            <span class="text-[#666666] dark:text-[#a1a1aa]">{t("clWgEndpoint")}</span>
            <input
              type="text"
              value={endpointHost()}
              onInput={(e) => setEndpointHost(e.currentTarget.value)}
              class={`mt-1 ${INPUT_CLS} font-mono`}
            />
          </label>
        </div>

        <div class="space-y-2.5">
          <For each={props.draft.wireguard.peers}>
            {(peer, idx) => (
              <div class={`grid items-center gap-2 sm:grid-cols-6 ${ROW_CLS}`}>
                <input
                  type="text"
                  value={peer.name}
                  onInput={(e) => {
                    const next = structuredClone(props.draft);
                    next.wireguard.peers[idx()].name = e.currentTarget.value;
                    props.onChange(next);
                  }}
                  class={INPUT_CLS}
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
                  class={`${INPUT_CLS} font-mono sm:col-span-2`}
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
                  class={`${INPUT_CLS} font-mono`}
                />
                <button
                  type="button"
                  onClick={() => void showExistingPeerQr(peer)}
                  class={BTN_OUTLINE}
                >
                  {t("clWgShowQr")}
                </button>
                <button
                  type="button"
                  onClick={() => {
                    const next = structuredClone(props.draft);
                    next.wireguard.peers.splice(idx(), 1);
                    props.onChange(next);
                  }}
                  class={`text-right ${BTN_DELETE}`}
                >
                  {t("commonDelete")}
                </button>
              </div>
            )}
          </For>
        </div>

        <Show when={qrDataUrl()}>
          {(url) => (
            <div class="mt-4 flex flex-wrap items-start gap-4 rounded-xl border border-[#0070f3]/30 bg-[#fafafa] dark:bg-[#111111] p-4 animate-card-enter">
              <img
                src={url()}
                alt="WireGuard QR"
                class="rounded-lg bg-white p-1.5 border border-[#eaeaea]"
              />
              <div class="flex-1 space-y-2 min-w-56">
                <div class="flex items-center justify-between">
                  <span class="text-xs font-medium text-[#0070f3] dark:text-sky-400">
                    {t("clWgClientConfig")} ({qrPeerName()})
                  </span>
                  <button
                    type="button"
                    onClick={() => setQrDataUrl(null)}
                    class="text-xs text-[#666666] dark:text-[#a1a1aa] hover:text-[#171717] dark:hover:text-[#ededed] bg-transparent border-0 cursor-pointer"
                  >
                    {t("commonClose")}
                  </button>
                </div>
                <pre class="overflow-auto rounded-xl border border-[#eaeaea] dark:border-[#262626] bg-white dark:bg-[#000000] p-3 font-mono text-xs text-[#171717] dark:text-[#ededed]">
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

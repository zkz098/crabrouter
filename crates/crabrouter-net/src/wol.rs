use std::net::IpAddr;

use crabrouter_model::{WolRequest, WolResult, validate_wol_mac};
use tokio::net::UdpSocket;

use crate::{NetError, Result};

/// Parses a normalized MAC string (`aa:bb:cc:dd:ee:ff`) into 6 raw bytes.
pub fn parse_mac_bytes(mac: &str) -> Result<[u8; 6]> {
    let normalized = validate_wol_mac(mac).map_err(|e| NetError::Apply(e.to_string()))?;
    let mut out = [0u8; 6];
    for (idx, part) in normalized.split(':').enumerate() {
        if idx >= 6 {
            return Err(NetError::Apply(format!("invalid MAC '{mac}'")));
        }
        out[idx] = u8::from_str_radix(part, 16)
            .map_err(|_| NetError::Apply(format!("invalid MAC '{mac}'")))?;
    }
    Ok(out)
}

/// Builds a 102-byte Wake-on-LAN magic packet (`6 x 0xFF` + `16 x MAC`).
pub fn build_magic_packet(mac: [u8; 6]) -> [u8; 102] {
    let mut pkt = [0xFFu8; 102];
    for chunk in pkt[6..].as_chunks_mut::<6>().0 {
        chunk.copy_from_slice(&mac);
    }
    pkt
}

/// Sends a Wake-on-LAN UDP magic packet (or validates in dry-run mode).
pub async fn send_wol_packet(req: &WolRequest, dry_run: bool) -> Result<WolResult> {
    let normalized_mac = validate_wol_mac(&req.mac).map_err(|e| NetError::Apply(e.to_string()))?;
    let mac_bytes = parse_mac_bytes(&normalized_mac)?;
    let broadcast = req
        .broadcast_ip
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("255.255.255.255")
        .to_string();
    let addr: IpAddr = broadcast
        .parse()
        .map_err(|e| NetError::Apply(format!("invalid WOL broadcast IP '{broadcast}': {e}")))?;
    let port = req.port.unwrap_or(9);

    if dry_run {
        tracing::info!(mac = %normalized_mac, broadcast = %addr, port, "dry-run: Wake-on-LAN packet validated");
        return Ok(WolResult {
            ok: true,
            mac: normalized_mac.clone(),
            broadcast_ip: addr.to_string(),
            port,
            message: format!("dry-run: magic packet ready for {normalized_mac} -> {addr}:{port}"),
        });
    }

    let pkt = build_magic_packet(mac_bytes);
    let bind_addr = if addr.is_ipv6() {
        "[::]:0"
    } else {
        "0.0.0.0:0"
    };
    let sock = UdpSocket::bind(bind_addr).await?;
    sock.set_broadcast(true)?;
    sock.send_to(&pkt, (addr, port)).await?;
    tracing::info!(mac = %normalized_mac, broadcast = %addr, port, "sent Wake-on-LAN magic packet");

    Ok(WolResult {
        ok: true,
        mac: normalized_mac.clone(),
        broadcast_ip: addr.to_string(),
        port,
        message: format!("sent magic packet to {normalized_mac} via {addr}:{port}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_valid_102_byte_magic_packet() {
        let mac = parse_mac_bytes("AA-BB-CC-11-22-33").unwrap();
        let pkt = build_magic_packet(mac);
        assert_eq!(&pkt[..6], &[0xFF; 6]);
        for chunk in pkt[6..].as_chunks::<6>().0 {
            assert_eq!(chunk, &[0xAA, 0xBB, 0xCC, 0x11, 0x22, 0x33]);
        }
    }

    #[tokio::test]
    async fn dry_run_wol_validates_mac_and_broadcast() {
        let req = WolRequest {
            mac: "aa:bb:cc:11:22:33".to_string(),
            broadcast_ip: Some("192.168.10.255".to_string()),
            port: Some(9),
        };
        let res = send_wol_packet(&req, true).await.unwrap();
        assert!(res.ok);
        assert_eq!(res.mac, "aa:bb:cc:11:22:33");
        assert_eq!(res.broadcast_ip, "192.168.10.255");
        assert_eq!(res.port, 9);
    }
}

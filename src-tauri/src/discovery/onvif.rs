use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use tokio::net::UdpSocket;

const WS_DISCOVERY_ADDR: &str = "239.255.255.250:3702";

/// One ONVIF device found via WS-Discovery.
#[derive(Debug, Clone)]
pub struct OnvifMatch {
    pub ip: Ipv4Addr,
    pub xaddr: Option<String>,
}

fn probe_message(message_id: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<e:Envelope xmlns:e="http://www.w3.org/2003/05/soap-envelope" xmlns:w="http://schemas.xmlsoap.org/ws/2004/08/addressing" xmlns:d="http://schemas.xmlsoap.org/ws/2005/04/discovery" xmlns:dn="http://www.onvif.org/ver10/network/wsdl">
 <e:Header>
  <w:MessageID>uuid:{message_id}</w:MessageID>
  <w:To e:mustUnderstand="true">urn:schemas-xmlsoap-org:ws:2005:04:discovery</w:To>
  <w:Action e:mustUnderstand="true">http://schemas.xmlsoap.org/ws/2005/04/discovery/Probe</w:Action>
 </e:Header>
 <e:Body>
  <d:Probe>
   <d:Types>dn:NetworkVideoTransmitter</d:Types>
  </d:Probe>
 </e:Body>
</e:Envelope>"#
    )
}

/// Extract the first `XAddrs` service URL from a WS-Discovery reply, ignoring
/// XML namespace prefixes.
fn extract_xaddr(body: &str) -> Option<String> {
    let lower = body.to_ascii_lowercase();
    let tag = "xaddrs>";
    let start = lower.find(tag)? + tag.len();
    let end = lower[start..].find('<')? + start;
    body[start..end].split_whitespace().next().map(String::from)
}

/// Send a WS-Discovery probe and collect replies for `dwell`.
pub async fn discover(dwell: Duration) -> Result<Vec<OnvifMatch>, String> {
    let socket = UdpSocket::bind("0.0.0.0:0")
        .await
        .map_err(|e| format!("绑定 UDP 套接字失败: {e}"))?;

    let message_id = format!(
        "{:x}-{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
        std::process::id()
    );
    let probe = probe_message(&message_id);

    socket
        .send_to(probe.as_bytes(), WS_DISCOVERY_ADDR)
        .await
        .map_err(|e| format!("发送 WS-Discovery 探测失败: {e}"))?;

    let mut matches: Vec<OnvifMatch> = Vec::new();
    let mut buf = vec![0u8; 65535];
    let deadline = Instant::now() + dwell;

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, socket.recv_from(&mut buf)).await {
            Ok(Ok((n, from))) => {
                if let std::net::IpAddr::V4(ip) = from.ip() {
                    if matches.iter().any(|m| m.ip == ip) {
                        continue;
                    }
                    let body = String::from_utf8_lossy(&buf[..n]);
                    matches.push(OnvifMatch {
                        ip,
                        xaddr: extract_xaddr(&body),
                    });
                }
            }
            Ok(Err(_)) => break,
            Err(_) => break, // dwell elapsed
        }
    }

    Ok(matches)
}

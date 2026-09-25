use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

/// Returns true if a TCP connection to `ip:port` succeeds within `timeout_ms`.
pub async fn check_port(ip: Ipv4Addr, port: u16, timeout_ms: u64) -> bool {
    let addr = SocketAddr::new(IpAddr::V4(ip), port);
    matches!(
        timeout(Duration::from_millis(timeout_ms), TcpStream::connect(addr)).await,
        Ok(Ok(_))
    )
}

/// Best-effort grab of the HTTP `Server` header from `ip:port`.
pub async fn grab_http_server(ip: Ipv4Addr, port: u16, timeout_ms: u64) -> Option<String> {
    let addr = SocketAddr::new(IpAddr::V4(ip), port);
    let mut stream = timeout(Duration::from_millis(timeout_ms), TcpStream::connect(addr))
        .await
        .ok()?
        .ok()?;

    let req = format!(
        "HEAD / HTTP/1.0\r\nHost: {ip}\r\nUser-Agent: camera-scanner\r\nConnection: close\r\n\r\n"
    );
    timeout(
        Duration::from_millis(timeout_ms),
        stream.write_all(req.as_bytes()),
    )
    .await
    .ok()?
    .ok()?;

    let mut buf = vec![0u8; 2048];
    let n = timeout(Duration::from_millis(timeout_ms), stream.read(&mut buf))
        .await
        .ok()?
        .ok()?;
    if n == 0 {
        return None;
    }

    let text = String::from_utf8_lossy(&buf[..n]);
    for line in text.lines() {
        if let Some(rest) = line
            .to_ascii_lowercase()
            .strip_prefix("server:")
            .map(|_| line["server:".len()..].trim().to_string())
        {
            if !rest.is_empty() {
                return Some(rest);
            }
        }
    }
    None
}

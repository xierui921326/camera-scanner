use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use mdns_sd::{ServiceDaemon, ServiceEvent};

/// One service instance found via mDNS/DNS-SD.
#[derive(Debug, Clone)]
pub struct MdnsMatch {
    pub ip: Ipv4Addr,
    pub port: u16,
    pub service: String,
    #[allow(dead_code)]
    pub hostname: String,
}

/// Service types commonly advertised by IP cameras / NVRs.
const SERVICES: &[&str] = &[
    "_rtsp._tcp.local.",
    "_http._tcp.local.",
    "_onvif._tcp.local.",
];

/// Browse the local network for the services above for `dwell`.
/// This is blocking (uses `recv_timeout`) and should be run off the async
/// runtime via `spawn_blocking`.
pub fn discover(dwell: Duration) -> Result<Vec<MdnsMatch>, String> {
    let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;

    let mut receivers = Vec::new();
    for &svc in SERVICES {
        if let Ok(rx) = daemon.browse(svc) {
            receivers.push((svc.to_string(), rx));
        }
    }

    let mut found: HashMap<(Ipv4Addr, u16), MdnsMatch> = HashMap::new();
    let deadline = Instant::now() + dwell;

    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let step = Duration::from_millis(100).min(remaining);
        for (svc, rx) in &receivers {
            if let Ok(ServiceEvent::ServiceResolved(info)) = rx.recv_timeout(step) {
                let port = info.get_port();
                let hostname = info.get_hostname().to_string();
                for ip in info.get_addresses_v4() {
                    let ip = *ip;
                    found.entry((ip, port)).or_insert_with(|| MdnsMatch {
                        ip,
                        port,
                        service: svc.clone(),
                        hostname: hostname.clone(),
                    });
                }
            }
        }
    }

    let _ = daemon.shutdown();
    Ok(found.into_values().collect())
}

/// Map an mDNS service type to a coarse protocol label.
pub fn protocol_for_service(service: &str) -> &'static str {
    if service.contains("rtsp") {
        "RTSP"
    } else if service.contains("onvif") {
        "ONVIF"
    } else {
        "HTTP"
    }
}

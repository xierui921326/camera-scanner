use serde::{Deserialize, Serialize};

/// A discovered device on the (authorized) network.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub ip: String,
    pub open_ports: Vec<u16>,
    /// Inferred protocols, e.g. "RTSP", "HTTP", "HTTPS", "ONVIF", "Dahua".
    pub protocols: Vec<String>,
    /// Discovery source: "portscan" | "onvif".
    pub source: String,
    /// ONVIF service address when discovered via WS-Discovery.
    pub onvif_xaddr: Option<String>,
    /// `Server` header grabbed from an HTTP port, best-effort.
    pub http_server: Option<String>,
    /// Unix epoch milliseconds when the device was discovered.
    pub discovered_at: i64,
}

/// Options passed from the frontend to start a scan.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanOptions {
    /// A single IPv4 (`192.168.1.10`) or CIDR (`192.168.1.0/24`).
    pub cidr: String,
    /// Ports to probe; empty means use the built-in default set.
    #[serde(default)]
    pub ports: Vec<u16>,
    /// Max concurrent hosts scanned in parallel.
    #[serde(default)]
    pub rate_limit: usize,
    /// Per-connection timeout in milliseconds.
    #[serde(default)]
    pub timeout_ms: u64,
    /// Whether to run ONVIF WS-Discovery in addition to the port scan.
    #[serde(default)]
    pub onvif: bool,
    /// Whether to run mDNS/DNS-SD discovery in addition to the port scan.
    #[serde(default)]
    pub mdns: bool,
}

/// Emitted on `scan://progress`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub task_id: u64,
    pub scanned: usize,
    pub total: usize,
}

/// Emitted on `scan://finished`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanFinished {
    pub task_id: u64,
    pub found: usize,
    pub cancelled: bool,
}

pub const DEFAULT_PORTS: &[u16] = &[80, 443, 554, 8000, 8080, 8899, 37777];
pub const DEFAULT_RATE_LIMIT: usize = 256;
pub const DEFAULT_TIMEOUT_MS: u64 = 800;
/// Guardrail: reject scans that would enumerate more hosts than this.
pub const MAX_HOSTS: usize = 8192;

pub fn protocol_for_port(port: u16) -> Option<&'static str> {
    match port {
        554 => Some("RTSP"),
        80 | 8080 => Some("HTTP"),
        8000 => Some("HTTP"),
        443 => Some("HTTPS"),
        8899 => Some("ONVIF"),
        37777 => Some("Dahua"),
        _ => None,
    }
}

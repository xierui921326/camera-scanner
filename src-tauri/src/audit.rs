use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

const RULES_JSON: &str = include_str!("../rules/camera_cves.json");

/// A vulnerability rule loaded from the embedded rule library.
#[derive(Debug, Clone, Deserialize)]
struct VulnRule {
    id: String,
    vendor: String,
    cve: String,
    title: String,
    severity: String,
    description: String,
    advice: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub id: String,
    pub cve: String,
    pub title: String,
    pub severity: String,
    pub description: String,
    pub advice: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeakCred {
    pub scheme: String,
    pub username: String,
    pub password: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditResult {
    pub ip: String,
    pub http_port: Option<u16>,
    pub vendor: Option<String>,
    pub realm: Option<String>,
    pub http_server: Option<String>,
    pub findings: Vec<Finding>,
    pub weak_credentials: Vec<WeakCred>,
    pub notes: Vec<String>,
}

struct HttpResp {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl HttpResp {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// Minimal, conservative default-credential list for authorized audits.
const DEFAULT_CREDS: &[(&str, &str)] = &[
    ("admin", "admin"),
    ("admin", "12345"),
    ("admin", ""),
    ("admin", "123456"),
    ("admin", "admin12345"),
    ("root", "root"),
    ("admin", "password"),
];

fn md5_hex(data: &str) -> String {
    let mut h = Md5::new();
    h.update(data.as_bytes());
    let out = h.finalize();
    let mut s = String::with_capacity(32);
    for b in out {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

async fn http_request(
    ip: Ipv4Addr,
    port: u16,
    method: &str,
    path: &str,
    extra_headers: &[(String, String)],
    timeout_ms: u64,
) -> Result<HttpResp, String> {
    let dur = Duration::from_millis(timeout_ms);
    let addr = SocketAddr::new(IpAddr::V4(ip), port);
    let mut stream = timeout(dur, TcpStream::connect(addr))
        .await
        .map_err(|_| "连接超时".to_string())?
        .map_err(|e| e.to_string())?;

    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {ip}\r\nUser-Agent: camera-scanner\r\nAccept: */*\r\nConnection: close\r\n"
    );
    for (k, v) in extra_headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");

    timeout(dur, stream.write_all(req.as_bytes()))
        .await
        .map_err(|_| "写超时".to_string())?
        .map_err(|e| e.to_string())?;

    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        match timeout(dur, stream.read(&mut tmp)).await {
            Ok(Ok(0)) => break,
            Ok(Ok(n)) => {
                buf.extend_from_slice(&tmp[..n]);
                if buf.len() > 65536 {
                    break;
                }
            }
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }

    parse_response(&buf)
}

fn parse_response(buf: &[u8]) -> Result<HttpResp, String> {
    let sep = buf
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("HTTP 响应不完整")?;
    let head = String::from_utf8_lossy(&buf[..sep]);
    let body = String::from_utf8_lossy(&buf[sep + 4..]).to_string();

    let mut lines = head.lines();
    let status_line = lines.next().ok_or("缺少状态行")?;
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or("无法解析状态码")?;

    let mut headers = Vec::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }

    Ok(HttpResp {
        status,
        headers,
        body,
    })
}

/// Parse a `WWW-Authenticate` header into (scheme, params).
fn parse_www_auth(header: &str) -> (String, HashMap<String, String>) {
    let header = header.trim();
    let (scheme, rest) = header.split_once(' ').unwrap_or((header, ""));
    let mut params = HashMap::new();

    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // key
        let key_start = i;
        while i < bytes.len() && bytes[i] != b'=' {
            i += 1;
        }
        let key = rest[key_start..i].trim().trim_matches(',').trim().to_string();
        if i >= bytes.len() {
            break;
        }
        i += 1; // skip '='
        // value (quoted or bare)
        let value = if i < bytes.len() && bytes[i] == b'"' {
            i += 1;
            let vs = i;
            while i < bytes.len() && bytes[i] != b'"' {
                i += 1;
            }
            let v = rest[vs..i].to_string();
            if i < bytes.len() {
                i += 1; // skip closing quote
            }
            v
        } else {
            let vs = i;
            while i < bytes.len() && bytes[i] != b',' {
                i += 1;
            }
            rest[vs..i].trim().to_string()
        };
        if !key.is_empty() {
            params.insert(key.to_ascii_lowercase(), value);
        }
        // skip separators
        while i < bytes.len() && (bytes[i] == b',' || bytes[i] == b' ') {
            i += 1;
        }
    }

    (scheme.to_string(), params)
}

fn digest_auth_header(
    user: &str,
    pass: &str,
    method: &str,
    uri: &str,
    params: &HashMap<String, String>,
) -> String {
    let realm = params.get("realm").cloned().unwrap_or_default();
    let nonce = params.get("nonce").cloned().unwrap_or_default();
    let ha1 = md5_hex(&format!("{user}:{realm}:{pass}"));
    let ha2 = md5_hex(&format!("{method}:{uri}"));

    let (response, extra) = if let Some(qop_raw) = params.get("qop") {
        let qop = qop_raw.split(',').next().unwrap_or("auth").trim().to_string();
        let nc = "00000001";
        let cnonce = format!(
            "{:x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let resp = md5_hex(&format!("{ha1}:{nonce}:{nc}:{cnonce}:{qop}:{ha2}"));
        (resp, format!(", qop={qop}, nc={nc}, cnonce=\"{cnonce}\""))
    } else {
        (md5_hex(&format!("{ha1}:{nonce}:{ha2}")), String::new())
    };

    let mut s = format!(
        "Digest username=\"{user}\", realm=\"{realm}\", nonce=\"{nonce}\", uri=\"{uri}\", response=\"{response}\""
    );
    if let Some(o) = params.get("opaque") {
        s.push_str(&format!(", opaque=\"{o}\""));
    }
    if let Some(a) = params.get("algorithm") {
        s.push_str(&format!(", algorithm={a}"));
    }
    s.push_str(&extra);
    s
}

fn detect_vendor(haystack: &str) -> Option<String> {
    let h = haystack.to_ascii_lowercase();
    let table = [
        ("hikvision", "hikvision"),
        ("dvrdvs", "hikvision"),
        ("dnvrs-webs", "hikvision"),
        ("dahua", "dahua"),
        ("uniview", "uniview"),
        ("d-link", "dlink"),
        ("dlink", "dlink"),
        ("axis", "axis"),
    ];
    for (needle, vendor) in table {
        if h.contains(needle) {
            return Some(vendor.to_string());
        }
    }
    None
}

fn match_rules(vendor: Option<&str>) -> Vec<Finding> {
    let rules: Vec<VulnRule> = serde_json::from_str(RULES_JSON).unwrap_or_default();
    rules
        .into_iter()
        .filter(|r| {
            r.vendor == "*" || vendor.map(|v| v == r.vendor).unwrap_or(false)
        })
        .map(|r| Finding {
            id: r.id,
            cve: r.cve,
            title: r.title,
            severity: r.severity,
            description: r.description,
            advice: r.advice,
        })
        .collect()
}

fn pick_http_port(ports: &[u16]) -> Option<u16> {
    for candidate in [80u16, 8000, 8080] {
        if ports.contains(&candidate) {
            return Some(candidate);
        }
    }
    None
}

async fn weak_cred_check(
    ip: Ipv4Addr,
    port: u16,
    timeout_ms: u64,
) -> (Vec<WeakCred>, Vec<String>) {
    let path = "/";
    let mut notes = Vec::new();

    let first = match http_request(ip, port, "GET", path, &[], timeout_ms).await {
        Ok(r) => r,
        Err(e) => {
            notes.push(format!("弱口令核查请求失败: {e}"));
            return (Vec::new(), notes);
        }
    };

    if first.status != 401 {
        if first.status == 200 {
            notes.push("Web 根路径无需认证即可访问（存在未授权访问风险）".to_string());
        } else {
            notes.push(format!("Web 未返回 401（状态 {}），跳过弱口令核查", first.status));
        }
        return (Vec::new(), notes);
    }

    let www = first.header("www-authenticate").unwrap_or("").to_string();
    if www.is_empty() {
        notes.push("收到 401 但缺少 WWW-Authenticate 头，跳过弱口令核查".to_string());
        return (Vec::new(), notes);
    }
    let (scheme, params) = parse_www_auth(&www);
    let scheme_lc = scheme.to_ascii_lowercase();

    let mut found = Vec::new();
    for (user, pass) in DEFAULT_CREDS {
        let auth = if scheme_lc == "digest" {
            digest_auth_header(user, pass, "GET", path, &params)
        } else {
            format!("Basic {}", base64(format!("{user}:{pass}").as_bytes()))
        };

        if let Ok(r) =
            http_request(ip, port, "GET", path, &[("Authorization".into(), auth)], timeout_ms)
                .await
        {
            if r.status == 200 {
                found.push(WeakCred {
                    scheme: scheme.clone(),
                    username: (*user).to_string(),
                    password: (*pass).to_string(),
                    path: path.to_string(),
                });
                break; // stop on first success to avoid lockouts
            }
        }
        // Conservative pacing between attempts.
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    if found.is_empty() {
        notes.push("未命中内置默认口令字典".to_string());
    }

    (found, notes)
}

/// Run an HTTP fingerprint + rule match, optionally probing default credentials.
pub async fn audit(
    ip: Ipv4Addr,
    ports: &[u16],
    try_weak_creds: bool,
    timeout_ms: u64,
) -> AuditResult {
    let mut notes = Vec::new();
    let http_port = pick_http_port(ports);

    if ports.contains(&443) {
        notes.push("检测到 443/HTTPS，本工具当前不解析 TLS，未对其做指纹/口令核查".to_string());
    }

    let mut vendor = None;
    let mut realm = None;
    let mut http_server = None;

    if let Some(port) = http_port {
        match http_request(ip, port, "GET", "/", &[], timeout_ms).await {
            Ok(resp) => {
                http_server = resp.header("server").map(str::to_string);
                let www = resp.header("www-authenticate").unwrap_or("").to_string();
                if !www.is_empty() {
                    let (_scheme, params) = parse_www_auth(&www);
                    realm = params.get("realm").cloned();
                }
                let body_snippet: String = resp.body.chars().take(4096).collect();
                let haystack = format!(
                    "{} {} {}",
                    http_server.clone().unwrap_or_default(),
                    www,
                    body_snippet
                );
                vendor = detect_vendor(&haystack);
            }
            Err(e) => notes.push(format!("指纹请求失败: {e}")),
        }
    } else {
        notes.push("未发现 80/8000/8080 等明文 HTTP 端口，指纹与弱口令核查受限".to_string());
    }

    let findings = match_rules(vendor.as_deref());

    let weak_credentials = if try_weak_creds {
        if let Some(port) = http_port {
            let (creds, mut cnotes) = weak_cred_check(ip, port, timeout_ms).await;
            notes.append(&mut cnotes);
            creds
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    AuditResult {
        ip: ip.to_string(),
        http_port,
        vendor,
        realm,
        http_server,
        findings,
        weak_credentials,
        notes,
    }
}

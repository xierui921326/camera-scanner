use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Semaphore;

use crate::audit::{self, AuditResult};
use crate::discovery::{mdns, onvif, portscan};
use crate::model::{
    protocol_for_port, Device, ScanFinished, ScanOptions, ScanProgress, DEFAULT_PORTS,
    DEFAULT_RATE_LIMIT, DEFAULT_TIMEOUT_MS,
};
use crate::safety;
use crate::state::{ScanState, StreamKind, StreamSession};
use crate::storage::{self, AuditLogEntry};
use crate::stream::{self, StreamInfo, StreamOpenArgs, MAX_STREAMS};

fn log_action(
    state: &ScanState,
    action: &str,
    target: Option<&str>,
    detail: Option<&str>,
    ok: bool,
) {
    if let Some(conn) = state.db.lock().unwrap().as_ref() {
        let _ = storage::audit_log_insert(conn, action, target, detail, ok);
    }
}

fn ensure_consent(state: &ScanState) -> Result<(), String> {
    let ok = state
        .db
        .lock()
        .unwrap()
        .as_ref()
        .map(storage::consent_accepted)
        .unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err("请先确认授权声明后再使用扫描/审计/预览功能".into())
    }
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Insert or merge a device into the task's result set, persist it, and emit
/// `scan://device`.
fn store_and_emit(app: &AppHandle, task_id: u64, device: Device) {
    let emit_device = {
        let st = app.state::<ScanState>();
        let mut map = st.devices.lock().unwrap();
        let list = map.entry(task_id).or_default();
        if let Some(existing) = list.iter_mut().find(|d| d.ip == device.ip) {
            for p in device.open_ports {
                if !existing.open_ports.contains(&p) {
                    existing.open_ports.push(p);
                }
            }
            existing.open_ports.sort_unstable();
            for pr in device.protocols {
                if !existing.protocols.contains(&pr) {
                    existing.protocols.push(pr);
                }
            }
            if existing.onvif_xaddr.is_none() {
                existing.onvif_xaddr = device.onvif_xaddr;
            }
            if existing.http_server.is_none() {
                existing.http_server = device.http_server;
            }
            if device.source == "onvif" && !existing.source.contains("onvif") {
                existing.source = format!("{}+onvif", existing.source);
            } else if device.source == "mdns" && !existing.source.contains("mdns") {
                existing.source = format!("{}+mdns", existing.source);
            }
            existing.clone()
        } else {
            list.push(device.clone());
            device
        }
    };

    if let Some(conn) = app.state::<ScanState>().db.lock().unwrap().as_ref() {
        let _ = storage::device_upsert(conn, task_id, &emit_device);
    }

    let _ = app.emit("scan://device", emit_device);
}

#[allow(clippy::too_many_arguments)]
async fn run_scan(
    app: AppHandle,
    task_id: u64,
    hosts: Vec<Ipv4Addr>,
    ports: Vec<u16>,
    rate_limit: usize,
    timeout_ms: u64,
    onvif_enabled: bool,
    mdns_enabled: bool,
    cancel: Arc<AtomicBool>,
) {
    let total = hosts.len();
    let scanned = Arc::new(AtomicUsize::new(0));

    if onvif_enabled && !cancel.load(Ordering::Relaxed) {
        if let Ok(found) = onvif::discover(Duration::from_secs(3)).await {
            for m in found {
                let device = Device {
                    ip: m.ip.to_string(),
                    open_ports: vec![],
                    protocols: vec!["ONVIF".into()],
                    source: "onvif".into(),
                    onvif_xaddr: m.xaddr,
                    http_server: None,
                    discovered_at: now_millis(),
                };
                store_and_emit(&app, task_id, device);
            }
        }
    }

    if mdns_enabled && !cancel.load(Ordering::Relaxed) {
        let dwell = Duration::from_secs(3);
        if let Ok(Ok(found)) =
            tokio::task::spawn_blocking(move || mdns::discover(dwell)).await
        {
            for m in found {
                let device = Device {
                    ip: m.ip.to_string(),
                    open_ports: vec![m.port],
                    protocols: vec![mdns::protocol_for_service(&m.service).to_string()],
                    source: "mdns".into(),
                    onvif_xaddr: None,
                    http_server: None,
                    discovered_at: now_millis(),
                };
                store_and_emit(&app, task_id, device);
            }
        }
    }

    let sem = Arc::new(Semaphore::new(rate_limit.max(1)));
    let mut handles = Vec::with_capacity(hosts.len());

    for ip in hosts {
        let sem = sem.clone();
        let ports = ports.clone();
        let app = app.clone();
        let cancel = cancel.clone();
        let scanned = scanned.clone();

        let handle = tauri::async_runtime::spawn(async move {
            let _permit = match sem.acquire_owned().await {
                Ok(p) => p,
                Err(_) => return,
            };
            if cancel.load(Ordering::Relaxed) {
                return;
            }

            let mut open = Vec::new();
            for &port in &ports {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                if portscan::check_port(ip, port, timeout_ms).await {
                    open.push(port);
                }
            }

            let done = scanned.fetch_add(1, Ordering::Relaxed) + 1;
            if done % 16 == 0 || done == total {
                let _ = app.emit(
                    "scan://progress",
                    ScanProgress {
                        task_id,
                        scanned: done,
                        total,
                    },
                );
            }

            if !open.is_empty() {
                let mut protocols: Vec<String> = open
                    .iter()
                    .filter_map(|p| protocol_for_port(*p).map(str::to_string))
                    .collect();
                protocols.sort();
                protocols.dedup();

                let mut http_server = None;
                for &p in &open {
                    if matches!(p, 80 | 8000 | 8080) {
                        if let Some(s) = portscan::grab_http_server(ip, p, timeout_ms).await {
                            http_server = Some(s);
                            break;
                        }
                    }
                }

                let device = Device {
                    ip: ip.to_string(),
                    open_ports: open,
                    protocols,
                    source: "portscan".into(),
                    onvif_xaddr: None,
                    http_server,
                    discovered_at: now_millis(),
                };
                store_and_emit(&app, task_id, device);
            }
        });
        handles.push(handle);
    }

    for h in handles {
        let _ = h.await;
    }

    let cancelled = cancel.load(Ordering::Relaxed);
    let _ = app.emit(
        "scan://progress",
        ScanProgress {
            task_id,
            scanned: scanned.load(Ordering::Relaxed),
            total,
        },
    );

    let found = {
        let st = app.state::<ScanState>();
        let map = st.devices.lock().unwrap();
        map.get(&task_id).map(|v| v.len()).unwrap_or(0)
    };

    if let Some(conn) = app.state::<ScanState>().db.lock().unwrap().as_ref() {
        let _ = storage::task_finish(conn, task_id, now_millis(), found, cancelled);
    }
    app.state::<ScanState>()
        .cancel_flags
        .lock()
        .unwrap()
        .remove(&task_id);

    let _ = app.emit(
        "scan://finished",
        ScanFinished {
            task_id,
            found,
            cancelled,
        },
    );
}

#[tauri::command]
pub async fn scan_start(
    app: AppHandle,
    state: State<'_, ScanState>,
    opts: ScanOptions,
) -> Result<u64, String> {
    ensure_consent(&state)?;
    let hosts = safety::parse_targets(&opts.cidr)?;
    {
        let wl = state.whitelist.lock().unwrap();
        if let Err(e) = safety::ensure_authorized(&opts.cidr, &hosts, &wl) {
            log_action(
                &state,
                "scan_start",
                Some(&opts.cidr),
                Some(&e),
                false,
            );
            return Err(e);
        }
    }

    let task_id = state.next_id();
    let cancel = Arc::new(AtomicBool::new(false));
    state
        .cancel_flags
        .lock()
        .unwrap()
        .insert(task_id, cancel.clone());
    state.devices.lock().unwrap().insert(task_id, Vec::new());

    if let Some(conn) = state.db.lock().unwrap().as_ref() {
        let _ = storage::task_insert(conn, task_id, &opts.cidr, now_millis());
    }

    let ports = if opts.ports.is_empty() {
        DEFAULT_PORTS.to_vec()
    } else {
        opts.ports.clone()
    };
    let rate_limit = if opts.rate_limit == 0 {
        DEFAULT_RATE_LIMIT
    } else {
        opts.rate_limit
    };
    let timeout_ms = if opts.timeout_ms == 0 {
        DEFAULT_TIMEOUT_MS
    } else {
        opts.timeout_ms
    };
    let onvif_enabled = opts.onvif;
    let mdns_enabled = opts.mdns;
    let host_count = hosts.len();

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        run_scan(
            app_handle,
            task_id,
            hosts,
            ports,
            rate_limit,
            timeout_ms,
            onvif_enabled,
            mdns_enabled,
            cancel,
        )
        .await;
    });

    log_action(
        &state,
        "scan_start",
        Some(&opts.cidr),
        Some(&format!("task_id={task_id}, hosts={host_count}")),
        true,
    );
    Ok(task_id)
}

#[tauri::command]
pub fn scan_cancel(state: State<'_, ScanState>, task_id: u64) -> Result<(), String> {
    match state.cancel_flags.lock().unwrap().get(&task_id) {
        Some(flag) => {
            flag.store(true, Ordering::Relaxed);
            Ok(())
        }
        None => Err(format!("任务 {task_id} 不存在或已结束")),
    }
}

#[tauri::command]
pub fn device_list(state: State<'_, ScanState>, task_id: u64) -> Vec<Device> {
    state
        .devices
        .lock()
        .unwrap()
        .get(&task_id)
        .cloned()
        .unwrap_or_default()
}

#[tauri::command]
pub fn history_devices(state: State<'_, ScanState>) -> Vec<Device> {
    state
        .db
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|c| storage::devices_all(c).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub fn whitelist_list(state: State<'_, ScanState>) -> Vec<String> {
    state.whitelist.lock().unwrap().clone()
}

#[tauri::command]
pub fn whitelist_add(
    state: State<'_, ScanState>,
    target: String,
    confirm: bool,
) -> Result<Vec<String>, String> {
    ensure_consent(&state)?;
    if !confirm {
        log_action(
            &state,
            "whitelist_add",
            Some(&target),
            Some("missing confirm"),
            false,
        );
        return Err("添加非私有目标到白名单需显式确认已获授权".into());
    }
    if let Err(e) = safety::parse_targets(&target) {
        log_action(&state, "whitelist_add", Some(&target), Some(&e), false);
        return Err(e);
    }
    {
        let mut wl = state.whitelist.lock().unwrap();
        if !wl.contains(&target) {
            wl.push(target.clone());
        }
    }
    if let Some(conn) = state.db.lock().unwrap().as_ref() {
        let _ = storage::whitelist_insert(conn, &target);
    }
    log_action(&state, "whitelist_add", Some(&target), None, true);
    Ok(state.whitelist.lock().unwrap().clone())
}

#[tauri::command]
pub fn whitelist_remove(state: State<'_, ScanState>, target: String) -> Vec<String> {
    {
        let mut wl = state.whitelist.lock().unwrap();
        wl.retain(|t| t != &target);
    }
    if let Some(conn) = state.db.lock().unwrap().as_ref() {
        let _ = storage::whitelist_delete(conn, &target);
    }
    state.whitelist.lock().unwrap().clone()
}

fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn devices_to_csv(devices: &[Device]) -> String {
    let mut out =
        String::from("ip,open_ports,protocols,source,onvif_xaddr,http_server,discovered_at\n");
    for d in devices {
        let ports = d
            .open_ports
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(" ");
        out.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            csv_field(&d.ip),
            csv_field(&ports),
            csv_field(&d.protocols.join(" ")),
            csv_field(&d.source),
            csv_field(d.onvif_xaddr.as_deref().unwrap_or("")),
            csv_field(d.http_server.as_deref().unwrap_or("")),
            d.discovered_at
        ));
    }
    out
}

#[tauri::command]
pub fn results_export(
    app: AppHandle,
    state: State<'_, ScanState>,
    task_id: u64,
    fmt: String,
) -> Result<String, String> {
    let devices = state
        .devices
        .lock()
        .unwrap()
        .get(&task_id)
        .cloned()
        .unwrap_or_default();

    let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let dir = base.join("exports");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("scan-{task_id}.{fmt}"));

    let content = match fmt.as_str() {
        "json" => serde_json::to_string_pretty(&devices).map_err(|e| e.to_string())?,
        "csv" => devices_to_csv(&devices),
        other => return Err(format!("不支持的导出格式: {other}")),
    };
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

/// Reject preview of a non-private IPv4 host unless it is whitelisted.
fn authorize_host(state: &ScanState, host: &str) -> Result<(), String> {
    if host.is_empty() {
        return Ok(());
    }
    if let Ok(ip) = host.parse::<Ipv4Addr>() {
        if ip.is_private() || ip.is_loopback() {
            return Ok(());
        }
        let wl = state.whitelist.lock().unwrap();
        if wl.iter().any(|w| w == host || w.starts_with(host)) {
            return Ok(());
        }
        return Err(format!("目标 {host} 为非私有地址，需先加入授权白名单"));
    }
    Ok(())
}

#[tauri::command]
pub async fn audit_device(
    state: State<'_, ScanState>,
    ip: String,
    ports: Vec<u16>,
    try_weak_creds: bool,
    timeout_ms: Option<u64>,
) -> Result<AuditResult, String> {
    ensure_consent(&state)?;
    let addr: Ipv4Addr = ip
        .parse()
        .map_err(|_| format!("非法的 IPv4 地址: {ip}"))?;

    // Same authorization guard as scanning/preview.
    {
        let wl = state.whitelist.lock().unwrap();
        if !(addr.is_private() || addr.is_loopback())
            && !wl.iter().any(|w| w == &ip || w.starts_with(&ip))
        {
            let err = format!("目标 {ip} 为非私有地址，需先加入授权白名单");
            log_action(&state, "audit_device", Some(&ip), Some(&err), false);
            return Err(err);
        }
    }

    let timeout_ms = timeout_ms.unwrap_or(2000);
    let result = audit::audit(addr, &ports, try_weak_creds, timeout_ms).await;
    if let Some(conn) = state.db.lock().unwrap().as_ref() {
        let _ = storage::audit_result_upsert(conn, &result);
    }
    log_action(
        &state,
        "audit_device",
        Some(&ip),
        Some(&format!(
            "findings={}, weak_creds={}, try_weak={try_weak_creds}",
            result.findings.len(),
            result.weak_credentials.len()
        )),
        true,
    );
    Ok(result)
}

#[tauri::command]
pub async fn stream_open(
    app: AppHandle,
    state: State<'_, ScanState>,
    opts: StreamOpenArgs,
) -> Result<StreamInfo, String> {
    ensure_consent(&state)?;
    let (rtsp_url, host) = opts.resolve()?;
    if let Err(e) = authorize_host(&state, &host) {
        log_action(&state, "stream_open", Some(&host), Some(&e), false);
        return Err(e);
    }

    {
        let streams = state.streams.lock().unwrap();
        if streams.len() >= MAX_STREAMS {
            return Err(format!(
                "同时预览路数已达上限 {MAX_STREAMS}，请先关闭一路再添加"
            ));
        }
    }

    let root = {
        let mut guard = state.streams_root.lock().unwrap();
        if guard.is_none() {
            let base = app
                .path()
                .app_data_dir()
                .map_err(|e| e.to_string())?
                .join("streams");
            std::fs::create_dir_all(&base).map_err(|e| e.to_string())?;
            *guard = Some(base);
        }
        guard.clone().unwrap()
    };

    let port = {
        let mut guard = state.media_port.lock().unwrap();
        if guard.is_none() {
            *guard = Some(stream::start_media_server(root.clone())?);
        }
        guard.unwrap()
    };

    let mode = opts.mode().to_string();
    let label = opts.label_or_default();
    let session_id = format!("s{}", state.next_id());
    let dir = root.join(&session_id);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let (kind, url, manifest_url) = match mode.as_str() {
        "retina" => {
            let cancel = Arc::new(AtomicBool::new(false));
            stream::retina_mse::spawn_retina_remux(rtsp_url, dir.clone(), cancel.clone())?;
            let url = format!("http://127.0.0.1:{port}/{session_id}/manifest.json");
            (
                StreamKind::Retina { cancel },
                url.clone(),
                Some(url),
            )
        }
        "mse" => {
            let child = stream::spawn_ffmpeg_fmp4_hls(&opts, &rtsp_url, &dir)?;
            let url = format!("http://127.0.0.1:{port}/{session_id}/index.m3u8");
            (StreamKind::Ffmpeg { child }, url, None)
        }
        _ => {
            let child = stream::spawn_ffmpeg_hls(&opts, &rtsp_url, &dir)?;
            let url = format!("http://127.0.0.1:{port}/{session_id}/index.m3u8");
            (StreamKind::Ffmpeg { child }, url, None)
        }
    };

    let info = StreamInfo {
        session_id: session_id.clone(),
        url: url.clone(),
        mode: mode.clone(),
        label: label.clone(),
        manifest_url: manifest_url.clone(),
    };

    state.streams.lock().unwrap().insert(
        session_id,
        StreamSession {
            kind,
            dir,
            mode,
            label,
            url,
            manifest_url,
        },
    );

    log_action(
        &state,
        "stream_open",
        Some(&host),
        Some(&format!(
            "session={}, mode={}",
            info.session_id, info.mode
        )),
        true,
    );
    Ok(info)
}

#[tauri::command]
pub async fn stream_close(state: State<'_, ScanState>, session_id: String) -> Result<(), String> {
    let session = state.streams.lock().unwrap().remove(&session_id);
    match session {
        Some(mut s) => {
            match &mut s.kind {
                StreamKind::Ffmpeg { child } => {
                    let _ = child.kill().await;
                }
                StreamKind::Retina { cancel } => {
                    cancel.store(true, Ordering::Relaxed);
                    // Give the remux task a moment to exit before deleting files.
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }
            }
            let _ = std::fs::remove_dir_all(&s.dir);
            Ok(())
        }
        None => Err(format!("会话 {session_id} 不存在")),
    }
}

#[tauri::command]
pub fn stream_list(state: State<'_, ScanState>) -> Vec<StreamInfo> {
    state
        .streams
        .lock()
        .unwrap()
        .iter()
        .map(|(id, s)| StreamInfo {
            session_id: id.clone(),
            url: s.url.clone(),
            mode: s.mode.clone(),
            label: s.label.clone(),
            manifest_url: s.manifest_url.clone(),
        })
        .collect()
}

#[tauri::command]
pub fn consent_status(state: State<'_, ScanState>) -> bool {
    state
        .db
        .lock()
        .unwrap()
        .as_ref()
        .map(storage::consent_accepted)
        .unwrap_or(false)
}

#[tauri::command]
pub fn consent_accept(state: State<'_, ScanState>) -> Result<(), String> {
    {
        let guard = state.db.lock().unwrap();
        let conn = guard.as_ref().ok_or("数据库未初始化")?;
        storage::consent_accept(conn).map_err(|e| e.to_string())?;
    }
    log_action(&state, "consent_accept", None, None, true);
    Ok(())
}

#[tauri::command]
pub fn audit_log_list(state: State<'_, ScanState>, limit: Option<usize>) -> Vec<AuditLogEntry> {
    let limit = limit.unwrap_or(200).clamp(1, 1000);
    state
        .db
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|c| storage::audit_log_list(c, limit).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub fn report_generate(
    app: AppHandle,
    state: State<'_, ScanState>,
    fmt: String,
) -> Result<String, String> {
    ensure_consent(&state)?;
    let (devices, audits) = {
        let guard = state.db.lock().unwrap();
        let conn = guard.as_ref().ok_or("数据库未初始化")?;
        let devices = storage::devices_all(conn).unwrap_or_default();
        let audits = storage::audit_results_all(conn).unwrap_or_default();
        (devices, audits)
    };

    let content = match fmt.as_str() {
        "md" | "markdown" => crate::report::render_markdown(&devices, &audits),
        "html" => crate::report::render_html(&devices, &audits),
        other => return Err(format!("不支持的报告格式: {other}（支持 md / html）")),
    };

    let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let dir = base.join("exports");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let ext = if fmt == "html" { "html" } else { "md" };
    let path = dir.join(format!("hardening-report-{}.{}", now_millis(), ext));
    std::fs::write(&path, content).map_err(|e| e.to_string())?;

    let path_str = path.to_string_lossy().to_string();
    log_action(
        &state,
        "report_generate",
        Some(&path_str),
        Some(&format!(
            "fmt={ext}, devices={}, audits={}",
            devices.len(),
            audits.len()
        )),
        true,
    );
    Ok(path_str)
}

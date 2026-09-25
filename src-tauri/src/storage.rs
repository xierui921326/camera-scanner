use std::path::Path;

use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};

use crate::audit::AuditResult;
use crate::model::Device;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogEntry {
    pub id: i64,
    pub ts: i64,
    pub action: String,
    pub target: Option<String>,
    pub detail: Option<String>,
    pub ok: bool,
}

pub fn init(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS whitelist (
            target TEXT PRIMARY KEY
         );
         CREATE TABLE IF NOT EXISTS tasks (
            id INTEGER PRIMARY KEY,
            cidr TEXT NOT NULL,
            started_at INTEGER NOT NULL,
            finished_at INTEGER,
            found INTEGER,
            cancelled INTEGER
         );
         CREATE TABLE IF NOT EXISTS devices (
            task_id INTEGER NOT NULL,
            ip TEXT NOT NULL,
            open_ports TEXT NOT NULL,
            protocols TEXT NOT NULL,
            source TEXT NOT NULL,
            onvif_xaddr TEXT,
            http_server TEXT,
            discovered_at INTEGER NOT NULL,
            PRIMARY KEY (task_id, ip)
         );
         CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS audit_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ts INTEGER NOT NULL,
            action TEXT NOT NULL,
            target TEXT,
            detail TEXT,
            ok INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS audit_results (
            ip TEXT PRIMARY KEY,
            result_json TEXT NOT NULL,
            audited_at INTEGER NOT NULL
         );",
    )?;
    Ok(conn)
}

pub fn whitelist_load(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT target FROM whitelist ORDER BY target")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    rows.collect()
}

pub fn whitelist_insert(conn: &Connection, target: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO whitelist(target) VALUES (?1)",
        params![target],
    )?;
    Ok(())
}

pub fn whitelist_delete(conn: &Connection, target: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM whitelist WHERE target = ?1", params![target])?;
    Ok(())
}

pub fn task_insert(conn: &Connection, id: u64, cidr: &str, started_at: i64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO tasks(id, cidr, started_at) VALUES (?1, ?2, ?3)",
        params![id as i64, cidr, started_at],
    )?;
    Ok(())
}

pub fn task_finish(
    conn: &Connection,
    id: u64,
    finished_at: i64,
    found: usize,
    cancelled: bool,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE tasks SET finished_at=?2, found=?3, cancelled=?4 WHERE id=?1",
        params![id as i64, finished_at, found as i64, cancelled as i64],
    )?;
    Ok(())
}

pub fn device_upsert(conn: &Connection, task_id: u64, d: &Device) -> rusqlite::Result<()> {
    let open_ports = serde_json::to_string(&d.open_ports).unwrap_or_else(|_| "[]".into());
    let protocols = serde_json::to_string(&d.protocols).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "INSERT OR REPLACE INTO devices(
            task_id, ip, open_ports, protocols, source, onvif_xaddr, http_server, discovered_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            task_id as i64,
            d.ip,
            open_ports,
            protocols,
            d.source,
            d.onvif_xaddr,
            d.http_server,
            d.discovered_at
        ],
    )?;
    Ok(())
}

fn row_to_device(r: &Row) -> rusqlite::Result<Device> {
    let open_ports: String = r.get("open_ports")?;
    let protocols: String = r.get("protocols")?;
    Ok(Device {
        ip: r.get("ip")?,
        open_ports: serde_json::from_str(&open_ports).unwrap_or_default(),
        protocols: serde_json::from_str(&protocols).unwrap_or_default(),
        source: r.get("source")?,
        onvif_xaddr: r.get("onvif_xaddr")?,
        http_server: r.get("http_server")?,
        discovered_at: r.get("discovered_at")?,
    })
}

pub fn devices_all(conn: &Connection) -> rusqlite::Result<Vec<Device>> {
    let mut stmt = conn.prepare("SELECT * FROM devices ORDER BY discovered_at DESC")?;
    let rows = stmt.query_map([], row_to_device)?;
    rows.collect()
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn setting_get(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
    let mut rows = stmt.query(params![key])?;
    if let Some(row) = rows.next()? {
        Ok(Some(row.get(0)?))
    } else {
        Ok(None)
    }
}

pub fn setting_set(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO settings(key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}

pub fn consent_accepted(conn: &Connection) -> bool {
    setting_get(conn, "consent_accepted")
        .ok()
        .flatten()
        .as_deref()
        == Some("1")
}

pub fn consent_accept(conn: &Connection) -> rusqlite::Result<()> {
    setting_set(conn, "consent_accepted", "1")?;
    setting_set(conn, "consent_at", &now_millis().to_string())?;
    Ok(())
}

pub fn audit_log_insert(
    conn: &Connection,
    action: &str,
    target: Option<&str>,
    detail: Option<&str>,
    ok: bool,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO audit_log(ts, action, target, detail, ok) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![now_millis(), action, target, detail, ok as i64],
    )?;
    Ok(())
}

pub fn audit_log_list(conn: &Connection, limit: usize) -> rusqlite::Result<Vec<AuditLogEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, ts, action, target, detail, ok FROM audit_log ORDER BY id DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], |r| {
        Ok(AuditLogEntry {
            id: r.get(0)?,
            ts: r.get(1)?,
            action: r.get(2)?,
            target: r.get(3)?,
            detail: r.get(4)?,
            ok: r.get::<_, i64>(5)? != 0,
        })
    })?;
    rows.collect()
}

pub fn audit_result_upsert(conn: &Connection, result: &AuditResult) -> rusqlite::Result<()> {
    let json = serde_json::to_string(result).unwrap_or_else(|_| "{}".into());
    conn.execute(
        "INSERT OR REPLACE INTO audit_results(ip, result_json, audited_at) VALUES (?1, ?2, ?3)",
        params![result.ip, json, now_millis()],
    )?;
    Ok(())
}

pub fn audit_results_all(conn: &Connection) -> rusqlite::Result<Vec<AuditResult>> {
    let mut stmt =
        conn.prepare("SELECT result_json FROM audit_results ORDER BY audited_at DESC")?;
    let rows = stmt.query_map([], |r| {
        let json: String = r.get(0)?;
        Ok(serde_json::from_str::<AuditResult>(&json).ok())
    })?;
    let mut out = Vec::new();
    for row in rows {
        if let Some(r) = row? {
            out.push(r);
        }
    }
    Ok(out)
}

import { useEffect, useRef, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import DeviceTable from "../components/DeviceTable";
import AuditPanel from "../components/AuditPanel";
import type { AuditResult, Device } from "../api/types";
import {
  onScanDevice,
  onScanFinished,
  onScanProgress,
  resultsExport,
  scanCancel,
  scanStart,
  whitelistAdd,
  whitelistList,
  whitelistRemove,
} from "../api/scan";
import { auditDevice } from "../api/stream";

function parsePorts(input: string): number[] {
  return input
    .split(/[\s,]+/)
    .map((s) => s.trim())
    .filter(Boolean)
    .map(Number)
    .filter((n) => Number.isInteger(n) && n > 0 && n < 65536);
}

export default function DiscoveryPage() {
  const [cidr, setCidr] = useState("192.168.1.0/24");
  const [portsText, setPortsText] = useState("80, 443, 554, 8000, 8080, 8899, 37777");
  const [rateLimit, setRateLimit] = useState(256);
  const [timeoutMs, setTimeoutMs] = useState(800);
  const [onvif, setOnvif] = useState(true);
  const [mdns, setMdns] = useState(true);

  const [scanning, setScanning] = useState(false);
  const [taskId, setTaskId] = useState<number | null>(null);
  const [progress, setProgress] = useState({ scanned: 0, total: 0 });
  const [devices, setDevices] = useState<Device[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const [whitelist, setWhitelist] = useState<string[]>([]);
  const [wlInput, setWlInput] = useState("");
  const [wlConfirm, setWlConfirm] = useState(false);

  const [tryWeakCreds, setTryWeakCreds] = useState(false);
  const [auditingIp, setAuditingIp] = useState<string | null>(null);
  const [audit, setAudit] = useState<AuditResult | null>(null);

  const taskIdRef = useRef<number | null>(null);
  taskIdRef.current = taskId;

  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];
    (async () => {
      unlisteners.push(
        await onScanDevice((d) => {
          setDevices((prev) => {
            const idx = prev.findIndex((x) => x.ip === d.ip);
            if (idx === -1) return [...prev, d];
            const next = prev.slice();
            next[idx] = d;
            return next;
          });
        }),
      );
      unlisteners.push(
        await onScanProgress((p) => {
          if (p.taskId === taskIdRef.current) {
            setProgress({ scanned: p.scanned, total: p.total });
          }
        }),
      );
      unlisteners.push(
        await onScanFinished((f) => {
          if (f.taskId === taskIdRef.current) {
            setScanning(false);
            setNotice(
              `扫描${f.cancelled ? "已取消" : "完成"}，共发现 ${f.found} 台设备。`,
            );
          }
        }),
      );
    })();
    return () => unlisteners.forEach((u) => u());
  }, []);

  useEffect(() => {
    whitelistList().then(setWhitelist).catch(() => {});
  }, []);

  async function handleStart() {
    setError(null);
    setNotice(null);
    setDevices([]);
    setProgress({ scanned: 0, total: 0 });
    try {
      const id = await scanStart({
        cidr: cidr.trim(),
        ports: parsePorts(portsText),
        rateLimit,
        timeoutMs,
        onvif,
        mdns,
      });
      setTaskId(id);
      setScanning(true);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleCancel() {
    if (taskId == null) return;
    try {
      await scanCancel(taskId);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleExport(fmt: "csv" | "json") {
    if (taskId == null) return;
    try {
      const path = await resultsExport(taskId, fmt);
      setNotice(`已导出到: ${path}`);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleAudit(device: Device) {
    setError(null);
    setAuditingIp(device.ip);
    setAudit(null);
    try {
      const result = await auditDevice(device.ip, device.openPorts, tryWeakCreds);
      setAudit(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setAuditingIp(null);
    }
  }

  async function handleWhitelistAdd() {
    setError(null);
    try {
      const list = await whitelistAdd(wlInput.trim(), wlConfirm);
      setWhitelist(list);
      setWlInput("");
      setWlConfirm(false);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleWhitelistRemove(target: string) {
    try {
      setWhitelist(await whitelistRemove(target));
    } catch (e) {
      setError(String(e));
    }
  }

  const pct =
    progress.total > 0 ? Math.round((progress.scanned / progress.total) * 100) : 0;

  return (
    <div className="page">
      <header>
        <h1>摄像头发现</h1>
        <p className="muted">
          仅限扫描你拥有或已获授权的私有网段。非私有目标需先在下方白名单确认授权。
        </p>
      </header>

      <section className="panel">
        <div className="grid">
          <label>
            目标网段 / IP
            <input
              value={cidr}
              onChange={(e) => setCidr(e.target.value)}
              placeholder="192.168.1.0/24"
              disabled={scanning}
            />
          </label>
          <label>
            端口
            <input
              value={portsText}
              onChange={(e) => setPortsText(e.target.value)}
              disabled={scanning}
            />
          </label>
          <label>
            并发上限
            <input
              type="number"
              value={rateLimit}
              min={1}
              onChange={(e) => setRateLimit(Number(e.target.value))}
              disabled={scanning}
            />
          </label>
          <label>
            超时(ms)
            <input
              type="number"
              value={timeoutMs}
              min={50}
              onChange={(e) => setTimeoutMs(Number(e.target.value))}
              disabled={scanning}
            />
          </label>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={onvif}
              onChange={(e) => setOnvif(e.target.checked)}
              disabled={scanning}
            />
            启用 ONVIF 发现
          </label>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={mdns}
              onChange={(e) => setMdns(e.target.checked)}
              disabled={scanning}
            />
            启用 mDNS 发现
          </label>
        </div>

        <div className="actions">
          {!scanning ? (
            <button className="primary" onClick={handleStart}>
              开始扫描
            </button>
          ) : (
            <button className="danger" onClick={handleCancel}>
              取消扫描
            </button>
          )}
          <button onClick={() => handleExport("csv")} disabled={devices.length === 0}>
            导出 CSV
          </button>
          <button onClick={() => handleExport("json")} disabled={devices.length === 0}>
            导出 JSON
          </button>
        </div>

        {scanning && (
          <div className="progress">
            <div className="bar" style={{ width: `${pct}%` }} />
            <span>
              {progress.scanned}/{progress.total} ({pct}%)
            </span>
          </div>
        )}

        {error && <p className="error">{error}</p>}
        {notice && <p className="notice">{notice}</p>}
      </section>

      <section className="panel">
        <div className="panel-head">
          <h2>结果（{devices.length}）</h2>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={tryWeakCreds}
              onChange={(e) => setTryWeakCreds(e.target.checked)}
            />
            审计时尝试弱口令核查（仅授权设备）
          </label>
        </div>
        <DeviceTable devices={devices} onAudit={handleAudit} auditingIp={auditingIp} />
      </section>

      {audit && <AuditPanel result={audit} onClose={() => setAudit(null)} />}

      <section className="panel">
        <h2>授权白名单</h2>
        <p className="muted">
          用于允许扫描非私有(公网)目标。添加前请确认你已获得资产所有者的书面授权。
        </p>
        <div className="actions">
          <input
            value={wlInput}
            onChange={(e) => setWlInput(e.target.value)}
            placeholder="203.0.113.0/24"
          />
          <label className="checkbox">
            <input
              type="checkbox"
              checked={wlConfirm}
              onChange={(e) => setWlConfirm(e.target.checked)}
            />
            我已获授权
          </label>
          <button onClick={handleWhitelistAdd} disabled={!wlInput.trim()}>
            添加
          </button>
        </div>
        <ul className="whitelist">
          {whitelist.map((t) => (
            <li key={t}>
              <span className="mono">{t}</span>
              <button className="link" onClick={() => handleWhitelistRemove(t)}>
                移除
              </button>
            </li>
          ))}
          {whitelist.length === 0 && <li className="muted">（空）</li>}
        </ul>
      </section>
    </div>
  );
}

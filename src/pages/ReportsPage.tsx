import { useEffect, useState } from "react";
import {
  auditLogList,
  reportGenerate,
  type AuditLogEntry,
} from "../api/admin";

function formatTs(ms: number): string {
  if (!ms) return "-";
  return new Date(ms).toLocaleString();
}

export default function ReportsPage() {
  const [logs, setLogs] = useState<AuditLogEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  async function refresh() {
    try {
      setLogs(await auditLogList(300));
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  async function handleReport(fmt: "md" | "html") {
    setError(null);
    setNotice(null);
    setLoading(true);
    try {
      const path = await reportGenerate(fmt);
      setNotice(`报告已生成：${path}`);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="page">
      <header>
        <h1>报告与审计</h1>
        <p className="muted">
          基于已发现设备与审计结果生成加固报告；下方为本地操作审计日志。
        </p>
      </header>

      <section className="panel">
        <h2>加固报告</h2>
        <p className="muted">
          请先在「设备发现」中扫描并对设备执行审计，再生成报告。报告写入应用数据目录的
          exports 文件夹。
        </p>
        <div className="actions">
          <button className="primary" onClick={() => handleReport("md")} disabled={loading}>
            导出 Markdown
          </button>
          <button onClick={() => handleReport("html")} disabled={loading}>
            导出 HTML
          </button>
          <button onClick={() => void refresh()}>刷新日志</button>
        </div>
        {error && <p className="error">{error}</p>}
        {notice && <p className="notice">{notice}</p>}
      </section>

      <section className="panel">
        <h2>审计日志（{logs.length}）</h2>
        {logs.length === 0 ? (
          <p className="muted">暂无日志。</p>
        ) : (
          <table className="device-table">
            <thead>
              <tr>
                <th>时间</th>
                <th>动作</th>
                <th>目标</th>
                <th>详情</th>
                <th>结果</th>
              </tr>
            </thead>
            <tbody>
              {logs.map((l) => (
                <tr key={l.id}>
                  <td>{formatTs(l.ts)}</td>
                  <td className="mono">{l.action}</td>
                  <td className="mono">{l.target ?? "-"}</td>
                  <td className="ellipsis" title={l.detail ?? ""}>
                    {l.detail ?? "-"}
                  </td>
                  <td>{l.ok ? "成功" : "失败"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
    </div>
  );
}

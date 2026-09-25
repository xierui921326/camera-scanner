import type { AuditResult } from "../api/types";

const SEVERITY_ORDER: Record<string, number> = {
  critical: 0,
  high: 1,
  medium: 2,
  low: 3,
};

export default function AuditPanel({
  result,
  onClose,
}: {
  result: AuditResult;
  onClose: () => void;
}) {
  const findings = [...result.findings].sort(
    (a, b) => (SEVERITY_ORDER[a.severity] ?? 9) - (SEVERITY_ORDER[b.severity] ?? 9),
  );

  return (
    <section className="panel">
      <div className="panel-head">
        <h2>审计结果 · {result.ip}</h2>
        <button className="link" onClick={onClose}>
          关闭
        </button>
      </div>

      <div className="audit-meta">
        <span>厂商: {result.vendor ?? "未知"}</span>
        <span>HTTP 端口: {result.httpPort ?? "-"}</span>
        <span>Server: {result.httpServer ?? "-"}</span>
        <span>Realm: {result.realm ?? "-"}</span>
      </div>

      {result.weakCredentials.length > 0 && (
        <div className="audit-block danger-block">
          <h3>命中弱口令</h3>
          <ul>
            {result.weakCredentials.map((c, i) => (
              <li key={i} className="mono">
                {c.scheme} {c.username}:{c.password || "(空)"} @ {c.path}
              </li>
            ))}
          </ul>
        </div>
      )}

      <div className="audit-block">
        <h3>风险条目（{findings.length}）</h3>
        {findings.length === 0 ? (
          <p className="muted">未匹配到规则库中的已知风险条目。</p>
        ) : (
          <ul className="findings">
            {findings.map((f) => (
              <li key={f.id}>
                <div className="finding-head">
                  <span className={`sev sev-${f.severity}`}>{f.severity}</span>
                  <strong>{f.title}</strong>
                  {f.cve !== "N/A" && <span className="mono cve">{f.cve}</span>}
                </div>
                <p>{f.description}</p>
                <p className="advice">加固建议：{f.advice}</p>
              </li>
            ))}
          </ul>
        )}
      </div>

      {result.notes.length > 0 && (
        <div className="audit-block">
          <h3>备注</h3>
          <ul className="muted">
            {result.notes.map((n, i) => (
              <li key={i}>{n}</li>
            ))}
          </ul>
        </div>
      )}

      <p className="muted">
        说明：风险条目基于厂商指纹与规则库比对给出，仅提示「可能受影响」，请结合实际固件版本核实。本工具不含漏洞利用载荷。
      </p>
    </section>
  );
}

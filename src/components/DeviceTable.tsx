import type { Device } from "../api/types";

function formatTime(ms: number): string {
  if (!ms) return "-";
  return new Date(ms).toLocaleTimeString();
}

export default function DeviceTable({
  devices,
  onAudit,
  auditingIp,
}: {
  devices: Device[];
  onAudit?: (device: Device) => void;
  auditingIp?: string | null;
}) {
  if (devices.length === 0) {
    return <p className="muted">暂无设备。设置目标网段后点击「开始扫描」。</p>;
  }

  return (
    <table className="device-table">
      <thead>
        <tr>
          <th>IP</th>
          <th>开放端口</th>
          <th>协议</th>
          <th>来源</th>
          <th>ONVIF 地址</th>
          <th>Server</th>
          <th>发现时间</th>
          {onAudit && <th>操作</th>}
        </tr>
      </thead>
      <tbody>
        {devices.map((d) => (
          <tr key={d.ip}>
            <td className="mono">{d.ip}</td>
            <td className="mono">{d.openPorts.join(", ") || "-"}</td>
            <td>
              {d.protocols.map((p) => (
                <span key={p} className="tag">
                  {p}
                </span>
              ))}
            </td>
            <td>{d.source}</td>
            <td className="mono ellipsis" title={d.onvifXaddr ?? ""}>
              {d.onvifXaddr ?? "-"}
            </td>
            <td className="ellipsis" title={d.httpServer ?? ""}>
              {d.httpServer ?? "-"}
            </td>
            <td>{formatTime(d.discoveredAt)}</td>
            {onAudit && (
              <td>
                <button
                  className="link audit"
                  onClick={() => onAudit(d)}
                  disabled={auditingIp === d.ip}
                >
                  {auditingIp === d.ip ? "审计中…" : "审计"}
                </button>
              </td>
            )}
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export default function ConsentGate({ onAccept }: { onAccept: () => void }) {
  return (
    <div className="consent-overlay">
      <div className="consent-card">
        <h1>使用前确认</h1>
        <p>
          <strong>camera-scanner</strong>{" "}
          是面向授权环境的摄像头安全审计与预览工具。继续使用即表示你确认：
        </p>
        <ul>
          <li>仅扫描 / 审计 / 预览你本人拥有或已获书面授权的设备与网段；</li>
          <li>不会对未授权的公网或他人资产进行探测、口令尝试或视频访问；</li>
          <li>理解未授权访问计算机信息系统可能违法，后果自负；</li>
          <li>工具默认限制私有网段，操作会写入本地审计日志以便追溯。</li>
        </ul>
        <p className="muted">
          本工具的漏洞模块只做指纹比对与加固建议，不包含攻击载荷。
        </p>
        <div className="actions">
          <button className="primary" onClick={onAccept}>
            我已获授权，同意并继续
          </button>
        </div>
      </div>
    </div>
  );
}

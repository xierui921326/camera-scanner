import { useState } from "react";
import StreamTile from "../components/StreamTile";
import type { StreamInfo } from "../api/types";
import { streamClose, streamOpen } from "../api/stream";

const MAX_STREAMS = 4;

export default function PreviewPage() {
  const [ip, setIp] = useState("");
  const [port, setPort] = useState(554);
  const [path, setPath] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [rtspUrl, setRtspUrl] = useState("");
  const [label, setLabel] = useState("");
  const [transcode, setTranscode] = useState(false);
  const [ffmpegPath, setFfmpegPath] = useState("");
  const [mode, setMode] = useState<"hls" | "mse" | "retina">("retina");

  const [streams, setStreams] = useState<StreamInfo[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  async function handleAdd() {
    setError(null);
    setLoading(true);
    try {
      const info = await streamOpen({
        rtspUrl: rtspUrl.trim() || undefined,
        ip: ip.trim() || undefined,
        port,
        path: path.trim() || undefined,
        username: username.trim() || undefined,
        password: password || undefined,
        transcode,
        ffmpegPath: ffmpegPath.trim() || undefined,
        mode,
        label: label.trim() || undefined,
      });
      setStreams((prev) => [...prev, info]);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  async function handleClose(sessionId: string) {
    try {
      await streamClose(sessionId);
    } catch {
      /* ignore */
    }
    setStreams((prev) => prev.filter((s) => s.sessionId !== sessionId));
  }

  async function handleCloseAll() {
    await Promise.all(streams.map((s) => streamClose(s.sessionId).catch(() => {})));
    setStreams([]);
  }

  const cols = streams.length <= 1 ? 1 : streams.length <= 4 ? 2 : 3;

  return (
    <div className="page">
      <header>
        <h1>实时预览</h1>
        <p className="muted">
          支持多路网格（最多 {MAX_STREAMS} 路）。模式：retina（纯 Rust fMP4/MSE，低延迟，仅
          H.264）、mse（ffmpeg fMP4 HLS）、hls（ffmpeg MPEG-TS）。仅限授权设备。
        </p>
      </header>

      <section className="panel">
        <div className="grid">
          <label>
            标签
            <input
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              placeholder="前门 / cam-1"
            />
          </label>
          <label>
            IP
            <input value={ip} onChange={(e) => setIp(e.target.value)} placeholder="192.168.1.64" />
          </label>
          <label>
            端口
            <input
              type="number"
              value={port}
              onChange={(e) => setPort(Number(e.target.value))}
            />
          </label>
          <label>
            RTSP 路径
            <input
              value={path}
              onChange={(e) => setPath(e.target.value)}
              placeholder="Streaming/Channels/101"
            />
          </label>
          <label>
            用户名
            <input value={username} onChange={(e) => setUsername(e.target.value)} />
          </label>
          <label>
            密码
            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
          </label>
        </div>

        <label style={{ marginTop: 12 }}>
          或直接填写完整 RTSP URL（优先生效）
          <input
            value={rtspUrl}
            onChange={(e) => setRtspUrl(e.target.value)}
            placeholder="rtsp://user:pass@192.168.1.64:554/Streaming/Channels/101"
          />
        </label>

        <div className="grid" style={{ marginTop: 12 }}>
          <label>
            播放模式
            <select value={mode} onChange={(e) => setMode(e.target.value as typeof mode)}>
              <option value="retina">retina（纯 Rust / MSE）</option>
              <option value="mse">mse（ffmpeg fMP4 HLS）</option>
              <option value="hls">hls（ffmpeg MPEG-TS）</option>
            </select>
          </label>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={transcode}
              onChange={(e) => setTranscode(e.target.checked)}
              disabled={mode === "retina"}
            />
            转码为 H.264（仅 ffmpeg 模式；H.265 需勾选）
          </label>
          <label>
            ffmpeg 路径（可选）
            <input
              value={ffmpegPath}
              onChange={(e) => setFfmpegPath(e.target.value)}
              placeholder="ffmpeg"
              disabled={mode === "retina"}
            />
          </label>
        </div>

        <div className="actions">
          <button
            className="primary"
            onClick={handleAdd}
            disabled={loading || streams.length >= MAX_STREAMS}
          >
            {loading ? "启动中…" : `添加预览（${streams.length}/${MAX_STREAMS}）`}
          </button>
          <button onClick={handleCloseAll} disabled={streams.length === 0}>
            全部关闭
          </button>
        </div>

        {error && <p className="error">{error}</p>}
      </section>

      <section className="panel">
        {streams.length === 0 ? (
          <p className="muted">尚未开始预览。添加一路后会显示在下方网格中。</p>
        ) : (
          <div className="stream-grid" style={{ gridTemplateColumns: `repeat(${cols}, 1fr)` }}>
            {streams.map((s) => (
              <StreamTile key={s.sessionId} stream={s} onClose={handleClose} />
            ))}
          </div>
        )}
      </section>
    </div>
  );
}

import { useEffect, useRef, useState } from "react";
import Hls from "hls.js";
import type { StreamInfo } from "../api/types";

interface Manifest {
  init: string;
  codec: string;
  nextSeq: number;
  segments: { seq: number; file: string }[];
}

async function fetchBuf(url: string): Promise<ArrayBuffer> {
  const res = await fetch(url, { cache: "no-store" });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.arrayBuffer();
}

function joinUrl(base: string, file: string): string {
  const trimmed = base.replace(/\/[^/]*$/, "/");
  return trimmed + file;
}

function appendBuffer(sb: SourceBuffer, data: ArrayBuffer): Promise<void> {
  return new Promise((resolve, reject) => {
    const onUpdate = () => {
      sb.removeEventListener("updateend", onUpdate);
      sb.removeEventListener("error", onError);
      resolve();
    };
    const onError = () => {
      sb.removeEventListener("updateend", onUpdate);
      sb.removeEventListener("error", onError);
      reject(new Error("SourceBuffer append failed"));
    };
    const tryAppend = () => {
      if (sb.updating) {
        sb.addEventListener("updateend", tryAppend, { once: true });
        return;
      }
      sb.addEventListener("updateend", onUpdate);
      sb.addEventListener("error", onError);
      try {
        sb.appendBuffer(data);
      } catch (e) {
        sb.removeEventListener("updateend", onUpdate);
        sb.removeEventListener("error", onError);
        reject(e);
      }
    };
    tryAppend();
  });
}

function sleep(ms: number) {
  return new Promise((r) => setTimeout(r, ms));
}

export default function StreamTile({
  stream,
  onClose,
}: {
  stream: StreamInfo;
  onClose: (sessionId: string) => void;
}) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const [error, setError] = useState<string | null>(null);
  const isRetina = stream.mode === "retina";

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    if (isRetina) {
      const manifestUrl = stream.manifestUrl ?? stream.url;
      let cancelled = false;
      let mediaSource: MediaSource | null = null;
      let sourceBuffer: SourceBuffer | null = null;
      let seen = new Set<number>();
      let timer: number | undefined;
      let objectUrl: string | null = null;

      async function pump() {
        if (cancelled) return;
        try {
          const res = await fetch(manifestUrl, { cache: "no-store" });
          if (!res.ok) {
            timer = window.setTimeout(pump, 500);
            return;
          }
          const manifest = (await res.json()) as Manifest;

          if (!mediaSource) {
            if (!("MediaSource" in window)) {
              setError("当前 WebView 不支持 MediaSource");
              return;
            }
            mediaSource = new MediaSource();
            objectUrl = URL.createObjectURL(mediaSource);
            video!.src = objectUrl;
            await new Promise<void>((resolve, reject) => {
              mediaSource!.addEventListener("sourceopen", () => resolve(), { once: true });
              mediaSource!.addEventListener(
                "error",
                () => reject(new Error("MediaSource error")),
                { once: true },
              );
            });
            const mime = `video/mp4; codecs="${manifest.codec}"`;
            if (!MediaSource.isTypeSupported(mime)) {
              setError(`不支持的编解码: ${mime}`);
              return;
            }
            sourceBuffer = mediaSource.addSourceBuffer(mime);
            const initUrl = joinUrl(manifestUrl, manifest.init);
            let gotInit = false;
            for (let i = 0; i < 50 && !cancelled; i++) {
              try {
                const init = await fetchBuf(initUrl);
                await appendBuffer(sourceBuffer, init);
                gotInit = true;
                break;
              } catch {
                await sleep(400);
              }
            }
            if (!gotInit) {
              setError("等待 init.mp4 超时（检查 RTSP/编解码）");
              return;
            }
          }

          if (sourceBuffer) {
            for (const seg of manifest.segments) {
              if (cancelled || seen.has(seg.seq)) continue;
              try {
                const buf = await fetchBuf(joinUrl(manifestUrl, seg.file));
                await appendBuffer(sourceBuffer, buf);
                seen.add(seg.seq);
                if (video!.paused) void video!.play().catch(() => {});
              } catch {
                /* pruned or not ready */
              }
            }
            if (seen.size > 64) {
              seen = new Set([...seen].sort((a, b) => a - b).slice(-32));
            }
          }
        } catch (e) {
          // keep polling; surface persistent errors via error.txt path later
          void e;
        }
        if (!cancelled) timer = window.setTimeout(pump, 400);
      }

      void pump();
      return () => {
        cancelled = true;
        if (timer) window.clearTimeout(timer);
        if (objectUrl) URL.revokeObjectURL(objectUrl);
      };
    }

    // HLS / MSE(fMP4-HLS) path via hls.js
    let hls: Hls | null = null;
    if (Hls.isSupported()) {
      hls = new Hls({
        lowLatencyMode: true,
        liveSyncDurationCount: 2,
        manifestLoadingMaxRetry: 30,
        manifestLoadingRetryDelay: 400,
        levelLoadingMaxRetry: 30,
      });
      hls.loadSource(stream.url);
      hls.attachMedia(video);
      hls.on(Hls.Events.MANIFEST_PARSED, () => {
        void video.play().catch(() => {});
      });
      hls.on(Hls.Events.ERROR, (_e, data) => {
        if (data.fatal) setError(`${data.type}/${data.details}`);
      });
    } else if (video.canPlayType("application/vnd.apple.mpegurl")) {
      video.src = stream.url;
      void video.play().catch(() => {});
    } else {
      setError("不支持 HLS");
    }

    return () => {
      hls?.destroy();
    };
  }, [stream.url, stream.manifestUrl, stream.mode, isRetina]);

  return (
    <div className="stream-tile">
      <div className="stream-tile-head">
        <span className="mono">{stream.label}</span>
        <span className="tag">{stream.mode}</span>
        <button className="link" onClick={() => onClose(stream.sessionId)}>
          关闭
        </button>
      </div>
      <video ref={videoRef} muted playsInline controls />
      {error && <p className="error">{error}</p>}
    </div>
  );
}

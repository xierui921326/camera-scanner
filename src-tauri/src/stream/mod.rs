pub mod fmp4;
pub mod retina_mse;

use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tiny_http::{Header, Response, Server};
use tokio::process::{Child, Command};

/// Max concurrent preview sessions (multi-grid guardrail).
pub const MAX_STREAMS: usize = 4;

/// Arguments accepted by the `stream_open` command.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamOpenArgs {
    /// A full RTSP URL. When absent it is built from `ip`/`port`/`path`.
    #[serde(default)]
    pub rtsp_url: Option<String>,
    #[serde(default)]
    pub ip: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    /// Transcode to H.264 instead of stream-copy (needed for H.265 cameras).
    #[serde(default)]
    pub transcode: bool,
    /// Optional ffmpeg binary path; defaults to `ffmpeg` on PATH.
    #[serde(default)]
    pub ffmpeg_path: Option<String>,
    /// Playback mode: `hls` (ffmpeg TS), `mse` (ffmpeg fMP4 HLS), `retina` (pure Rust fMP4).
    #[serde(default)]
    pub mode: Option<String>,
    /// Optional label shown in the multi-grid UI.
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamInfo {
    pub session_id: String,
    pub url: String,
    pub mode: String,
    pub label: String,
    /// For MSE/retina: JSON manifest URL. For HLS: null.
    pub manifest_url: Option<String>,
}

impl StreamOpenArgs {
    pub fn mode(&self) -> &str {
        match self.mode.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some("mse") | Some("fmp4") => "mse",
            Some("retina") => "retina",
            _ => "hls",
        }
    }

    pub fn label_or_default(&self) -> String {
        if let Some(l) = self.label.as_ref().filter(|s| !s.trim().is_empty()) {
            return l.trim().to_string();
        }
        if let Some(ip) = self.ip.as_ref().filter(|s| !s.trim().is_empty()) {
            return ip.trim().to_string();
        }
        "camera".into()
    }

    /// Resolve the RTSP URL and the host it targets (for authorization).
    pub fn resolve(&self) -> Result<(String, String), String> {
        if let Some(url) = self.rtsp_url.as_ref().filter(|s| !s.trim().is_empty()) {
            let host = host_from_rtsp(url).unwrap_or_default();
            return Ok((url.trim().to_string(), host));
        }
        let ip = self
            .ip
            .as_ref()
            .filter(|s| !s.trim().is_empty())
            .ok_or("必须提供 rtspUrl 或 ip")?
            .trim();
        let port = self.port.unwrap_or(554);
        let path = self.path.clone().unwrap_or_default();
        let path = path.trim_start_matches('/');
        let auth = match (&self.username, &self.password) {
            (Some(u), Some(p)) if !u.is_empty() => format!("{u}:{p}@"),
            (Some(u), None) if !u.is_empty() => format!("{u}@"),
            _ => String::new(),
        };
        let url = format!("rtsp://{auth}{ip}:{port}/{path}");
        Ok((url, ip.to_string()))
    }
}

fn host_from_rtsp(url: &str) -> Option<String> {
    let after = url.strip_prefix("rtsp://")?;
    let after = after.split('/').next()?;
    let after = after.rsplit('@').next()?;
    let host = after.split(':').next()?;
    Some(host.to_string())
}

fn apply_video_codec(cmd: &mut Command, args: &StreamOpenArgs) {
    if args.transcode {
        cmd.arg("-c:v")
            .arg("libx264")
            .arg("-preset")
            .arg("veryfast")
            .arg("-tune")
            .arg("zerolatency");
    } else {
        cmd.arg("-c:v").arg("copy");
    }
}

/// Launch ffmpeg to remux/transcode RTSP into a live HLS playlist (MPEG-TS).
pub fn spawn_ffmpeg_hls(args: &StreamOpenArgs, rtsp_url: &str, out_dir: &Path) -> Result<Child, String> {
    let ffmpeg = args.ffmpeg_path.clone().unwrap_or_else(|| "ffmpeg".to_string());
    let playlist = out_dir.join("index.m3u8");
    let seg = out_dir.join("seg_%05d.ts");

    let mut cmd = Command::new(&ffmpeg);
    cmd.arg("-nostdin")
        .arg("-rtsp_transport")
        .arg("tcp")
        .arg("-i")
        .arg(rtsp_url)
        .arg("-an");
    apply_video_codec(&mut cmd, args);
    cmd.arg("-f")
        .arg("hls")
        .arg("-hls_time")
        .arg("1")
        .arg("-hls_list_size")
        .arg("6")
        .arg("-hls_flags")
        .arg("delete_segments+append_list+omit_endlist")
        .arg("-hls_segment_type")
        .arg("mpegts")
        .arg("-hls_segment_filename")
        .arg(&seg)
        .arg(&playlist)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);

    cmd.spawn()
        .map_err(|e| format!("启动 ffmpeg 失败（请确认已安装 ffmpeg 或在设置中指定路径）: {e}"))
}

/// Launch ffmpeg producing fMP4 HLS (lower latency than MPEG-TS HLS).
pub fn spawn_ffmpeg_fmp4_hls(
    args: &StreamOpenArgs,
    rtsp_url: &str,
    out_dir: &Path,
) -> Result<Child, String> {
    let ffmpeg = args.ffmpeg_path.clone().unwrap_or_else(|| "ffmpeg".to_string());
    let playlist = out_dir.join("index.m3u8");
    let seg = out_dir.join("seg_%05d.m4s");

    let mut cmd = Command::new(&ffmpeg);
    cmd.arg("-nostdin")
        .arg("-rtsp_transport")
        .arg("tcp")
        .arg("-i")
        .arg(rtsp_url)
        .arg("-an");
    apply_video_codec(&mut cmd, args);
    cmd.arg("-f")
        .arg("hls")
        .arg("-hls_time")
        .arg("1")
        .arg("-hls_list_size")
        .arg("6")
        .arg("-hls_flags")
        .arg("delete_segments+append_list+omit_endlist")
        .arg("-hls_segment_type")
        .arg("fmp4")
        .arg("-hls_fmp4_init_filename")
        .arg("init.mp4")
        .arg("-hls_segment_filename")
        .arg(&seg)
        .arg(&playlist)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);

    cmd.spawn()
        .map_err(|e| format!("启动 ffmpeg 失败（请确认已安装 ffmpeg 或在设置中指定路径）: {e}"))
}

/// Start a loopback-only static file server rooted at `root`, returning its port.
pub fn start_media_server(root: PathBuf) -> Result<u16, String> {
    let server = Server::http("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = server
        .server_addr()
        .to_ip()
        .map(|a| a.port())
        .ok_or("无法获取媒体服务端口")?;

    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let url = request.url().split('?').next().unwrap_or("/").to_string();
            let safe = url
                .trim_start_matches('/')
                .split('/')
                .filter(|s| !s.is_empty() && *s != "..")
                .collect::<Vec<_>>()
                .join("/");
            let path = root.join(&safe);

            match std::fs::read(&path) {
                Ok(bytes) => {
                    let ctype = if safe.ends_with(".m3u8") {
                        "application/vnd.apple.mpegurl"
                    } else if safe.ends_with(".ts") {
                        "video/mp2t"
                    } else if safe.ends_with(".m4s") || safe.ends_with(".mp4") {
                        "video/mp4"
                    } else if safe.ends_with(".json") {
                        "application/json"
                    } else {
                        "application/octet-stream"
                    };
                    let mut resp = Response::from_data(bytes);
                    if let Ok(h) = Header::from_bytes(&b"Content-Type"[..], ctype.as_bytes()) {
                        resp.add_header(h);
                    }
                    if let Ok(h) =
                        Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..])
                    {
                        resp.add_header(h);
                    }
                    if let Ok(h) = Header::from_bytes(&b"Cache-Control"[..], &b"no-cache"[..]) {
                        resp.add_header(h);
                    }
                    let _ = request.respond(resp);
                }
                Err(_) => {
                    let _ = request
                        .respond(Response::from_string("not found").with_status_code(404));
                }
            }
        }
    });

    Ok(port)
}

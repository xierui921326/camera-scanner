//! Pure-Rust RTSP → fragmented MP4 remux using `retina`.
//!
//! Writes `init.mp4`, rolling `seg_XXXXX.m4s`, and a `manifest.json` that the
//! frontend MSE player polls. Only H.264 (`avc1.*`) is supported for MSE.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use retina::client::{Credentials, PlayOptions, Session, SessionOptions, SetupOptions, Transport};
use retina::codec::{CodecItem, FrameFormat, ParametersRef};
use serde::Serialize;
use url::Url;

use super::fmp4::{self, Sample};

const MAX_SEGMENTS: usize = 12;
const DEFAULT_FRAME_DURATION: u32 = 3000; // 90000/30

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    init: String,
    codec: String,
    next_seq: u32,
    segments: Vec<ManifestSeg>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ManifestSeg {
    seq: u32,
    file: String,
}

fn parse_rtsp(rtsp_url: &str) -> Result<(Url, Option<Credentials>), String> {
    let mut url = Url::parse(rtsp_url).map_err(|e| format!("非法 RTSP URL: {e}"))?;
    let creds = {
        let user = url.username();
        if !user.is_empty() {
            let password = url.password().unwrap_or("").to_string();
            let username = user.to_string();
            let _ = url.set_username("");
            let _ = url.set_password(None);
            Some(Credentials {
                username,
                password,
            })
        } else {
            None
        }
    };
    Ok((url, creds))
}

fn write_manifest(dir: &Path, codec: &str, next_seq: u32, segments: &[ManifestSeg]) {
    let m = Manifest {
        init: "init.mp4".into(),
        codec: codec.to_string(),
        next_seq,
        segments: segments.to_vec(),
    };
    if let Ok(json) = serde_json::to_string(&m) {
        let _ = std::fs::write(dir.join("manifest.json"), json);
    }
}

fn prune_old(dir: &Path, segments: &mut Vec<ManifestSeg>) {
    while segments.len() > MAX_SEGMENTS {
        if let Some(old) = segments.first().cloned() {
            let _ = std::fs::remove_file(dir.join(&old.file));
            segments.remove(0);
        } else {
            break;
        }
    }
}

/// Spawn a background remux task. Returns immediately; cancellation is via `cancel`.
pub fn spawn_retina_remux(
    rtsp_url: String,
    out_dir: PathBuf,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    // Quick URL parse up-front so the command can fail fast.
    let _ = parse_rtsp(&rtsp_url)?;
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;

    tauri::async_runtime::spawn(async move {
        if let Err(e) = run_remux(&rtsp_url, &out_dir, cancel).await {
            let _ = std::fs::write(out_dir.join("error.txt"), e);
        }
    });
    Ok(())
}

async fn run_remux(rtsp_url: &str, out_dir: &Path, cancel: Arc<AtomicBool>) -> Result<(), String> {
    let (url, creds) = parse_rtsp(rtsp_url)?;
    let mut opts = SessionOptions::default();
    if creds.is_some() {
        opts = opts.creds(creds);
    }

    let mut session = Session::describe(url, opts)
        .await
        .map_err(|e| format!("RTSP DESCRIBE 失败: {e}"))?;

    let mut video_i = None;
    for (i, stream) in session.streams().iter().enumerate() {
        if stream.media() == "video" {
            video_i = Some(i);
            break;
        }
    }
    let video_i = video_i.ok_or("未找到视频流")?;

    session
        .setup(
            video_i,
            SetupOptions::default()
                .transport(Transport::Tcp(Default::default()))
                .frame_format(FrameFormat::MP4),
        )
        .await
        .map_err(|e| format!("RTSP SETUP 失败: {e}"))?;

    let session = session
        .play(PlayOptions::default())
        .await
        .map_err(|e| format!("RTSP PLAY 失败: {e}"))?;

    let mut demuxed = session
        .demuxed()
        .map_err(|e| format!("demuxed 失败: {e}"))?;

    let mut init_written = false;
    let mut codec = String::from("avc1.42E01E");
    let mut timescale = 90_000u32;
    let mut seq = 1u32;
    let mut segments: Vec<ManifestSeg> = Vec::new();
    let mut last_ts: Option<i64> = None;
    let mut base_time = 0u64;

    // Placeholder manifest so the player can start polling immediately.
    write_manifest(out_dir, &codec, seq, &segments);

    while !cancel.load(Ordering::Relaxed) {
        let item = match tokio::time::timeout(Duration::from_millis(500), demuxed.next()).await {
            Ok(v) => v,
            Err(_) => continue, // timeout — re-check cancel
        };

        let Some(item) = next_item(item)? else {
            break;
        };

        let CodecItem::VideoFrame(frame) = item else {
            continue;
        };

        if !init_written || frame.has_new_parameters() {
            let stream = demuxed
                .streams()
                .get(video_i)
                .ok_or("视频流索引失效")?;
            let Some(ParametersRef::Video(params)) = stream.parameters() else {
                continue;
            };
            if !params.rfc6381_codec().starts_with("avc1") {
                return Err(format!(
                    "纯 Rust MSE 目前仅支持 H.264(avc1)，当前为 {}",
                    params.rfc6381_codec()
                ));
            }
            codec = params.rfc6381_codec().to_string();
            let (w, h) = params.pixel_dimensions();
            let sample_entry = params
                .mp4_sample_entry()
                .build()
                .map_err(|e| format!("构建 sample entry 失败: {e}"))?;
            timescale = frame.timestamp().clock_rate().get();
            let init = fmp4::build_init_segment(&sample_entry, w as u16, h as u16, timescale);
            std::fs::write(out_dir.join("init.mp4"), init).map_err(|e| e.to_string())?;
            init_written = true;
            write_manifest(out_dir, &codec, seq, &segments);
        }

        if !init_written {
            continue;
        }

        let ts = frame.timestamp().timestamp();
        let duration = match last_ts {
            Some(prev) => {
                let d = (ts - prev).clamp(1, timescale as i64 * 2) as u32;
                d
            }
            None => DEFAULT_FRAME_DURATION.min(timescale / 15).max(1),
        };
        last_ts = Some(ts);

        let sample = Sample {
            data: frame.data(),
            duration,
            is_keyframe: frame.is_random_access_point(),
        };
        let frag = fmp4::build_fragment(seq, base_time, &[sample]);
        let file = format!("seg_{seq:05}.m4s");
        std::fs::write(out_dir.join(&file), frag).map_err(|e| e.to_string())?;
        segments.push(ManifestSeg {
            seq,
            file: file.clone(),
        });
        prune_old(out_dir, &mut segments);
        seq = seq.wrapping_add(1);
        if seq == 0 {
            seq = 1;
        }
        base_time = base_time.saturating_add(duration as u64);
        write_manifest(out_dir, &codec, seq, &segments);
    }

    Ok(())
}

fn next_item(
    item: Option<Result<CodecItem, retina::Error>>,
) -> Result<Option<CodecItem>, String> {
    match item {
        None => Ok(None),
        Some(Ok(v)) => Ok(Some(v)),
        Some(Err(e)) => Err(format!("拉流错误: {e}")),
    }
}

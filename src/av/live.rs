//! A live stream going on, like Owncast's: what OBS sends (see `rtmp`) goes
//! to ffmpeg as FLV, which writes HLS (2-second pieces of MP4) in
//! `public/live/<random>/`, played in the browser by `assets/system.js`.
//! The stream is a post of kind `live`, sent to followers like Owncast's "is
//! live" notes. When it ends, its replay becomes a video if the streamer
//! keeps replays, like on PeerTube.

use std::{path::PathBuf, process::Stdio, time::Duration};

use tokio::{
    io::AsyncWriteExt,
    process::{Child, ChildStdin, Command},
};

use crate::{
    AppState,
    av::{self, jobs},
    entities::video,
    handlers::notifier,
};

/// Where the HLS of streams is written, served under `/public/live`.
pub const LIVE_DIR: &str = "public/live";

/// The FLV header: version 1, audio and video, then the size of the
/// "previous tag" before the first one (0).
const FLV_HEADER: [u8; 13] = [b'F', b'L', b'V', 1, 5, 0, 0, 0, 9, 0, 0, 0, 0];

pub struct Session {
    pub username: String,
    post_id: i64,
    dir: PathBuf,
    keep_replay: bool,
    /// Started at the first audio or video, when the size is known.
    ffmpeg: Option<Child>,
    stdin: Option<ChildStdin>,
    /// Width and height, from the stream's metadata (`onMetaData`).
    size: Option<(u32, u32)>,
    /// The qualities made, best first: the replay is the first.
    levels: Vec<av::Level>,
}

impl Session {
    /// Checks the stream key and starts ffmpeg. The error is told to OBS.
    pub async fn start(state: &AppState, key: &str) -> Result<Self, String> {
        let (Some(pool), Some(_)) = (&state.pool, av::tools()) else {
            return Err("Live streams are off on this server.".to_string());
        };
        if !crate::settings::services().live_streaming {
            return Err("Live streams are off on this server.".to_string());
        }
        if key.is_empty() {
            return Err("No stream key.".to_string());
        }
        let streamer = video::find_by_key(pool, key)
            .await
            .map_err(|err| format!("failed to check the key: {err}"))?
            .ok_or("Unknown stream key.")?;
        if streamer.live {
            return Err("Already live with this key.".to_string());
        }

        let folder = crate::handlers::session::random_part();
        let dir = PathBuf::from(LIVE_DIR).join(&folder);
        tokio::fs::create_dir_all(&dir).await.map_err(|err| format!("failed to create {}: {err}", dir.display()))?;

        let playlist = format!("/{LIVE_DIR}/{folder}/master.m3u8");
        let post_id = match video::start(pool, &streamer, &playlist).await {
            Ok(post_id) => post_id,
            Err(err) => {
                let _ = tokio::fs::remove_dir_all(&dir).await;
                return Err(format!("failed to save the stream: {err}"));
            }
        };
        // Followers hear of it, like Owncast's "is live" notes.
        notifier::post_created(state, pool, post_id).await;
        Ok(Self {
            username: streamer.username,
            post_id,
            dir,
            keep_replay: streamer.keep_replay,
            ffmpeg: None,
            stdin: None,
            size: None,
            levels: Vec::new(),
        })
    }

    /// An RTMP audio or video message, as an FLV tag. The metadata (sent
    /// first by OBS and ffmpeg) gives the size, which chooses the qualities;
    /// ffmpeg starts at the first audio or video.
    pub async fn write_tag(&mut self, kind: u8, timestamp: u32, payload: &[u8]) -> Result<(), String> {
        if kind == 18 {
            if self.size.is_none() {
                self.size = metadata_size(payload);
            }
            return Ok(());
        }
        if kind != 8 && kind != 9 {
            return Ok(());
        }
        let first = self.ffmpeg.is_none();
        if first {
            let tools = av::tools().ok_or("no ffmpeg")?;
            let (width, height) = self.size.unwrap_or((1920, 1080));
            self.levels = av::live_levels(width, height);
            let mut ffmpeg = spawn_ffmpeg(tools, &self.dir, self.keep_replay, &self.levels, height > width)?;
            self.stdin = ffmpeg.stdin.take();
            self.ffmpeg = Some(ffmpeg);
            tracing::info!(
                "live streams: {} sends {}×{}, made in {}",
                self.username,
                width,
                height,
                self.levels.iter().map(|level| format!("{}p", level.lines)).collect::<Vec<_>>().join(", ")
            );
        }
        let Some(stdin) = &mut self.stdin else {
            return Err("ffmpeg stopped".to_string());
        };
        let mut tag = Vec::with_capacity(payload.len() + 15 + FLV_HEADER.len());
        if first {
            tag.extend(FLV_HEADER);
        }
        let size = payload.len() as u32;
        tag.push(kind);
        tag.extend(&size.to_be_bytes()[1..]);
        tag.extend(&timestamp.to_be_bytes()[1..]);
        tag.push((timestamp >> 24) as u8);
        tag.extend([0, 0, 0]);
        tag.extend(payload);
        tag.extend((size + 11).to_be_bytes());
        stdin.write_all(&tag).await.map_err(|err| format!("ffmpeg stopped: {err}"))
    }

    /// OBS stopped: ffmpeg finishes the HLS, the post says the stream ended,
    /// and its replay becomes a video if kept.
    pub async fn end(mut self, state: &AppState) {
        drop(self.stdin.take());
        if let Some(ffmpeg) = &mut self.ffmpeg
            && tokio::time::timeout(Duration::from_secs(30), ffmpeg.wait()).await.is_err()
        {
            let _ = ffmpeg.kill().await;
        }
        tracing::info!("live streams: {} stopped", self.username);
        let Some(pool) = state.pool.clone() else {
            return;
        };
        if let Err(err) = video::end(&pool, self.post_id).await {
            tracing::error!("failed to end live post {}: {}", self.post_id, err);
        }
        notifier::post_updated(state, &pool, self.post_id).await;
        let state = state.clone();
        // The replay: the best quality.
        let best = self.levels.first().map(|level| level.lines);
        tokio::spawn(async move {
            if self.keep_replay
                && let Some(best) = best
                && let Err(err) = save_replay(&state, &pool, self.post_id, &self.dir.join(format!("q{best}"))).await
            {
                tracing::warn!("no replay for live post {}: {}", self.post_id, err);
            }
            let _ = tokio::fs::remove_dir_all(&self.dir).await;
        });
    }
}

/// The width and height of `onMetaData`: `["@setDataFrame", "onMetaData",
/// {width, height, ...}]`, as OBS and ffmpeg send it.
fn metadata_size(payload: &[u8]) -> Option<(u32, u32)> {
    let values = av::amf::decode_all(payload).ok()?;
    values.iter().find_map(|value| match value {
        av::amf::Amf::Object(properties) => {
            let number = |name: &str| {
                properties
                    .iter()
                    .find(|(key, _)| key == name)
                    .and_then(|(_, value)| value.as_number())
                    .filter(|number| *number >= 16.0 && *number <= 8192.0)
                    .map(|number| number as u32)
            };
            Some((number("width")?, number("height")?))
        }
        _ => None,
    })
}

/// ffmpeg making every quality of `levels` at once, each in its folder
/// (`q720/`...), and the master playlist listing them with their size,
/// bitrate and codecs, like Owncast's and PeerTube's lives.
fn spawn_ffmpeg(tools: &av::Tools, dir: &std::path::Path, keep_replay: bool, levels: &[av::Level], portrait: bool) -> Result<Child, String> {
    let path = |name: &str| dir.join(name).to_string_lossy().into_owned();
    let labels: String = (0..levels.len()).map(|index| format!("[v{index}]")).collect();
    let mut filter = format!("[0:v:0]split={}{labels}", levels.len());
    for (index, level) in levels.iter().enumerate() {
        // Never larger than what is sent.
        let size = if portrait { format!("'min({},iw)':-2", level.lines) } else { format!("-2:'min({},ih)'", level.lines) };
        filter.push_str(&format!(";[v{index}]scale={size},setsar=1[o{index}]"));
    }
    let mut args: Vec<String> = ["-hide_banner", "-loglevel", "error", "-f", "flv", "-i", "pipe:0", "-filter_complex", &filter]
        .map(str::to_string)
        .to_vec();
    let mut streams = Vec::new();
    for (index, level) in levels.iter().enumerate() {
        args.extend(["-map".to_string(), format!("[o{index}]"), "-map".to_string(), "0:a:0?".to_string()]);
        args.extend([format!("-c:v:{index}"), tools.video_encoder.to_string(), format!("-b:v:{index}"), format!("{}k", level.max_kbps * 3 / 4)]);
        args.extend([format!("-maxrate:v:{index}"), format!("{}k", level.max_kbps), format!("-bufsize:v:{index}"), format!("{}k", level.max_kbps * 2)]);
        args.extend([format!("-c:a:{index}"), "aac".to_string(), format!("-b:a:{index}"), format!("{}k", level.audio_kbps.max(64))]);
        streams.push(format!("v:{index},a:{index},name:{}", level.lines));
    }
    args.extend(["-pix_fmt", "yuv420p", "-ar", "48000"].map(str::to_string));
    if tools.video_encoder == "libx264" {
        args.extend(["-preset", "veryfast", "-tune", "zerolatency", "-profile:v", "high"].map(str::to_string));
    }
    args.extend(
        [
            // A key frame every 2 seconds in every quality, at the same
            // times: each piece starts with one, and the qualities line up
            // for switching.
            "-force_key_frames",
            "expr:gte(t,n_forced*2)",
            "-f",
            "hls",
            "-hls_time",
            "2",
            "-hls_segment_type",
            "fmp4",
            "-hls_fmp4_init_filename",
            "init.mp4",
            // The replay needs every piece; otherwise only the last ones stay.
            "-hls_list_size",
            if keep_replay { "0" } else { "10" },
            "-hls_flags",
            if keep_replay { "independent_segments" } else { "independent_segments+delete_segments" },
            "-var_stream_map",
        ]
        .map(str::to_string),
    );
    args.push(streams.join(" "));
    args.extend(["-master_pl_name".to_string(), "master.m3u8".to_string(), "-hls_segment_filename".to_string()]);
    args.push(path("q%v/segment-%05d.m4s"));
    args.push(path("q%v/index.m3u8"));
    Command::new(&tools.ffmpeg)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|err| format!("ffmpeg failed to start: {err}"))
}

/// The stream as one MP4, converted like an upload (every quality, see
/// `jobs`): its post becomes a video, like PeerTube's replays.
/// `quality` is the folder of the best quality.
async fn save_replay(state: &AppState, pool: &sqlx::PgPool, post_id: i64, quality: &std::path::Path) -> Result<(), String> {
    let tools = av::tools().ok_or("no ffmpeg")?;
    tokio::fs::create_dir_all(jobs::SOURCES_DIR).await.map_err(|err| err.to_string())?;
    let source = PathBuf::from(jobs::SOURCES_DIR).join(format!("{}.mp4", crate::handlers::session::random_part()));
    if let Err(err) = av::hls_to_mp4(tools, &quality.join("index.m3u8"), &source).await {
        let _ = tokio::fs::remove_file(&source).await;
        return Err(err);
    }
    let updated = sqlx::query(
        "UPDATE posts SET kind = 'video', media_status = 'processing', media_source = $2, updated_at = NOW() WHERE id = $1",
    )
    .bind(post_id)
    .bind(source.to_string_lossy().as_ref())
    .execute(pool)
    .await;
    if let Err(err) = updated {
        let _ = tokio::fs::remove_file(&source).await;
        return Err(format!("failed to save: {err}"));
    }
    tracing::info!("replay of live post {} converting", post_id);
    jobs::enqueue(state.clone(), post_id);
    Ok(())
}

/// At start: streams going on stopped with the server, their HLS is removed.
pub async fn clean(pool: &sqlx::PgPool) {
    match video::end_all(pool).await {
        Ok(0) => {}
        Ok(count) => tracing::info!("live streams: {} streams stopped with the server", count),
        Err(err) => tracing::warn!("live streams: failed to end old streams: {}", err),
    }
    let _ = tokio::fs::remove_dir_all(LIVE_DIR).await;
}

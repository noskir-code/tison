//! Videos, sounds and live streams, converted by ffmpeg like PeerTube,
//! Funkwhale and Owncast do: uploads become MP4 (H.264 and AAC) or MP3 that
//! every browser and podcast app plays, with their size, length and a
//! picture; live streams sent by OBS (see `rtmp`) become HLS.
//!
//! ffmpeg is a program of the server, found at start (`find`); without it
//! these services are off.

pub mod amf;
pub mod jobs;
pub mod live;
pub mod rtmp;

use std::{path::Path, process::Stdio, sync::OnceLock};

use tokio::process::Command;

/// The programs, and the encoders found in this ffmpeg.
#[derive(Debug)]
pub struct Tools {
    pub ffmpeg: String,
    pub ffprobe: String,
    /// `libx264`, else `libopenh264`.
    pub(crate) video_encoder: &'static str,
    /// `libmp3lame` (MP3, read by every podcast app), else `aac`.
    audio_encoder: &'static str,
}

static TOOLS: OnceLock<Tools> = OnceLock::new();

/// ffmpeg, when it was found at start.
pub fn tools() -> Option<&'static Tools> {
    TOOLS.get()
}

/// Looks for ffmpeg and ffprobe and their encoders. Returns why they can't
/// be used, if so.
pub fn find(ffmpeg: &str, ffprobe: &str) -> Result<(), String> {
    let run = |program: &str, args: &[&str]| {
        std::process::Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
    };
    run(ffprobe, &["-version"]).ok_or_else(|| format!("{ffprobe} not found"))?;
    let encoders = run(ffmpeg, &["-hide_banner", "-encoders"]).ok_or_else(|| format!("{ffmpeg} not found"))?;
    let has = |name: &str| encoders.lines().any(|line| line.split_whitespace().nth(1) == Some(name));
    let video_encoder = ["libx264", "libopenh264"]
        .into_iter()
        .find(|name| has(name))
        .ok_or("ffmpeg has no H.264 encoder (libx264 or libopenh264)")?;
    let audio_encoder = if has("libmp3lame") { "libmp3lame" } else { "aac" };
    let _ = TOOLS.set(Tools {
        ffmpeg: ffmpeg.to_string(),
        ffprobe: ffprobe.to_string(),
        video_encoder,
        audio_encoder,
    });
    Ok(())
}

/// What ffprobe says of a file.
#[derive(Debug, Default, PartialEq)]
pub struct Probe {
    pub seconds: f64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub has_video: bool,
    pub has_audio: bool,
}

/// Reads `key=value` lines of `ffprobe -of default=noprint_wrappers=1`.
fn parse_probe(output: &str) -> Probe {
    let mut probe = Probe::default();
    let mut in_video = false;
    for line in output.lines() {
        let Some((key, value)) = line.trim().split_once('=') else {
            continue;
        };
        match key {
            "codec_type" => {
                in_video = value == "video";
                // A cover picture in an MP3 is a "video" of one frame:
                // `attached_pic` tells it apart, see below.
                probe.has_video |= in_video;
                probe.has_audio |= value == "audio";
            }
            "width" if in_video && probe.width.is_none() => probe.width = value.parse().ok(),
            "height" if in_video && probe.height.is_none() => probe.height = value.parse().ok(),
            // Phones film sideways and say how to turn: the shown size.
            "rotation" if in_video => {
                if let (Ok(degrees), Some(width), Some(height)) = (value.parse::<i32>(), probe.width, probe.height)
                    && degrees.rem_euclid(180) == 90
                {
                    probe.width = Some(height);
                    probe.height = Some(width);
                }
            }
            "DISPOSITION:attached_pic" if in_video && value == "1" => {
                probe.has_video = false;
                probe.width = None;
                probe.height = None;
            }
            "duration" => {
                if let Ok(seconds) = value.parse::<f64>()
                    && seconds.is_finite()
                {
                    probe.seconds = probe.seconds.max(seconds);
                }
            }
            _ => {}
        }
    }
    probe
}

pub async fn probe(tools: &Tools, path: &Path) -> Result<Probe, String> {
    let output = Command::new(&tools.ffprobe)
        .args(["-v", "error", "-show_entries", "format=duration:stream=codec_type,width,height:stream_disposition=attached_pic:stream_side_data=rotation"])
        .args(["-of", "default=noprint_wrappers=1"])
        .arg(path)
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|err| format!("ffprobe failed to start: {err}"))?;
    if !output.status.success() {
        return Err("not a video or a sound ffprobe can read".to_string());
    }
    Ok(parse_probe(&String::from_utf8_lossy(&output.stdout)))
}

/// Runs ffmpeg, its errors in the message.
async fn ffmpeg(tools: &Tools, args: &[&str]) -> Result<(), String> {
    let output = Command::new(&tools.ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
        .args(args)
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|err| format!("ffmpeg failed to start: {err}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let errors = String::from_utf8_lossy(&output.stderr);
        Err(format!("ffmpeg failed: {}", errors.lines().last().unwrap_or("unknown error")))
    }
}

/// Runs a conversion of `seconds` of media, telling `progress` how far it
/// is (0 to 100) as ffmpeg writes it (`-progress`, every half second).
async fn ffmpeg_with_progress(tools: &Tools, args: &[&str], seconds: f64, progress: &(dyn Fn(u8) + Sync)) -> Result<(), String> {
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
    let mut child = Command::new(&tools.ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-nostats", "-progress", "pipe:1"])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|err| format!("ffmpeg failed to start: {err}"))?;
    let stdout = child.stdout.take().ok_or("ffmpeg has no output")?;
    let mut stderr = child.stderr.take().ok_or("ffmpeg has no output")?;
    // Errors are few lines (`-loglevel error`), read beside the progress.
    let errors = tokio::spawn(async move {
        let mut errors = String::new();
        let _ = stderr.read_to_string(&mut errors).await;
        errors
    });
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        // `out_time_us=12345678`: how much of the media is done.
        if let Some(done) = line.strip_prefix("out_time_us=").and_then(|value| value.trim().parse::<f64>().ok())
            && seconds > 0.0
        {
            progress((done / 1_000_000.0 / seconds * 100.0).clamp(0.0, 99.0) as u8);
        }
    }
    let status = child.wait().await.map_err(|err| format!("ffmpeg failed: {err}"))?;
    let errors = errors.await.unwrap_or_default();
    if status.success() {
        Ok(())
    } else {
        Err(format!("ffmpeg failed: {}", errors.lines().last().unwrap_or("unknown error")))
    }
}

/// A quality of videos, like PeerTube's resolutions: named by the short
/// side (720p is 1280×720, or 720×1280 for a portrait video).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Level {
    pub lines: u32,
    /// Quality of x264: lower is better and larger. Lower levels are
    /// compressed harder: few pixels, and seen on small screens.
    pub crf: u8,
    /// The most bitrate x264 may use, so the video streams on slow
    /// connections (kb/s).
    pub max_kbps: u32,
    /// The sound (AAC, kb/s).
    pub audio_kbps: u32,
}

/// From 4K to 144p.
pub const LEVELS: [Level; 8] = [
    Level { lines: 2160, crf: 22, max_kbps: 20_000, audio_kbps: 192 },
    Level { lines: 1440, crf: 22, max_kbps: 12_000, audio_kbps: 192 },
    Level { lines: 1080, crf: 23, max_kbps: 8_000, audio_kbps: 160 },
    Level { lines: 720, crf: 24, max_kbps: 5_000, audio_kbps: 128 },
    Level { lines: 480, crf: 26, max_kbps: 2_500, audio_kbps: 128 },
    Level { lines: 360, crf: 28, max_kbps: 1_200, audio_kbps: 96 },
    Level { lines: 240, crf: 31, max_kbps: 700, audio_kbps: 64 },
    Level { lines: 144, crf: 34, max_kbps: 400, audio_kbps: 48 },
];

/// The source's own level is compressed lightly: it's the best copy kept.
pub const SOURCE_CRF: u8 = 20;

/// The levels made for a video of `width`×`height`: its own size (at most
/// `max_lines`), lightly compressed, then every smaller level, never larger
/// than the source. A source under 144p gets one level, its own size.
pub fn levels_for(width: u32, height: u32, max_lines: u32) -> Vec<Level> {
    let short_side = width.min(height).max(2);
    let mut levels: Vec<Level> = LEVELS.iter().copied().filter(|level| level.lines <= short_side.min(max_lines)).collect();
    match levels.first_mut() {
        // Between two levels (900 lines): its own size is kept as the top
        // level, the next one down comes after.
        Some(top) if top.lines < short_side.min(max_lines) => {
            let mut own = *top;
            own.lines = short_side.min(max_lines) / 2 * 2;
            own.crf = SOURCE_CRF;
            levels.insert(0, own);
        }
        Some(top) => top.crf = SOURCE_CRF,
        None => levels.push(Level { lines: short_side / 2 * 2, crf: SOURCE_CRF, ..LEVELS[LEVELS.len() - 1] }),
    }
    levels
}

/// The qualities of a live stream sent at `width`×`height`: its own size
/// (at most 1080p), then 720p, 480p and 360p below it. Fewer than uploads:
/// they're all made at once, as it's sent.
pub fn live_levels(width: u32, height: u32) -> Vec<Level> {
    let own = width.min(height).clamp(144, 1080) / 2 * 2;
    let mut levels = vec![LEVELS.iter().find(|level| level.lines <= own).map_or(LEVELS[LEVELS.len() - 1], |level| Level { lines: own, ..*level })];
    levels.extend(LEVELS.iter().filter(|level| matches!(level.lines, 720 | 480 | 360) && level.lines < own));
    levels
}

/// How videos are encoded (`VIDEO_PRESET`, `VIDEO_MAX_LINES`).
#[derive(Clone, Debug)]
pub struct VideoSettings {
    /// x264's speed: `slow` makes smaller files of the same quality than
    /// `medium`, and takes about twice as long.
    pub preset: String,
    pub max_lines: u32,
}

static VIDEO_SETTINGS: OnceLock<VideoSettings> = OnceLock::new();

pub fn set_video_settings(settings: VideoSettings) {
    let _ = VIDEO_SETTINGS.set(settings);
}

pub fn video_settings() -> VideoSettings {
    VIDEO_SETTINGS.get().cloned().unwrap_or(VideoSettings { preset: "slow".to_string(), max_lines: 2160 })
}

/// A video at every level of `outputs` in one run of ffmpeg (the source is
/// read once): MP4, H.264 and AAC, the start of the file holding its index
/// so it plays while loading, a key frame every 2 seconds for seeking.
/// Without the source's metadata (place, camera...), like photos.
/// `portrait` scales by the width instead of the height.
pub async fn convert_video(
    tools: &Tools,
    input: &Path,
    outputs: &[(Level, std::path::PathBuf)],
    portrait: bool,
    seconds: f64,
    progress: &(dyn Fn(u8) + Sync),
) -> Result<(), String> {
    let input = path_str(input)?;
    let settings = video_settings();
    // One decoded picture split into a scaled copy per level.
    let labels: Vec<String> = (0..outputs.len()).map(|index| format!("[v{index}]")).collect();
    let mut filter = format!("[0:v:0]split={}{}", outputs.len(), labels.concat());
    for (index, (level, _)) in outputs.iter().enumerate() {
        let size = if portrait { format!("{}:-2", level.lines) } else { format!("-2:{}", level.lines) };
        filter.push_str(&format!(";[v{index}]scale={size}:flags=lanczos,setsar=1[o{index}]"));
    }
    let mut args: Vec<String> = ["-i", input, "-filter_complex", &filter, "-map_metadata", "-1"].map(str::to_string).to_vec();
    for (index, (level, output)) in outputs.iter().enumerate() {
        args.extend([format!("[o{index}]"), "0:a:0?".to_string()].into_iter().flat_map(|stream| ["-map".to_string(), stream]));
        args.extend(["-c:v", tools.video_encoder, "-pix_fmt", "yuv420p"].map(str::to_string));
        if tools.video_encoder == "libx264" {
            args.extend([
                "-preset".to_string(),
                settings.preset.clone(),
                "-crf".to_string(),
                level.crf.to_string(),
                "-profile:v".to_string(),
                "high".to_string(),
            ]);
        } else {
            // Without x264 there is no CRF: the cap is the bitrate.
            args.extend(["-b:v".to_string(), format!("{}k", level.max_kbps * 3 / 4)]);
        }
        args.extend([
            "-maxrate".to_string(),
            format!("{}k", level.max_kbps),
            "-bufsize".to_string(),
            format!("{}k", level.max_kbps * 2),
            "-force_key_frames".to_string(),
            "expr:gte(t,n_forced*2)".to_string(),
            "-c:a".to_string(),
            "aac".to_string(),
            "-b:a".to_string(),
            format!("{}k", level.audio_kbps),
            "-movflags".to_string(),
            "+faststart".to_string(),
            path_str(output)?.to_string(),
        ]);
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    ffmpeg_with_progress(tools, &args, seconds, progress).await
}

/// A sound for every podcast app: MP3 at 128 kb/s (AAC in M4A without an
/// MP3 encoder), without its metadata.
pub async fn convert_audio(tools: &Tools, input: &Path, output: &Path, seconds: f64, progress: &(dyn Fn(u8) + Sync)) -> Result<(), String> {
    let (input, output) = (path_str(input)?, path_str(output)?);
    let args = ["-i", input, "-vn", "-map", "0:a:0", "-map_metadata", "-1", "-c:a", tools.audio_encoder, "-b:a", "128k", output];
    ffmpeg_with_progress(tools, &args, seconds, progress).await
}

/// The extension and media type of `convert_audio`'s files.
pub fn audio_format(tools: &Tools) -> (&'static str, &'static str) {
    match tools.audio_encoder {
        "libmp3lame" => ("mp3", "audio/mpeg"),
        _ => ("m4a", "audio/mp4"),
    }
}

/// The picture of a video: a frame a quarter in (at most a minute in),
/// like PeerTube's thumbnails, at most 1280 pixels wide.
pub async fn poster(tools: &Tools, video: &Path, seconds: f64, output: &Path) -> Result<(), String> {
    let at = format!("{:.2}", (seconds / 4.0).min(60.0));
    let (video, output) = (path_str(video)?, path_str(output)?);
    ffmpeg(tools, &["-ss", &at, "-i", video, "-frames:v", "1", "-vf", "scale='min(1280,iw)':-2", "-q:v", "3", output]).await
}

/// The size of thumbnails uploaded for videos, like PeerTube's and YouTube's.
pub const THUMBNAIL_SIZE: (u32, u32) = (1280, 720);

/// A picture as a video's thumbnail: 1280×720 JPEG, scaled and cropped to
/// fill it (the middle is kept), the first frame of an animated GIF.
pub async fn thumbnail(tools: &Tools, picture: &Path, output: &Path) -> Result<(), String> {
    let (width, height) = THUMBNAIL_SIZE;
    let filter = format!("scale={width}:{height}:force_original_aspect_ratio=increase,crop={width}:{height},setsar=1");
    let (picture, output) = (path_str(picture)?, path_str(output)?);
    ffmpeg(tools, &["-i", picture, "-frames:v", "1", "-map_metadata", "-1", "-vf", &filter, "-q:v", "3", output]).await
}

/// Joins the HLS segments of a finished live stream into one MP4: its replay.
pub async fn hls_to_mp4(tools: &Tools, playlist: &Path, output: &Path) -> Result<(), String> {
    let (playlist, output) = (path_str(playlist)?, path_str(output)?);
    ffmpeg(tools, &["-i", playlist, "-c", "copy", "-movflags", "+faststart", output]).await
}

fn path_str(path: &Path) -> Result<&str, String> {
    path.to_str().ok_or_else(|| format!("unusable path {}", path.display()))
}

/// `PT1H2M3S`, as ActivityPub (PeerTube, Funkwhale) writes durations.
pub fn iso_duration(seconds: i32) -> String {
    format!("PT{}S", seconds.max(0))
}

/// The reverse, also reading a plain number of seconds.
pub fn parse_iso_duration(value: &serde_json::Value) -> Option<i32> {
    if let Some(seconds) = value.as_f64() {
        return Some(seconds as i32);
    }
    let text = value.as_str()?.strip_prefix("PT")?;
    let (mut total, mut number) = (0.0, String::new());
    for letter in text.chars() {
        match letter {
            '0'..='9' | '.' => number.push(letter),
            'H' | 'M' | 'S' => {
                let value: f64 = number.parse().ok()?;
                total += value * match letter {
                    'H' => 3600.0,
                    'M' => 60.0,
                    _ => 1.0,
                };
                number.clear();
            }
            _ => return None,
        }
    }
    Some(total as i32)
}

/// `1:02:03` or `4:05`, as players show it.
pub fn duration_label(seconds: i32) -> String {
    let seconds = seconds.max(0);
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    match hours {
        0 => format!("{minutes}:{seconds:02}"),
        _ => format!("{hours}:{minutes:02}:{seconds:02}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_is_read() {
        let output = "codec_type=video\nwidth=1920\nheight=1080\ncodec_type=audio\nduration=12.5\n";
        assert_eq!(parse_probe(output), Probe { seconds: 12.5, width: Some(1920), height: Some(1080), has_video: true, has_audio: true });
        // An MP3 with a cover picture is a sound.
        let output = "codec_type=audio\ncodec_type=video\nwidth=500\nheight=500\nDISPOSITION:attached_pic=1\nduration=60\n";
        let probe = parse_probe(output);
        assert!(!probe.has_video && probe.has_audio && probe.width.is_none());
    }

    #[test]
    fn rotated_videos_are_turned() {
        let output = "codec_type=video\nwidth=1920\nheight=1080\nrotation=-90\nduration=2\n";
        let probe = parse_probe(output);
        assert_eq!((probe.width, probe.height), (Some(1080), Some(1920)));
    }

    #[test]
    fn levels_up_to_the_source() {
        let lines = |levels: Vec<Level>| levels.iter().map(|level| (level.lines, level.crf)).collect::<Vec<_>>();
        // 1080p: its own size lightly compressed, then harder and harder.
        assert_eq!(lines(levels_for(1920, 1080, 2160)), [(1080, 20), (720, 24), (480, 26), (360, 28), (240, 31), (144, 34)]);
        // Portrait 4K, by its short side.
        assert_eq!(levels_for(2160, 3840, 2160).len(), 8);
        // Limited to 1080p by the settings.
        assert_eq!(lines(levels_for(3840, 2160, 1080))[0], (1080, 20));
        // Between two levels: its own size first.
        assert_eq!(lines(levels_for(1600, 900, 2160))[..2], [(900, 20), (720, 24)]);
        // Tiny.
        assert_eq!(lines(levels_for(160, 100, 2160)), [(100, 20)]);
    }

    #[test]
    fn live_qualities() {
        let lines = |levels: Vec<Level>| levels.iter().map(|level| level.lines).collect::<Vec<_>>();
        assert_eq!(lines(live_levels(1920, 1080)), [1080, 720, 480, 360]);
        assert_eq!(lines(live_levels(3840, 2160)), [1080, 720, 480, 360]);
        assert_eq!(lines(live_levels(1280, 720)), [720, 480, 360]);
        assert_eq!(lines(live_levels(1600, 900)), [900, 720, 480, 360]);
        assert_eq!(lines(live_levels(640, 360)), [360]);
        // A phone held upright: by its short side.
        assert_eq!(lines(live_levels(720, 1280)), [720, 480, 360]);
    }

    #[test]
    fn durations() {
        assert_eq!(parse_iso_duration(&serde_json::json!("PT1H2M3S")), Some(3723));
        assert_eq!(parse_iso_duration(&serde_json::json!("PT225S")), Some(225));
        assert_eq!(parse_iso_duration(&serde_json::json!(42.7)), Some(42));
        assert_eq!(parse_iso_duration(&serde_json::json!("1:00")), None);
        assert_eq!(duration_label(3723), "1:02:03");
        assert_eq!(duration_label(65), "1:05");
        assert_eq!(iso_duration(65), "PT65S");
    }
}

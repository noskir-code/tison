//! Converts uploaded videos and episodes with ffmpeg, one at a time, like
//! PeerTube's transcoding jobs. The post waits as `processing` (only its
//! author sees it), then becomes `ready` and is sent to the Fediverse, or
//! `failed`.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex, OnceLock},
};

use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use tokio::sync::{Semaphore, watch};

use crate::{
    AppState,
    av::{self, Tools},
    entities::{
        media::{self, NewAttachment},
        video::{self, VideoFile},
    },
    handlers::{
        image::{self, ImageOwner},
        notifier,
    },
};

/// Uploads waiting for ffmpeg, outside `public`.
pub const SOURCES_DIR: &str = "cache/uploads";

/// How many conversions run at once (`VIDEO_CONVERSIONS`): each ffmpeg
/// already uses several cores. Set at start by `set_parallel`.
static SLOTS: OnceLock<Semaphore> = OnceLock::new();

fn slots() -> &'static Semaphore {
    SLOTS.get_or_init(|| Semaphore::new(2))
}

pub fn set_parallel(conversions: usize) {
    let _ = SLOTS.set(Semaphore::new(conversions.max(1)));
}

/// One conversion at a time for each user, their uploads in order: someone
/// uploading ten videos doesn't take every slot from the others.
static USER_TURNS: LazyLock<Mutex<HashMap<i32, Arc<tokio::sync::Mutex<()>>>>> = LazyLock::new(Mutex::default);

fn user_turn(user_id: i32) -> Arc<tokio::sync::Mutex<()>> {
    let mut turns = USER_TURNS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // Users with nothing converting anymore are forgotten.
    turns.retain(|_, turn| Arc::strong_count(turn) > 1);
    turns.entry(user_id).or_default().clone()
}

/// Where a conversion is, shown live on the post's page (see
/// `handlers::video::progress`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    /// Another upload converts first: one of the user's (`own`), or every
    /// slot is taken by others.
    Waiting { own: bool },
    /// How much is done, 0 to 99.
    Converting(u8),
    Ready,
    Failed,
}

impl Progress {
    pub fn to_json(self) -> Value {
        match self {
            Self::Waiting { own } => json!({ "state": "waiting", "own": own }),
            Self::Converting(percent) => json!({ "state": "converting", "percent": percent }),
            Self::Ready => json!({ "state": "ready" }),
            Self::Failed => json!({ "state": "failed" }),
        }
    }

    pub fn is_finished(self) -> bool {
        matches!(self, Self::Ready | Self::Failed)
    }
}

/// The conversions queued or going on, by post.
static PROGRESS: LazyLock<Mutex<HashMap<i64, watch::Sender<Progress>>>> = LazyLock::new(Mutex::default);

fn progress_map() -> std::sync::MutexGuard<'static, HashMap<i64, watch::Sender<Progress>>> {
    PROGRESS.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Tells the pages watching the post. A finished conversion is forgotten:
/// its pages still get this last value.
fn report(post_id: i64, progress: Progress) {
    let mut conversions = progress_map();
    if let Some(sender) = conversions.get(&post_id) {
        // Only every new percent, not each half second.
        sender.send_if_modified(|current| {
            let changed = *current != progress;
            *current = progress;
            changed
        });
    }
    if progress.is_finished() {
        conversions.remove(&post_id);
    }
}

/// The progress of a post's conversion, `None` when there is none.
pub fn watch(post_id: i64) -> Option<watch::Receiver<Progress>> {
    progress_map().get(&post_id).map(watch::Sender::subscribe)
}

/// Converts the post's upload, in the background.
pub fn enqueue(state: AppState, post_id: i64) {
    progress_map().insert(post_id, watch::channel(Progress::Waiting { own: false }).0);
    tokio::spawn(async move {
        let (Some(pool), Some(tools)) = (state.pool.clone(), av::tools()) else {
            report(post_id, Progress::Failed);
            return;
        };
        let user_id: Option<i32> = sqlx::query_scalar("SELECT user_id FROM posts WHERE id = $1")
            .bind(post_id)
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten()
            .flatten();
        let Some(user_id) = user_id else {
            progress_map().remove(&post_id);
            return;
        };
        // Their own uploads first, one by one, then a free slot.
        let turn = user_turn(user_id);
        let _own_turn = match turn.try_lock() {
            Ok(guard) => guard,
            Err(_) => {
                report(post_id, Progress::Waiting { own: true });
                turn.lock().await
            }
        };
        let _slot = match slots().try_acquire() {
            Ok(permit) => permit,
            Err(_) => {
                report(post_id, Progress::Waiting { own: false });
                let Ok(permit) = slots().acquire().await else {
                    return;
                };
                permit
            }
        };
        report(post_id, Progress::Converting(0));
        match convert(&pool, tools, post_id).await {
            Ok(true) => {
                tracing::info!("post {} converted", post_id);
                report(post_id, Progress::Ready);
                notifier::post_created(&state, &pool, post_id).await;
            }
            Ok(false) => {
                progress_map().remove(&post_id);
            }
            Err(err) => {
                tracing::warn!("failed to convert post {}: {}", post_id, err);
                if let Err(err) = failed(&pool, post_id).await {
                    tracing::error!("failed to mark post {} as failed: {}", post_id, err);
                }
                report(post_id, Progress::Failed);
            }
        }
    });
}

/// Uploads left waiting when the server stopped.
pub async fn resume(state: &AppState) {
    let Some(pool) = &state.pool else {
        return;
    };
    let waiting: Vec<i64> = sqlx::query_scalar("SELECT id FROM posts WHERE media_status = 'processing' ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    if !waiting.is_empty() {
        tracing::info!("converting {} uploads left waiting", waiting.len());
    }
    for post_id in waiting {
        enqueue(state.clone(), post_id);
    }
}

async fn failed(pool: &PgPool, post_id: i64) -> Result<(), sqlx::Error> {
    let source: Option<String> = sqlx::query_scalar("SELECT media_source FROM posts WHERE id = $1")
        .bind(post_id)
        .fetch_optional(pool)
        .await?
        .flatten();
    sqlx::query("UPDATE posts SET media_status = 'failed', media_source = NULL WHERE id = $1")
        .bind(post_id)
        .execute(pool)
        .await?;
    if let Some(source) = source {
        let _ = tokio::fs::remove_file(source).await;
    }
    Ok(())
}

/// `Ok(false)` when the post is gone or done already.
async fn convert(pool: &PgPool, tools: &Tools, post_id: i64) -> Result<bool, String> {
    let row = sqlx::query("SELECT user_id, kind, media_source, poster_url FROM posts WHERE id = $1 AND media_status = 'processing'")
        .bind(post_id)
        .fetch_optional(pool)
        .await
        .map_err(|err| err.to_string())?;
    let Some(row) = row else {
        return Ok(false);
    };
    let user_id: i32 = row.try_get("user_id").map_err(|err| err.to_string())?;
    let kind: String = row.try_get("kind").map_err(|err| err.to_string())?;
    let source: String = row
        .try_get::<Option<String>, _>("media_source")
        .map_err(|err| err.to_string())?
        .ok_or("no uploaded file")?;
    let source = PathBuf::from(source);
    // Uploaded with the video: no frame is taken then.
    let chosen_poster: Option<String> = row.try_get("poster_url").map_err(|err| err.to_string())?;

    let probe = av::probe(tools, &source).await?;
    let owner = ImageOwner::User(user_id);
    let is_video = kind == "video";
    if is_video && !probe.has_video {
        return Err("no video in the file".to_string());
    }
    if !is_video && !probe.has_audio {
        return Err("no sound in the file".to_string());
    }

    let progress = |percent| report(post_id, Progress::Converting(percent));
    // The files made: (url, path, level), the attachment's first.
    let (files, main) = match is_video {
        true => convert_levels(tools, owner, &source, &probe, &progress).await?,
        false => {
            let (extension, media_type) = av::audio_format(tools);
            let (output, url) = image::new_file(owner, &kind, extension).await.map_err(|err| err.to_string())?;
            if let Err(err) = av::convert_audio(tools, &source, &output, probe.seconds, &progress).await {
                let _ = tokio::fs::remove_file(&output).await;
                return Err(err);
            }
            let main = NewAttachment::new(url.clone(), media_type.to_string(), "", None, None);
            (vec![(url, output, None)], main)
        }
    };
    let urls: Vec<String> = files.iter().map(|(url, _, _)| url.clone()).collect();
    // Its real length, from the main file.
    let main_path = files.first().map(|(_, path, _)| path.clone()).unwrap_or_default();
    let result = av::probe(tools, &main_path).await.unwrap_or_default();
    let seconds = if result.seconds > 0.0 { result.seconds } else { probe.seconds };

    let poster_url = match (is_video, &chosen_poster) {
        (true, None) => make_poster(tools, owner, &main_path, seconds).await,
        _ => chosen_poster.clone(),
    };
    let video_files: Vec<VideoFile> = files.iter().filter_map(|(_, _, file)| file.clone()).collect();
    let saved = async {
        media::save_attachments(pool, media::Owner::Post(post_id), &[main]).await?;
        if is_video {
            video::save_files(pool, post_id, &video_files).await?;
        }
        sqlx::query(
            "UPDATE posts SET media_status = 'ready', media_source = NULL, duration_seconds = $2, poster_url = $3,
                    created_at = NOW(), updated_at = NOW()
             WHERE id = $1",
        )
        .bind(post_id)
        .bind(seconds.round() as i32)
        .bind(&poster_url)
        .execute(pool)
        .await
    }
    .await;
    if let Err(err) = saved {
        let mut files = urls;
        // Only a frame made here: a chosen thumbnail goes with the post.
        files.extend(poster_url.filter(|poster| chosen_poster.as_ref() != Some(poster)));
        media::remove_local_files(&files).await;
        return Err(format!("failed to save: {err}"));
    }
    let _ = tokio::fs::remove_file(&source).await;
    Ok(true)
}

/// A converted file: its URL, its path, and for a video its quality.
type Converted = (String, PathBuf, Option<VideoFile>);

/// Every level of a video (see `av::levels_for`), and its main attachment:
/// the best level up to 720p, what Mastodon and others play. The main file
/// comes first.
async fn convert_levels(
    tools: &Tools,
    owner: ImageOwner,
    source: &Path,
    probe: &av::Probe,
    progress: &(dyn Fn(u8) + Sync),
) -> Result<(Vec<Converted>, NewAttachment), String> {
    let (width, height) = (probe.width.unwrap_or(1280), probe.height.unwrap_or(720));
    let levels = av::levels_for(width, height, av::video_settings().max_lines);
    let mut outputs = Vec::new();
    let mut urls = Vec::new();
    for level in &levels {
        match image::new_file(owner, &format!("video-{}p", level.lines), "mp4").await {
            Ok((path, url)) => {
                outputs.push((*level, path));
                urls.push(url);
            }
            Err(err) => return Err(err.to_string()),
        }
    }
    if let Err(err) = av::convert_video(tools, source, &outputs, height > width, probe.seconds, progress).await {
        for (_, path) in &outputs {
            let _ = tokio::fs::remove_file(path).await;
        }
        return Err(err);
    }
    let mut files = Vec::new();
    for ((level, path), url) in outputs.into_iter().zip(urls) {
        let made = av::probe(tools, &path).await.unwrap_or_default();
        let size_bytes = tokio::fs::metadata(&path).await.map(|meta| meta.len() as i64).unwrap_or(0);
        let file = VideoFile {
            lines: level.lines as i32,
            width: made.width.unwrap_or(0) as i32,
            height: made.height.unwrap_or(0) as i32,
            url: url.clone(),
            size_bytes,
        };
        files.push((url, path, Some(file)));
    }
    let main_index = files
        .iter()
        .position(|(_, _, file)| file.as_ref().is_some_and(|file| file.lines <= 720))
        .unwrap_or(files.len() - 1);
    let main_file = files.remove(main_index);
    let attachment = {
        let file = main_file.2.as_ref().expect("a video file");
        NewAttachment::new(main_file.0.clone(), "video/mp4".to_string(), "", Some(file.width), Some(file.height))
    };
    files.insert(0, main_file);
    Ok((files, attachment))
}

/// The picture of a video, `None` if ffmpeg can't make it.
pub async fn make_poster(tools: &Tools, owner: ImageOwner, video: &Path, seconds: f64) -> Option<String> {
    let (path, url) = image::new_file(owner, "poster", "jpg").await.ok()?;
    match av::poster(tools, video, seconds, &path).await {
        Ok(()) => Some(url),
        Err(err) => {
            tracing::warn!("no picture for {}: {}", video.display(), err);
            let _ = tokio::fs::remove_file(path).await;
            None
        }
    }
}

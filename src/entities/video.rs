//! Live streams, like Owncast's and PeerTube's: each user has a stream key
//! for OBS (see `av::rtmp`). Going live makes a post of kind `live`, seen
//! in feeds and on the Videos page; its replay becomes a video. Videos
//! themselves are posts, see `post::Kind`.

use std::collections::HashMap;

use sqlx::{PgPool, Row};

use crate::entities::schema::{Column, Index, Table};

/// Kept up to date automatically at startup, see `entities::schema`.
pub const LIVE_TABLE: Table = Table {
    name: "live_streams",
    columns: &[
        Column::new("user_id", "INTEGER").primary_key().references("users(id) ON DELETE CASCADE"),
        // Given to OBS: whoever has it streams as this user.
        Column::new("stream_key", "TEXT"),
        // Shown while live, for the next stream.
        Column::new("title", "TEXT").default("''"),
        Column::new("description", "TEXT").default("''"),
        // The stream is kept as a video when it ends, like PeerTube's replays.
        Column::new("keep_replay", "BOOLEAN").default("TRUE"),
        // The post of the stream going on.
        Column::new("post_id", "BIGINT").nullable().references("posts(id) ON DELETE SET NULL"),
        Column::new("created_at", "TIMESTAMPTZ").default("NOW()"),
    ],
    indexes: &[Index::unique("live_streams_stream_key_idx", "stream_key")],
};

pub const TITLE_MAX_CHARS: usize = 120;
pub const DESCRIPTION_MAX_CHARS: usize = 2_000;

/// A user's live settings.
pub struct LiveSettings {
    pub stream_key: String,
    pub title: String,
    pub description: String,
    pub keep_replay: bool,
    /// The post of the stream going on.
    pub live_post_id: Option<i64>,
}

fn new_key() -> String {
    format!("live_{}", crate::handlers::session::random_part())
}

/// Their settings, made with a new key the first time.
pub async fn settings(pool: &PgPool, user_id: i32) -> Result<LiveSettings, sqlx::Error> {
    sqlx::query("INSERT INTO live_streams (user_id, stream_key) VALUES ($1, $2) ON CONFLICT (user_id) DO NOTHING")
        .bind(user_id)
        .bind(new_key())
        .execute(pool)
        .await?;
    let row = sqlx::query(
        "SELECT ls.stream_key, ls.title, ls.description, ls.keep_replay, p.id AS live_post_id
         FROM live_streams ls
         LEFT JOIN posts p ON p.id = ls.post_id AND p.media_status = 'live'
         WHERE ls.user_id = $1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(LiveSettings {
        stream_key: row.try_get("stream_key")?,
        title: row.try_get("title")?,
        description: row.try_get("description")?,
        keep_replay: row.try_get("keep_replay")?,
        live_post_id: row.try_get("live_post_id")?,
    })
}

pub async fn update_settings(pool: &PgPool, user_id: i32, title: &str, description: &str, keep_replay: bool) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE live_streams SET title = $2, description = $3, keep_replay = $4 WHERE user_id = $1")
        .bind(user_id)
        .bind(title)
        .bind(description)
        .bind(keep_replay)
        .execute(pool)
        .await?;
    Ok(())
}

/// A new key: the old one stops working, like resetting it in OBS.
pub async fn reset_key(pool: &PgPool, user_id: i32) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE live_streams SET stream_key = $2 WHERE user_id = $1")
        .bind(user_id)
        .bind(new_key())
        .execute(pool)
        .await?;
    Ok(())
}

/// Who streams with this key.
pub struct Streamer {
    pub user_id: i32,
    pub username: String,
    pub title: String,
    pub description: String,
    pub keep_replay: bool,
    /// Already live: a second stream is refused.
    pub live: bool,
}

/// The user of a key, unless suspended.
pub async fn find_by_key(pool: &PgPool, key: &str) -> Result<Option<Streamer>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT ls.user_id, u.username, ls.title, ls.description, ls.keep_replay,
                EXISTS (SELECT 1 FROM posts p WHERE p.id = ls.post_id AND p.media_status = 'live') AS live
         FROM live_streams ls
         JOIN users u ON u.id = ls.user_id
         WHERE ls.stream_key = $1 AND u.suspended_at IS NULL",
    )
    .bind(key)
    .fetch_optional(pool)
    .await?;
    row.map(|row| {
        Ok(Streamer {
            user_id: row.try_get("user_id")?,
            username: row.try_get("username")?,
            title: row.try_get("title")?,
            description: row.try_get("description")?,
            keep_replay: row.try_get("keep_replay")?,
            live: row.try_get("live")?,
        })
    })
    .transpose()
}

/// Going live: its post, public, playing `playlist` (the HLS of `av::live`).
pub async fn start(pool: &PgPool, streamer: &Streamer, playlist: &str) -> Result<i64, sqlx::Error> {
    let title = match streamer.title.trim() {
        "" => format!("{} is live", streamer.username),
        title => title.to_string(),
    };
    let post_id: i64 = sqlx::query_scalar(
        "INSERT INTO posts (user_id, kind, title, body, visibility, media_status, media_source)
         VALUES ($1, 'live', $2, $3, 'public', 'live', $4) RETURNING id",
    )
    .bind(streamer.user_id)
    .bind(title)
    .bind(&streamer.description)
    .bind(playlist)
    .fetch_one(pool)
    .await?;
    sqlx::query("UPDATE live_streams SET post_id = $2 WHERE user_id = $1")
        .bind(streamer.user_id)
        .bind(post_id)
        .execute(pool)
        .await?;
    Ok(post_id)
}

/// The stream stopped: its post says so, without a player.
pub async fn end(pool: &PgPool, post_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE posts SET media_status = 'ended', media_source = NULL, updated_at = NOW() WHERE id = $1 AND kind = 'live'")
        .bind(post_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Streams still marked live from before a restart: they stopped with it.
pub async fn end_all(pool: &PgPool) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query("UPDATE posts SET media_status = 'ended', media_source = NULL WHERE kind = 'live' AND media_status = 'live'")
        .execute(pool)
        .await?
        .rows_affected())
}

/// The stream going on of a user, by name: `/live/{username}`.
pub async fn live_post_of(pool: &PgPool, username: &str) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT p.id FROM live_streams ls
         JOIN users u ON u.id = ls.user_id
         JOIN posts p ON p.id = ls.post_id
         WHERE LOWER(u.username) = LOWER($1) AND p.media_status = 'live'",
    )
    .bind(username)
    .fetch_optional(pool)
    .await
}

/// The files of a video, one per quality (see `av::LEVELS`), like
/// PeerTube's resolutions. Its main attachment is one of them.
pub const FILES_TABLE: Table = Table {
    name: "video_files",
    columns: &[
        Column::new("post_id", "BIGINT").references("posts(id) ON DELETE CASCADE"),
        // The level: the short side, like 720 for 720p.
        Column::new("lines", "INTEGER"),
        Column::new("width", "INTEGER"),
        Column::new("height", "INTEGER"),
        Column::new("url", "TEXT"),
        Column::new("size_bytes", "BIGINT").default("0"),
    ],
    indexes: &[Index::new("video_files_post_id_idx", "post_id, lines DESC")],
};

/// One quality of a video.
#[derive(Clone, Debug)]
pub struct VideoFile {
    pub lines: i32,
    pub width: i32,
    pub height: i32,
    pub url: String,
    pub size_bytes: i64,
}

impl VideoFile {
    /// `1080p`, or `4K` like on PeerTube.
    pub fn label(&self) -> String {
        match self.lines {
            2160 => "4K".to_string(),
            lines => format!("{lines}p"),
        }
    }

    /// `12.3 MB`.
    pub fn size_label(&self) -> String {
        let megabytes = self.size_bytes as f64 / 1024.0 / 1024.0;
        if megabytes >= 1024.0 {
            format!("{:.1} GB", megabytes / 1024.0)
        } else {
            format!("{megabytes:.1} MB")
        }
    }
}

/// Replaces the files of a video.
pub async fn save_files(pool: &PgPool, post_id: i64, files: &[VideoFile]) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    sqlx::query("DELETE FROM video_files WHERE post_id = $1").bind(post_id).execute(&mut *transaction).await?;
    for file in files {
        sqlx::query("INSERT INTO video_files (post_id, lines, width, height, url, size_bytes) VALUES ($1, $2, $3, $4, $5, $6)")
            .bind(post_id)
            .bind(file.lines)
            .bind(file.width)
            .bind(file.height)
            .bind(&file.url)
            .bind(file.size_bytes)
            .execute(&mut *transaction)
            .await?;
    }
    transaction.commit().await
}

/// The files of these posts, best first.
pub async fn files_for(pool: &PgPool, post_ids: &[i64]) -> Result<HashMap<i64, Vec<VideoFile>>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT post_id, lines, width, height, url, size_bytes FROM video_files
         WHERE post_id = ANY($1) ORDER BY post_id, lines DESC",
    )
    .bind(post_ids)
    .fetch_all(pool)
    .await?;
    let mut files: HashMap<i64, Vec<VideoFile>> = HashMap::new();
    for row in rows {
        files.entry(row.try_get("post_id")?).or_default().push(VideoFile {
            lines: row.try_get("lines")?,
            width: row.try_get("width")?,
            height: row.try_get("height")?,
            url: row.try_get("url")?,
            size_bytes: row.try_get("size_bytes")?,
        });
    }
    Ok(files)
}

/// Their addresses, for deleting them with the post.
pub async fn file_urls(pool: &PgPool, post_id: i64) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT url FROM video_files WHERE post_id = $1").bind(post_id).fetch_all(pool).await
}

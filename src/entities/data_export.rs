//! Copies of a user's data they asked for (GDPR art. 15 and 20, Québec's
//! Law 25 art. 27): a ZIP made in the background (see `handlers::account`),
//! downloaded from their settings, deleted after `KEEP_DAYS`.

use sqlx::{
    PgPool, Row,
    types::chrono::{DateTime, Utc},
};

use crate::entities::schema::{Column, Index, Table};

/// Kept up to date automatically at startup, see `entities::schema`.
pub const TABLE: Table = Table {
    name: "data_exports",
    columns: &[
        Column::new("id", "BIGSERIAL").primary_key(),
        Column::new("user_id", "INTEGER").references("users(id) ON DELETE CASCADE"),
        // Random: the file's name in `DIR`, and its address.
        Column::new("token", "TEXT"),
        // `preparing`, `ready` or `failed`.
        Column::new("status", "TEXT").default("'preparing'"),
        Column::new("size_bytes", "BIGINT").default("0"),
        Column::new("created_at", "TIMESTAMPTZ").default("NOW()"),
    ],
    indexes: &[Index::unique("data_exports_token_idx", "token"), Index::new("data_exports_user_idx", "user_id, created_at DESC")],
};

/// Where the archives are, out of `public`: only their owner downloads them.
pub const DIR: &str = "cache/exports";
/// An archive is deleted after this.
pub const KEEP_DAYS: i32 = 7;
/// One new archive at most every this many hours.
pub const EVERY_HOURS: i32 = 24;

pub struct DataExport {
    pub token: String,
    pub status: String,
    pub size_bytes: i64,
    pub created_at: DateTime<Utc>,
}

impl DataExport {
    pub fn is_ready(&self) -> bool {
        self.status == "ready"
    }

    pub fn is_preparing(&self) -> bool {
        self.status == "preparing"
    }

    pub fn created_label(&self) -> String {
        crate::i18n::date_time(self.created_at)
    }

    /// `12.3 MB`.
    pub fn size_label(&self) -> String {
        let megabytes = self.size_bytes as f64 / 1024.0 / 1024.0;
        if megabytes >= 1024.0 { format!("{:.1} GB", megabytes / 1024.0) } else { format!("{megabytes:.1} MB") }
    }

    /// Can't be asked again before this.
    pub fn too_recent(&self) -> bool {
        self.status != "failed" && self.created_at.timestamp() > Utc::now().timestamp() - i64::from(EVERY_HOURS) * 3600
    }

    pub fn path(&self) -> std::path::PathBuf {
        std::path::Path::new(DIR).join(format!("{}.zip", self.token))
    }
}

fn from_row(row: &sqlx::postgres::PgRow) -> Result<DataExport, sqlx::Error> {
    Ok(DataExport {
        token: row.try_get("token")?,
        status: row.try_get("status")?,
        size_bytes: row.try_get("size_bytes")?,
        created_at: row.try_get("created_at")?,
    })
}

/// The user's last one, when it's still kept.
pub async fn latest(pool: &PgPool, user_id: i32) -> Result<Option<DataExport>, sqlx::Error> {
    sqlx::query(
        "SELECT token, status, size_bytes, created_at FROM data_exports
         WHERE user_id = $1 AND created_at > NOW() - make_interval(days => $2)
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(user_id)
    .bind(KEEP_DAYS)
    .fetch_optional(pool)
    .await?
    .as_ref()
    .map(from_row)
    .transpose()
}

pub async fn find(pool: &PgPool, user_id: i32, token: &str) -> Result<Option<DataExport>, sqlx::Error> {
    sqlx::query("SELECT token, status, size_bytes, created_at FROM data_exports WHERE user_id = $1 AND token = $2")
        .bind(user_id)
        .bind(token)
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(from_row)
        .transpose()
}

pub async fn create(pool: &PgPool, user_id: i32, token: &str) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO data_exports (user_id, token) VALUES ($1, $2)").bind(user_id).bind(token).execute(pool).await?;
    Ok(())
}

pub async fn finish(pool: &PgPool, token: &str, size: Option<u64>) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE data_exports SET status = $2, size_bytes = $3 WHERE token = $1")
        .bind(token)
        .bind(if size.is_some() { "ready" } else { "failed" })
        .bind(size.map_or(0, |size| size as i64))
        .execute(pool)
        .await?;
    Ok(())
}

/// The archives of `user_id`, or older than `KEEP_DAYS` for everyone:
/// removed with their files.
pub async fn remove(pool: &PgPool, user_id: Option<i32>) -> Result<usize, sqlx::Error> {
    let tokens: Vec<String> = sqlx::query_scalar(
        "DELETE FROM data_exports
         WHERE ($1::INTEGER IS NOT NULL AND user_id = $1) OR ($1::INTEGER IS NULL AND created_at <= NOW() - make_interval(days => $2))
         RETURNING token",
    )
    .bind(user_id)
    .bind(KEEP_DAYS)
    .fetch_all(pool)
    .await?;
    for token in &tokens {
        let _ = tokio::fs::remove_file(std::path::Path::new(DIR).join(format!("{token}.zip"))).await;
    }
    Ok(tokens.len())
}

//! Pictures uploaded into articles (see `handlers::article_picture`): the
//! Markdown holds their address, this table who uploaded them. A picture
//! goes once no post shows it anymore: when its article is edited or
//! deleted, or a day after an upload that was never published.

use sqlx::PgPool;

use crate::entities::schema::{Column, Index, Table};

/// Kept up to date automatically at startup, see `entities::schema`.
pub const TABLE: Table = Table {
    name: "article_pictures",
    columns: &[
        // Like `/public/uploads/users/12/article-....jpg`.
        Column::new("url", "TEXT").primary_key(),
        Column::new("user_id", "INTEGER").references("users(id) ON DELETE CASCADE"),
        Column::new("created_at", "TIMESTAMPTZ").default("NOW()"),
    ],
    indexes: &[Index::new("article_pictures_created_at_idx", "created_at")],
};

/// Hours an upload waits to be published before being deleted.
const UNPUBLISHED_HOURS: i32 = 24;

pub async fn add(pool: &PgPool, user_id: i32, url: &str) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO article_pictures (url, user_id) VALUES ($1, $2) ON CONFLICT (url) DO NOTHING")
        .bind(url)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Deletes, among `urls`, the uploaded pictures no post shows anymore.
/// Others' pictures and those of the web are left alone.
pub async fn remove_unused(pool: &PgPool, urls: &[String]) -> Result<(), sqlx::Error> {
    if urls.is_empty() {
        return Ok(());
    }
    let removed: Vec<String> = sqlx::query_scalar(
        "DELETE FROM article_pictures a WHERE a.url = ANY($1)
           AND NOT EXISTS (SELECT 1 FROM posts p WHERE POSITION(a.url IN p.body) > 0)
         RETURNING a.url",
    )
    .bind(urls)
    .fetch_all(pool)
    .await?;
    crate::entities::media::remove_local_files(&removed).await;
    Ok(())
}

/// Deletes the uploads never published, at startup and once a day.
pub async fn clean_unpublished(pool: &PgPool) -> Result<usize, sqlx::Error> {
    let removed: Vec<String> = sqlx::query_scalar(
        "DELETE FROM article_pictures a WHERE a.created_at < NOW() - make_interval(hours => $1)
           AND NOT EXISTS (SELECT 1 FROM posts p WHERE POSITION(a.url IN p.body) > 0)
         RETURNING a.url",
    )
    .bind(UNPUBLISHED_HOURS)
    .fetch_all(pool)
    .await?;
    crate::entities::media::remove_local_files(&removed).await;
    Ok(removed.len())
}

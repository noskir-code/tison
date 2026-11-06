//! Hashtags a user follows, like on Mastodon: the public posts with them,
//! here and from other servers known here (followed accounts, relays, see
//! `API::relay`), show in their feed (see `post::feed`).

use sqlx::PgPool;

use crate::entities::schema::{Column, Index, Table};

/// Hashtags one user can follow.
pub const MAX_PER_USER: i64 = 100;

pub const TABLE: Table = Table {
    name: "hashtag_follows",
    columns: &[
        Column::new("user_id", "INTEGER").references("users(id) ON DELETE CASCADE"),
        // Lowercase, without `#`, see `hashtag::normalize`.
        Column::new("tag", "TEXT"),
        Column::new("created_at", "TIMESTAMPTZ").default("NOW()"),
    ],
    indexes: &[Index::unique("hashtag_follows_user_tag_idx", "user_id, tag")],
};

/// Follows `tag` (normalized). `false` when the user follows too many.
pub async fn follow(pool: &PgPool, user_id: i32, tag: &str) -> Result<bool, sqlx::Error> {
    let added = sqlx::query(
        "INSERT INTO hashtag_follows (user_id, tag)
         SELECT $1, $2 WHERE (SELECT COUNT(*) FROM hashtag_follows WHERE user_id = $1) < $3
         ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(tag)
    .bind(MAX_PER_USER)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(added > 0 || is_following(pool, user_id, tag).await?)
}

pub async fn unfollow(pool: &PgPool, user_id: i32, tag: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM hashtag_follows WHERE user_id = $1 AND tag = $2").bind(user_id).bind(tag).execute(pool).await?;
    Ok(())
}

pub async fn is_following(pool: &PgPool, user_id: i32, tag: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM hashtag_follows WHERE user_id = $1 AND tag = $2)")
        .bind(user_id)
        .bind(tag)
        .fetch_one(pool)
        .await
}

/// The hashtags the user follows, alphabetically.
pub async fn list(pool: &PgPool, user_id: i32) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT tag FROM hashtag_follows WHERE user_id = $1 ORDER BY tag").bind(user_id).fetch_all(pool).await
}

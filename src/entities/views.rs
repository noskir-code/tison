//! Views of videos: counted once a viewer has watched a while (see
//! `setupPlayers` in assets/system.js), once per viewer and day, never for
//! the video's author. Kept per day for the creator dashboard (see
//! `entities::creator_stats`), the total on the post for its pages.

use rsa::sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::entities::schema::{Column, Index, Table};

/// Views of each post, per day (UTC).
pub const DAILY_TABLE: Table = Table {
    name: "post_views_daily",
    columns: &[
        Column::new("post_id", "BIGINT").references("posts(id) ON DELETE CASCADE"),
        Column::new("day", "DATE"),
        Column::new("views", "INTEGER").default("0"),
    ],
    indexes: &[Index::unique("post_views_daily_post_day_idx", "post_id, day")],
};

/// Who viewed a post today, hashed: a view counts once per viewer and day.
/// Deleted after two days, see `clean`.
pub const MARKS_TABLE: Table = Table {
    name: "post_view_marks",
    columns: &[
        Column::new("post_id", "BIGINT").references("posts(id) ON DELETE CASCADE"),
        Column::new("viewer", "TEXT"),
        Column::new("day", "DATE").default("CURRENT_DATE"),
    ],
    indexes: &[
        Index::unique("post_view_marks_idx", "post_id, viewer, day"),
        Index::new("post_view_marks_day_idx", "day"),
    ],
};

/// The viewer as stored: a hash, so the marks don't say who watched what.
fn viewer_hash(viewer: &str) -> String {
    Sha256::digest(viewer.as_bytes()).iter().take(16).map(|byte| format!("{byte:02x}")).collect()
}

/// Counts a view of the video `post_id` by `viewer` (`user:12`, or the
/// random id of a visitor's browser). `false` when it didn't count: already
/// viewed today, by its author, or not a ready video.
pub async fn record(pool: &PgPool, post_id: i64, viewer: &str, viewer_user_id: Option<i32>) -> Result<bool, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let counted = sqlx::query(
        "INSERT INTO post_view_marks (post_id, viewer, day)
         SELECT p.id, $2, CURRENT_DATE FROM posts p
         WHERE p.id = $1 AND p.kind = 'video' AND p.media_status = 'ready'
           AND p.user_id IS DISTINCT FROM $3
         ON CONFLICT (post_id, viewer, day) DO NOTHING",
    )
    .bind(post_id)
    .bind(viewer_hash(viewer))
    .bind(viewer_user_id)
    .execute(&mut *transaction)
    .await?
    .rows_affected()
        > 0;
    if counted {
        sqlx::query(
            "INSERT INTO post_views_daily (post_id, day, views) VALUES ($1, CURRENT_DATE, 1)
             ON CONFLICT (post_id, day) DO UPDATE SET views = post_views_daily.views + 1",
        )
        .bind(post_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("UPDATE posts SET view_count = view_count + 1 WHERE id = $1")
            .bind(post_id)
            .execute(&mut *transaction)
            .await?;
    }
    transaction.commit().await?;
    Ok(counted)
}

/// Forgets who viewed what before yesterday: only the counts stay.
pub async fn clean(pool: &PgPool) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query("DELETE FROM post_view_marks WHERE day < CURRENT_DATE - 1")
        .execute(pool)
        .await?
        .rows_affected())
}

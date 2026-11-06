//! Blocks between people (Server rules, 2): who blocks someone stops seeing
//! them, and they can't reach them anymore: no follow, friend request,
//! message, comment or notification, either way. Accounts of other servers
//! can be blocked too; they are sent a Block, which Mastodon follows.

use sqlx::{PgPool, Row};

use crate::entities::schema::{Column, Index, Table};

/// Kept up to date automatically at startup, see `entities::schema`.
pub const TABLE: Table = Table {
    name: "user_blocks",
    columns: &[
        Column::new("id", "BIGSERIAL").primary_key(),
        Column::new("user_id", "INTEGER").references("users(id) ON DELETE CASCADE"),
        // One of the two: a user here, or an account of another server.
        Column::new("blocked_user_id", "INTEGER").nullable().references("users(id) ON DELETE CASCADE"),
        Column::new("blocked_actor_id", "BIGINT").nullable().references("federation_actors(id) ON DELETE CASCADE"),
        Column::new("created_at", "TIMESTAMPTZ").default("NOW()"),
    ],
    indexes: &[
        Index::unique("user_blocks_user_idx", "user_id, blocked_user_id"),
        Index::unique("user_blocks_actor_idx", "user_id, blocked_actor_id"),
        Index::new("user_blocks_blocked_idx", "blocked_user_id"),
    ],
};

/// Whether the users `$a` and `$b` (SQL) block each other, either way. A
/// macro for `concat!`.
macro_rules! blocked_between {
    ($a:literal, $b:literal) => {
        concat!(
            "EXISTS (SELECT 1 FROM user_blocks bb WHERE (bb.user_id = ", $a, " AND bb.blocked_user_id = ", $b, ")
                OR (bb.user_id = ", $b, " AND bb.blocked_user_id = ", $a, "))"
        )
    };
}

/// Whether the user `$user` blocked the account of another server `$actor`.
macro_rules! actor_blocked {
    ($user:literal, $actor:literal) => {
        concat!("EXISTS (SELECT 1 FROM user_blocks ba WHERE ba.user_id = ", $user, " AND ba.blocked_actor_id = ", $actor, ")")
    };
}

/// A blocked account, for the list in the settings.
pub struct Blocked {
    /// `@alice` or `@bob@mastodon.social`.
    pub handle: String,
    /// Its page here.
    pub url: String,
    /// Where the unblock form goes.
    pub unblock_url: String,
}

/// `me` blocks the user `other`: they stop following each other and stop
/// being friends.
pub async fn block_user(pool: &PgPool, me: i32, other: i32) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO user_blocks (user_id, blocked_user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(me)
        .bind(other)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM user_follows WHERE (follower_id = $1 AND followed_id = $2) OR (follower_id = $2 AND followed_id = $1)")
        .bind(me)
        .bind(other)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM friendships WHERE (requester_id = $1 AND addressee_id = $2) OR (requester_id = $2 AND addressee_id = $1)")
        .bind(me)
        .bind(other)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

pub async fn unblock_user(pool: &PgPool, me: i32, other: i32) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM user_blocks WHERE user_id = $1 AND blocked_user_id = $2").bind(me).bind(other).execute(pool).await?;
    Ok(())
}

/// `me` blocks an account of another server: it stops following them, and
/// they stop following it.
pub async fn block_actor(pool: &PgPool, me: i32, actor_id: i64) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO user_blocks (user_id, blocked_actor_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(me)
        .bind(actor_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM federation_followers WHERE user_id = $1 AND actor_id = $2").bind(me).bind(actor_id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM federation_following WHERE user_id = $1 AND actor_id = $2").bind(me).bind(actor_id).execute(&mut *tx).await?;
    tx.commit().await
}

pub async fn unblock_actor(pool: &PgPool, me: i32, actor_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM user_blocks WHERE user_id = $1 AND blocked_actor_id = $2").bind(me).bind(actor_id).execute(pool).await?;
    Ok(())
}

/// Whether `me` blocked the user `other`.
pub async fn blocks_user(pool: &PgPool, me: i32, other: i32) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM user_blocks WHERE user_id = $1 AND blocked_user_id = $2)")
        .bind(me)
        .bind(other)
        .fetch_one(pool)
        .await
}

/// Whether the users block each other, either way: they can't reach each other.
pub async fn between(pool: &PgPool, first: i32, second: i32) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(concat!("SELECT ", blocked_between!("$1", "$2")))
        .bind(first)
        .bind(second)
        .fetch_one(pool)
        .await
}

/// Whether the user blocked the account of another server.
pub async fn blocks_actor(pool: &PgPool, me: i32, actor_id: i64) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(concat!("SELECT ", actor_blocked!("$1", "$2")))
        .bind(me)
        .bind(actor_id)
        .fetch_one(pool)
        .await
}

/// Who `me` blocked, newest first.
pub async fn list(pool: &PgPool, me: i32) -> Result<Vec<Blocked>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT u.username, a.id AS actor_id, a.username AS actor_username, a.host
         FROM user_blocks b
         LEFT JOIN users u ON u.id = b.blocked_user_id
         LEFT JOIN federation_actors a ON a.id = b.blocked_actor_id
         WHERE b.user_id = $1
         ORDER BY b.created_at DESC",
    )
    .bind(me)
    .fetch_all(pool)
    .await?;
    let mut blocked = Vec::with_capacity(rows.len());
    for row in &rows {
        if let Some(username) = row.try_get::<Option<String>, _>("username")? {
            blocked.push(Blocked { handle: format!("@{username}"), url: format!("/user/{username}"), unblock_url: format!("/user/{username}/unblock") });
        } else if let Some(actor_id) = row.try_get::<Option<i64>, _>("actor_id")? {
            let username: String = row.try_get("actor_username")?;
            let host: String = row.try_get("host")?;
            blocked.push(Blocked { handle: format!("@{username}@{host}"), url: format!("/remote/{actor_id}"), unblock_url: format!("/remote/{actor_id}/unblock") });
        }
    }
    Ok(blocked)
}

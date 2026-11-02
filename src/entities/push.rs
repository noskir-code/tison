//! Browser notifications (see `push`): where each browser that turned them
//! on receives them, and the key this server signs them with.

use sqlx::{PgPool, Row};

use crate::entities::schema::{Column, Index, Table};

/// Kept up to date automatically at startup, see `entities::schema`.
pub const TABLE: Table = Table {
    name: "push_subscriptions",
    columns: &[
        Column::new("id", "BIGSERIAL").primary_key(),
        Column::new("user_id", "INTEGER").references("users(id) ON DELETE CASCADE"),
        // Given by the browser: an address of its push service (Mozilla,
        // Google, Apple, Microsoft...), one per browser.
        Column::new("endpoint", "TEXT"),
        // The browser's P-256 key and secret, base64url: the push service
        // can't read what is sent.
        Column::new("p256dh", "TEXT"),
        Column::new("auth", "TEXT"),
        Column::new("created_at", "TIMESTAMPTZ").default("NOW()"),
    ],
    indexes: &[
        // A browser belongs to the last user who turned them on in it.
        Index::unique("push_subscriptions_endpoint_idx", "endpoint"),
        Index::new("push_subscriptions_user_idx", "user_id"),
    ],
};

/// Keys of this server, made once and shared by the servers using the database.
pub const KEYS_TABLE: Table = Table {
    name: "server_keys",
    columns: &[
        Column::new("name", "TEXT").primary_key(),
        Column::new("value", "TEXT"),
    ],
    indexes: &[],
};

/// The most browsers one user receives notifications in: the oldest go.
pub const MAX_PER_USER: i64 = 20;

pub struct Subscription {
    pub id: i64,
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
}

/// The key named `name`, made with `make` the first time. When two servers
/// start at once, both keep the one stored first.
pub async fn server_key(pool: &PgPool, name: &str, make: impl FnOnce() -> String) -> Result<String, sqlx::Error> {
    if let Some(value) = sqlx::query_scalar("SELECT value FROM server_keys WHERE name = $1")
        .bind(name)
        .fetch_optional(pool)
        .await?
    {
        return Ok(value);
    }
    sqlx::query("INSERT INTO server_keys (name, value) VALUES ($1, $2) ON CONFLICT (name) DO NOTHING")
        .bind(name)
        .bind(make())
        .execute(pool)
        .await?;
    sqlx::query_scalar("SELECT value FROM server_keys WHERE name = $1").bind(name).fetch_one(pool).await
}

/// Saves the browser for `user_id`, taking it from another user signed in
/// there before.
pub async fn subscribe(pool: &PgPool, user_id: i32, endpoint: &str, p256dh: &str, auth: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO push_subscriptions (user_id, endpoint, p256dh, auth) VALUES ($1, $2, $3, $4)
         ON CONFLICT (endpoint) DO UPDATE SET user_id = $1, p256dh = $3, auth = $4, created_at = NOW()",
    )
    .bind(user_id)
    .bind(endpoint)
    .bind(p256dh)
    .bind(auth)
    .execute(pool)
    .await?;
    sqlx::query(
        "DELETE FROM push_subscriptions WHERE user_id = $1 AND id NOT IN
         (SELECT id FROM push_subscriptions WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2)",
    )
    .bind(user_id)
    .bind(MAX_PER_USER)
    .execute(pool)
    .await?;
    Ok(())
}

/// Only the user's own browser.
pub async fn unsubscribe(pool: &PgPool, user_id: i32, endpoint: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM push_subscriptions WHERE user_id = $1 AND endpoint = $2")
        .bind(user_id)
        .bind(endpoint)
        .execute(pool)
        .await?;
    Ok(())
}

/// Whether this browser receives the notifications of `user_id`.
pub async fn is_subscribed(pool: &PgPool, user_id: i32, endpoint: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM push_subscriptions WHERE user_id = $1 AND endpoint = $2)")
        .bind(user_id)
        .bind(endpoint)
        .fetch_one(pool)
        .await
}

pub async fn of_user(pool: &PgPool, user_id: i32) -> Result<Vec<Subscription>, sqlx::Error> {
    let rows = sqlx::query("SELECT id, endpoint, p256dh, auth FROM push_subscriptions WHERE user_id = $1")
        .bind(user_id)
        .fetch_all(pool)
        .await?;
    rows.iter()
        .map(|row| {
            Ok(Subscription {
                id: row.try_get("id")?,
                endpoint: row.try_get("endpoint")?,
                p256dh: row.try_get("p256dh")?,
                auth: row.try_get("auth")?,
            })
        })
        .collect()
}

/// The push service forgot it: the browser turned them off or was reset.
pub async fn delete(pool: &PgPool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM push_subscriptions WHERE id = $1").bind(id).execute(pool).await?;
    Ok(())
}

//! Two-factor authentication of users (see `totp`): their secret once
//! turned on, and their recovery codes, for when the phone is lost.

use sqlx::{
    PgPool, Row,
    types::chrono::{DateTime, Utc},
};

use crate::entities::schema::{Column, Index, Table};

/// Kept up to date automatically at startup, see `entities::schema`.
pub const TABLE: Table = Table {
    name: "two_factor",
    columns: &[
        Column::new("user_id", "INTEGER").primary_key().references("users(id) ON DELETE CASCADE"),
        // Base32, as authenticator apps show it.
        Column::new("secret", "TEXT"),
        Column::new("enabled_at", "TIMESTAMPTZ").default("NOW()"),
        // The time step of the last code used: each code works once.
        Column::new("last_step", "BIGINT").nullable(),
    ],
    indexes: &[],
};

/// Ten codes given when it's turned on, each usable once instead of a code.
pub const RECOVERY_TABLE: Table = Table {
    name: "recovery_codes",
    columns: &[
        Column::new("id", "BIGSERIAL").primary_key(),
        Column::new("user_id", "INTEGER").references("users(id) ON DELETE CASCADE"),
        // Argon2, like passwords: the codes are shown once.
        Column::new("code_hash", "TEXT"),
        Column::new("used_at", "TIMESTAMPTZ").nullable(),
    ],
    indexes: &[Index::new("recovery_codes_user_idx", "user_id")],
};

/// A user's two-factor authentication, when it's on.
pub struct TwoFactor {
    pub secret: String,
    pub enabled_at: DateTime<Utc>,
    pub last_step: Option<u64>,
    /// Recovery codes not used yet.
    pub recovery_left: i64,
}

impl TwoFactor {
    pub fn enabled_label(&self) -> String {
        crate::i18n::date(self.enabled_at.date_naive())
    }

    /// The secret as bytes, for `totp::verify`.
    pub fn secret_bytes(&self) -> Vec<u8> {
        crate::totp::base32_decode(&self.secret).unwrap_or_default()
    }
}

pub async fn find(pool: &PgPool, user_id: i32) -> Result<Option<TwoFactor>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT secret, enabled_at, last_step,
                (SELECT COUNT(*) FROM recovery_codes r WHERE r.user_id = t.user_id AND r.used_at IS NULL) AS recovery_left
         FROM two_factor t WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    row.map(|row| {
        Ok(TwoFactor {
            secret: row.try_get("secret")?,
            enabled_at: row.try_get("enabled_at")?,
            last_step: row.try_get::<Option<i64>, _>("last_step")?.map(|step| step as u64),
            recovery_left: row.try_get("recovery_left")?,
        })
    })
    .transpose()
}

/// Turns it on with `secret`, the first code having used `step`, and these
/// recovery codes (hashed).
pub async fn enable(pool: &PgPool, user_id: i32, secret: &str, step: u64, recovery_hashes: &[String]) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    sqlx::query(
        "INSERT INTO two_factor (user_id, secret, last_step) VALUES ($1, $2, $3)
         ON CONFLICT (user_id) DO UPDATE SET secret = $2, last_step = $3, enabled_at = NOW()",
    )
    .bind(user_id)
    .bind(secret)
    .bind(step as i64)
    .execute(&mut *transaction)
    .await?;
    replace_recovery(&mut transaction, user_id, recovery_hashes).await?;
    transaction.commit().await
}

async fn replace_recovery(transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>, user_id: i32, hashes: &[String]) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM recovery_codes WHERE user_id = $1").bind(user_id).execute(&mut **transaction).await?;
    for hash in hashes {
        sqlx::query("INSERT INTO recovery_codes (user_id, code_hash) VALUES ($1, $2)")
            .bind(user_id)
            .bind(hash)
            .execute(&mut **transaction)
            .await?;
    }
    Ok(())
}

/// New recovery codes: the old ones stop working.
pub async fn set_recovery(pool: &PgPool, user_id: i32, hashes: &[String]) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    replace_recovery(&mut transaction, user_id, hashes).await?;
    transaction.commit().await
}

pub async fn disable(pool: &PgPool, user_id: i32) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM two_factor WHERE user_id = $1").bind(user_id).execute(pool).await?;
    sqlx::query("DELETE FROM recovery_codes WHERE user_id = $1").bind(user_id).execute(pool).await?;
    Ok(())
}

/// A code was used at `step`: it, and those before, won't work again.
/// `false` when another sign-in used it first.
pub async fn use_step(pool: &PgPool, user_id: i32, step: u64) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("UPDATE two_factor SET last_step = $2 WHERE user_id = $1 AND (last_step IS NULL OR last_step < $2)")
        .bind(user_id)
        .bind(step as i64)
        .execute(pool)
        .await?
        .rows_affected()
        > 0)
}

/// The unused recovery codes: (id, hash).
pub async fn unused_recovery(pool: &PgPool, user_id: i32) -> Result<Vec<(i64, String)>, sqlx::Error> {
    let rows = sqlx::query("SELECT id, code_hash FROM recovery_codes WHERE user_id = $1 AND used_at IS NULL")
        .bind(user_id)
        .fetch_all(pool)
        .await?;
    rows.iter().map(|row| Ok((row.try_get("id")?, row.try_get("code_hash")?))).collect()
}

/// `false` when it was used meanwhile.
pub async fn use_recovery(pool: &PgPool, code_id: i64) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("UPDATE recovery_codes SET used_at = NOW() WHERE id = $1 AND used_at IS NULL")
        .bind(code_id)
        .execute(pool)
        .await?
        .rows_affected()
        > 0)
}

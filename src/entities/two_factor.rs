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
        // Base32 as authenticator apps show it, encrypted: see `vault`.
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
    /// Base32, decrypted (see `vault`); empty when it can't be: then no code
    /// works, only recovery codes.
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

/// What the secret of `user_id` is sealed for: a secret copied onto another
/// account doesn't open.
fn context(user_id: i32) -> String {
    format!("two_factor:{user_id}")
}

/// The stored secret readable again: opened, or as it was written before
/// secrets were encrypted. Empty when it can't be opened (no key, or not
/// this one): two-factor sign-ins then fail rather than letting anyone in.
fn readable(user_id: i32, stored: &str) -> String {
    if !crate::vault::is_sealed(stored) {
        return stored.to_string();
    }
    match crate::vault::open(stored, &context(user_id)).and_then(|plain| String::from_utf8(plain).ok()) {
        Some(secret) => secret,
        None => {
            tracing::error!("the two-factor secret of user {} can't be decrypted: wrong or missing key file", user_id);
            String::new()
        }
    }
}

/// Encrypts a new secret for `user_id`, before `enable`.
pub fn seal_secret(user_id: i32, secret: &str) -> Result<String, String> {
    crate::vault::seal(secret.as_bytes(), &context(user_id))
}

/// How many secrets are encrypted, and encrypts those written before (at
/// start, once the key is loaded). Returns (already, now) encrypted.
pub async fn secure_secrets(pool: &PgPool) -> Result<(usize, usize), String> {
    let rows: Vec<(i32, String)> = sqlx::query_as("SELECT user_id, secret FROM two_factor")
        .fetch_all(pool)
        .await
        .map_err(|err| format!("failed to load the two-factor secrets: {err}"))?;
    let sealed = rows.iter().filter(|(_, secret)| crate::vault::is_sealed(secret)).count();
    let plain: Vec<&(i32, String)> = rows.iter().filter(|(_, secret)| !crate::vault::is_sealed(secret)).collect();
    if plain.is_empty() || !crate::vault::is_ready() {
        return Ok((sealed, 0));
    }
    let mut transaction = pool.begin().await.map_err(|err| err.to_string())?;
    for (user_id, secret) in &plain {
        let sealed_secret = seal_secret(*user_id, secret)?;
        sqlx::query("UPDATE two_factor SET secret = $2 WHERE user_id = $1 AND secret = $3")
            .bind(user_id)
            .bind(sealed_secret)
            .bind(secret)
            .execute(&mut *transaction)
            .await
            .map_err(|err| format!("failed to encrypt a two-factor secret: {err}"))?;
    }
    transaction.commit().await.map_err(|err| err.to_string())?;
    Ok((sealed, plain.len()))
}

/// Whether some secrets are encrypted: then a lost key can't be replaced by
/// a new one, see `main`.
pub async fn has_sealed(pool: &PgPool) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM two_factor WHERE secret LIKE 'v1:%')").fetch_one(pool).await
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
            secret: readable(user_id, &row.try_get::<String, _>("secret")?),
            enabled_at: row.try_get("enabled_at")?,
            last_step: row.try_get::<Option<i64>, _>("last_step")?.map(|step| step as u64),
            recovery_left: row.try_get("recovery_left")?,
        })
    })
    .transpose()
}

/// Turns it on with `secret` (sealed, see `seal_secret`), the first code
/// having used `step`, and these recovery codes (hashed).
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

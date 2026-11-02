//! Email domains nobody can sign up with: those of throwaway addresses used
//! for fake accounts, set by the staff in the Users tab of the moderation.
//! A domain also blocks its subdomains.

use sqlx::{
    PgPool, Row,
    types::chrono::{DateTime, Utc},
};

use crate::entities::schema::{Column, Table};

/// Kept up to date automatically at startup, see `entities::schema`.
pub const TABLE: Table = Table {
    name: "blocked_email_domains",
    columns: &[
        // Lowercase, like `mailinator.com`.
        Column::new("domain", "TEXT").primary_key(),
        Column::new("created_at", "TIMESTAMPTZ").default("NOW()"),
        Column::new("created_by", "INTEGER").nullable().references("users(id) ON DELETE SET NULL"),
    ],
    indexes: &[],
};

pub struct BlockedDomain {
    pub domain: String,
    pub created_at: DateTime<Utc>,
    /// Who added it, `None` when that account is gone.
    pub created_by: Option<String>,
    /// Accounts already using it, not removed by the block.
    pub accounts: i64,
}

impl BlockedDomain {
    pub fn created_label(&self) -> String {
        crate::i18n::date(self.created_at.date_naive())
    }
}

/// `Mail.Example.COM`, `@example.com` or a whole address → `example.com`,
/// `None` when it isn't a domain.
pub fn normalize(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let domain = raw.rsplit('@').next().unwrap_or(raw).trim().trim_end_matches('.').to_ascii_lowercase();
    let valid = domain.contains('.')
        && domain.len() <= 253
        && domain.split('.').all(|label| {
            !label.is_empty() && label.len() <= 63 && !label.starts_with('-') && !label.ends_with('-')
                && label.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        });
    valid.then_some(domain)
}

/// The domain of `email` and each one it's under: `a.b.com`, `b.com`, `com`.
fn domains_of(email: &str) -> Vec<String> {
    let domain = email.rsplit('@').next().unwrap_or_default().trim_end_matches('.').to_ascii_lowercase();
    let mut found = Vec::new();
    let mut rest = domain.as_str();
    loop {
        found.push(rest.to_string());
        match rest.split_once('.') {
            Some((_, parent)) => rest = parent,
            None => return found,
        }
    }
}

pub async fn is_blocked(pool: &PgPool, email: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM blocked_email_domains WHERE domain = ANY($1))")
        .bind(domains_of(email))
        .fetch_one(pool)
        .await
}

pub async fn list(pool: &PgPool) -> Result<Vec<BlockedDomain>, sqlx::Error> {
    sqlx::query(
        "SELECT d.domain, d.created_at, u.username,
                (SELECT COUNT(*) FROM users a
                  WHERE LOWER(a.email) LIKE '%@' || d.domain OR LOWER(a.email) LIKE '%.' || d.domain) AS accounts
         FROM blocked_email_domains d LEFT JOIN users u ON u.id = d.created_by
         ORDER BY d.domain",
    )
    .fetch_all(pool)
    .await?
    .iter()
    .map(|row| {
        Ok(BlockedDomain {
            domain: row.try_get("domain")?,
            created_at: row.try_get("created_at")?,
            created_by: row.try_get("username")?,
            accounts: row.try_get("accounts")?,
        })
    })
    .collect()
}

/// Returns whether it was added (false: already blocked).
pub async fn add(pool: &PgPool, domain: &str, user_id: i32) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("INSERT INTO blocked_email_domains (domain, created_by) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(domain)
        .bind(user_id)
        .execute(pool)
        .await?
        .rows_affected()
        > 0)
}

pub async fn remove(pool: &PgPool, domain: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM blocked_email_domains WHERE domain = $1").bind(domain).execute(pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domains_are_normalized() {
        assert_eq!(normalize(" Mail.Example.COM. "), Some("mail.example.com".to_string()));
        assert_eq!(normalize("@yopmail.com"), Some("yopmail.com".to_string()));
        assert_eq!(normalize("fake@temp-mail.org"), Some("temp-mail.org".to_string()));
        assert_eq!(normalize("localhost"), None);
        assert_eq!(normalize("bad_domain.com"), None);
        assert_eq!(normalize("-a.com"), None);
        assert_eq!(normalize("a..com"), None);
    }

    #[test]
    fn subdomains_are_checked() {
        assert_eq!(domains_of("x@Mail.Spam.com"), ["mail.spam.com", "spam.com", "com"]);
    }
}

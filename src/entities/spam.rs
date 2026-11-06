//! Anti-spam: the same text posted again and again, by a user here or an
//! account of another server, is refused (here, with an error; from other
//! servers, dropped). Each refusal is kept as an event; past
//! `REPORT_THRESHOLD` events in a day, the moderators get an automatic
//! report and decide (Server rules: spam). Nothing is suspended or blocked
//! automatically.
//!
//! Texts are compared without case nor extra spaces. Short ones ("Thanks!",
//! "+1") are never taken for spam.

use sqlx::PgPool;

use crate::entities::{
    report::{self, NewReport, Reason, Target},
    schema::{Column, Index, Table},
};

/// Texts shorter than this are never refused: short answers repeat.
pub const MIN_CHARS: usize = 20;
/// Refused repeats in a day that make an automatic report.
pub const REPORT_THRESHOLD: i64 = 3;

pub const TABLE: Table = Table {
    name: "spam_events",
    columns: &[
        Column::new("id", "BIGSERIAL").primary_key(),
        // Who sent it: a user here, or an account of another server.
        Column::new("user_id", "INTEGER").nullable().references("users(id) ON DELETE CASCADE"),
        Column::new("actor_id", "BIGINT")
            .nullable()
            .references("federation_actors(id) ON DELETE CASCADE"),
        // `post`, `comment` or `message`.
        Column::new("kind", "TEXT"),
        // The start of the text, for the report.
        Column::new("excerpt", "TEXT").default("''"),
        Column::new("created_at", "TIMESTAMPTZ").default("NOW()"),
    ],
    indexes: &[
        Index::new("spam_events_user_idx", "user_id, created_at"),
        Index::new("spam_events_actor_idx", "actor_id, created_at"),
    ],
};

/// Who sent a text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sender {
    Local(i32),
    /// A row of `federation_actors`.
    Remote(i64),
}

impl Sender {
    fn ids(self) -> (Option<i32>, Option<i64>) {
        match self {
            Self::Local(id) => (Some(id), None),
            Self::Remote(id) => (None, Some(id)),
        }
    }
}

/// What was sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Post,
    Comment,
    Message,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Post => "post",
            Self::Comment => "comment",
            Self::Message => "message",
        }
    }

    /// How many times the same text was already sent in how many hours:
    /// one more is spam. A post isn't posted twice in a day; a comment or a
    /// message can be, not as often.
    fn rule(self) -> (i64, i32) {
        match self {
            Self::Post => (1, 24),
            Self::Comment => (2, 1),
            Self::Message => (3, 1),
        }
    }
}

/// The text as compared: lowercase, spaces made one.
pub fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// The same expression in SQL, for `column`.
macro_rules! normalized {
    ($column:literal) => {
        concat!("LOWER(TRIM(regexp_replace(", $column, ", '\\s+', ' ', 'g')))")
    };
}

/// Whether `text` is a repeat by `sender` (see `Kind::rule`), other than
/// the object `ap_id` itself (an edit received again).
pub async fn is_repeated(pool: &PgPool, sender: Sender, kind: Kind, text: &str, ap_id: Option<&str>) -> Result<bool, sqlx::Error> {
    let normalized = normalize(text);
    if normalized.chars().count() < MIN_CHARS {
        return Ok(false);
    }
    let (user_id, actor_id) = sender.ids();
    let (times, hours) = kind.rule();
    let query = match kind {
        Kind::Post => concat!(
            "SELECT COUNT(*) FROM posts WHERE (user_id = $1 OR remote_actor_id = $2)
               AND created_at > NOW() - make_interval(hours => $3) AND ($5::text IS NULL OR ap_id IS DISTINCT FROM $5)
               AND ",
            normalized!("body"),
            " = $4"
        ),
        Kind::Comment => concat!(
            "SELECT COUNT(*) FROM comments WHERE (user_id = $1 OR remote_actor_id = $2)
               AND created_at > NOW() - make_interval(hours => $3) AND ($5::text IS NULL OR ap_id IS DISTINCT FROM $5)
               AND ",
            normalized!("body"),
            " = $4"
        ),
        Kind::Message => concat!(
            "SELECT COUNT(*) FROM messages WHERE (sender_user_id = $1 OR sender_actor_id = $2)
               AND created_at > NOW() - make_interval(hours => $3) AND ($5::text IS NULL OR ap_id IS DISTINCT FROM $5)
               AND ",
            normalized!("body"),
            " = $4"
        ),
    };
    let count: i64 = sqlx::query_scalar(query)
        .bind(user_id)
        .bind(actor_id)
        .bind(hours)
        .bind(&normalized)
        .bind(ap_id)
        .fetch_one(pool)
        .await?;
    Ok(count >= times)
}

/// Keeps a refused repeat, and past `REPORT_THRESHOLD` in a day reports the
/// account to the moderators, once a day at most.
pub async fn record(pool: &PgPool, sender: Sender, kind: Kind, text: &str, account_label: &str) -> Result<(), sqlx::Error> {
    let (user_id, actor_id) = sender.ids();
    let excerpt = report::snapshot(text);
    sqlx::query("INSERT INTO spam_events (user_id, actor_id, kind, excerpt) VALUES ($1, $2, $3, $4)")
        .bind(user_id)
        .bind(actor_id)
        .bind(kind.as_str())
        .bind(&excerpt)
        .execute(pool)
        .await?;
    let recent: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM spam_events WHERE (user_id = $1 OR actor_id = $2) AND created_at > NOW() - INTERVAL '1 day'",
    )
    .bind(user_id)
    .bind(actor_id)
    .fetch_one(pool)
    .await?;
    if recent < REPORT_THRESHOLD {
        return Ok(());
    }
    let target = match sender {
        Sender::Local(id) => Target::User(id),
        Sender::Remote(id) => Target::Remote(id),
    };
    let new = NewReport {
        target,
        account: None,
        snapshot: excerpt,
        target_label: account_label.to_string(),
        reason: Reason::Spam,
        comment: format!("Sent the same {} again {recent} times in a day: the repeats were refused.", kind.as_str()),
    };
    if report::create_automatic(pool, &new).await? {
        tracing::warn!("automatic spam report on {}", account_label);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texts_are_compared_loosely() {
        assert_eq!(normalize("  Buy   CHEAP\nwatches  "), "buy cheap watches");
        assert_eq!(Kind::Post.rule(), (1, 24));
    }
}

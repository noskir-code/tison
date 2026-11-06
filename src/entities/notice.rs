//! What the moderators did to an account, told to its owner with the rule
//! broken (Server rules, enforcement; the EU's DSA art. 17 asks for this
//! "statement of reasons"), and their appeal: a warning, content removed or
//! marked sensitive, a limit, a suspension or a ban. Shown on `/notices`,
//! even to suspended accounts, see `handlers::notice`.

use sqlx::{
    PgPool, Row,
    postgres::PgRow,
    types::chrono::{DateTime, Utc},
};

use crate::entities::{
    report::Reason,
    schema::{Column, Index, Table},
};

/// Kept up to date automatically at startup, see `entities::schema`.
pub const TABLE: Table = Table {
    name: "moderation_notices",
    columns: &[
        Column::new("id", "BIGSERIAL").primary_key(),
        Column::new("user_id", "INTEGER").references("users(id) ON DELETE CASCADE"),
        // See `Action`.
        Column::new("action", "TEXT"),
        // The rule broken, a `report::Reason`.
        Column::new("reason", "TEXT"),
        // What the moderator adds for the person.
        Column::new("note", "TEXT").default("''"),
        Column::new("post_id", "BIGINT").nullable().references("posts(id) ON DELETE SET NULL"),
        Column::new("comment_id", "BIGINT").nullable().references("comments(id) ON DELETE SET NULL"),
        // What it was about, kept when it's deleted.
        Column::new("excerpt", "TEXT").default("''"),
        // The end of a suspension.
        Column::new("until", "TIMESTAMPTZ").nullable(),
        Column::new("created_by", "INTEGER").nullable().references("users(id) ON DELETE SET NULL"),
        Column::new("created_at", "TIMESTAMPTZ").default("NOW()"),
        // Seen by the person on `/notices`.
        Column::new("read_at", "TIMESTAMPTZ").nullable(),
        Column::new("appeal", "TEXT").default("''"),
        Column::new("appealed_at", "TIMESTAMPTZ").nullable(),
        // `''`, `pending`, `accepted` or `rejected`, see `AppealStatus`.
        Column::new("appeal_status", "TEXT").default("''"),
        Column::new("answer", "TEXT").default("''"),
        Column::new("answered_by", "INTEGER").nullable().references("users(id) ON DELETE SET NULL"),
        Column::new("answered_at", "TIMESTAMPTZ").nullable(),
    ],
    indexes: &[
        Index::new("moderation_notices_user_idx", "user_id, created_at DESC"),
        Index::new("moderation_notices_appeal_idx", "appeal_status"),
    ],
};

/// Longest note, appeal or answer.
pub const TEXT_MAX_CHARS: usize = 2000;

/// Longest excerpt of what a notice is about.
const EXCERPT_CHARS: usize = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Warning,
    /// A post or comment deleted.
    Removed,
    /// A post hidden behind a warning, or its media blurred.
    MarkedSensitive,
    /// Only their followers see what they post, see `user::set_limited`.
    Limited,
    Suspended,
    Banned,
}

impl Action {
    pub const ALL: [Self; 6] = [Self::Warning, Self::Removed, Self::MarkedSensitive, Self::Limited, Self::Suspended, Self::Banned];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Removed => "removed",
            Self::MarkedSensitive => "sensitive",
            Self::Limited => "limited",
            Self::Suspended => "suspended",
            Self::Banned => "banned",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.as_str() == value)
    }

    /// What the person reads.
    pub fn label(self) -> &'static str {
        match self {
            Self::Warning => "Warning",
            Self::Removed => "Content removed",
            Self::MarkedSensitive => "Content marked sensitive",
            Self::Limited => "Account limited",
            Self::Suspended => "Account suspended",
            Self::Banned => "Account banned",
        }
    }

    pub fn explanation(self) -> &'static str {
        match self {
            Self::Warning => "Nothing was removed: the moderators ask you to follow the server rules.",
            Self::Removed => "The moderators removed it, here and on the other servers that had it. It's kept hidden for a while: if your appeal is accepted in time, it comes back.",
            Self::MarkedSensitive => "The moderators added a content warning to it: it's hidden until clicked.",
            Self::Limited => "Only the people following you, your friends and the moderators see what you post.",
            Self::Suspended => "You can't sign in, and your profile and posts are hidden, until the end of the suspension.",
            Self::Banned => "You can't sign in anymore, and your profile and posts are hidden.",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppealStatus {
    None,
    Pending,
    Accepted,
    Rejected,
}

impl AppealStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "pending" => Self::Pending,
            "accepted" => Self::Accepted,
            "rejected" => Self::Rejected,
            _ => Self::None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Pending => "Appeal waiting for a moderator",
            Self::Accepted => "Appeal accepted: the decision was reversed",
            Self::Rejected => "Appeal rejected: the decision stays",
        }
    }
}

pub struct Notice {
    pub id: i64,
    pub user_id: i32,
    /// For the staff's list of appeals.
    pub username: String,
    pub action: Action,
    pub reason: Reason,
    pub note: String,
    pub post_id: Option<i64>,
    pub comment_id: Option<i64>,
    pub excerpt: String,
    pub until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub read: bool,
    pub appeal: String,
    pub appealed_at: Option<DateTime<Utc>>,
    pub appeal_status: AppealStatus,
    pub answer: String,
}

impl Notice {
    fn from_row(row: &PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            user_id: row.try_get("user_id")?,
            username: row.try_get::<Option<String>, _>("username")?.unwrap_or_default(),
            action: Action::parse(&row.try_get::<String, _>("action")?).unwrap_or(Action::Warning),
            reason: Reason::parse(&row.try_get::<String, _>("reason")?).unwrap_or(Reason::Other),
            note: row.try_get("note")?,
            post_id: row.try_get("post_id")?,
            comment_id: row.try_get("comment_id")?,
            excerpt: row.try_get("excerpt")?,
            until: row.try_get("until")?,
            created_at: row.try_get("created_at")?,
            read: row.try_get::<Option<DateTime<Utc>>, _>("read_at")?.is_some(),
            appeal: row.try_get("appeal")?,
            appealed_at: row.try_get("appealed_at")?,
            appeal_status: AppealStatus::parse(&row.try_get::<String, _>("appeal_status")?),
            answer: row.try_get("answer")?,
        })
    }

    pub fn created_label(&self) -> String {
        crate::i18n::date_time(self.created_at)
    }

    pub fn until_label(&self) -> Option<String> {
        self.until.map(crate::i18n::date_time)
    }

    pub fn appealed_label(&self) -> Option<String> {
        self.appealed_at.map(crate::i18n::date_time)
    }

    /// A warning can't be reversed, but can be discussed: every notice can
    /// be appealed once.
    pub fn can_appeal(&self) -> bool {
        self.appeal_status == AppealStatus::None
    }

    /// Where what it's about is, while it exists: not while removed by the
    /// moderators (only an accepted appeal brings it back).
    pub fn link(&self) -> Option<String> {
        if self.action == Action::Removed && self.appeal_status != AppealStatus::Accepted {
            return None;
        }
        match (self.post_id, self.comment_id) {
            (Some(post_id), Some(comment_id)) => Some(format!("/posts/{post_id}#comment-{comment_id}")),
            (Some(post_id), None) => Some(format!("/posts/{post_id}")),
            _ => None,
        }
    }

    pub fn is_answered(&self) -> bool {
        matches!(self.appeal_status, AppealStatus::Accepted | AppealStatus::Rejected)
    }
}

/// What a moderator gives when acting.
pub struct NewNotice<'a> {
    pub user_id: i32,
    pub action: Action,
    pub reason: Reason,
    pub note: &'a str,
    pub post_id: Option<i64>,
    pub comment_id: Option<i64>,
    pub excerpt: &'a str,
    pub until: Option<DateTime<Utc>>,
    pub created_by: i32,
}

/// Trims the text, cut at `TEXT_MAX_CHARS`, without control characters other
/// than line breaks.
pub fn clean_text(raw: &str) -> String {
    raw.trim()
        .replace("\r\n", "\n")
        .chars()
        .filter(|letter| !letter.is_control() || *letter == '\n')
        .take(TEXT_MAX_CHARS)
        .collect()
}

pub async fn create(pool: &PgPool, notice: &NewNotice<'_>) -> Result<i64, sqlx::Error> {
    let excerpt: String = notice.excerpt.trim().chars().take(EXCERPT_CHARS).collect();
    sqlx::query_scalar(
        "INSERT INTO moderation_notices (user_id, action, reason, note, post_id, comment_id, excerpt, until, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
    )
    .bind(notice.user_id)
    .bind(notice.action.as_str())
    .bind(notice.reason.as_str())
    .bind(clean_text(notice.note))
    .bind(notice.post_id)
    .bind(notice.comment_id)
    .bind(excerpt)
    .bind(notice.until)
    .bind(notice.created_by)
    .fetch_one(pool)
    .await
}

macro_rules! notice_select {
    () => {
        "SELECT n.*, u.username FROM moderation_notices n LEFT JOIN users u ON u.id = n.user_id"
    };
}

/// The notices of a user, newest first.
pub async fn for_user(pool: &PgPool, user_id: i32) -> Result<Vec<Notice>, sqlx::Error> {
    sqlx::query(concat!(notice_select!(), " WHERE n.user_id = $1 ORDER BY n.created_at DESC, n.id DESC"))
        .bind(user_id)
        .fetch_all(pool)
        .await?
        .iter()
        .map(Notice::from_row)
        .collect()
}

pub async fn find(pool: &PgPool, id: i64) -> Result<Option<Notice>, sqlx::Error> {
    sqlx::query(concat!(notice_select!(), " WHERE n.id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(Notice::from_row)
        .transpose()
}

/// Notices the user hasn't opened yet: a banner shows it, see `Layout`.
pub async fn unread_count(pool: &PgPool, user_id: i32) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM moderation_notices WHERE user_id = $1 AND read_at IS NULL")
        .bind(user_id)
        .fetch_one(pool)
        .await
}

pub async fn mark_read(pool: &PgPool, user_id: i32) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE moderation_notices SET read_at = NOW() WHERE user_id = $1 AND read_at IS NULL")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// The person appeals the notice, once. `false` when it isn't theirs or was
/// already appealed.
pub async fn appeal(pool: &PgPool, id: i64, user_id: i32, text: &str) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query(
        "UPDATE moderation_notices SET appeal = $3, appealed_at = NOW(), appeal_status = 'pending'
         WHERE id = $1 AND user_id = $2 AND appeal_status = ''",
    )
    .bind(id)
    .bind(user_id)
    .bind(clean_text(text))
    .execute(pool)
    .await?
    .rows_affected()
        > 0)
}

/// Appeals waiting for a moderator, oldest first.
pub async fn pending_appeals(pool: &PgPool) -> Result<Vec<Notice>, sqlx::Error> {
    sqlx::query(concat!(notice_select!(), " WHERE n.appeal_status = 'pending' ORDER BY n.appealed_at, n.id"))
        .fetch_all(pool)
        .await?
        .iter()
        .map(Notice::from_row)
        .collect()
}

pub async fn pending_count(pool: &PgPool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM moderation_notices WHERE appeal_status = 'pending'").fetch_one(pool).await
}

/// A moderator answers an appeal. `false` when it wasn't pending anymore.
pub async fn answer(pool: &PgPool, id: i64, accepted: bool, answer: &str, moderator_id: i32) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query(
        "UPDATE moderation_notices SET appeal_status = $2, answer = $3, answered_by = $4, answered_at = NOW(), read_at = NULL
         WHERE id = $1 AND appeal_status = 'pending'",
    )
    .bind(id)
    .bind(if accepted { AppealStatus::Accepted } else { AppealStatus::Rejected }.as_str())
    .bind(clean_text(answer))
    .bind(moderator_id)
    .execute(pool)
    .await?
    .rows_affected()
        > 0)
}

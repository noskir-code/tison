//! Relays (see `API::relay`): the relays this site follows, to receive the
//! public posts of other servers and send them its own, and, when it acts
//! as a relay (`RELAY-SERVER`), the servers following it.

use sqlx::{
    PgPool, Row,
    postgres::PgRow,
    types::chrono::{DateTime, Utc},
};

use crate::entities::schema::{Column, Index, Table};

/// The relays followed by the site.
pub const RELAYS_TABLE: Table = Table {
    name: "relays",
    columns: &[
        Column::new("id", "BIGSERIAL").primary_key(),
        // As the admin wrote it: an inbox (Mastodon's relays) or an actor
        // (LitePub's, Pleroma's).
        Column::new("url", "TEXT"),
        Column::new("inbox", "TEXT"),
        // The relay's actor, for LitePub relays; `NULL` for Mastodon's.
        Column::new("actor_id", "TEXT").nullable(),
        // Its server, to know what it signs.
        Column::new("host", "TEXT"),
        // The Follow sent, answered by an Accept or a Reject.
        Column::new("follow_id", "TEXT"),
        // `pending`, `accepted`, `rejected` or `paused`.
        Column::new("state", "TEXT").default("'pending'"),
        Column::new("created_at", "TIMESTAMPTZ").default("NOW()"),
    ],
    indexes: &[Index::unique("relays_url_idx", "url"), Index::new("relays_host_idx", "host")],
};

/// The servers using this site as their relay.
pub const SUBSCRIBERS_TABLE: Table = Table {
    name: "relay_subscribers",
    columns: &[
        Column::new("id", "BIGSERIAL").primary_key(),
        // Their actor (often their instance actor), and where to send.
        Column::new("actor_id", "TEXT"),
        Column::new("inbox", "TEXT"),
        Column::new("host", "TEXT"),
        // `mastodon` (followed the public collection: gets the activities
        // forwarded as they are) or `litepub` (followed this actor: gets
        // Announces).
        Column::new("kind", "TEXT").default("'mastodon'"),
        Column::new("follow_id", "TEXT"),
        // `pending` until an admin accepts it, `accepted`.
        Column::new("state", "TEXT").default("'pending'"),
        Column::new("created_at", "TIMESTAMPTZ").default("NOW()"),
    ],
    indexes: &[Index::unique("relay_subscribers_actor_idx", "actor_id"), Index::new("relay_subscribers_host_idx", "host")],
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Pending,
    Accepted,
    Rejected,
    Paused,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Paused => "paused",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "accepted" => Self::Accepted,
            "rejected" => Self::Rejected,
            "paused" => Self::Paused,
            _ => Self::Pending,
        }
    }

    /// In English, translated by the templates.
    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "Waiting for its answer",
            Self::Accepted => "Connected",
            Self::Rejected => "Refused",
            Self::Paused => "Paused",
        }
    }
}

/// A relay the site follows.
pub struct Relay {
    pub id: i64,
    pub url: String,
    pub inbox: String,
    pub actor_id: Option<String>,
    pub host: String,
    pub follow_id: String,
    pub state: State,
    pub created_at: DateTime<Utc>,
}

impl Relay {
    fn from_row(row: &PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            url: row.try_get("url")?,
            inbox: row.try_get("inbox")?,
            actor_id: row.try_get("actor_id")?,
            host: row.try_get("host")?,
            follow_id: row.try_get("follow_id")?,
            state: State::parse(&row.try_get::<String, _>("state")?),
            created_at: row.try_get("created_at")?,
        })
    }

    pub fn created_label(&self) -> String {
        crate::i18n::date(self.created_at.date_naive())
    }
}

const RELAY_COLUMNS: &str = "id, url, inbox, actor_id, host, follow_id, state, created_at";

pub async fn list(pool: &PgPool) -> Result<Vec<Relay>, sqlx::Error> {
    sqlx::query(sqlx::AssertSqlSafe(format!("SELECT {RELAY_COLUMNS} FROM relays ORDER BY created_at")))
        .fetch_all(pool)
        .await?
        .iter()
        .map(Relay::from_row)
        .collect()
}

pub async fn find(pool: &PgPool, id: i64) -> Result<Option<Relay>, sqlx::Error> {
    sqlx::query(sqlx::AssertSqlSafe(format!("SELECT {RELAY_COLUMNS} FROM relays WHERE id = $1")))
        .bind(id)
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(Relay::from_row)
        .transpose()
}

/// Adds a relay (its Follow just sent), or follows it again. Returns its id.
pub async fn add(pool: &PgPool, url: &str, inbox: &str, actor_id: Option<&str>, host: &str, follow_id: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO relays (url, inbox, actor_id, host, follow_id) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (url) DO UPDATE SET inbox = EXCLUDED.inbox, actor_id = EXCLUDED.actor_id, host = EXCLUDED.host,
                                         follow_id = EXCLUDED.follow_id, state = 'pending'
         RETURNING id",
    )
    .bind(url)
    .bind(inbox)
    .bind(actor_id)
    .bind(host)
    .bind(follow_id)
    .fetch_one(pool)
    .await
}

pub async fn set_state(pool: &PgPool, id: i64, state: State) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE relays SET state = $2 WHERE id = $1").bind(id).bind(state.as_str()).execute(pool).await?;
    Ok(())
}

/// The relay whose Follow is `follow_id`, answered by `host`.
pub async fn answered(pool: &PgPool, follow_id: &str, host: &str, accepted: bool) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("UPDATE relays SET state = $3 WHERE follow_id = $1 AND host = $2 AND state <> 'paused'")
        .bind(follow_id)
        .bind(host)
        .bind(if accepted { "accepted" } else { "rejected" })
        .execute(pool)
        .await?
        .rows_affected()
        > 0)
}

pub async fn remove(pool: &PgPool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM relays WHERE id = $1").bind(id).execute(pool).await?;
    Ok(())
}

/// Whether `host` is a relay the site follows and that accepted: what it
/// relays is read.
pub async fn is_followed_relay(pool: &PgPool, host: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM relays WHERE host = $1 AND state = 'accepted')")
        .bind(host)
        .fetch_one(pool)
        .await
}

/// A server using this site as its relay.
pub struct Subscriber {
    pub id: i64,
    pub actor_id: String,
    pub inbox: String,
    pub host: String,
    pub kind: String,
    pub follow_id: String,
    pub state: State,
    pub created_at: DateTime<Utc>,
}

impl Subscriber {
    fn from_row(row: &PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            id: row.try_get("id")?,
            actor_id: row.try_get("actor_id")?,
            inbox: row.try_get("inbox")?,
            host: row.try_get("host")?,
            kind: row.try_get("kind")?,
            follow_id: row.try_get("follow_id")?,
            state: State::parse(&row.try_get::<String, _>("state")?),
            created_at: row.try_get("created_at")?,
        })
    }

    pub fn created_label(&self) -> String {
        crate::i18n::date(self.created_at.date_naive())
    }

    pub fn is_pending(&self) -> bool {
        self.state == State::Pending
    }
}

const SUBSCRIBER_COLUMNS: &str = "id, actor_id, inbox, host, kind, follow_id, state, created_at";

pub async fn subscribers(pool: &PgPool) -> Result<Vec<Subscriber>, sqlx::Error> {
    sqlx::query(sqlx::AssertSqlSafe(format!("SELECT {SUBSCRIBER_COLUMNS} FROM relay_subscribers ORDER BY state = 'pending' DESC, created_at")))
        .fetch_all(pool)
        .await?
        .iter()
        .map(Subscriber::from_row)
        .collect()
}

pub async fn find_subscriber(pool: &PgPool, id: i64) -> Result<Option<Subscriber>, sqlx::Error> {
    sqlx::query(sqlx::AssertSqlSafe(format!("SELECT {SUBSCRIBER_COLUMNS} FROM relay_subscribers WHERE id = $1")))
        .bind(id)
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(Subscriber::from_row)
        .transpose()
}

/// A server asks to use this site as its relay: waits for an admin. Asked
/// again, its Follow and inbox are updated, its state kept.
pub async fn add_subscriber(pool: &PgPool, actor_id: &str, inbox: &str, host: &str, kind: &str, follow_id: &str) -> Result<Subscriber, sqlx::Error> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO relay_subscribers (actor_id, inbox, host, kind, follow_id) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (actor_id) DO UPDATE SET inbox = EXCLUDED.inbox, kind = EXCLUDED.kind, follow_id = EXCLUDED.follow_id,
                                              state = CASE WHEN relay_subscribers.state = 'rejected' THEN 'pending' ELSE relay_subscribers.state END
         RETURNING {SUBSCRIBER_COLUMNS}"
    )))
    .bind(actor_id)
    .bind(inbox)
    .bind(host)
    .bind(kind)
    .bind(follow_id)
    .fetch_one(pool)
    .await?;
    Subscriber::from_row(&row)
}

pub async fn set_subscriber_state(pool: &PgPool, id: i64, state: State) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE relay_subscribers SET state = $2 WHERE id = $1").bind(id).bind(state.as_str()).execute(pool).await?;
    Ok(())
}

/// Removes a subscriber: by an admin (`id`), or its own Undo (`actor_id`).
pub async fn remove_subscriber(pool: &PgPool, id: Option<i64>, actor_id: Option<&str>) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM relay_subscribers WHERE id = $1 OR actor_id = $2").bind(id).bind(actor_id).execute(pool).await?;
    Ok(())
}

/// The accepted subscriber `actor_id`, sending content to relay.
pub async fn accepted_subscriber(pool: &PgPool, actor_id: &str) -> Result<Option<Subscriber>, sqlx::Error> {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {SUBSCRIBER_COLUMNS} FROM relay_subscribers WHERE actor_id = $1 AND state = 'accepted'"
    )))
    .bind(actor_id)
    .fetch_optional(pool)
    .await?
    .as_ref()
    .map(Subscriber::from_row)
    .transpose()
}

/// Accepted subscribers, of a `kind` or all, but the server `except_host`
/// (the one the content comes from): (inbox, kind).
pub async fn subscriber_inboxes(pool: &PgPool, except_host: Option<&str>) -> Result<Vec<(String, String)>, sqlx::Error> {
    sqlx::query("SELECT DISTINCT inbox, kind FROM relay_subscribers WHERE state = 'accepted' AND host IS DISTINCT FROM $1")
        .bind(except_host)
        .fetch_all(pool)
        .await?
        .iter()
        .map(|row| Ok((row.try_get("inbox")?, row.try_get("kind")?)))
        .collect()
}

/// Where the site's public posts go besides followers: the accepted relays
/// it follows and the servers it relays for.
pub async fn outgoing_inboxes(pool: &PgPool) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT inbox FROM relays WHERE state = 'accepted'
         UNION SELECT inbox FROM relay_subscribers WHERE state = 'accepted'",
    )
    .fetch_all(pool)
    .await
}

/// Counts for the moderation tab: subscribers waiting for an admin.
pub async fn pending_subscribers(pool: &PgPool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT COUNT(*) FROM relay_subscribers WHERE state = 'pending'").fetch_one(pool).await
}

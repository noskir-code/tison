//! Events and meetups, like Mobilizon's (`EVENT-PLANNING` in
//! `configs/services.conf`). An event is a post of kind `event` (comments,
//! likes, shares, feeds and federation come with it), its details here:
//! when, where (a place or online), how to join, how many can come. People
//! join it here or from Mobilizon (`Join`, `Leave`), and users here join
//! Mobilizon's events the same way.

use sqlx::{
    PgPool, Row,
    postgres::PgRow,
    types::chrono::{DateTime, Utc},
};

use crate::entities::schema::{Column, Index, Table};

/// Kept up to date automatically at startup, see `entities::schema`.
pub const TABLE: Table = Table {
    name: "events",
    columns: &[
        Column::new("post_id", "BIGINT").primary_key().references("posts(id) ON DELETE CASCADE"),
        Column::new("starts_at", "TIMESTAMPTZ"),
        Column::new("ends_at", "TIMESTAMPTZ").nullable(),
        // Where the organizer is, like `Europe/Paris`: told to Mobilizon.
        Column::new("timezone", "TEXT").default("'UTC'"),
        // A place: its name and address, written as people say them.
        Column::new("place_name", "TEXT").default("''"),
        Column::new("address", "TEXT").default("''"),
        // Online: the meeting's address, shown to participants.
        Column::new("online_url", "TEXT").nullable(),
        // One of `CATEGORIES`, Mobilizon's.
        Column::new("category", "TEXT").default("'MEETING'"),
        // Who can come: NULL is anyone.
        Column::new("capacity", "INTEGER").nullable(),
        // `free` (join at once) or `restricted` (the organizer accepts).
        Column::new("join_mode", "TEXT").default("'free'"),
        // `confirmed`, `tentative` or `cancelled`, like iCalendar.
        Column::new("status", "TEXT").default("'confirmed'"),
        // Counted by the event's server, for events of other servers.
        Column::new("remote_participants", "INTEGER").nullable(),
        // Where the address is, found by `geocode` or given by Mobilizon:
        // the map of the event.
        Column::new("latitude", "DOUBLE PRECISION").nullable(),
        Column::new("longitude", "DOUBLE PRECISION").nullable(),
        // The online link for everyone; else only for participants.
        Column::new("online_public", "BOOLEAN").default("FALSE"),
        // One date of a recurring event, see `SERIES_TABLE`.
        Column::new("series_id", "BIGINT").nullable().references("event_series(id) ON DELETE SET NULL"),
    ],
    indexes: &[
        Index::new("events_starts_at_idx", "starts_at"),
        Index::new("events_series_idx", "series_id, starts_at"),
    ],
};

/// A recurring event: its dates are events of their own (Mobilizon has no
/// recurrence), made a few at a time by `extend_series`.
pub const SERIES_TABLE: Table = Table {
    name: "event_series",
    columns: &[
        Column::new("id", "BIGSERIAL").primary_key(),
        // `weekly`, `biweekly` or `monthly`, see `Frequency`.
        Column::new("frequency", "TEXT"),
        // No date after this.
        Column::new("until", "TIMESTAMPTZ"),
        Column::new("created_at", "TIMESTAMPTZ").default("NOW()"),
    ],
    indexes: &[],
};

/// Reminders sent, so each goes once.
pub const REMINDERS_TABLE: Table = Table {
    name: "event_reminders",
    columns: &[
        Column::new("post_id", "BIGINT").references("posts(id) ON DELETE CASCADE"),
        Column::new("user_id", "INTEGER").references("users(id) ON DELETE CASCADE"),
        // The notification sent: `event_tomorrow` or `event_soon`.
        Column::new("kind", "TEXT"),
        Column::new("sent_at", "TIMESTAMPTZ").default("NOW()"),
    ],
    indexes: &[Index::unique("event_reminders_idx", "post_id, user_id, kind")],
};

/// How often a recurring event comes back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frequency {
    Weekly,
    Biweekly,
    Monthly,
}

impl Frequency {
    pub const ALL: [Self; 3] = [Self::Weekly, Self::Biweekly, Self::Monthly];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Weekly => "weekly",
            Self::Biweekly => "biweekly",
            Self::Monthly => "monthly",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Weekly => "Every week",
            Self::Biweekly => "Every 2 weeks",
            Self::Monthly => "Every month",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|frequency| frequency.as_str() == value)
    }

    /// For Postgres: added in the event's time zone, so 19:00 stays 19:00
    /// across summer time.
    fn interval(self) -> &'static str {
        match self {
            Self::Weekly => "7 days",
            Self::Biweekly => "14 days",
            Self::Monthly => "1 month",
        }
    }
}

/// The longest a series can go on.
pub const SERIES_MAX_DAYS: i64 = 366;
/// Dates made ahead, not over yet.
const SERIES_AHEAD: i64 = 2;

/// Who goes, here or from other servers.
pub const PARTICIPANTS_TABLE: Table = Table {
    name: "event_participants",
    columns: &[
        Column::new("id", "BIGSERIAL").primary_key(),
        Column::new("post_id", "BIGINT").references("posts(id) ON DELETE CASCADE"),
        Column::new("user_id", "INTEGER").nullable().references("users(id) ON DELETE CASCADE"),
        Column::new("remote_actor_id", "BIGINT").nullable().references("federation_actors(id) ON DELETE CASCADE"),
        // `going`, `pending` (waiting for the organizer) or `rejected`.
        Column::new("role", "TEXT").default("'going'"),
        // The Join: ours for an event of another server, theirs for one here.
        Column::new("join_id", "TEXT").nullable(),
        Column::new("message", "TEXT").default("''"),
        Column::new("created_at", "TIMESTAMPTZ").default("NOW()"),
    ],
    indexes: &[
        Index::unique("event_participants_user_idx", "post_id, user_id"),
        Index::unique("event_participants_remote_idx", "post_id, remote_actor_id"),
        Index::new("event_participants_join_idx", "join_id"),
        Index::new("event_participants_user_events_idx", "user_id, post_id"),
    ],
};

pub const TITLE_MAX_CHARS: usize = 200;
pub const PLACE_MAX_CHARS: usize = 200;
pub const ADDRESS_MAX_CHARS: usize = 500;

/// Mobilizon's categories, and how they're shown.
pub const CATEGORIES: [(&str, &str); 20] = [
    ("MEETING", "Meeting"),
    ("ARTS", "Arts"),
    ("BOOK_CLUBS", "Book clubs"),
    ("BUSINESS", "Business"),
    ("CAUSES", "Causes"),
    ("COMEDY", "Comedy"),
    ("COMMUNITY", "Community"),
    ("CRAFTS", "Crafts"),
    ("FAMILY_EDUCATION", "Family & education"),
    ("FILM_MEDIA", "Film & media"),
    ("FOOD_DRINK", "Food & drink"),
    ("GAMES", "Games"),
    ("HEALTH", "Health"),
    ("LANGUAGE_CULTURE", "Language & culture"),
    ("LEARNING", "Learning"),
    ("MUSIC", "Music"),
    ("OUTDOORS_ADVENTURE", "Outdoors & adventure"),
    ("PARTY", "Party"),
    ("SCIENCE_TECH", "Science & tech"),
    ("SPORTS", "Sports"),
];

pub fn category_label(category: &str) -> &'static str {
    CATEGORIES.iter().find(|(code, _)| *code == category).map_or("Meeting", |(_, label)| label)
}

/// The details of an event, with the viewer's part in it.
#[derive(Clone, Debug)]
pub struct EventDetails {
    pub starts_at: DateTime<Utc>,
    pub ends_at: Option<DateTime<Utc>>,
    pub timezone: String,
    pub place_name: String,
    pub address: String,
    pub online_url: Option<String>,
    pub category: String,
    pub capacity: Option<i32>,
    pub join_mode: String,
    pub status: String,
    /// Going: here, or as counted by the event's server.
    pub going: i64,
    /// Waiting for the organizer.
    pub pending: i64,
    /// `going`, `pending` or `rejected` for the viewer.
    pub viewer_role: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub online_public: bool,
    /// Its recurring event, with how often it comes back and until when.
    pub series: Option<(i64, String, DateTime<Utc>)>,
}

/// Columns of `EventDetails` for `post_select!`, from `ev` (the event) and
/// `$1` (the viewer).
macro_rules! event_columns {
    () => {
        "ev.starts_at AS event_starts_at, ev.ends_at AS event_ends_at, ev.timezone AS event_timezone,
         ev.place_name AS event_place_name, ev.address AS event_address, ev.online_url AS event_online_url,
         ev.category AS event_category, ev.capacity AS event_capacity, ev.join_mode AS event_join_mode,
         ev.status AS event_status,
         GREATEST(COALESCE(ev.remote_participants, 0),
                  (SELECT COUNT(*) FROM event_participants eg WHERE eg.post_id = p.id AND eg.role = 'going')) AS event_going,
         (SELECT COUNT(*) FROM event_participants eq WHERE eq.post_id = p.id AND eq.role = 'pending') AS event_pending,
         (SELECT er.role FROM event_participants er WHERE er.post_id = p.id AND er.user_id = $1) AS event_role,
         ev.latitude AS event_latitude, ev.longitude AS event_longitude, ev.online_public AS event_online_public,
         ev.series_id AS event_series_id,
         (SELECT es.frequency FROM event_series es WHERE es.id = ev.series_id) AS event_frequency,
         (SELECT es.until FROM event_series es WHERE es.id = ev.series_id) AS event_until"
    };
}
pub(crate) use event_columns;

impl EventDetails {
    /// From `event_columns!`, `None` for posts that aren't events.
    pub fn from_row(row: &PgRow) -> Result<Option<Self>, sqlx::Error> {
        let Some(starts_at) = row.try_get::<Option<DateTime<Utc>>, _>("event_starts_at")? else {
            return Ok(None);
        };
        Ok(Some(Self {
            starts_at,
            ends_at: row.try_get("event_ends_at")?,
            timezone: row.try_get("event_timezone")?,
            place_name: row.try_get("event_place_name")?,
            address: row.try_get("event_address")?,
            online_url: row.try_get("event_online_url")?,
            category: row.try_get("event_category")?,
            capacity: row.try_get("event_capacity")?,
            join_mode: row.try_get("event_join_mode")?,
            status: row.try_get("event_status")?,
            going: row.try_get("event_going")?,
            pending: row.try_get("event_pending")?,
            viewer_role: row.try_get("event_role")?,
            latitude: row.try_get("event_latitude")?,
            longitude: row.try_get("event_longitude")?,
            online_public: row.try_get("event_online_public")?,
            series: match (
                row.try_get::<Option<i64>, _>("event_series_id")?,
                row.try_get::<Option<String>, _>("event_frequency")?,
                row.try_get::<Option<DateTime<Utc>>, _>("event_until")?,
            ) {
                (Some(id), Some(frequency), Some(until)) => Some((id, frequency, until)),
                _ => None,
            },
        }))
    }

    pub fn is_cancelled(&self) -> bool {
        self.status == "cancelled"
    }

    pub fn is_tentative(&self) -> bool {
        self.status == "tentative"
    }

    /// Over: ended, or started more than a few hours ago without an end.
    pub fn is_past(&self) -> bool {
        let end = match self.ends_at {
            Some(end) => end.timestamp(),
            None => self.starts_at.timestamp() + 4 * 3600,
        };
        end < Utc::now().timestamp()
    }

    pub fn needs_approval(&self) -> bool {
        self.join_mode == "restricted"
    }

    pub fn is_online(&self) -> bool {
        self.online_url.is_some() && self.address.is_empty() && self.place_name.is_empty()
    }

    /// Places left, when there is a limit.
    pub fn places_left(&self) -> Option<i64> {
        self.capacity.map(|capacity| (i64::from(capacity) - self.going).max(0))
    }

    pub fn is_full(&self) -> bool {
        self.places_left() == Some(0)
    }

    pub fn is_going(&self) -> bool {
        self.viewer_role.as_deref() == Some("going")
    }

    pub fn is_pending(&self) -> bool {
        self.viewer_role.as_deref() == Some("pending")
    }

    pub fn category_label(&self) -> &'static str {
        category_label(&self.category)
    }

    pub fn starts_iso(&self) -> String {
        self.starts_at.to_rfc3339()
    }

    pub fn ends_iso(&self) -> Option<String> {
        self.ends_at.as_ref().map(DateTime::to_rfc3339)
    }

    /// Shown without JavaScript, which writes it in the viewer's time.
    pub fn starts_label(&self) -> String {
        self.starts_at.format("%a %b %-d, %Y · %H:%M UTC").to_string()
    }

    pub fn ends_label(&self) -> Option<String> {
        self.ends_at.map(|end| end.format("%a %b %-d, %Y · %H:%M UTC").to_string())
    }

    /// `NOV`, `7`: the date block of cards, in UTC (system.js redoes it).
    pub fn month_label(&self) -> String {
        self.starts_at.format("%b").to_string().to_uppercase()
    }

    pub fn day_label(&self) -> String {
        self.starts_at.format("%-d").to_string()
    }

    /// The place and address on one line.
    pub fn where_label(&self) -> String {
        match (self.place_name.trim(), self.address.trim()) {
            ("", "") => String::new(),
            (place, "") => place.to_string(),
            ("", address) => address.to_string(),
            (place, address) => format!("{place}, {address}"),
        }
    }

    /// OpenStreetMap: the place itself when it was found, else a search
    /// of the address.
    pub fn map_url(&self) -> Option<String> {
        if let (Some(latitude), Some(longitude)) = (self.latitude, self.longitude) {
            return Some(format!("https://www.openstreetmap.org/?mlat={latitude:.6}&mlon={longitude:.6}#map=17/{latitude:.6}/{longitude:.6}"));
        }
        let place = self.where_label();
        (!place.is_empty()).then(|| format!("https://www.openstreetmap.org/search?query={}", url_encode(&place)))
    }

    /// The map shown on its page: OpenStreetMap's, a pin on the place.
    pub fn map_embed_url(&self) -> Option<String> {
        let (latitude, longitude) = (self.latitude?, self.longitude?);
        let (dx, dy) = (0.006, 0.0035);
        Some(format!(
            "https://www.openstreetmap.org/export/embed.html?bbox={:.6}%2C{:.6}%2C{:.6}%2C{:.6}&layer=mapnik&marker={latitude:.6}%2C{longitude:.6}",
            longitude - dx,
            latitude - dy,
            longitude + dx,
            latitude + dy
        ))
    }

    /// `Every week until Dec 31, 2026`, for a recurring event.
    pub fn repeat_label(&self) -> Option<String> {
        let (_, frequency, until) = self.series.as_ref()?;
        let frequency = Frequency::parse(frequency)?;
        Some(format!("{} until {}", frequency.label(), until.format("%b %-d, %Y")))
    }
}

pub(crate) fn url_encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (byte as char).to_string(),
            b' ' => "+".to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// What the form gives, or another server's Event.
pub struct NewEvent<'a> {
    pub starts_at: DateTime<Utc>,
    pub ends_at: Option<DateTime<Utc>>,
    pub timezone: &'a str,
    pub place_name: &'a str,
    pub address: &'a str,
    pub online_url: Option<&'a str>,
    pub category: &'a str,
    pub capacity: Option<i32>,
    pub join_mode: &'a str,
    pub status: &'a str,
    pub remote_participants: Option<i32>,
    pub online_public: bool,
    /// Given by Mobilizon; `None` for events written here (see `geocode`).
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub series_id: Option<i64>,
}

/// Saves (or replaces) the details of the event `post_id`.
pub async fn save(pool: &PgPool, post_id: i64, event: &NewEvent<'_>) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO events (post_id, starts_at, ends_at, timezone, place_name, address, online_url, category,
                             capacity, join_mode, status, remote_participants, online_public, latitude, longitude, series_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
         ON CONFLICT (post_id) DO UPDATE SET starts_at = $2, ends_at = $3, timezone = $4, place_name = $5,
             address = $6, online_url = $7, category = $8, capacity = $9, join_mode = $10, status = $11,
             remote_participants = $12, online_public = $13,
             -- The place moved: found again (see `geocode`), unless given.
             latitude = CASE WHEN $14::double precision IS NOT NULL OR events.address <> $6 OR events.place_name <> $5
                             THEN $14 ELSE events.latitude END,
             longitude = CASE WHEN $15::double precision IS NOT NULL OR events.address <> $6 OR events.place_name <> $5
                              THEN $15 ELSE events.longitude END,
             series_id = COALESCE($16, events.series_id)",
    )
    .bind(post_id)
    .bind(event.starts_at)
    .bind(event.ends_at)
    .bind(event.timezone)
    .bind(event.place_name)
    .bind(event.address)
    .bind(event.online_url)
    .bind(event.category)
    .bind(event.capacity)
    .bind(event.join_mode)
    .bind(event.status)
    .bind(event.remote_participants)
    .bind(event.online_public)
    .bind(event.latitude)
    .bind(event.longitude)
    .bind(event.series_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Where the address is, found by `geocode`: for the other dates of a
/// recurring event at the same place too (made before it was found).
pub async fn set_coordinates(pool: &PgPool, post_id: i64, latitude: f64, longitude: f64) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE events e SET latitude = $2, longitude = $3
         FROM events found
         WHERE found.post_id = $1
           AND (e.post_id = $1 OR (e.series_id = found.series_id AND e.latitude IS NULL
                                  AND e.address = found.address AND e.place_name = found.place_name))",
    )
        .bind(post_id)
        .bind(latitude)
        .bind(longitude)
        .execute(pool)
        .await?;
    Ok(())
}

/// A new recurring event. `until` is its last possible start.
pub async fn create_series(pool: &PgPool, frequency: Frequency, until: DateTime<Utc>) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("INSERT INTO event_series (frequency, until) VALUES ($1, $2) RETURNING id")
        .bind(frequency.as_str())
        .bind(until)
        .fetch_one(pool)
        .await
}

/// Ends a recurring event after its dates already made.
pub async fn stop_series(pool: &PgPool, series_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE event_series SET until = LEAST(until, NOW()) WHERE id = $1").bind(series_id).execute(pool).await?;
    Ok(())
}

/// The next date of each recurring event with fewer than `SERIES_AHEAD`
/// dates to come: a copy of its last one, moved by its frequency (in its
/// time zone). Returns the new events' ids, to announce them.
pub async fn extend_series(pool: &PgPool) -> Result<Vec<i64>, sqlx::Error> {
    let due = sqlx::query(
        "SELECT es.id, es.frequency, es.until,
                (SELECT ev.post_id FROM events ev WHERE ev.series_id = es.id ORDER BY ev.starts_at DESC LIMIT 1) AS last_post
         FROM event_series es
         WHERE es.until > NOW()
           AND (SELECT COUNT(*) FROM events ev WHERE ev.series_id = es.id AND ev.starts_at > NOW() AND ev.status <> 'cancelled') < $1",
    )
    .bind(SERIES_AHEAD)
    .fetch_all(pool)
    .await?;
    let mut created = Vec::new();
    for row in due {
        let (Some(last_post), Some(frequency)) = (
            row.try_get::<Option<i64>, _>("last_post")?,
            Frequency::parse(&row.try_get::<String, _>("frequency")?),
        ) else {
            continue;
        };
        let until: DateTime<Utc> = row.try_get("until")?;
        if let Some(post_id) = next_date(pool, last_post, frequency, until).await? {
            created.push(post_id);
        }
    }
    Ok(created)
}

/// `z.zone`: the time zone of the event `$1` when Postgres knows it, else
/// UTC. Dates move in it, so 19:00 stays 19:00 across summer time.
macro_rules! next_zone {
    () => {
        "WITH z AS (SELECT CASE WHEN EXISTS (SELECT 1 FROM pg_timezone_names WHERE name = ev.timezone)
                                THEN ev.timezone ELSE 'UTC' END AS zone
                    FROM events ev WHERE ev.post_id = $1)"
    };
}

/// The date after `last_post`, unless it would start after `until`.
async fn next_date(pool: &PgPool, last_post: i64, frequency: Frequency, until: DateTime<Utc>) -> Result<Option<i64>, sqlx::Error> {
    let mut transaction = pool.begin().await?;
    let next_start: DateTime<Utc> = sqlx::query_scalar(concat!(
        next_zone!(),
        " SELECT ((ev.starts_at AT TIME ZONE z.zone) + $2::interval) AT TIME ZONE z.zone FROM events ev, z WHERE ev.post_id = $1"
    ))
    .bind(last_post)
    .bind(frequency.interval())
    .fetch_one(&mut *transaction)
    .await?;
    if next_start > until {
        return Ok(None);
    }
    let post_id: i64 = sqlx::query_scalar(
        "INSERT INTO posts (user_id, kind, title, body, visibility, poster_url, community_id)
         SELECT user_id, 'event', title, body, visibility, poster_url, community_id FROM posts WHERE id = $1
         RETURNING id",
    )
    .bind(last_post)
    .fetch_one(&mut *transaction)
    .await?;
    sqlx::query(concat!(
        next_zone!(),
        " INSERT INTO events (post_id, starts_at, ends_at, timezone, place_name, address, online_url, category, capacity,
                              join_mode, status, online_public, latitude, longitude, series_id)
          SELECT $2, ((ev.starts_at AT TIME ZONE z.zone) + $3::interval) AT TIME ZONE z.zone,
                 ((ev.ends_at AT TIME ZONE z.zone) + $3::interval) AT TIME ZONE z.zone,
                 timezone, place_name, address, online_url, category, capacity,
                 join_mode, CASE WHEN status = 'cancelled' THEN 'confirmed' ELSE status END, online_public, latitude, longitude, series_id
          FROM events ev, z WHERE ev.post_id = $1"
    ))
    .bind(last_post)
    .bind(post_id)
    .bind(frequency.interval())
    .execute(&mut *transaction)
    .await?;
    // The organizer goes, like on the first date.
    sqlx::query(
        "INSERT INTO event_participants (post_id, user_id, role)
         SELECT $2, user_id, 'going' FROM posts WHERE id = $1 AND user_id IS NOT NULL",
    )
    .bind(last_post)
    .bind(post_id)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(Some(post_id))
}

pub async fn set_status(pool: &PgPool, post_id: i64, status: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE events SET status = $2 WHERE post_id = $1").bind(post_id).bind(status).execute(pool).await?;
    sqlx::query("UPDATE posts SET edited_at = NOW(), updated_at = NOW() WHERE id = $1").bind(post_id).execute(pool).await?;
    Ok(())
}

/// A user joins (or asks to join, `pending`). `join_id` is the Join sent to
/// the event's server, for an event of another server.
pub async fn join_local(pool: &PgPool, post_id: i64, user_id: i32, role: &str, join_id: Option<&str>) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO event_participants (post_id, user_id, role, join_id) VALUES ($1, $2, $3, $4)
         ON CONFLICT (post_id, user_id) DO UPDATE SET role = $3, join_id = $4, created_at = NOW()",
    )
    .bind(post_id)
    .bind(user_id)
    .bind(role)
    .bind(join_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Someone of another server joins an event here, with their Join's id.
/// Returns the participant's id.
pub async fn join_remote(pool: &PgPool, post_id: i64, remote_actor_id: i64, role: &str, join_id: &str, message: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO event_participants (post_id, remote_actor_id, role, join_id, message) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (post_id, remote_actor_id) DO UPDATE SET role = $3, join_id = $4, message = $5
         RETURNING id",
    )
    .bind(post_id)
    .bind(remote_actor_id)
    .bind(role)
    .bind(join_id)
    .bind(message)
    .fetch_one(pool)
    .await
}

/// Leaves: returns the Join it had, to undo on the event's server.
pub async fn leave_local(pool: &PgPool, post_id: i64, user_id: i32) -> Result<Option<String>, sqlx::Error> {
    Ok(sqlx::query_scalar("DELETE FROM event_participants WHERE post_id = $1 AND user_id = $2 RETURNING join_id")
        .bind(post_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .flatten())
}

pub async fn leave_remote(pool: &PgPool, post_id: i64, remote_actor_id: i64) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("DELETE FROM event_participants WHERE post_id = $1 AND remote_actor_id = $2")
        .bind(post_id)
        .bind(remote_actor_id)
        .execute(pool)
        .await?
        .rows_affected()
        > 0)
}

/// The answer of the event's server to a Join sent from here.
pub async fn answered(pool: &PgPool, join_id: &str, role: &str) -> Result<Option<(i64, i32)>, sqlx::Error> {
    let row = sqlx::query(
        "UPDATE event_participants SET role = $2 WHERE join_id = $1 AND user_id IS NOT NULL RETURNING post_id, user_id",
    )
    .bind(join_id)
    .bind(role)
    .fetch_optional(pool)
    .await?;
    row.map(|row| Ok((row.try_get("post_id")?, row.try_get("user_id")?))).transpose()
}

/// Someone on the list of an event, as its organizer sees them.
pub struct Participant {
    pub id: i64,
    pub role: String,
    pub message: String,
    pub name: String,
    /// `@alice` or `@alice@mobilizon.fr`.
    pub handle: String,
    pub url: String,
    pub photo_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub user_id: Option<i32>,
    pub remote_actor_id: Option<i64>,
    pub join_id: Option<String>,
}

impl Participant {
    pub fn is_pending(&self) -> bool {
        self.role == "pending"
    }

    /// The local user `user_id` (a reference, as Askama gives it).
    pub fn is_user(&self, user_id: &i32) -> bool {
        self.user_id == Some(*user_id)
    }

    pub fn initial(&self) -> String {
        self.name.chars().next().map(|letter| letter.to_uppercase().collect()).unwrap_or_default()
    }

    pub fn created_at_iso(&self) -> String {
        self.created_at.to_rfc3339()
    }
}

/// Everyone who joined or asked, those waiting first.
pub async fn participants(pool: &PgPool, post_id: i64) -> Result<Vec<Participant>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT ep.id, ep.role, ep.message, ep.created_at, ep.user_id, ep.remote_actor_id, ep.join_id,
                u.username, u.displayname, u.photo_url,
                ra.actor_id AS remote_url, ra.username AS remote_username, ra.display_name AS remote_name, ra.icon_url, ra.host
         FROM event_participants ep
         LEFT JOIN users u ON u.id = ep.user_id
         LEFT JOIN federation_actors ra ON ra.id = ep.remote_actor_id
         WHERE ep.post_id = $1 AND ep.role <> 'rejected'
         ORDER BY ep.role = 'pending' DESC, ep.created_at",
    )
    .bind(post_id)
    .fetch_all(pool)
    .await?;
    rows.iter()
        .map(|row| {
            let remote_actor_id: Option<i64> = row.try_get("remote_actor_id")?;
            let (name, handle, url, photo_url) = match remote_actor_id {
                Some(id) => {
                    let username: String = row.try_get::<Option<String>, _>("remote_username")?.unwrap_or_default();
                    let host: String = row.try_get::<Option<String>, _>("host")?.unwrap_or_default();
                    let name: Option<String> = row.try_get("remote_name")?;
                    let icon: Option<String> = row.try_get("icon_url")?;
                    (
                        name.filter(|name| !name.trim().is_empty()).unwrap_or_else(|| username.clone()),
                        format!("@{username}@{host}"),
                        format!("/remote/{id}"),
                        icon.map(|icon| crate::entities::media::proxy_url(&icon)),
                    )
                }
                None => {
                    let username: String = row.try_get::<Option<String>, _>("username")?.unwrap_or_default();
                    let display: String = row.try_get::<Option<String>, _>("displayname")?.unwrap_or_default();
                    (
                        if display.trim().is_empty() { username.clone() } else { display },
                        format!("@{username}"),
                        format!("/user/{username}"),
                        row.try_get("photo_url")?,
                    )
                }
            };
            Ok(Participant {
                id: row.try_get("id")?,
                role: row.try_get("role")?,
                message: row.try_get("message")?,
                name,
                handle,
                url,
                photo_url,
                created_at: row.try_get("created_at")?,
                user_id: row.try_get("user_id")?,
                remote_actor_id,
                join_id: row.try_get("join_id")?,
            })
        })
        .collect()
}

/// The organizer's answer to a request. Returns the participant, to tell them.
pub async fn set_role(pool: &PgPool, post_id: i64, participant_id: i64, role: &str) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("UPDATE event_participants SET role = $3 WHERE post_id = $1 AND id = $2")
        .bind(post_id)
        .bind(participant_id)
        .bind(role)
        .execute(pool)
        .await?
        .rows_affected()
        > 0)
}

/// The ActivityPub ids of the participants of other servers: they hear of
/// the event's changes like Mobilizon's participants.
pub async fn remote_participant_ids(pool: &PgPool, post_id: i64) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT ra.actor_id FROM event_participants ep JOIN federation_actors ra ON ra.id = ep.remote_actor_id
         WHERE ep.post_id = $1 AND ep.role = 'going'",
    )
    .bind(post_id)
    .fetch_all(pool)
    .await
}

/// The local participants, to tell them of a cancellation.
pub async fn local_participant_ids(pool: &PgPool, post_id: i64) -> Result<Vec<i32>, sqlx::Error> {
    sqlx::query_scalar("SELECT user_id FROM event_participants WHERE post_id = $1 AND user_id IS NOT NULL AND role = 'going'")
        .bind(post_id)
        .fetch_all(pool)
        .await
}

//! Podcasts, like Funkwhale's channels and Castopod's: a title, a cover and
//! a description, and episodes (posts of kind `episode`, see `post::Kind`)
//! read in Tison or in any podcast app through the RSS feed
//! (`handlers::podcast::feed`).

use sqlx::{PgPool, Row, postgres::PgRow};

use crate::entities::schema::{Column, Index, Table};

/// Kept up to date automatically at startup, see `entities::schema`.
pub const TABLE: Table = Table {
    name: "podcasts",
    columns: &[
        Column::new("id", "BIGSERIAL").primary_key(),
        Column::new("user_id", "INTEGER").references("users(id) ON DELETE CASCADE"),
        Column::new("title", "TEXT"),
        Column::new("description", "TEXT").default("''"),
        // Square, at least 1400 pixels for Apple Podcasts.
        Column::new("cover_url", "TEXT").nullable(),
        // One of `CATEGORIES`, read by podcast apps.
        Column::new("category", "TEXT").default("'Society & Culture'"),
        // Like `en` or `fr`.
        Column::new("language", "TEXT").default("'en'"),
        Column::new("explicit", "BOOLEAN").default("FALSE"),
        Column::new("created_at", "TIMESTAMPTZ").default("NOW()"),
    ],
    indexes: &[Index::new("podcasts_user_id_idx", "user_id, created_at DESC")],
};

pub const TITLE_MAX_CHARS: usize = 120;
pub const DESCRIPTION_MAX_CHARS: usize = 4_000;

/// The top categories of Apple Podcasts, which other apps use too.
pub const CATEGORIES: [&str; 19] = [
    "Arts",
    "Business",
    "Comedy",
    "Education",
    "Fiction",
    "Government",
    "Health & Fitness",
    "History",
    "Kids & Family",
    "Leisure",
    "Music",
    "News",
    "Religion & Spirituality",
    "Science",
    "Society & Culture",
    "Sports",
    "Technology",
    "True Crime",
    "TV & Film",
];

pub struct Podcast {
    pub id: i64,
    pub user_id: i32,
    pub username: String,
    pub author_name: String,
    pub title: String,
    pub description: String,
    pub cover_url: Option<String>,
    pub category: String,
    pub language: String,
    pub explicit: bool,
    /// Public episodes, ready to play.
    pub episode_count: i64,
}

impl Podcast {
    fn from_row(row: &PgRow) -> Result<Self, sqlx::Error> {
        let username: String = row.try_get("username")?;
        let display_name: String = row.try_get("displayname")?;
        Ok(Self {
            id: row.try_get("id")?,
            user_id: row.try_get("user_id")?,
            // The name shown on the profile; names can be private.
            author_name: if display_name.trim().is_empty() { username.clone() } else { display_name },
            username,
            title: row.try_get("title")?,
            description: row.try_get("description")?,
            cover_url: row.try_get("cover_url")?,
            category: row.try_get("category")?,
            language: row.try_get("language")?,
            explicit: row.try_get("explicit")?,
            episode_count: row.try_get("episode_count")?,
        })
    }

    pub fn description_html(&self) -> String {
        crate::linkify::to_html(&self.description)
    }
}

macro_rules! podcast_select {
    () => {
        "SELECT pc.id, pc.user_id, u.username, u.displayname, pc.title, pc.description,
                pc.cover_url, pc.category, pc.language, pc.explicit, pc.created_at,
                (SELECT COUNT(*) FROM posts e WHERE e.podcast_id = pc.id AND e.visibility = 'public'
                   AND e.media_status = 'ready') AS episode_count
         FROM podcasts pc
         JOIN users u ON u.id = pc.user_id
         WHERE u.suspended_at IS NULL"
    };
}

/// Every podcast, those with a new episode first.
pub async fn all(pool: &PgPool) -> Result<Vec<Podcast>, sqlx::Error> {
    sqlx::query(concat!(
        podcast_select!(),
        " ORDER BY COALESCE((SELECT MAX(e.created_at) FROM posts e WHERE e.podcast_id = pc.id
                              AND e.visibility = 'public' AND e.media_status = 'ready'), pc.created_at) DESC
          LIMIT 200"
    ))
    .fetch_all(pool)
    .await?
    .iter()
    .map(Podcast::from_row)
    .collect()
}

pub async fn find(pool: &PgPool, id: i64) -> Result<Option<Podcast>, sqlx::Error> {
    sqlx::query(concat!(podcast_select!(), " AND pc.id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(Podcast::from_row)
        .transpose()
}

pub struct NewPodcast<'a> {
    pub title: &'a str,
    pub description: &'a str,
    pub cover_url: Option<&'a str>,
    pub category: &'a str,
    pub language: &'a str,
    pub explicit: bool,
}

pub async fn create(pool: &PgPool, user_id: i32, podcast: &NewPodcast<'_>) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO podcasts (user_id, title, description, cover_url, category, language, explicit)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(user_id)
    .bind(podcast.title)
    .bind(podcast.description)
    .bind(podcast.cover_url)
    .bind(podcast.category)
    .bind(podcast.language)
    .bind(podcast.explicit)
    .fetch_one(pool)
    .await
}

/// The episodes of a podcast, for deleting their files with it.
pub async fn episode_ids(pool: &PgPool, podcast_id: i64) -> Result<Vec<i64>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM posts WHERE podcast_id = $1")
        .bind(podcast_id)
        .fetch_all(pool)
        .await
}

pub async fn delete(pool: &PgPool, podcast_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM podcasts WHERE id = $1").bind(podcast_id).execute(pool).await?;
    Ok(())
}

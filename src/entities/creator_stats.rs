//! Numbers of the creator dashboard (see `handlers::dashboard`): per day
//! over a period, for a user's videos, posts with images, other posts, one
//! of their posts, or their followers. Days are UTC dates.
//!
//! Likes and shares from other servers have no date: they're in the totals
//! of a post, not in its days.

use sqlx::{PgPool, Row, types::chrono::NaiveDate};

use crate::entities::media;

/// What a dashboard tab or page is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Videos,
    ImagePosts,
    /// Written posts and articles, without pictures.
    Posts,
    /// One post of the user.
    Post(i64),
}

impl Scope {
    /// Which posts: `$1` is the user, `$3` the post for `Post`, else NULL.
    /// Each uses `$3`: Postgres refuses a parameter given but not used.
    fn filter(self) -> &'static str {
        match self {
            Self::Videos => "p.user_id = $1 AND $3::bigint IS NULL AND p.kind = 'video'",
            Self::ImagePosts => {
                "p.user_id = $1 AND $3::bigint IS NULL AND p.kind = 'post'
                 AND EXISTS (SELECT 1 FROM attachments a WHERE a.post_id = p.id AND a.media_type LIKE 'image/%')"
            }
            Self::Posts => {
                "p.user_id = $1 AND $3::bigint IS NULL AND p.kind = 'post'
                 AND NOT EXISTS (SELECT 1 FROM attachments a WHERE a.post_id = p.id AND a.media_type LIKE 'image/%')"
            }
            Self::Post(_) => "p.user_id = $1 AND p.id = $3",
        }
    }

    fn post_id(self) -> Option<i64> {
        match self {
            Self::Post(id) => Some(id),
            _ => None,
        }
    }
}

/// What is counted per day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Metric {
    Views,
    Likes,
    /// By others than the author.
    Comments,
    Shares,
    /// Posts of the scope published that day.
    Published,
}

/// Per day: `(day, count)` for each day of the period, oldest first.
pub type Series = Vec<(NaiveDate, i64)>;

/// The last `days` days, today included, with the counts of `metric`.
pub async fn daily(pool: &PgPool, user_id: i32, scope: Scope, metric: Metric, days: i32) -> Result<Series, sqlx::Error> {
    // One statement per pair: SQL text can't be assembled at run time here.
    let counted = match metric {
        Metric::Views => "SELECT v.day, SUM(v.views)::bigint AS n FROM post_views_daily v JOIN posts p ON p.id = v.post_id WHERE {filter} GROUP BY v.day",
        Metric::Likes => "SELECT l.created_at::date AS day, COUNT(*) AS n FROM post_likes l JOIN posts p ON p.id = l.post_id WHERE {filter} GROUP BY 1",
        Metric::Comments => {
            "SELECT c.created_at::date AS day, COUNT(*) AS n FROM comments c JOIN posts p ON p.id = c.post_id
             WHERE {filter} AND c.user_id IS DISTINCT FROM p.user_id GROUP BY 1"
        }
        Metric::Shares => "SELECT s.created_at::date AS day, COUNT(*) AS n FROM post_shares s JOIN posts p ON p.id = s.post_id WHERE {filter} GROUP BY 1",
        Metric::Published => "SELECT p.created_at::date AS day, COUNT(*) AS n FROM posts p WHERE {filter} GROUP BY 1",
    };
    let query = format!(
        "WITH counted AS ({}) SELECT d::date AS day, COALESCE(c.n, 0)::bigint AS n
         FROM generate_series(CURRENT_DATE - ($2 - 1), CURRENT_DATE, INTERVAL '1 day') AS d
         LEFT JOIN counted c ON c.day = d::date
         ORDER BY d",
        counted.replace("{filter}", scope.filter())
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(query))
        .bind(user_id)
        .bind(days)
        .bind(scope.post_id())
        .fetch_all(pool)
        .await?;
    rows.iter().map(|row| Ok((row.try_get("day")?, row.try_get("n")?))).collect()
}

/// Followers on each day of the period: here and on the Fediverse, by when
/// they followed. Those who left aren't known anymore: they don't count.
pub async fn followers(pool: &PgPool, user_id: i32, days: i32) -> Result<Series, sqlx::Error> {
    let rows = sqlx::query(
        "WITH follows AS (
            SELECT created_at::date AS day FROM user_follows WHERE followed_id = $1
            UNION ALL
            SELECT created_at::date AS day FROM federation_followers WHERE user_id = $1
         )
         SELECT d::date AS day, (SELECT COUNT(*) FROM follows f WHERE f.day <= d::date) AS n
         FROM generate_series(CURRENT_DATE - ($2 - 1), CURRENT_DATE, INTERVAL '1 day') AS d
         ORDER BY d",
    )
    .bind(user_id)
    .bind(days)
    .fetch_all(pool)
    .await?;
    rows.iter().map(|row| Ok((row.try_get("day")?, row.try_get("n")?))).collect()
}

/// Followers here and on the Fediverse, now.
pub async fn follower_split(pool: &PgPool, user_id: i32) -> Result<(i64, i64), sqlx::Error> {
    let row = sqlx::query(
        "SELECT (SELECT COUNT(*) FROM user_follows WHERE followed_id = $1) AS here,
                (SELECT COUNT(*) FROM federation_followers WHERE user_id = $1) AS fediverse",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok((row.try_get("here")?, row.try_get("fediverse")?))
}

/// A post of the user in the content list, with its totals of all time.
pub struct ContentRow {
    pub id: i64,
    /// The title, or the start of the text.
    pub label: String,
    /// Its picture, for the preview: a video's thumbnail, the first photo, or
    /// an article's first picture. `None` for a written post.
    pub picture: Option<String>,
    /// Photos of the post, shown as "+2" on the preview past the first.
    pub photos: i64,
    pub is_video: bool,
    pub is_article: bool,
    pub duration_seconds: Option<i32>,
    /// The start of the text, without Markdown, for a written post's preview.
    pub excerpt: String,
    pub created_at: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>,
    pub views: i64,
    /// Here and from other servers.
    pub likes: i64,
    pub comments: i64,
    pub shares: i64,
}

impl ContentRow {
    /// A post of text only: its preview is its text, its page isn't titled with it again.
    pub fn is_written(&self) -> bool {
        self.picture.is_none() && !self.is_video && !self.is_article
    }

    pub fn date_label(&self) -> String {
        crate::i18n::date(self.created_at.date_naive())
    }

    pub fn duration_label(&self) -> Option<String> {
        self.duration_seconds.filter(|seconds| *seconds > 0).map(crate::av::duration_label)
    }

    /// "+2" past the first photo, empty for one or none.
    pub fn more_photos(&self) -> String {
        if self.photos > 1 { format!("+{}", self.photos - 1) } else { String::new() }
    }

    /// The start of `excerpt`, at most `chars` letters, cut at a word.
    pub fn excerpt_of(&self, chars: usize) -> String {
        let text = self.excerpt.trim();
        if text.chars().count() <= chars {
            return text.to_string();
        }
        let cut: String = text.chars().take(chars).collect();
        let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(start, _)| start);
        format!("{}…", cut.trim_end_matches([',', '.', ';', ':']))
    }
}

/// How much text the previews may show.
const EXCERPT_CHARS: i32 = 400;

/// How many posts the content list shows, the newest first.
pub const CONTENT_SHOWN: i64 = 100;

/// The user's posts of `scope`, newest first, with their totals.
pub async fn content(pool: &PgPool, user_id: i32, scope: Scope) -> Result<Vec<ContentRow>, sqlx::Error> {
    let query = format!(
        "SELECT p.id, p.created_at, p.view_count, p.kind, p.duration_seconds, p.poster_url,
                COALESCE(p.title, '') AS title,
                (p.kind = 'post' AND p.title IS NOT NULL) AS is_article,
                -- Articles in whole: their picture and text come from the Markdown.
                CASE WHEN p.kind = 'post' AND p.title IS NOT NULL THEN p.body
                     ELSE LEFT(p.body, {EXCERPT_CHARS}) END AS text,
                (SELECT a.url FROM attachments a WHERE a.post_id = p.id AND a.media_type LIKE 'image/%'
                 ORDER BY a.position, a.id LIMIT 1) AS first_photo,
                (SELECT COUNT(*) FROM attachments a WHERE a.post_id = p.id AND a.media_type LIKE 'image/%') AS photos,
                (SELECT COUNT(*) FROM post_likes l WHERE l.post_id = p.id)
                  + (SELECT COUNT(*) FROM remote_reactions r WHERE r.post_id = p.id AND r.kind = 'like') AS likes,
                (SELECT COUNT(*) FROM comments c WHERE c.post_id = p.id AND c.user_id IS DISTINCT FROM p.user_id) AS comments,
                (SELECT COUNT(*) FROM post_shares s WHERE s.post_id = p.id)
                  + (SELECT COUNT(*) FROM remote_reactions r WHERE r.post_id = p.id AND r.kind = 'share') AS shares
         FROM posts p WHERE {}
         ORDER BY p.created_at DESC LIMIT $2",
        scope.filter()
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(query))
        .bind(user_id)
        .bind(CONTENT_SHOWN)
        .bind(scope.post_id())
        .fetch_all(pool)
        .await?;
    rows.iter()
        .map(|row| {
            let is_article: bool = row.try_get("is_article")?;
            let text: String = row.try_get("text")?;
            let shown = |url: String| if media::is_local(&url) { url } else { media::proxy_url(&url) };
            let picture = match row.try_get::<String, _>("kind")?.as_str() {
                "video" => row.try_get::<Option<String>, _>("poster_url")?,
                _ if is_article => crate::markdown::first_picture(&text).or(row.try_get("first_photo")?),
                _ => row.try_get("first_photo")?,
            };
            let excerpt = if is_article { crate::markdown::to_text(&text) } else { text.split_whitespace().collect::<Vec<_>>().join(" ") };
            let mut content = ContentRow {
                id: row.try_get("id")?,
                label: row.try_get("title")?,
                picture: picture.map(shown),
                photos: row.try_get("photos")?,
                is_video: row.try_get::<String, _>("kind")? == "video",
                is_article,
                duration_seconds: row.try_get("duration_seconds")?,
                excerpt,
                created_at: row.try_get("created_at")?,
                views: row.try_get("view_count")?,
                likes: row.try_get("likes")?,
                comments: row.try_get("comments")?,
                shares: row.try_get("shares")?,
            };
            // Without a title, the start of the text, cut at a word.
            if content.label.is_empty() {
                content.label = content.excerpt_of(90);
            }
            Ok(content)
        })
        .collect()
}

/// Which scope one of the user's posts is in, `None` when it isn't theirs.
pub async fn scope_of(pool: &PgPool, user_id: i32, post_id: i64) -> Result<Option<Scope>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT p.kind, EXISTS (SELECT 1 FROM attachments a WHERE a.post_id = p.id AND a.media_type LIKE 'image/%') AS images
         FROM posts p WHERE p.id = $1 AND p.user_id = $2",
    )
    .bind(post_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let kind: String = row.try_get("kind")?;
    Ok(Some(match (kind.as_str(), row.try_get::<bool, _>("images")?) {
        ("video", _) => Scope::Videos,
        (_, true) => Scope::ImagePosts,
        _ => Scope::Posts,
    }))
}

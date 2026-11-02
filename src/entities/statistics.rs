//! Counts for the site's staff: what was made here (the Statistics tab) and
//! what this site met of the Fediverse (the Federation tab). Counted when the
//! page opens, each in one query.

use sqlx::{PgPool, Row, postgres::PgRow};

/// One number, with a short explanation when its label isn't enough.
pub struct Stat {
    pub label: &'static str,
    pub value: i64,
    pub hint: &'static str,
}

impl Stat {
    /// `12 345`: grouped by thousands, with a thin space.
    pub fn value_label(&self) -> String {
        let digits = self.value.unsigned_abs().to_string();
        let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
        for (index, digit) in digits.chars().enumerate() {
            if index > 0 && (digits.len() - index) % 3 == 0 {
                grouped.push('\u{202F}');
            }
            grouped.push(digit);
        }
        if self.value < 0 { format!("-{grouped}") } else { grouped }
    }
}

/// Numbers shown together, under a title.
pub struct StatGroup {
    pub title: &'static str,
    pub stats: Vec<Stat>,
}

/// `(label, column, hint, shown)`: a number of the row, kept when `shown`.
type Wanted = (&'static str, &'static str, &'static str, bool);

fn group(row: &PgRow, title: &'static str, wanted: &[Wanted]) -> Result<Option<StatGroup>, sqlx::Error> {
    let mut stats = Vec::new();
    for (label, column, hint, shown) in wanted {
        if *shown {
            stats.push(Stat { label, value: row.try_get(*column)?, hint });
        }
    }
    Ok((!stats.is_empty()).then_some(StatGroup { title, stats }))
}

/// What was made on this site, the groups of services turned off left out.
pub async fn local(pool: &PgPool) -> Result<Vec<StatGroup>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT
            (SELECT COUNT(*) FROM users) AS users,
            (SELECT COUNT(*) FROM users WHERE created_at > NOW() - INTERVAL '30 days') AS new_users,
            (SELECT COUNT(*) FROM users WHERE role IN ('moderator', 'admin')) AS staff,
            (SELECT COUNT(*) FROM users WHERE suspended_at IS NOT NULL) AS suspended,
            (SELECT COUNT(*) FROM two_factor) AS two_factor,
            (SELECT COUNT(DISTINCT user_id) FROM push_subscriptions) AS push_users,
            (SELECT COUNT(*) FROM posts WHERE user_id IS NOT NULL AND kind = 'post' AND title IS NULL) AS posts,
            (SELECT COUNT(*) FROM posts WHERE user_id IS NOT NULL AND kind = 'post' AND title IS NOT NULL) AS articles,
            (SELECT COUNT(*) FROM comments WHERE user_id IS NOT NULL) AS comments,
            (SELECT COUNT(*) FROM post_likes) + (SELECT COUNT(*) FROM comment_likes) AS likes,
            (SELECT COUNT(*) FROM post_shares) AS shares,
            (SELECT COUNT(*) FROM attachments a
               LEFT JOIN posts p ON p.id = a.post_id LEFT JOIN comments c ON c.id = a.comment_id
              WHERE a.media_type LIKE 'image/%' AND COALESCE(p.user_id, c.user_id) IS NOT NULL) AS images,
            (SELECT COUNT(*) FROM posts p WHERE p.user_id IS NOT NULL AND EXISTS (
                SELECT 1 FROM attachments a WHERE a.post_id = p.id AND a.media_type LIKE 'image/%')) AS image_posts,
            (SELECT COUNT(*) FROM article_pictures) AS article_pictures,
            (SELECT COUNT(*) FROM posts WHERE user_id IS NOT NULL AND kind = 'video') AS videos,
            (SELECT COUNT(*) FROM posts WHERE user_id IS NOT NULL AND kind = 'video' AND media_status = 'processing') AS converting,
            (SELECT COALESCE(SUM(duration_seconds), 0) / 3600 FROM posts WHERE user_id IS NOT NULL AND kind = 'video')::bigint AS video_hours,
            (SELECT COUNT(*) FROM posts WHERE user_id IS NOT NULL AND kind = 'live') AS lives,
            (SELECT COUNT(*) FROM posts WHERE user_id IS NOT NULL AND kind = 'live' AND media_status = 'live') AS live_now,
            (SELECT COUNT(*) FROM podcasts) AS podcasts,
            (SELECT COUNT(*) FROM posts WHERE user_id IS NOT NULL AND kind = 'episode') AS episodes,
            (SELECT COUNT(*) FROM posts WHERE user_id IS NOT NULL AND kind = 'event') AS events,
            (SELECT COUNT(*) FROM posts p JOIN events e ON e.post_id = p.id
              WHERE p.user_id IS NOT NULL AND e.starts_at > NOW()) AS upcoming_events,
            (SELECT COUNT(*) FROM communities) AS communities,
            (SELECT COUNT(*) FROM community_members) AS members,
            (SELECT COUNT(*) FROM friendships WHERE accepted) AS friendships,
            (SELECT COUNT(*) FROM conversations) AS conversations,
            (SELECT COUNT(*) FROM messages WHERE sender_user_id IS NOT NULL) AS messages,
            (SELECT COUNT(*) FROM reports WHERE status = 'open') AS open_reports",
    )
    .fetch_one(pool)
    .await?;
    let services = crate::settings::services();
    let photos = services.image_sharing;
    let groups = [
        group(&row, "People", &[
            ("Users", "users", "", true),
            ("New, last 30 days", "new_users", "", true),
            ("Staff", "staff", "The creator and the moderators", true),
            ("Suspended", "suspended", "", true),
            ("With two-factor", "two_factor", "Users who turned on two-factor authentication", true),
            ("With browser notifications", "push_users", "Users receiving notifications in at least one browser", true),
        ])?,
        group(&row, "Writing", &[
            ("Posts", "posts", "Without the articles, videos, episodes, live streams and events", true),
            ("Articles", "articles", "", services.publishing),
            ("Comments", "comments", "", true),
            ("Likes", "likes", "Of posts and comments, by users here", true),
            ("Shares", "shares", "By users here", true),
            ("Open reports", "open_reports", "", true),
        ])?,
        group(&row, "Pictures", &[
            ("Images", "images", "Photos of posts and comments", photos),
            ("Posts with images", "image_posts", "", photos),
            ("Article pictures", "article_pictures", "Uploaded into articles", photos && services.publishing),
        ])?,
        group(&row, "Videos and live", &[
            ("Videos", "videos", "", services.video),
            ("Converting now", "converting", "", services.video),
            ("Hours of video", "video_hours", "", services.video),
            ("Live streams", "lives", "With their replays", services.live_streaming),
            ("Live now", "live_now", "", services.live_streaming),
        ])?,
        group(&row, "Podcasts", &[
            ("Podcasts", "podcasts", "", services.podcasts),
            ("Episodes", "episodes", "", services.podcasts),
        ])?,
        group(&row, "Events", &[
            ("Events", "events", "Each date of a recurring event counts", services.events),
            ("Upcoming", "upcoming_events", "", services.events),
        ])?,
        group(&row, "Communities", &[
            ("Communities", "communities", "", services.content_aggregator),
            ("Memberships", "members", "", services.content_aggregator),
        ])?,
        group(&row, "Friends and messenger", &[
            ("Friendships", "friendships", "", services.dfrn),
            ("Conversations", "conversations", "", services.dfrn),
            ("Messages sent", "messages", "By users here", services.dfrn),
        ])?,
    ];
    Ok(groups.into_iter().flatten().collect())
}

/// What this site met of the Fediverse: the servers, their accounts, and
/// what they sent.
pub async fn federation(pool: &PgPool) -> Result<Vec<StatGroup>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT
            (SELECT COUNT(DISTINCT host) FROM federation_actors WHERE host <> '') AS instances,
            (SELECT COUNT(*) FROM instance_policies WHERE policy = 'limited') AS limited,
            (SELECT COUNT(*) FROM instance_policies WHERE policy = 'banned') AS banned,
            (SELECT COUNT(*) FROM federation_actors) AS accounts,
            (SELECT COUNT(DISTINCT actor_id) FROM federation_followers) AS followers,
            (SELECT COUNT(DISTINCT actor_id) FROM federation_following WHERE accepted) AS followed,
            (SELECT COUNT(*) FROM posts WHERE remote_actor_id IS NOT NULL AND kind = 'post' AND title IS NULL) AS posts,
            (SELECT COUNT(*) FROM posts WHERE remote_actor_id IS NOT NULL AND kind = 'post' AND title IS NOT NULL) AS articles,
            (SELECT COUNT(*) FROM comments WHERE remote_actor_id IS NOT NULL) AS comments,
            (SELECT COUNT(*) FROM attachments a
               LEFT JOIN posts p ON p.id = a.post_id LEFT JOIN comments c ON c.id = a.comment_id
              WHERE a.media_type LIKE 'image/%' AND COALESCE(p.remote_actor_id, c.remote_actor_id) IS NOT NULL) AS images,
            (SELECT COUNT(*) FROM posts p WHERE p.remote_actor_id IS NOT NULL AND EXISTS (
                SELECT 1 FROM attachments a WHERE a.post_id = p.id AND a.media_type LIKE 'image/%')) AS image_posts,
            (SELECT COUNT(*) FROM posts WHERE remote_actor_id IS NOT NULL AND kind = 'video') AS videos,
            (SELECT COUNT(*) FROM posts p WHERE p.remote_actor_id IS NOT NULL AND p.kind = 'post' AND EXISTS (
                SELECT 1 FROM attachments a WHERE a.post_id = p.id AND a.media_type LIKE 'video/%')) AS video_posts,
            (SELECT COUNT(*) FROM posts WHERE remote_actor_id IS NOT NULL AND kind = 'episode') AS episodes,
            (SELECT COUNT(*) FROM posts WHERE remote_actor_id IS NOT NULL AND kind = 'event') AS events,
            (SELECT COUNT(*) FROM remote_reactions) AS reactions,
            (SELECT COUNT(*) FROM messages WHERE sender_actor_id IS NOT NULL) AS messages,
            (SELECT COUNT(*) FROM federation_deliveries) AS deliveries",
    )
    .fetch_one(pool)
    .await?;
    let services = crate::settings::services();
    let groups = [
        group(&row, "Servers and accounts", &[
            ("Servers", "instances", "Servers of the accounts known here", true),
            ("Limited", "limited", "", true),
            ("Banned", "banned", "", true),
            ("Accounts discovered", "accounts", "People, communities and groups of other servers", true),
            ("Following here", "followers", "Accounts following people here", true),
            ("Followed from here", "followed", "Accounts people here follow", true),
        ])?,
        group(&row, "What they sent", &[
            ("Posts", "posts", "Without the articles, videos, episodes and events", true),
            ("Articles", "articles", "From WriteFreely, Plume...", true),
            ("Comments", "comments", "", true),
            ("Images", "images", "Of posts and comments", true),
            ("Posts with images", "image_posts", "", true),
            ("Videos", "videos", "From PeerTube...", true),
            ("Posts with a video", "video_posts", "Like Mastodon's", true),
            ("Podcast episodes", "episodes", "From Funkwhale, Castopod...", services.podcasts),
            ("Events", "events", "From Mobilizon...", services.events),
            ("Likes and shares", "reactions", "Of posts and comments here", true),
            ("Messages", "messages", "", services.dfrn),
        ])?,
        group(&row, "Sending", &[("Waiting to be sent", "deliveries", "Activities queued for other servers", true)])?,
    ];
    Ok(groups.into_iter().flatten().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_grouped() {
        let label = |value| Stat { label: "", value, hint: "" }.value_label();
        assert_eq!(label(0), "0");
        assert_eq!(label(999), "999");
        assert_eq!(label(1000), "1\u{202F}000");
        assert_eq!(label(1234567), "1\u{202F}234\u{202F}567");
    }
}

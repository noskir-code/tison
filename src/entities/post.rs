use sqlx::{
    PgPool, Row,
    postgres::PgRow,
    types::chrono::{DateTime, Utc},
};

use crate::entities::{
    article_picture,
    author::{Author, remote_author_columns},
    media::{self, Attachment},
    preview::{self, LinkPreview},
    schema::{Column, Index, Table},
    user::user_columns,
};

pub const BODY_MAX_CHARS: usize = 10_000;
/// Articles, like on WriteFreely: a title and a longer Markdown text.
pub const TITLE_MAX_CHARS: usize = 200;
pub const ARTICLE_MAX_CHARS: usize = 100_000;
/// Characters of an article shown in lists.
const ARTICLE_EXCERPT_CHARS: usize = 280;
/// Posts shown at once on a page, newest first.
pub const PAGE_SIZE: i64 = 50;

/// Kept up to date automatically at startup, see `entities::schema`.
pub const TABLE: Table = Table {
    name: "posts",
    columns: &[
        Column::new("id", "BIGSERIAL").primary_key(),
        // Posts are deleted with their author: a local user, or for a post
        // received from another server, its author there.
        Column::new("user_id", "INTEGER")
            .nullable()
            .references("users(id) ON DELETE CASCADE"),
        Column::new("remote_actor_id", "BIGINT")
            .nullable()
            .references("federation_actors(id) ON DELETE CASCADE"),
        // Its ActivityPub id and page, for posts of other servers.
        Column::new("ap_id", "TEXT").nullable(),
        Column::new("ap_url", "TEXT").nullable(),
        // Posted in a community, deleted with it.
        Column::new("community_id", "BIGINT")
            .nullable()
            .references("communities(id) ON DELETE CASCADE"),
        // Set for an article (WriteFreely's Article), its body is Markdown
        // then, see `crate::markdown`. Videos, episodes and live streams
        // have one too.
        Column::new("title", "TEXT").nullable(),
        // What it is, see `Kind`. Articles are `post` with a title.
        Column::new("kind", "TEXT").default("'post'"),
        // Videos and episodes: `processing` (only their author sees them,
        // see `av::jobs`), `ready` or `failed`. Live streams: `live` or
        // `ended`. NULL for the others.
        Column::new("media_status", "TEXT").nullable(),
        // The uploaded file waiting for ffmpeg, outside `public`.
        Column::new("media_source", "TEXT").nullable(),
        Column::new("duration_seconds", "INTEGER").nullable(),
        // The picture of a video, from ffmpeg or its server.
        Column::new("poster_url", "TEXT").nullable(),
        // An episode of this podcast, deleted with it.
        Column::new("podcast_id", "BIGINT")
            .nullable()
            .references("podcasts(id) ON DELETE CASCADE"),
        Column::new("body", "TEXT"),
        Column::new("visibility", "TEXT").default("'public'"),
        // Its media are hidden until clicked, like on Mastodon.
        Column::new("sensitive", "BOOLEAN").default("FALSE"),
        // See `Labels`.
        Column::new("content_warning", "TEXT").default("''"),
        Column::new("ai_generated", "BOOLEAN").default("FALSE"),
        Column::new("sponsored", "BOOLEAN").default("FALSE"),
        // Views of a video, see `entities::views`.
        Column::new("view_count", "BIGINT").default("0"),
        // Hashtags given apart from the text, like PeerTube's tags: lowercase,
        // without `#`, separated by spaces. Those of the text are read from it.
        Column::new("tags", "TEXT").default("''"),
        // A video's category, PeerTube's numbers: see `video::CATEGORIES`.
        Column::new("category", "SMALLINT").nullable(),
        Column::new("created_at", "TIMESTAMPTZ").default("NOW()"),
        Column::new("updated_at", "TIMESTAMPTZ").default("NOW()"),
        // Last edit by its author, shown as "edited".
        Column::new("edited_at", "TIMESTAMPTZ").nullable(),
        // Its first link was looked at for a preview, see `crate::preview`.
        // Reset by an edit.
        Column::new("link_checked", "BOOLEAN").default("FALSE"),
        // Removed by the moderators: hidden from everyone, erased after
        // `settings::removed_content_days` unless an appeal brings it back.
        Column::new("removed_at", "TIMESTAMPTZ").nullable(),
    ],
    indexes: &[
        Index::new("posts_removed_at_idx", "removed_at"),
        // Home timeline, and the posts of one user.
        Index::new("posts_created_at_idx", "created_at DESC"),
        Index::new("posts_remote_actor_created_at_idx", "remote_actor_id, created_at DESC"),
        Index::new("posts_user_id_created_at_idx", "user_id, created_at DESC"),
        Index::new("posts_community_created_at_idx", "community_id, created_at DESC"),
        Index::unique("posts_ap_id_idx", "ap_id"),
        // `search`: words of posts and titles, fast with ILIKE.
        Index::trigram("posts_body_trgm_idx", "body"),
        Index::trigram("posts_title_trgm_idx", "title"),
        Index::trigram("posts_tags_trgm_idx", "tags"),
        Index::new("posts_category_idx", "category, created_at DESC"),
    ],
};

/// One row per user who liked a post.
pub const LIKES_TABLE: Table = Table {
    name: "post_likes",
    columns: &[
        Column::new("post_id", "BIGINT").references("posts(id) ON DELETE CASCADE"),
        Column::new("user_id", "INTEGER").references("users(id) ON DELETE CASCADE"),
        Column::new("created_at", "TIMESTAMPTZ").default("NOW()"),
    ],
    indexes: &[Index::unique("post_likes_post_user_idx", "post_id, user_id")],
};

/// One row per user who shared (reposted) a post on their profile.
pub const SHARES_TABLE: Table = Table {
    name: "post_shares",
    columns: &[
        Column::new("post_id", "BIGINT").references("posts(id) ON DELETE CASCADE"),
        Column::new("user_id", "INTEGER").references("users(id) ON DELETE CASCADE"),
        Column::new("created_at", "TIMESTAMPTZ").default("NOW()"),
    ],
    indexes: &[
        Index::unique("post_shares_post_user_idx", "post_id, user_id"),
        // Shares shown on a profile.
        Index::new("post_shares_user_created_at_idx", "user_id, created_at DESC"),
    ],
};

/// What a post is: written, or a video, podcast episode or live stream
/// (see `crate::av`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Post,
    /// Like PeerTube's.
    Video,
    /// Like Funkwhale's and Castopod's, in a podcast or from another server.
    Episode,
    /// Like Owncast's: live from OBS, then its replay (a video) if kept.
    Live,
    /// Like Mobilizon's, see `entities::event`.
    Event,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Post => "post",
            Self::Video => "video",
            Self::Episode => "episode",
            Self::Live => "live",
            Self::Event => "event",
        }
    }

    fn parse(value: &str) -> Self {
        [Self::Video, Self::Episode, Self::Live, Self::Event]
            .into_iter()
            .find(|kind| kind.as_str() == value)
            .unwrap_or(Self::Post)
    }
}

/// The podcast of an episode, as shown on it.
#[derive(Clone, Debug)]
pub struct PostPodcast {
    pub id: i64,
    pub title: String,
    pub cover_url: Option<String>,
}

/// Who can see a post. "Friends" and "Following" will come with relationships.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Public,
    /// Anyone with the link, the profile and the followers' feeds, but not
    /// the public lists (timeline, hashtags, search, videos): Mastodon's
    /// "unlisted", the default of bots (Server rules, 7).
    Unlisted,
    /// Only the author.
    Private,
    /// Only for posts of other servers: the local users following its author
    /// (Mastodon's "followers only").
    Followers,
}

impl Visibility {
    /// What local users can choose.
    pub const ALL: [Self; 3] = [Self::Public, Self::Unlisted, Self::Private];

    /// Stored in the database and sent by the form.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Unlisted => "unlisted",
            Self::Private => "private",
            Self::Followers => "followers",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Public => "Public",
            Self::Unlisted => "Unlisted: not in public lists",
            Self::Private => "Only me",
            Self::Followers => "Followers only",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        [Self::Public, Self::Unlisted, Self::Private, Self::Followers]
            .into_iter()
            .find(|visibility| visibility.as_str() == value)
    }
}

/// For the `datetime` attribute of `<time>`, read by `assets/system.js`.
pub fn time_iso(time: &DateTime<Utc>) -> String {
    time.to_rfc3339()
}

/// Shown when JavaScript is off, `system.js` replaces it with "x minutes ago".
pub fn time_label(time: &DateTime<Utc>) -> String {
    crate::i18n::date_time(*time)
}

/// The community a post is in, as shown on the post.
#[derive(Clone, Debug)]
pub struct PostCommunity {
    pub id: i64,
    pub public_id: i64,
    pub slug: String,
    pub name: String,
    pub is_public: bool,
}

/// Who shared a post shown on a profile or in the feed.
pub struct Sharer {
    /// `alice`, or `bob@misskey.io` for an account of another server.
    pub name: String,
    pub url: String,
}

/// A post with its author and reactions, as seen by one viewer.
pub struct Post {
    pub id: i64,
    pub kind: Kind,
    /// Articles, videos, episodes and live streams have one.
    pub title: Option<String>,
    /// See the `media_status` column.
    pub media_status: Option<String>,
    pub duration_seconds: Option<i32>,
    /// Views of a video, see `entities::views`.
    pub view_count: i64,
    /// The `tags` column, see `hashtags`.
    pub extra_tags: String,
    /// A video's category, see `video::CATEGORIES`.
    pub category: Option<i16>,
    pub poster_url: Option<String>,
    pub podcast: Option<PostPodcast>,
    /// The HLS of a stream going on, see `av::live`.
    pub live_playlist: Option<String>,
    /// When and where, for an event.
    pub event: Option<crate::entities::event::EventDetails>,
    pub body: String,
    pub visibility: Visibility,
    pub sensitive: bool,
    /// Hides the post behind it until clicked, like Mastodon's (`summary`).
    pub content_warning: String,
    /// Made or changed with AI, said by its author (Server rules, 7).
    pub ai_generated: bool,
    /// Paid for: an ad or a partnership (Server rules, 7).
    pub sponsored: bool,
    pub created_at: DateTime<Utc>,
    pub edited_at: Option<DateTime<Utc>>,
    pub author: Author,
    pub community: Option<PostCommunity>,
    /// ActivityPub id and page, for posts received from other servers.
    pub ap_id: Option<String>,
    pub ap_url: Option<String>,
    pub like_count: i64,
    pub comment_count: i64,
    pub share_count: i64,
    /// Whether the viewer liked / shared it, always `false` for visitors.
    pub liked: bool,
    pub shared: bool,
    /// Whether the viewer wrote it.
    pub is_own: bool,
    /// Set when the post is on a profile or in the feed because they shared it.
    pub shared_by: Option<Sharer>,
    /// Media of posts received from other servers.
    pub attachments: Vec<Attachment>,
    /// The qualities of a video made here, best first (see `av::LEVELS`).
    pub video_files: Vec<crate::entities::video::VideoFile>,
    /// The card of its first link, when it has no media.
    pub preview: Option<LinkPreview>,
    /// An article's: the cards of its links, see `article_html`.
    pub link_previews: Vec<LinkPreview>,
}

impl Post {
    pub fn labels(&self) -> Labels {
        Labels { content_warning: self.content_warning.clone(), ai_generated: self.ai_generated, sponsored: self.sponsored }
    }

    fn from_row(row: &PgRow) -> Result<Self, sqlx::Error> {
        let visibility: String = row.try_get("visibility")?;
        Ok(Self {
            id: row.try_get("post_id")?,
            kind: Kind::parse(&row.try_get::<String, _>("kind")?),
            title: row.try_get("title")?,
            media_status: row.try_get("media_status")?,
            duration_seconds: row.try_get("duration_seconds")?,
            view_count: row.try_get("view_count")?,
            extra_tags: row.try_get("tags")?,
            category: row.try_get("category")?,
            poster_url: row.try_get("poster_url")?,
            live_playlist: row.try_get("live_playlist")?,
            event: crate::entities::event::EventDetails::from_row(row)?,
            podcast: match row.try_get::<Option<i64>, _>("podcast_id")? {
                Some(id) => Some(PostPodcast {
                    id,
                    title: row.try_get::<Option<String>, _>("podcast_title")?.unwrap_or_default(),
                    cover_url: row.try_get("podcast_cover")?,
                }),
                None => None,
            },
            body: row.try_get("body")?,
            // An unknown value is treated as the most restrictive one.
            visibility: Visibility::parse(&visibility).unwrap_or(Visibility::Private),
            sensitive: row.try_get("sensitive")?,
            content_warning: row.try_get("content_warning")?,
            ai_generated: row.try_get("ai_generated")?,
            sponsored: row.try_get("sponsored")?,
            created_at: row.try_get("posted_at")?,
            edited_at: row.try_get("edited_at")?,
            author: Author::from_row(row)?,
            community: match row.try_get::<Option<i64>, _>("community_id")? {
                Some(id) => Some(PostCommunity {
                    id,
                    public_id: row.try_get("community_public_id")?,
                    slug: row.try_get("community_slug")?,
                    name: row.try_get("community_name")?,
                    is_public: row.try_get::<String, _>("community_visibility")? == "public",
                }),
                None => None,
            },
            ap_id: row.try_get("ap_id")?,
            ap_url: row.try_get("ap_url")?,
            like_count: row.try_get("like_count")?,
            comment_count: row.try_get("comment_count")?,
            share_count: row.try_get("share_count")?,
            liked: row.try_get("liked")?,
            shared: row.try_get("shared")?,
            is_own: row.try_get("is_own")?,
            shared_by: match (row.try_get::<Option<String>, _>("shared_by")?, row.try_get::<Option<String>, _>("shared_by_url")?) {
                (Some(name), Some(url)) => Some(Sharer { name, url }),
                _ => None,
            },
            attachments: Vec::new(),
            video_files: Vec::new(),
            preview: None,
            link_previews: Vec::new(),
        })
    }

    pub fn is_private(&self) -> bool {
        self.visibility == Visibility::Private
    }

    /// Only pictures: shown like on Pixelfed with `IMAGESHARING` and
    /// `IMAGESYSTEM` on in `configs/services.conf`, see `macros/post.html`.
    pub fn is_photo_post(&self) -> bool {
        let services = crate::settings::services();
        services.image_sharing
            && services.image_system
            && !self.attachments.is_empty()
            && self.attachments.iter().all(|attachment| attachment.is_image())
    }

    /// The shape of the photo frame, from the first photo: between 4:5
    /// (portrait) and 1.91:1 (landscape) like Pixelfed and Instagram, square
    /// when its size is unknown. A CSS `aspect-ratio`.
    pub fn photo_ratio(&self) -> String {
        let ratio = self
            .attachments
            .first()
            .and_then(|photo| Some(photo.width? as f64 / photo.height? as f64))
            .filter(|ratio| ratio.is_finite() && *ratio > 0.0)
            .unwrap_or(1.0)
            .clamp(0.8, 1.91);
        format!("{ratio:.4}")
    }

    /// Written as an article, here or on WriteFreely: its page shows it
    /// whole, lists show its title and start.
    pub fn is_article(&self) -> bool {
        self.kind == Kind::Post && self.title.is_some()
    }

    pub fn is_video(&self) -> bool {
        self.kind == Kind::Video
    }

    pub fn is_episode(&self) -> bool {
        self.kind == Kind::Episode
    }

    pub fn is_live(&self) -> bool {
        self.kind == Kind::Live
    }

    pub fn is_event(&self) -> bool {
        self.kind == Kind::Event && self.event.is_some()
    }

    /// Being live now.
    pub fn is_on_air(&self) -> bool {
        self.is_live() && self.media_status.as_deref() == Some("live")
    }

    /// Waiting for ffmpeg, or failed: only its author sees it.
    pub fn is_processing(&self) -> bool {
        self.media_status.as_deref() == Some("processing")
    }

    pub fn has_failed(&self) -> bool {
        self.media_status.as_deref() == Some("failed")
    }

    /// The file of a video or episode, through the media proxy when it's
    /// from another server.
    pub fn media_url(&self) -> Option<String> {
        self.attachments
            .iter()
            .find(|attachment| attachment.is_video() || attachment.is_audio())
            .map(Attachment::proxy_url)
    }

    /// The picture of a video, or the cover of an episode's podcast.
    pub fn poster(&self) -> Option<String> {
        let url = self
            .poster_url
            .clone()
            .or_else(|| self.podcast.as_ref().and_then(|podcast| podcast.cover_url.clone()))?;
        Some(if media::is_local(&url) { url } else { media::proxy_url(&url) })
    }

    /// "1 view", "1.2K views".
    pub fn views_label(&self) -> String {
        let count = self.view_count;
        let number = match count {
            ..1_000 => count.to_string(),
            1_000..1_000_000 => compact(count as f64 / 1_000.0, "K"),
            _ => compact(count as f64 / 1_000_000.0, "M"),
        };
        crate::i18n::tf(if count == 1 { "{} view" } else { "{} views" }, number)
    }

    pub fn duration_label(&self) -> Option<String> {
        self.duration_seconds.filter(|seconds| *seconds > 0).map(crate::av::duration_label)
    }

    pub fn title_text(&self) -> &str {
        self.title.as_deref().unwrap_or_default()
    }

    /// The article as HTML, pictures of other servers through the media
    /// proxy.
    pub fn article_html(&self) -> String {
        let html = crate::markdown::to_html(&self.body, &|address| match media::is_local(address) {
            true => address.to_string(),
            false => media::proxy_url(address),
        });
        // The cards of its links, small, under their paragraph.
        crate::markdown::with_link_cards(&html, &|link| {
            let found = self.link_previews.iter().find(|preview| preview.url == link)?;
            let escape = crate::linkify::escape;
            let image = match found.image_proxy_url() {
                Some(image) => format!("<img src=\"{}\" alt=\"\" loading=\"lazy\" class=\"link-card-image\">", escape(&image)),
                None => String::new(),
            };
            Some(format!(
                "<a href=\"{}\" class=\"link-card article-link-card{}\" rel=\"nofollow noopener noreferrer ugc\" target=\"_blank\">{image}\
                 <span class=\"link-card-text\"><span class=\"link-card-site\">{}</span><strong class=\"link-card-title\">{}</strong></span></a>\n",
                escape(&found.url),
                if image.is_empty() { " no-image" } else { "" },
                escape(&found.site()),
                escape(&found.title),
            ))
        })
    }

    /// The start of the article, as text, for lists.
    pub fn article_excerpt(&self) -> String {
        let text = crate::markdown::to_text(&self.body);
        match text.char_indices().nth(ARTICLE_EXCERPT_CHARS) {
            Some((end, _)) => format!("{}…", text[..end].trim_end()),
            None => text,
        }
    }

    /// The picture of its preview card in feeds, like Mastodon's: its first
    /// picture, else its first attached one, else the site's logo for the
    /// moderation team's (see `user::sync_team_account`).
    pub fn article_picture(&self) -> Option<String> {
        match crate::markdown::first_picture(&self.body) {
            Some(address) if media::is_local(&address) => Some(address),
            Some(address) => Some(media::proxy_url(&address)),
            None => self
                .attachments
                .iter()
                .find(|attachment| attachment.is_image())
                .map(Attachment::proxy_url)
                .or_else(|| self.author.is_team().then(|| SITE_LOGO.to_string())),
        }
    }

    /// `article_picture` as a full address, for other sites (Open Graph,
    /// the Fediverse).
    pub fn article_picture_url(&self) -> Option<String> {
        self.article_picture().map(|picture| match picture.starts_with('/') {
            true => format!("{}{picture}", crate::settings::site_base_url()),
            false => picture,
        })
    }

    /// Where it was published, on its preview card: this site, or the
    /// server of a remote author (a WriteFreely blog...).
    pub fn article_site(&self) -> String {
        match &self.ap_url {
            Some(url) => crate::entities::author::host_of(url).trim_start_matches("www.").to_string(),
            None => crate::settings::instance_name().to_string(),
        }
    }

    pub fn reading_minutes(&self) -> usize {
        crate::markdown::reading_minutes(&self.body)
    }

    /// The text as HTML, with clickable addresses, see `linkify`.
    pub fn body_html(&self) -> String {
        crate::linkify::to_html(&self.body)
    }

    pub fn is_followers_only(&self) -> bool {
        self.visibility == Visibility::Followers
    }

    /// Its hashtags, lowercase without `#`: of its title and text, then those
    /// given apart (PeerTube's).
    pub fn hashtags(&self) -> Vec<String> {
        let mut tags = crate::hashtag::find(&[self.title_text(), &self.body]);
        for tag in self.extra_tags.split_whitespace() {
            if !tags.iter().any(|known| known == tag) {
                tags.push(tag.to_string());
            }
        }
        tags
    }

    /// The tags given apart from the text, linked under it.
    pub fn extra_tag_list(&self) -> Vec<&str> {
        self.extra_tags.split_whitespace().collect()
    }

    /// A video's category, in English (translated by the templates).
    pub fn category_name(&self) -> Option<&'static str> {
        self.category.and_then(crate::entities::video::category_name)
    }

    /// Written here by the viewer: they can edit it.
    pub fn can_edit(&self) -> bool {
        self.is_own && self.author.local().is_some()
    }

    pub fn edited_at_iso(&self) -> Option<String> {
        self.edited_at.as_ref().map(time_iso)
    }

    /// Only someone else's public posts can be shared, not those of a
    /// private community.
    pub fn can_share(&self) -> bool {
        !self.is_own
            && matches!(self.visibility, Visibility::Public | Visibility::Unlisted)
            && self.community.as_ref().is_none_or(|community| community.is_public)
    }

    pub fn created_at_iso(&self) -> String {
        time_iso(&self.created_at)
    }

    pub fn created_at_label(&self) -> String {
        time_label(&self.created_at)
    }
}

/// Likes of the post `p`, here and on other servers.
macro_rules! like_count {
    () => {
        "((SELECT COUNT(*) FROM post_likes l WHERE l.post_id = p.id)
          + (SELECT COUNT(*) FROM remote_reactions rl WHERE rl.post_id = p.id AND rl.kind = 'like'))"
    };
}

/// The site's logo, the picture of the team's articles without one.
pub const SITE_LOGO: &str = "/public/logo.png";

/// Columns read by `Post::from_row`. `$1` is always the viewer id (NULL for
/// visitors), `$shared_by` and `$shared_by_url` the SQL giving who shared it
/// and their page. Queries using it must only show what the viewer can see,
/// with `post_visible!()`.
macro_rules! post_select {
    ($shared_by:literal, $shared_by_url:literal) => {
        concat!(
            "SELECT p.id AS post_id, p.kind, p.title, p.media_status, p.duration_seconds, p.poster_url, p.view_count,
                    p.tags, p.category,
                    p.podcast_id, pod.title AS podcast_title, pod.cover_url AS podcast_cover,
                    CASE WHEN p.kind = 'live' AND p.media_status = 'live' THEN p.media_source END AS live_playlist, ",
            crate::entities::event::event_columns!(),
            ",
                    p.body, p.visibility, p.sensitive, p.content_warning, p.ai_generated, p.sponsored, p.created_at AS posted_at,
                    p.edited_at, p.ap_id, p.ap_url,
                    COALESCE(p.user_id = $1 OR (u.team AND EXISTS (
                        SELECT 1 FROM users staff WHERE staff.id = $1 AND staff.role IN ('moderator', 'admin'))), FALSE) AS is_own, ",
            like_count!(),
            " AS like_count,
                    (SELECT COUNT(*) FROM comments pc WHERE pc.post_id = p.id AND pc.removed_at IS NULL) AS comment_count,
                    (SELECT COUNT(*) FROM post_shares s WHERE s.post_id = p.id)
                    + (SELECT COUNT(*) FROM remote_reactions rs WHERE rs.post_id = p.id AND rs.kind = 'share') AS share_count,
                    EXISTS (SELECT 1 FROM post_likes l WHERE l.post_id = p.id AND l.user_id = $1) AS liked,
                    EXISTS (SELECT 1 FROM post_shares s WHERE s.post_id = p.id AND s.user_id = $1) AS shared,
                    p.community_id, cm.public_id AS community_public_id, cm.slug AS community_slug,
                    cm.name AS community_name, cm.visibility AS community_visibility, ",
            $shared_by,
            " AS shared_by, ",
            $shared_by_url,
            " AS shared_by_url, ",
            user_columns!("u."),
            ", ",
            remote_author_columns!(),
            " FROM posts p
              LEFT JOIN users u ON u.id = p.user_id
              LEFT JOIN federation_actors ra ON ra.id = p.remote_actor_id
              LEFT JOIN communities cm ON cm.id = p.community_id
              LEFT JOIN podcasts pod ON pod.id = p.podcast_id
              LEFT JOIN events ev ON ev.post_id = p.id"
        )
    };
}

/// Whether the viewer `$1` can see the post `p` (its community joined as
/// `cm`): their own posts, public posts outside communities, posts of
/// communities that aren't banned and are public or have the viewer as
/// member, and followers-only posts of other servers when the viewer follows
/// their author. Never posts of suspended users, nor posts of a limited
/// server to someone not following their author, nor posts between people
/// blocking each other, nor posts of a limited account to someone not
/// following them.
macro_rules! post_visible {
    () => {
        concat!(
        "(p.removed_at IS NULL
          AND (NOT EXISTS (SELECT 1 FROM users su WHERE su.id = p.user_id AND su.suspended_at IS NOT NULL))
          -- Blocks, either way (Server rules, 2), see `entities::block`.
          AND NOT EXISTS (SELECT 1 FROM user_blocks pb WHERE
                (pb.user_id = $1 AND (pb.blocked_user_id = p.user_id OR pb.blocked_actor_id = p.remote_actor_id))
                OR (pb.user_id = p.user_id AND pb.blocked_user_id = $1))
          -- A limited account: only their followers, friends and the staff.
          AND NOT EXISTS (SELECT 1 FROM users lu WHERE lu.id = p.user_id AND lu.limited_at IS NOT NULL
                AND COALESCE(lu.id <> $1, TRUE)
                AND NOT EXISTS (SELECT 1 FROM user_follows lfw WHERE lfw.followed_id = lu.id AND lfw.follower_id = $1)
                AND NOT EXISTS (SELECT 1 FROM friendships lfr WHERE lfr.accepted
                    AND ((lfr.requester_id = lu.id AND lfr.addressee_id = $1) OR (lfr.addressee_id = lu.id AND lfr.requester_id = $1)))
                AND NOT EXISTS (SELECT 1 FROM users lst WHERE lst.id = $1 AND lst.role IN ('moderator', 'admin')))
          AND (p.media_status IS NULL OR p.media_status IN ('ready', 'live', 'ended') OR COALESCE(p.user_id = $1, FALSE))
          AND (p.community_id IS NULL OR ", crate::entities::service_on!(aggregator), ")
          AND (NOT EXISTS (SELECT 1 FROM federation_actors la JOIN instance_policies lp ON lp.domain = la.host
                WHERE la.id = p.remote_actor_id AND lp.policy = 'limited'
                  AND NOT EXISTS (SELECT 1 FROM federation_following lf
                                  WHERE lf.actor_id = la.id AND lf.user_id = $1 AND lf.accepted)))
          AND (COALESCE(p.user_id = $1, FALSE) OR (p.visibility IN ('public', 'unlisted') AND (p.community_id IS NULL OR (
            cm.status <> 'banned' AND (cm.visibility = 'public' OR EXISTS (
                SELECT 1 FROM community_members mm WHERE mm.community_id = cm.id AND mm.user_id = $1)))))
          OR (p.visibility = 'followers' AND p.remote_actor_id IS NOT NULL AND EXISTS (
                SELECT 1 FROM federation_following ff
                WHERE ff.actor_id = p.remote_actor_id AND ff.user_id = $1 AND ff.accepted))))")
    };
}
pub(crate) use post_visible;

/// The posts `p` shown in public lists (timeline, hashtags, search, videos,
/// events): not unlisted ones, unless the viewer `$1` wrote them or follows
/// their author. With `post_visible!`.
macro_rules! post_listed {
    () => {
        "(p.visibility <> 'unlisted' OR COALESCE(p.user_id = $1, FALSE)
          OR EXISTS (SELECT 1 FROM user_follows uf WHERE uf.followed_id = p.user_id AND uf.follower_id = $1)
          OR EXISTS (SELECT 1 FROM federation_following ul WHERE ul.actor_id = p.remote_actor_id AND ul.user_id = $1 AND ul.accepted))"
    };
}

/// Trims the text and checks it's not empty, at most `max_chars`, and has no
/// control characters other than line breaks and tabs.
pub fn validate_text(raw: &str, max_chars: usize) -> Result<String, BodyError> {
    let text = raw.trim();
    if text.is_empty() {
        return Err(BodyError::Empty);
    }
    if text.chars().count() > max_chars {
        return Err(BodyError::TooLong);
    }
    if text
        .chars()
        .any(|letter| letter.is_control() && !matches!(letter, '\n' | '\r' | '\t'))
    {
        return Err(BodyError::InvalidCharacters);
    }
    // Browsers send line breaks as \r\n.
    Ok(text.replace("\r\n", "\n"))
}

pub fn validate_body(raw: &str) -> Result<String, BodyError> {
    validate_text(raw, BODY_MAX_CHARS)
}

#[derive(Clone, Copy, Debug)]
pub enum BodyError {
    Empty,
    TooLong,
    InvalidCharacters,
}

/// Longest content warning.
pub const WARNING_MAX_CHARS: usize = 200;

/// What the author says about a post (Server rules, 4 and 7): a content
/// warning hiding it, made with AI, sponsored.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Labels {
    pub content_warning: String,
    pub ai_generated: bool,
    pub sponsored: bool,
}

/// Hashtags other servers read as the labels, and which they send.
pub const AI_TAGS: [&str; 6] = ["aigenerated", "aiart", "madewithai", "generativeai", "aiimage", "aivideo"];
pub const SPONSORED_TAGS: [&str; 4] = ["ad", "sponsored", "advertisement", "paidpartnership"];

impl Labels {
    /// From the fields of a form: `content_warning`, `ai_generated`,
    /// `sponsored`. The warning is cut at `WARNING_MAX_CHARS`, on one line.
    pub fn from_form(field: impl Fn(&str) -> Option<String>) -> Self {
        let checked = |name: &str| field(name).is_some_and(|value| !value.is_empty());
        Self {
            content_warning: clean_warning(&field("content_warning").unwrap_or_default()),
            ai_generated: checked("ai_generated"),
            sponsored: checked("sponsored"),
        }
    }

    /// Of an object of another server: its `summary` (a Note's, not an
    /// Article's excerpt) and its hashtags.
    pub fn from_remote(summary: Option<&str>, tags: &[String]) -> Self {
        let has = |list: &[&str]| tags.iter().any(|tag| list.contains(&tag.to_lowercase().as_str()));
        Self {
            content_warning: clean_warning(summary.unwrap_or_default()),
            ai_generated: has(&AI_TAGS),
            sponsored: has(&SPONSORED_TAGS),
        }
    }
}

fn clean_warning(raw: &str) -> String {
    let one_line: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let clean: String = one_line.chars().filter(|letter| !letter.is_control()).collect();
    clean.chars().take(WARNING_MAX_CHARS).collect()
}

/// A moderator hides the post behind `warning` and blurs its media (or no
/// more, after an appeal: `None`).
pub async fn set_moderated_warning(pool: &PgPool, post_id: i64, warning: Option<&str>) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE posts SET content_warning = $2, sensitive = $3 WHERE id = $1")
        .bind(post_id)
        .bind(warning.map(clean_warning).unwrap_or_default())
        .bind(warning.is_some())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_labels(pool: &PgPool, post_id: i64, labels: &Labels) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE posts SET content_warning = $2, ai_generated = $3, sponsored = $4 WHERE id = $1")
        .bind(post_id)
        .bind(&labels.content_warning)
        .bind(labels.ai_generated)
        .bind(labels.sponsored)
        .execute(pool)
        .await?;
    Ok(())
}

/// `community_id` posts it in a community: the author must be a member.
pub async fn create(
    pool: &PgPool,
    user_id: i32,
    body: &str,
    visibility: Visibility,
    community_id: Option<i64>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO posts (user_id, body, visibility, community_id) VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(user_id)
    .bind(body)
    .bind(visibility.as_str())
    .bind(community_id)
    .fetch_one(pool)
    .await
}

/// An article, see `crate::markdown`.
pub async fn create_article(pool: &PgPool, user_id: i32, title: &str, body: &str, visibility: Visibility) -> Result<i64, sqlx::Error> {
    media::register_article_pictures(pool, body).await?;
    sqlx::query_scalar("INSERT INTO posts (user_id, title, body, visibility) VALUES ($1, $2, $3, $4) RETURNING id")
        .bind(user_id)
        .bind(title)
        .bind(body)
        .bind(visibility.as_str())
        .fetch_one(pool)
        .await
}

/// How many articles the profile lists.
pub const ARTICLES_SHOWN: i64 = 100;

/// Articles of a local user, newest first: their blog, like on WriteFreely.
pub async fn articles_by_author(pool: &PgPool, author_id: i32, viewer_id: Option<i32>) -> Result<Vec<Post>, sqlx::Error> {
    let rows = sqlx::query(concat!(
        post_select!("NULL::text", "NULL::text"),
        // Written articles: not videos, episodes, live streams or events,
        // which have a title too.
        " WHERE p.user_id = $2 AND p.kind = 'post' AND p.title IS NOT NULL AND ",
        post_visible!(),
        " ORDER BY p.created_at DESC, p.id DESC
         LIMIT $3"
    ))
    .bind(viewer_id)
    .bind(author_id)
    .bind(ARTICLES_SHOWN)
    .fetch_all(pool)
    .await?;
    to_posts(pool, rows).await
}

/// A post with photos, see `photo`: saved with them, or not at all.
pub async fn create_with_photos(
    pool: &PgPool,
    user_id: i32,
    body: &str,
    visibility: Visibility,
    sensitive: bool,
    photos: &[media::NewAttachment],
) -> Result<i64, sqlx::Error> {
    let post_id: i64 = sqlx::query_scalar(
        "INSERT INTO posts (user_id, body, visibility, sensitive) VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(user_id)
    .bind(body)
    .bind(visibility.as_str())
    .bind(sensitive)
    .fetch_one(pool)
    .await?;
    if let Err(err) = media::save_attachments(pool, media::Owner::Post(post_id), photos).await {
        sqlx::query("DELETE FROM posts WHERE id = $1").bind(post_id).execute(pool).await?;
        return Err(err);
    }
    Ok(post_id)
}

/// How many photo posts the profile grid shows.
pub const PHOTOS_SHOWN: i64 = 60;

/// Posts of a local user with pictures, newest first: their profile grid,
/// like on Pixelfed.
pub async fn photos_by_author(pool: &PgPool, author_id: i32, viewer_id: Option<i32>) -> Result<Vec<Post>, sqlx::Error> {
    let rows = sqlx::query(concat!(
        post_select!("NULL::text", "NULL::text"),
        " WHERE p.user_id = $2
           AND EXISTS (SELECT 1 FROM attachments pa WHERE pa.post_id = p.id AND pa.media_type LIKE 'image/%')
           AND ",
        post_visible!(),
        " ORDER BY p.created_at DESC, p.id DESC
         LIMIT $3"
    ))
    .bind(viewer_id)
    .bind(author_id)
    .bind(PHOTOS_SHOWN)
    .fetch_all(pool)
    .await?;
    to_posts(pool, rows).await
}

/// What a local user made that the viewer can see: the profile shows a tab
/// for each (see `photos_by_author`, `articles_by_author` and
/// `MediaList::VideosBy`, which list them with the same rules).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ContentKinds {
    pub photos: bool,
    pub articles: bool,
    pub videos: bool,
}

pub async fn content_kinds(pool: &PgPool, author_id: i32, viewer_id: Option<i32>) -> Result<ContentKinds, sqlx::Error> {
    let row = sqlx::query(concat!(
        "SELECT COALESCE(BOOL_OR(EXISTS (SELECT 1 FROM attachments pa WHERE pa.post_id = p.id AND pa.media_type LIKE 'image/%')), FALSE) AS photos,
                COALESCE(BOOL_OR(p.kind = 'post' AND p.title IS NOT NULL), FALSE) AS articles,
                COALESCE(BOOL_OR(p.kind = 'video'), FALSE) AS videos
         FROM posts p
         LEFT JOIN communities cm ON cm.id = p.community_id
         WHERE p.user_id = $2 AND ",
        post_visible!()
    ))
    .bind(viewer_id)
    .bind(author_id)
    .fetch_one(pool)
    .await?;
    Ok(ContentKinds {
        photos: row.try_get("photos")?,
        articles: row.try_get("articles")?,
        videos: row.try_get("videos")?,
    })
}

/// Lists of the Videos and Podcasts pages and the profile's Videos tab.
#[derive(Clone, Copy)]
pub enum MediaList {
    /// The newest videos, ready to watch.
    Videos,
    /// Streams going on.
    LiveNow,
    /// A user's videos, those still converting for themselves.
    VideosBy(i32),
    /// A podcast's episodes.
    Episodes(i64),
    /// The newest episodes, here and from other servers.
    LatestEpisodes,
    /// A user's videos and episodes still converting: only for themselves.
    ConvertingBy(i32),
    /// The newest videos of a category, see `video::CATEGORIES`.
    VideosIn(i16),
}

/// How many posts a media list shows.
pub const MEDIA_SHOWN: i64 = 60;

/// One of the `MediaList`s, newest first. `$2` is the user or podcast id.
macro_rules! media_query {
    ($filter:literal) => {
        concat!(
            post_select!("NULL::text", "NULL::text"),
            " WHERE ",
            $filter,
            " AND ",
            post_visible!(),
            " ORDER BY p.created_at DESC, p.id DESC LIMIT $3"
        )
    };
}

/// `media_query!` for the public lists: without unlisted posts, see `post_listed!`.
macro_rules! listed_media_query {
    ($filter:literal) => {
        concat!(
            post_select!("NULL::text", "NULL::text"),
            " WHERE ",
            $filter,
            " AND ",
            post_visible!(),
            " AND ",
            post_listed!(),
            " ORDER BY p.created_at DESC, p.id DESC LIMIT $3"
        )
    };
}

pub async fn media_list(pool: &PgPool, viewer_id: Option<i32>, list: MediaList) -> Result<Vec<Post>, sqlx::Error> {
    let (query, id) = match list {
        MediaList::Videos => (listed_media_query!("p.kind = 'video' AND p.media_status = 'ready' AND $2::bigint IS NULL"), None),
        MediaList::LiveNow => (listed_media_query!("p.kind = 'live' AND p.media_status = 'live' AND $2::bigint IS NULL"), None),
        MediaList::VideosIn(category) => (
            listed_media_query!("p.kind = 'video' AND p.media_status = 'ready' AND p.category = $2::bigint"),
            Some(i64::from(category)),
        ),
        MediaList::VideosBy(user_id) => (media_query!("p.kind = 'video' AND p.user_id = $2"), Some(i64::from(user_id))),
        MediaList::Episodes(podcast_id) => (media_query!("p.kind = 'episode' AND p.podcast_id = $2"), Some(podcast_id)),
        MediaList::LatestEpisodes => (listed_media_query!("p.kind = 'episode' AND p.media_status = 'ready' AND $2::bigint IS NULL"), None),
        MediaList::ConvertingBy(user_id) => (
            media_query!("p.kind IN ('video', 'episode') AND p.media_status = 'processing' AND p.user_id = $2 AND p.user_id = $1"),
            Some(i64::from(user_id)),
        ),
    };
    let rows = sqlx::query(query).bind(viewer_id).bind(id).bind(MEDIA_SHOWN).fetch_all(pool).await?;
    to_posts(pool, rows).await
}

/// Lists of the Events page.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EventList {
    /// Not over yet, the soonest first.
    Upcoming,
    /// Over, the latest first.
    Past,
    /// The viewer's: organized, going or asked, not over.
    Mine(i32),
    /// A community's, not over: the events it organizes.
    Community(i64),
    /// The dates of a recurring event, not over.
    Series(i64),
}

/// How many events a list shows.
pub const EVENTS_SHOWN: i64 = 100;

/// Not over: not ended, or started less than 4 hours ago without an end.
macro_rules! event_not_over {
    () => {
        "COALESCE(ev.ends_at, ev.starts_at + INTERVAL '4 hours') >= NOW()"
    };
}

pub async fn events(pool: &PgPool, viewer_id: Option<i32>, list: EventList) -> Result<Vec<Post>, sqlx::Error> {
    let (query, user_id) = match list {
        EventList::Upcoming => (
            concat!(
                post_select!("NULL::text", "NULL::text"),
                " WHERE p.kind = 'event' AND $2::bigint IS NULL AND ", event_not_over!(), " AND ", post_visible!(), " AND ", post_listed!(),
                " ORDER BY ev.starts_at, p.id LIMIT $3"
            ),
            None,
        ),
        EventList::Past => (
            concat!(
                post_select!("NULL::text", "NULL::text"),
                " WHERE p.kind = 'event' AND $2::bigint IS NULL AND NOT ", event_not_over!(), " AND ", post_visible!(), " AND ", post_listed!(),
                " ORDER BY ev.starts_at DESC, p.id DESC LIMIT $3"
            ),
            None,
        ),
        EventList::Mine(user_id) => (
            concat!(
                post_select!("NULL::text", "NULL::text"),
                " WHERE p.kind = 'event' AND ", event_not_over!(),
                " AND (p.user_id = $2 OR EXISTS (SELECT 1 FROM event_participants mp
                       WHERE mp.post_id = p.id AND mp.user_id = $2 AND mp.role IN ('going', 'pending')))
                  AND ", post_visible!(),
                " ORDER BY ev.starts_at, p.id LIMIT $3"
            ),
            Some(i64::from(user_id)),
        ),
        EventList::Community(community_id) => (
            concat!(
                post_select!("NULL::text", "NULL::text"),
                " WHERE p.kind = 'event' AND p.community_id = $2 AND ", event_not_over!(), " AND ", post_visible!(),
                " ORDER BY ev.starts_at, p.id LIMIT $3"
            ),
            Some(community_id),
        ),
        EventList::Series(series_id) => (
            concat!(
                post_select!("NULL::text", "NULL::text"),
                " WHERE p.kind = 'event' AND ev.series_id = $2 AND ", event_not_over!(), " AND ", post_visible!(),
                " ORDER BY ev.starts_at, p.id LIMIT $3"
            ),
            Some(series_id),
        ),
    };
    let rows = sqlx::query(query).bind(viewer_id).bind(user_id).bind(EVENTS_SHOWN).fetch_all(pool).await?;
    to_posts(pool, rows).await
}

/// A new event written here, with its details (see `entities::event`).
pub async fn create_event(
    pool: &PgPool,
    user_id: i32,
    title: &str,
    body: &str,
    banner_url: Option<&str>,
    community_id: Option<i64>,
    event: &crate::entities::event::NewEvent<'_>,
) -> Result<i64, sqlx::Error> {
    let post_id: i64 = sqlx::query_scalar(
        "INSERT INTO posts (user_id, kind, title, body, visibility, poster_url, community_id)
         VALUES ($1, 'event', $2, $3, 'public', $4, $5) RETURNING id",
    )
    .bind(user_id)
    .bind(title)
    .bind(body)
    .bind(banner_url)
    .bind(community_id)
    .fetch_one(pool)
    .await?;
    if let Err(err) = crate::entities::event::save(pool, post_id, event).await {
        sqlx::query("DELETE FROM posts WHERE id = $1").bind(post_id).execute(pool).await?;
        return Err(err);
    }
    Ok(post_id)
}

/// The organizer's changes of an event, and its banner (`None` keeps it).
pub async fn update_event(
    pool: &PgPool,
    post_id: i64,
    user_id: i32,
    title: &str,
    body: &str,
    banner_url: Option<&str>,
    event: &crate::entities::event::NewEvent<'_>,
) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(
        "UPDATE posts SET title = $3, body = $4, poster_url = COALESCE($5, poster_url), edited_at = NOW(), updated_at = NOW(),
                link_checked = FALSE
         WHERE id = $1 AND user_id = $2 AND kind = 'event'",
    )
    .bind(post_id)
    .bind(user_id)
    .bind(title)
    .bind(body)
    .bind(banner_url)
    .execute(pool)
    .await?
    .rows_affected()
        > 0;
    if updated {
        crate::entities::event::save(pool, post_id, event).await?;
    }
    Ok(updated)
}

/// A video or episode uploaded here, waiting for ffmpeg (see `av::jobs`).
pub struct NewMedia<'a> {
    pub kind: Kind,
    pub title: &'a str,
    pub body: &'a str,
    pub visibility: Visibility,
    pub sensitive: bool,
    pub podcast_id: Option<i64>,
    /// The uploaded file, outside `public`.
    pub source: &'a str,
    /// A thumbnail chosen by the author, else ffmpeg makes one.
    pub poster_url: Option<&'a str>,
    /// A video's category, see `video::CATEGORIES`.
    pub category: Option<i16>,
}

pub async fn create_media(pool: &PgPool, user_id: i32, media: &NewMedia<'_>) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "INSERT INTO posts (user_id, kind, title, body, visibility, sensitive, podcast_id, media_status, media_source, poster_url, category)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 'processing', $8, $9, $10) RETURNING id",
    )
    .bind(user_id)
    .bind(media.kind.as_str())
    .bind(media.title)
    .bind(media.body)
    .bind(media.visibility.as_str())
    .bind(media.sensitive)
    .bind(media.podcast_id)
    .bind(media.source)
    .bind(media.poster_url)
    .bind(media.category)
    .fetch_one(pool)
    .await
}

/// A new thumbnail for the author's video; the old one is returned to be
/// deleted. `None` when it isn't their video.
pub async fn set_poster(pool: &PgPool, post_id: i64, user_id: i32, poster_url: &str) -> Result<Option<Option<String>>, sqlx::Error> {
    sqlx::query_scalar(
        "UPDATE posts p SET poster_url = $3, updated_at = NOW()
         FROM (SELECT poster_url FROM posts WHERE id = $1) old
         WHERE p.id = $1 AND p.user_id = $2 AND p.kind = 'video'
         RETURNING old.poster_url",
    )
    .bind(post_id)
    .bind(user_id)
    .bind(poster_url)
    .fetch_optional(pool)
    .await
}

/// A post of another server: what it says, read from its Note.
pub struct RemotePost<'a> {
    pub ap_id: &'a str,
    pub ap_url: Option<&'a str>,
    /// Articles, videos and episodes.
    pub title: Option<&'a str>,
    pub body: &'a str,
    /// A video (PeerTube) or episode (Funkwhale), with its length and picture.
    pub kind: Kind,
    pub duration_seconds: Option<i32>,
    pub poster_url: Option<&'a str>,
    pub visibility: Visibility,
    pub sensitive: bool,
    pub published: Option<DateTime<Utc>>,
    /// Its hashtags not in its text, see the `tags` column.
    pub tags: String,
    /// A PeerTube video's category.
    pub category: Option<i16>,
}

/// A post received from another server, in a community or from an account
/// followed here. Returns `None` when a post with this ActivityPub id is
/// already stored.
pub async fn create_remote(
    pool: &PgPool,
    remote_actor_id: i64,
    community_id: Option<i64>,
    post: &RemotePost<'_>,
) -> Result<Option<i64>, sqlx::Error> {
    // An article: Markdown made from its HTML.
    if post.title.is_some() && post.kind == Kind::Post {
        media::register_article_pictures(pool, post.body).await?;
    }
    sqlx::query_scalar(
        "INSERT INTO posts (remote_actor_id, community_id, ap_id, ap_url, body, created_at, visibility, sensitive, title,
                            kind, duration_seconds, poster_url, media_status, tags, category)
         VALUES ($1, $2, $3, $4, $5, COALESCE($6, NOW()), $7, $8, $9, $10, $11, $12,
                 CASE WHEN $10 IN ('post', 'event') THEN NULL ELSE 'ready' END, $13, $14)
         ON CONFLICT (ap_id) DO NOTHING
         RETURNING id",
    )
    .bind(remote_actor_id)
    .bind(community_id)
    .bind(post.ap_id)
    .bind(post.ap_url)
    .bind(post.body)
    .bind(post.published)
    .bind(post.visibility.as_str())
    .bind(post.sensitive)
    .bind(post.title)
    .bind(post.kind.as_str())
    .bind(post.duration_seconds)
    .bind(post.poster_url)
    .bind(&post.tags)
    .bind(post.category)
    .fetch_optional(pool)
    .await
}

/// Changes the text of the user's own post, and the title of an article
/// (`None` keeps it a post). Returns `false` when it isn't theirs.
pub async fn update_own(pool: &PgPool, post_id: i64, user_id: i32, title: Option<&str>, body: &str) -> Result<bool, sqlx::Error> {
    // Articles, and the descriptions of videos: their pictures are only shown in articles.
    if title.is_some() {
        media::register_article_pictures(pool, body).await?;
    }
    let old_body: Option<String> = sqlx::query_scalar(
        "UPDATE posts p SET body = $3, title = $4, edited_at = NOW(), updated_at = NOW(), link_checked = FALSE
         FROM posts old WHERE p.id = $1 AND p.user_id = $2 AND old.id = p.id
         RETURNING old.body",
    )
    .bind(post_id)
    .bind(user_id)
    .bind(body)
    .bind(title)
    .fetch_optional(pool)
    .await?;
    let Some(old_body) = old_body else {
        return Ok(false);
    };
    // Pictures uploaded into the article and taken out of it.
    let kept = crate::markdown::pictures(body);
    let removed: Vec<String> = crate::markdown::pictures(&old_body).into_iter().filter(|url| !kept.contains(url)).collect();
    article_picture::remove_unused(pool, &removed).await?;
    Ok(true)
}

/// Deletes a post with its comments and reactions.
pub async fn delete(pool: &PgPool, post_id: i64) -> Result<bool, sqlx::Error> {
    let mut photos = media::local_files(pool, post_id).await?;
    // Every quality of a video (its attachment is one of them).
    for url in crate::entities::video::file_urls(pool, post_id).await? {
        if !photos.contains(&url) {
            photos.push(url);
        }
    }
    // A video's picture, and an upload still waiting for ffmpeg.
    // Not a picture other posts still show: the dates of a recurring event
    // share their banner.
    let (poster, source): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT CASE WHEN EXISTS (SELECT 1 FROM posts o WHERE o.poster_url = p.poster_url AND o.id <> p.id)
                     THEN NULL ELSE p.poster_url END,
                p.media_source
         FROM posts p WHERE p.id = $1",
    )
            .bind(post_id)
            .fetch_optional(pool)
            .await?
            .unwrap_or_default();
    photos.extend(poster.filter(|url| media::is_local(url)));
    if let Some(source) = source {
        let _ = tokio::fs::remove_file(source).await;
    }
    let body: Option<String> = sqlx::query_scalar("DELETE FROM posts WHERE id = $1 RETURNING body")
        .bind(post_id)
        .fetch_optional(pool)
        .await?;
    let Some(body) = body else {
        return Ok(false);
    };
    media::remove_local_files(&photos).await;
    // The pictures uploaded into the article.
    article_picture::remove_unused(pool, &crate::markdown::pictures(&body)).await?;
    Ok(true)
}

/// Removes a post for the moderators: hidden from everyone at once, its
/// files kept, erased by `purge_removed` after
/// `settings::removed_content_days` unless `restore` brings it back first.
pub async fn remove(pool: &PgPool, post_id: i64) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("UPDATE posts SET removed_at = NOW() WHERE id = $1 AND removed_at IS NULL")
        .bind(post_id)
        .execute(pool)
        .await?
        .rows_affected()
        > 0)
}

/// Brings back a post `remove`d by the moderators (an accepted appeal).
/// `false` when it isn't removed anymore: erased, or never removed.
pub async fn restore(pool: &PgPool, post_id: i64) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("UPDATE posts SET removed_at = NULL WHERE id = $1 AND removed_at IS NOT NULL")
        .bind(post_id)
        .execute(pool)
        .await?
        .rows_affected()
        > 0)
}

/// The posts removed by the moderators more than `days` ago: to erase.
pub async fn removed_before(pool: &PgPool, days: u32) -> Result<Vec<i64>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM posts WHERE removed_at < NOW() - make_interval(days => $1) ORDER BY id LIMIT 500")
        .bind(i32::try_from(days).unwrap_or(30))
        .fetch_all(pool)
        .await
}

/// A post or comment stored from another server, by its ActivityPub id.
pub enum RemoteObject {
    /// (post id, community id)
    Post(i64, Option<i64>),
    /// The comment's id.
    Comment(i64),
}

/// What `ap_id` is, when it was received from `remote_actor_id`.
pub async fn find_remote_object(pool: &PgPool, remote_actor_id: i64, ap_id: &str) -> Result<Option<RemoteObject>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT 'post' AS kind, id, community_id AS parent FROM posts WHERE ap_id = $1 AND remote_actor_id = $2
         UNION ALL
         SELECT 'comment', id, NULL FROM comments WHERE ap_id = $1 AND remote_actor_id = $2
         LIMIT 1",
    )
    .bind(ap_id)
    .bind(remote_actor_id)
    .fetch_optional(pool)
    .await?;
    row.map(|row| {
        let id: i64 = row.try_get("id")?;
        let parent: Option<i64> = row.try_get("parent")?;
        Ok(match row.try_get::<String, _>("kind")?.as_str() {
            "post" => RemoteObject::Post(id, parent),
            _ => RemoteObject::Comment(id),
        })
    })
    .transpose()
}

/// A post or comment edited on its server. `title` is kept for a comment.
/// An edit other than the text (a photo cropped): shown as edited, and
/// the federated object gets its `updated`.
pub async fn mark_edited(pool: &PgPool, post_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE posts SET edited_at = NOW(), updated_at = NOW() WHERE id = $1").bind(post_id).execute(pool).await?;
    Ok(())
}

/// Changes a video's category, by its author (or the staff, for the team's).
pub async fn set_category(pool: &PgPool, post_id: i64, category: Option<i16>) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE posts SET category = $2 WHERE id = $1 AND kind = 'video'")
        .bind(post_id)
        .bind(category)
        .execute(pool)
        .await?;
    Ok(())
}

/// `tags` and `category`: see `RemotePost`; kept for comments.
pub async fn update_remote(
    pool: &PgPool,
    object: &RemoteObject,
    title: Option<&str>,
    body: &str,
    sensitive: bool,
    tags: &str,
    category: Option<i16>,
) -> Result<(), sqlx::Error> {
    if title.is_some() && matches!(object, RemoteObject::Post(..)) {
        media::register_article_pictures(pool, body).await?;
    }
    let query = match object {
        RemoteObject::Post(id, _) => sqlx::query(
            "UPDATE posts SET body = $1, sensitive = $3, title = $4, tags = $5, category = $6,
                              edited_at = NOW(), updated_at = NOW(), link_checked = FALSE
             WHERE id = $2",
        )
        .bind(body)
        .bind(*id)
        .bind(sensitive)
        .bind(title)
        .bind(tags)
        .bind(category),
        RemoteObject::Comment(id) => {
            sqlx::query("UPDATE comments SET body = $1, sensitive = $3, edited_at = NOW(), updated_at = NOW() WHERE id = $2")
                .bind(body)
                .bind(*id)
                .bind(sensitive)
        }
    };
    query.execute(pool).await?;
    Ok(())
}

pub async fn delete_remote(pool: &PgPool, object: &RemoteObject) -> Result<(), sqlx::Error> {
    let (query, id) = match object {
        RemoteObject::Post(id, _) => ("DELETE FROM posts WHERE id = $1", *id),
        RemoteObject::Comment(id) => ("DELETE FROM comments WHERE id = $1", *id),
    };
    sqlx::query(query).bind(id).execute(pool).await?;
    Ok(())
}

/// Removes a post of a community, by its moderators.
pub async fn delete_in_community(pool: &PgPool, post_id: i64, community_id: i64) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("DELETE FROM posts WHERE id = $1 AND community_id = $2")
        .bind(post_id)
        .bind(community_id)
        .execute(pool)
        .await?
        .rows_affected()
        > 0)
}

/// Local id of a post from its ActivityPub id: one of ours
/// (`/ap/posts/{id}`, see `API::Federation`), or one received.
pub async fn find_by_ap_id(pool: &PgPool, ap_id: &str) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM posts WHERE ap_id = $1")
        .bind(ap_id)
        .fetch_optional(pool)
        .await
}

/// The posts of the rows, with their attachments, or else the preview of
/// their first link.
async fn to_posts(pool: &PgPool, rows: Vec<PgRow>) -> Result<Vec<Post>, sqlx::Error> {
    let mut posts = rows.iter().map(Post::from_row).collect::<Result<Vec<_>, _>>()?;
    let ids: Vec<i64> = posts.iter().map(|post| post.id).collect();
    let mut attachments = media::for_posts(pool, &ids).await?;
    let video_ids: Vec<i64> = posts.iter().filter(|post| post.is_video()).map(|post| post.id).collect();
    let mut video_files = match video_ids.is_empty() {
        true => Default::default(),
        false => crate::entities::video::files_for(pool, &video_ids).await?,
    };
    for post in &mut posts {
        if let Some(found) = attachments.remove(&post.id) {
            post.attachments = found;
        }
        if let Some(found) = video_files.remove(&post.id) {
            post.video_files = found;
        }
    }
    let links: Vec<Option<String>> = posts
        .iter()
        .map(|post| post.attachments.is_empty().then(|| crate::linkify::first_link(&post.body)).flatten())
        .collect();
    // Articles: every link, for the cards in them.
    let article_links: Vec<Vec<String>> = posts
        .iter()
        .map(|post| if post.is_article() { crate::markdown::links(&post.body) } else { Vec::new() })
        .collect();
    let urls: Vec<String> = links.iter().flatten().chain(article_links.iter().flatten()).cloned().collect();
    let previews = preview::find_many(pool, &urls).await?;
    for ((post, link), article_links) in posts.iter_mut().zip(links).zip(article_links) {
        post.preview = link.and_then(|link| previews.get(&link).cloned());
        post.link_previews = article_links.iter().filter_map(|link| previews.get(link).cloned()).collect();
    }
    Ok(posts)
}

/// Public posts of everyone here and of communities, and the viewer's own
/// private ones. Posts of the communities the viewer ignores are left out, and
/// posts of accounts followed from other servers only go to the feed.
/// Posts with the hashtag `tag` (lowercase, see `hashtag::normalize`) in
/// their text or title, or given apart (`tags`), newest first. `tag` has
/// only letters, digits and `_`: safe in the regular expression.
pub async fn by_tag(pool: &PgPool, viewer_id: Option<i32>, tag: &str, limit: i64) -> Result<Vec<Post>, sqlx::Error> {
    let pattern = format!("(^|[^[:alnum:]_&/#])#{tag}([^[:alnum:]_]|$)");
    let rows = sqlx::query(concat!(
        post_select!("NULL::text", "NULL::text"),
        " WHERE ",
        post_visible!(),
        " AND ",
        post_listed!(),
        " AND (p.body ~* $2 OR p.title ~* $2 OR (' ' || p.tags || ' ') LIKE '% ' || $3 || ' %')
         ORDER BY p.created_at DESC, p.id DESC
         LIMIT $4"
    ))
    .bind(viewer_id)
    .bind(pattern)
    .bind(tag)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    to_posts(pool, rows).await
}

/// Searches of fewer characters only find words starting with them: one
/// letter is in nearly every post.
pub const SEARCH_WORD_START_BELOW: usize = 3;

/// `text` with the characters special in a regular expression escaped.
pub fn escape_regex(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 8);
    for letter in text.chars() {
        if "\\.^$|?*+()[]{}".contains(letter) {
            escaped.push('\\');
        }
        escaped.push(letter);
    }
    escaped
}

/// Which posts a search may find (Mastodon's `indexable`): those of
/// indexable accounts, here or of other servers, and for the viewer `$1`
/// their own and those they liked, shared or commented on.
macro_rules! post_indexable {
    () => {
        "(COALESCE(p.user_id = $1, FALSE)
          OR EXISTS (SELECT 1 FROM users iu WHERE iu.id = p.user_id AND iu.indexable)
          OR EXISTS (SELECT 1 FROM federation_actors ia WHERE ia.id = p.remote_actor_id AND ia.indexable)
          OR EXISTS (SELECT 1 FROM post_likes il WHERE il.post_id = p.id AND il.user_id = $1)
          OR EXISTS (SELECT 1 FROM post_shares ish WHERE ish.post_id = p.id AND ish.user_id = $1)
          OR EXISTS (SELECT 1 FROM comments ic WHERE ic.post_id = p.id AND ic.user_id = $1))"
    };
}

/// Posts found by the search bar, of here and of other servers already
/// received, most relevant first: `words` at the start of the title, then
/// starting a word of the title, then of the text, then anywhere; among
/// those, the liked and recent ones first. Under
/// `SEARCH_WORD_START_BELOW` characters, only words starting with them.
pub async fn search(pool: &PgPool, viewer_id: Option<i32>, words: &str, kind: SearchKind, limit: i64) -> Result<Vec<Post>, sqlx::Error> {
    let words = words.trim();
    let pattern = format!("%{}%", crate::entities::user::escape_like(words));
    let word_start = format!("\\m{}", escape_regex(words));
    let short = words.chars().count() < SEARCH_WORD_START_BELOW;
    let rows = sqlx::query(concat!(
        post_select!("NULL::text", "NULL::text"),
        " WHERE ",
        post_visible!(),
        " AND ",
        post_listed!(),
        " AND ",
        post_indexable!(),
        " AND (CASE WHEN $6 THEN (p.body ~* $5 OR p.title ~* $5) ELSE (p.body ILIKE $2 OR p.title ILIKE $2) END)
          AND ($4 = 'all'
               OR ($4 = 'posts' AND p.kind = 'post' AND p.title IS NULL)
               OR ($4 = 'articles' AND p.kind = 'post' AND p.title IS NOT NULL)
               OR ($4 = 'videos' AND p.kind = 'video' AND p.media_status = 'ready'))
         ORDER BY CASE
                    WHEN p.title ILIKE SUBSTRING($2 FROM 2) THEN 0
                    WHEN p.title ~* $5 THEN 1
                    WHEN p.body ~* $5 THEN 2
                    ELSE 3
                  END,
                  LN(2 + (SELECT COUNT(*) FROM post_likes rl WHERE rl.post_id = p.id)
                       + (SELECT COUNT(*) FROM remote_reactions rr WHERE rr.post_id = p.id)) * 7
                    - EXTRACT(EPOCH FROM NOW() - p.created_at) / 86400.0 DESC,
                  p.id DESC
         LIMIT $3"
    ))
    .bind(viewer_id)
    .bind(pattern)
    .bind(limit)
    .bind(kind.as_str())
    .bind(word_start)
    .bind(short)
    .fetch_all(pool)
    .await?;
    to_posts(pool, rows).await
}

/// Which posts a search looks at, see `search`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SearchKind {
    All,
    /// Posts and photo posts, not articles nor videos.
    Posts,
    Articles,
    Videos,
}

impl SearchKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Posts => "posts",
            Self::Articles => "articles",
            Self::Videos => "videos",
        }
    }
}

/// A hashtag found by the search, with how many posts use it.
pub struct FoundTag {
    pub tag: String,
    pub uses: i64,
}

impl FoundTag {
    pub fn path(&self) -> String {
        crate::hashtag::path(&self.tag)
    }
}

/// The hashtags containing `words` (`game` finds `#videogames`) in the
/// posts the viewer can see, of here and of other servers known here: the
/// exact one, then those starting with it, then the most used. The trigram index of the texts finds the posts fast.
pub async fn search_tags(pool: &PgPool, viewer_id: Option<i32>, words: &str, limit: i64) -> Result<Vec<FoundTag>, sqlx::Error> {
    let words = crate::entities::user::escape_like(&words.trim().trim_start_matches('#').to_lowercase());
    let rows = sqlx::query(concat!(
        "SELECT LOWER(m[1]) AS tag, COUNT(DISTINCT p.id) AS uses
         FROM posts p
         LEFT JOIN communities cm ON cm.id = p.community_id
         CROSS JOIN LATERAL regexp_matches(
             COALESCE(p.title, '') || ' ' || p.body || ' ' || regexp_replace(p.tags, '(\\S+)', '#\\1', 'g'),
             '(?:^|[^[:alnum:]_&/#])#([[:alnum:]_]+)', 'g') m
         WHERE ",
        post_visible!(),
        " AND ",
        post_listed!(),
        " AND (p.body ILIKE '%' || $2 || '%' OR p.title ILIKE '%' || $2 || '%' OR p.tags ILIKE '%' || $2 || '%')
          AND LOWER(m[1]) LIKE '%' || $2 || '%'
          -- Short: only the hashtags starting with it.
          AND (LENGTH($2) >= 3 OR LOWER(m[1]) LIKE $2 || '%')
         GROUP BY 1
         ORDER BY LOWER(m[1]) = $2 DESC, LOWER(m[1]) LIKE $2 || '%' DESC, uses DESC, tag
         LIMIT $3"
    ))
    .bind(viewer_id)
    .bind(&words)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    let mut found = Vec::new();
    for row in &rows {
        let tag: String = row.try_get("tag")?;
        // Only real hashtags: with a letter, not too long (see `hashtag`).
        if let Some(tag) = crate::hashtag::normalize(&tag) {
            found.push(FoundTag { tag, uses: row.try_get("uses")? });
        }
    }
    Ok(found)
}

pub async fn timeline(pool: &PgPool, viewer_id: Option<i32>) -> Result<Vec<Post>, sqlx::Error> {
    let rows = sqlx::query(concat!(
        post_select!("NULL::text", "NULL::text"),
        " WHERE ",
        post_visible!(),
        " AND ",
        post_listed!(),
        " AND (p.remote_actor_id IS NULL OR p.community_id IS NOT NULL)
          AND NOT EXISTS (SELECT 1 FROM community_ignores ci
                          WHERE ci.community_id = p.community_id AND ci.user_id = $1)
         ORDER BY p.created_at DESC, p.id DESC
         LIMIT $2"
    ))
    .bind(viewer_id)
    .bind(PAGE_SIZE)
    .fetch_all(pool)
    .await?;
    to_posts(pool, rows).await
}

/// The viewer's feed, newest first: their posts and shares, and the posts and
/// shares (boosts, renotes) of their friends and of the users they follow,
/// here and on other servers.
pub async fn feed(pool: &PgPool, viewer_id: i32) -> Result<Vec<Post>, sqlx::Error> {
    let rows = sqlx::query(concat!(
        "WITH followed AS (
            SELECT actor_id FROM federation_following WHERE user_id = $1 AND accepted
         ), followed_users AS (
            SELECT followed_id FROM user_follows WHERE follower_id = $1
            UNION
            SELECT CASE WHEN requester_id = $1 THEN addressee_id ELSE requester_id END FROM friendships
            WHERE (requester_id = $1 OR addressee_id = $1) AND accepted AND ", crate::entities::service_on!(dfrn), "
         ), entries AS (
            SELECT p.id AS entry_post_id, p.created_at AS entry_at, NULL::integer AS sharer_user_id,
                   NULL::bigint AS sharer_actor_id
            FROM posts p
            WHERE p.user_id = $1 OR p.user_id IN (SELECT followed_id FROM followed_users)
               OR (p.community_id IS NULL AND p.remote_actor_id IN (SELECT actor_id FROM followed))
            UNION ALL
            -- Public posts of the last days with a hashtag the user follows
            -- (`hashtag_follow`); tags have only letters, digits and `_`.
            SELECT p.id, p.created_at, NULL, NULL FROM posts p
            WHERE p.visibility = 'public' AND p.created_at > NOW() - INTERVAL '30 days'
              AND EXISTS (SELECT 1 FROM hashtag_follows hf WHERE hf.user_id = $1
                    AND (p.body ~* ('(^|[^[:alnum:]_&/#])#' || hf.tag || '([^[:alnum:]_]|$)')
                         OR p.title ~* ('(^|[^[:alnum:]_&/#])#' || hf.tag || '([^[:alnum:]_]|$)')
                         OR (' ' || p.tags || ' ') LIKE '% ' || hf.tag || ' %'))
            UNION ALL
            SELECT s.post_id, s.created_at, s.user_id, NULL FROM post_shares s
            WHERE s.user_id = $1 OR s.user_id IN (SELECT followed_id FROM followed_users)
            UNION ALL
            SELECT r.post_id, r.created_at, NULL, r.actor_id FROM remote_reactions r
            WHERE r.kind = 'share' AND r.post_id IS NOT NULL AND r.actor_id IN (SELECT actor_id FROM followed)
         ), feed AS (
            -- A post shared several times appears once, at its latest share.
            SELECT DISTINCT ON (entry_post_id) * FROM entries ORDER BY entry_post_id, entry_at DESC
         ) ",
        post_select!(
            "CASE WHEN f.sharer_user_id IS NOT NULL AND p.user_id IS DISTINCT FROM f.sharer_user_id THEN su.username
                  WHEN f.sharer_actor_id IS NOT NULL
                  THEN sa.username || '@' || substring(sa.actor_id from '://([^/]+)') END",
            "CASE WHEN f.sharer_user_id IS NOT NULL AND p.user_id IS DISTINCT FROM f.sharer_user_id THEN '/user/' || su.username
                  WHEN f.sharer_actor_id IS NOT NULL THEN '/remote/' || sa.id END"
        ),
        " JOIN feed f ON f.entry_post_id = p.id
          LEFT JOIN users su ON su.id = f.sharer_user_id
          LEFT JOIN federation_actors sa ON sa.id = f.sharer_actor_id
         WHERE ",
        post_visible!(),
        " ORDER BY f.entry_at DESC, p.id DESC
         LIMIT $2"
    ))
    .bind(viewer_id)
    .bind(PAGE_SIZE)
    .fetch_all(pool)
    .await?;
    to_posts(pool, rows).await
}

/// Posts of an account of another server stored here, newest first.
pub async fn by_remote_author(pool: &PgPool, remote_actor_id: i64, viewer_id: Option<i32>) -> Result<Vec<Post>, sqlx::Error> {
    let rows = sqlx::query(concat!(
        post_select!("NULL::text", "NULL::text"),
        " WHERE p.remote_actor_id = $2 AND ",
        post_visible!(),
        " ORDER BY p.created_at DESC, p.id DESC
         LIMIT $3"
    ))
    .bind(viewer_id)
    .bind(remote_actor_id)
    .bind(PAGE_SIZE)
    .fetch_all(pool)
    .await?;
    to_posts(pool, rows).await
}

/// What a profile shows: the posts the user wrote and the ones they shared,
/// newest first (by share date for shares), limited to what the viewer can see.
pub async fn by_author(
    pool: &PgPool,
    author_id: i32,
    viewer_id: Option<i32>,
) -> Result<Vec<Post>, sqlx::Error> {
    let rows = sqlx::query(concat!(
        post_select!(
            "CASE WHEN p.user_id IS DISTINCT FROM $2 THEN sharer.username END",
            "CASE WHEN p.user_id IS DISTINCT FROM $2 THEN '/user/' || sharer.username END"
        ),
        " LEFT JOIN post_shares ps ON ps.post_id = p.id AND ps.user_id = $2
          LEFT JOIN users sharer ON sharer.id = ps.user_id
         WHERE (p.user_id = $2 OR ps.user_id IS NOT NULL)
           AND ",
        post_visible!(),
        "
         ORDER BY COALESCE(ps.created_at, p.created_at) DESC, p.id DESC
         LIMIT $3"
    ))
    .bind(viewer_id)
    .bind(author_id)
    .bind(PAGE_SIZE)
    .fetch_all(pool)
    .await?;
    to_posts(pool, rows).await
}

/// Order of a community's posts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    New,
    /// Most liked first.
    Top,
    /// Likes and comments, cooling down with age: what's active now.
    Hot,
}

impl Sort {
    pub const ALL: [Self; 3] = [Self::Hot, Self::New, Self::Top];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Top => "top",
            Self::Hot => "hot",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::New => "New",
            Self::Top => "Top",
            Self::Hot => "Hot",
        }
    }

    /// Hot when missing or unknown.
    pub fn parse(value: Option<&str>) -> Self {
        Self::ALL
            .into_iter()
            .find(|sort| Some(sort.as_str()) == value)
            .unwrap_or(Self::Hot)
    }
}

/// Posts of a community the viewer can see, see `post_visible!`.
pub async fn for_community(
    pool: &PgPool,
    community_id: i64,
    viewer_id: Option<i32>,
    sort: Sort,
) -> Result<Vec<Post>, sqlx::Error> {
    // Three queries rather than a dynamic ORDER BY: sqlx wants SQL known at
    // compile time.
    macro_rules! ordered {
        ($order:literal) => {
            concat!(
                post_select!("NULL::text", "NULL::text"),
                " WHERE p.community_id = $2 AND ",
                post_visible!(),
                " ORDER BY ",
                $order,
                " LIMIT $3"
            )
        };
    }
    let query = match sort {
        Sort::New => ordered!("p.created_at DESC, p.id DESC"),
        Sort::Top => ordered!("like_count DESC, p.created_at DESC, p.id DESC"),
        // Hacker News' formula: points over (age in hours + 2) ^ 1.5. The
        // counts are written out: Postgres doesn't accept column aliases
        // inside an ORDER BY expression.
        Sort::Hot => ordered!(
            "((SELECT COUNT(*) FROM post_likes hl WHERE hl.post_id = p.id)
              + (SELECT COUNT(*) FROM remote_reactions hr WHERE hr.post_id = p.id AND hr.kind = 'like')
              + (SELECT COUNT(*) FROM comments hc WHERE hc.post_id = p.id AND hc.removed_at IS NULL) + 1)
              / POWER(EXTRACT(EPOCH FROM (NOW() - p.created_at)) / 3600 + 2, 1.5) DESC, p.id DESC"
        ),
    };
    let rows = sqlx::query(query)
        .bind(viewer_id)
        .bind(community_id)
        .bind(PAGE_SIZE)
        .fetch_all(pool)
        .await?;
    to_posts(pool, rows).await
}

/// One post, `None` if it doesn't exist or the viewer can't see it.
pub async fn find(pool: &PgPool, post_id: i64, viewer_id: Option<i32>) -> Result<Option<Post>, sqlx::Error> {
    let rows = sqlx::query(concat!(
        post_select!("NULL::text", "NULL::text"),
        " WHERE p.id = $2 AND ",
        post_visible!()
    ))
    .bind(viewer_id)
    .bind(post_id)
    .fetch_optional(pool)
    .await?;
    Ok(to_posts(pool, rows.into_iter().collect()).await?.pop())
}

/// A post other Fediverse servers may hear about: what visitors can see, and
/// followers-only posts of other servers (only their author is told, see
/// `handlers::notifier`).
pub async fn find_federable(pool: &PgPool, post_id: i64) -> Result<Option<Post>, sqlx::Error> {
    if let Some(post) = find(pool, post_id, None).await? {
        return Ok(Some(post));
    }
    let rows = sqlx::query(concat!(
        post_select!("NULL::text", "NULL::text"),
        " WHERE p.id = $2 AND p.removed_at IS NULL AND p.visibility = 'followers' AND p.remote_actor_id IS NOT NULL
           AND $1::integer IS NULL"
    ))
    .bind(None::<i32>)
    .bind(post_id)
    .fetch_optional(pool)
    .await?;
    Ok(to_posts(pool, rows.into_iter().collect()).await?.pop())
}

#[derive(Clone, Copy, Debug)]
pub enum Reaction {
    Like,
    Share,
}

/// A reaction after `toggle`.
#[derive(Clone, Copy, Debug)]
pub struct Toggled {
    /// `false` when it was removed.
    pub active: bool,
    /// When the reaction was made, also for a removed one.
    pub reacted_at: DateTime<Utc>,
}

/// Adds the reaction, or removes it when the user already reacted.
/// Returns `None` when the post doesn't exist or can't be reacted to:
/// hidden from the user, or a share of their own or a private post.
pub async fn toggle(
    pool: &PgPool,
    reaction: Reaction,
    post_id: i64,
    user_id: i32,
) -> Result<Option<Toggled>, sqlx::Error> {
    let Some(post) = find(pool, post_id, Some(user_id)).await? else {
        return Ok(None);
    };
    if matches!(reaction, Reaction::Share) && !post.can_share() {
        return Ok(None);
    }

    let (delete, insert) = match reaction {
        Reaction::Like => (
            "DELETE FROM post_likes WHERE post_id = $1 AND user_id = $2 RETURNING created_at",
            "INSERT INTO post_likes (post_id, user_id) VALUES ($1, $2)
             ON CONFLICT (post_id, user_id) DO UPDATE SET created_at = post_likes.created_at
             RETURNING created_at",
        ),
        Reaction::Share => (
            "DELETE FROM post_shares WHERE post_id = $1 AND user_id = $2 RETURNING created_at",
            "INSERT INTO post_shares (post_id, user_id) VALUES ($1, $2)
             ON CONFLICT (post_id, user_id) DO UPDATE SET created_at = post_shares.created_at
             RETURNING created_at",
        ),
    };

    let removed: Option<DateTime<Utc>> = sqlx::query_scalar(delete)
        .bind(post_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    if let Some(reacted_at) = removed {
        return Ok(Some(Toggled { active: false, reacted_at }));
    }
    // DO UPDATE instead of DO NOTHING so a double click still returns the row.
    let reacted_at = sqlx::query_scalar(insert).bind(post_id).bind(user_id).fetch_one(pool).await?;
    Ok(Some(Toggled { active: true, reacted_at }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn labels_are_read_from_forms_and_other_servers() {
        use super::{Labels, WARNING_MAX_CHARS};
        let fields = |name: &str| match name {
            "content_warning" => Some("  spoilers \n  for   the end ".to_string()),
            "ai_generated" => Some("on".to_string()),
            _ => None,
        };
        let labels = Labels::from_form(fields);
        assert_eq!(labels.content_warning, "spoilers for the end");
        assert!(labels.ai_generated);
        assert!(!labels.sponsored);
        let long = "x".repeat(WARNING_MAX_CHARS + 50);
        assert_eq!(Labels::from_form(|_| Some(long.clone())).content_warning.chars().count(), WARNING_MAX_CHARS);

        let remote = Labels::from_remote(Some("food"), &["AIArt".to_string(), "Ad".to_string()]);
        assert_eq!(remote, Labels { content_warning: "food".to_string(), ai_generated: true, sponsored: true });
        assert_eq!(Labels::from_remote(None, &["cats".to_string()]), Labels::default());
    }

    /// The rule is pasted into many queries: unbalanced, every post is hidden.
    #[test]
    fn visibility_rule_is_balanced() {
        let rule = post_visible!();
        let mut depth = 0i32;
        for letter in rule.chars() {
            match letter {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            assert!(depth >= 0, "a parenthesis closes too early");
        }
        assert_eq!(depth, 0);
        // One group: safe next to AND and OR.
        assert!(rule.starts_with('(') && rule.ends_with(')'));
    }
}

/// `1.2K`, `12K`: one decimal under 10.
fn compact(value: f64, unit: &str) -> String {
    if value < 10.0 {
        let rounded = (value * 10.0).floor() / 10.0;
        format!("{}{unit}", format!("{rounded:.1}").trim_end_matches(".0"))
    } else {
        format!("{}{unit}", value.floor())
    }
}

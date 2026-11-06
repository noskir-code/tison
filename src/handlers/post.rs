use std::collections::HashMap;

use askama::Template;
use axum::{
    Form,
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use tower_sessions::Session;

use crate::{
    AppState,
    entities::{
        media::NewAttachment,
        post::{self, ARTICLE_MAX_CHARS, BODY_MAX_CHARS, BodyError, Post, TITLE_MAX_CHARS, Visibility},
    },
    handlers::{
        image::{self, ImageOwner, MAX_IMAGE_BYTES},
        notifier,
        system::{Layout, error_page, local_path, render_template},
        user::signed_in_user,
    },
};
/// `|t` and `|tf` in the templates, see `i18n`.
use crate::i18n::filters;

/// Pages with the post form. Sent back by the form so the author returns
/// where they posted from, only to these fixed URLs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PostPage {
    Home,
    Feed,
    /// Moderation > Staff: posts of the team account, see `Composer::for_team`.
    Staff,
}

impl PostPage {
    fn as_str(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Feed => "feed",
            Self::Staff => "staff",
        }
    }

    fn url(self) -> &'static str {
        match self {
            Self::Home => "/",
            Self::Feed => "/your-feed",
            Self::Staff => "/moderation/staff",
        }
    }

    fn parse(value: &str) -> Self {
        match value {
            "feed" => Self::Feed,
            "staff" => Self::Staff,
            _ => Self::Home,
        }
    }
}

/// Errors sent back to the page as `?error=<code>`.
#[derive(Clone, Copy)]
enum PostError {
    Empty,
    TooLong,
    InvalidCharacters,
    InvalidVisibility,
    TooManyPhotos,
    PhotoTooLarge,
    PhotoFormat,
    NoTitle,
    TitleTooLong,
    ArticleTooLong,
    /// A bot published `settings::bot_posts_per_hour` posts this hour.
    BotLimit,
    /// Text alone, with `BLOGGING` off: a photo is needed.
    NeedsPhoto,
    /// The same text as a recent post, see `entities::spam`.
    Duplicate,
    /// Too many posts this hour (`security::Limit::Post`).
    TooMany,
    Server,
}

impl PostError {
    const ALL: [Self; 15] = [
        Self::Empty,
        Self::TooLong,
        Self::InvalidCharacters,
        Self::InvalidVisibility,
        Self::TooManyPhotos,
        Self::PhotoTooLarge,
        Self::PhotoFormat,
        Self::NoTitle,
        Self::TitleTooLong,
        Self::ArticleTooLong,
        Self::BotLimit,
        Self::NeedsPhoto,
        Self::Duplicate,
        Self::TooMany,
        Self::Server,
    ];

    fn code(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::TooLong => "too-long",
            Self::InvalidCharacters => "invalid-characters",
            Self::InvalidVisibility => "invalid-visibility",
            Self::TooManyPhotos => "too-many-photos",
            Self::PhotoTooLarge => "photo-too-large",
            Self::PhotoFormat => "photo-format",
            Self::NoTitle => "no-title",
            Self::TitleTooLong => "title-too-long",
            Self::ArticleTooLong => "article-too-long",
            Self::BotLimit => "bot-limit",
            Self::NeedsPhoto => "needs-photo",
            Self::Duplicate => "duplicate",
            Self::TooMany => "too-many",
            Self::Server => "server",
        }
    }

    fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|error| error.code() == code)
    }

    fn message(self) -> String {
        match self {
            Self::Empty => "Write something or add a photo before publishing.".to_string(),
            Self::TooLong => crate::i18n::tf(
                "A post can be at most {} characters (a link counts for 23, and @name@server for @name).",
                crate::settings::services().blogging_max_chars,
            ),
            Self::InvalidCharacters => "The post contains characters that are not allowed.".to_string(),
            Self::InvalidVisibility => "Choose who can see the post.".to_string(),
            Self::TooManyPhotos => format!("A post can have at most {MAX_PHOTOS} photos."),
            Self::PhotoTooLarge => format!("A photo can be at most {} MB.", MAX_IMAGE_BYTES / 1024 / 1024),
            Self::PhotoFormat => "Photos must be JPEG, PNG, GIF or WebP.".to_string(),
            Self::NoTitle => "Give the article a title.".to_string(),
            Self::TitleTooLong => format!("A title can be at most {TITLE_MAX_CHARS} characters, on one line."),
            Self::ArticleTooLong => crate::i18n::tf("An article can be at most {} characters.", ARTICLE_MAX_CHARS),
            Self::BotLimit => bot_limit_message(),
            Self::NeedsPhoto => crate::i18n::t("Add a photo: this server shares photos, not text alone."),
            Self::Duplicate => crate::i18n::t("You already posted this text today: repeating it looks like spam."),
            Self::TooMany => crate::i18n::t("You published a lot this hour: wait a little before publishing again."),
            Self::Server => "Something went wrong, try again later.".to_string(),
        }
    }
}

impl From<BodyError> for PostError {
    fn from(err: BodyError) -> Self {
        match err {
            BodyError::Empty => Self::Empty,
            BodyError::TooLong => Self::TooLong,
            BodyError::InvalidCharacters => Self::InvalidCharacters,
        }
    }
}

/// Photos of one post, like Pixelfed's albums by default and what Mastodon
/// shows.
pub const MAX_PHOTOS: usize = 4;
/// What `/posts` accepts: the photos and the text.
pub const MAX_POST_BYTES: usize = MAX_PHOTOS * MAX_IMAGE_BYTES + 1024 * 1024;

/// Data used by the post form in `macros/post.html`.
pub struct Composer {
    pub(crate) page: &'static str,
    pub(crate) error: Option<String>,
    pub(crate) visibilities: [Visibility; 3],
    pub(crate) body_max_chars: usize,
    /// The Post tab: text posts (`BLOGGING`) or photos (`IMAGESHARING`).
    pub(crate) posts: bool,
    /// `BLOGGING`: text alone can be posted, else a photo is needed.
    pub(crate) text_posts: bool,
    /// The longest post, counted like Mastodon (`BLOGGING-MAX-CHARS`).
    pub(crate) post_max_chars: usize,
    /// `IMAGESHARING` in `configs/services.conf`.
    pub(crate) photos: bool,
    pub(crate) max_photos: usize,
    /// The Article tab, with `PUBLISHING` in `configs/services.conf`.
    pub(crate) articles: bool,
    /// Opened on the Article tab (`?compose=article`), after an error in it.
    pub(crate) article_open: bool,
    pub(crate) title_max_chars: usize,
    pub(crate) article_max_chars: usize,
    /// The username of the team account, which the staff can post as.
    pub(crate) team: Option<String>,
    /// Always as the team account, without the choice (the Staff tab).
    pub(crate) team_only: bool,
    /// Chosen at first: unlisted for bots (Server rules, 7), public otherwise.
    pub(crate) default_visibility: Visibility,
}

impl Composer {
    fn new(page: PostPage, query: &HashMap<String, String>) -> Self {
        Self {
            team: None,
            team_only: false,
            default_visibility: Visibility::Public,
            page: page.as_str(),
            error: query
                .get("error")
                .and_then(|code| PostError::from_code(code))
                .map(PostError::message),
            visibilities: Visibility::ALL,
            body_max_chars: BODY_MAX_CHARS,
            posts: crate::settings::services().blogging || crate::settings::services().image_sharing,
            text_posts: crate::settings::services().blogging,
            post_max_chars: crate::settings::services().blogging_max_chars,
            photos: crate::settings::services().image_sharing,
            max_photos: MAX_PHOTOS,
            articles: crate::settings::services().publishing,
            // Without the Post tab, articles are all there is.
            article_open: crate::settings::services().publishing
                && (query.get("compose").is_some_and(|kind| kind == "article")
                    || !(crate::settings::services().blogging || crate::settings::services().image_sharing)),
            title_max_chars: TITLE_MAX_CHARS,
            article_max_chars: ARTICLE_MAX_CHARS,
        }
    }
}

#[derive(Template)]
#[template(path = "index.html")]
struct HomeTemplate {
    title: String,
    layout: Layout,
    composer: Composer,
    posts: Vec<Post>,
}

#[derive(Template)]
#[template(path = "feed.html")]
struct FeedTemplate {
    title: String,
    layout: Layout,
    composer: Composer,
    posts: Vec<Post>,
}

/// A page still works without its posts: they are logged and shown as empty.
fn or_empty(result: Result<Vec<Post>, sqlx::Error>) -> Vec<Post> {
    result.unwrap_or_else(|err| {
        tracing::error!("failed to load posts: {}", err);
        Vec::new()
    })
}

/// Public posts of everyone.
pub async fn home_page(
    State(state): State<AppState>,
    session: Session,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let layout = Layout::load(&state, &session).await;
    let posts = match &state.pool {
        Some(pool) => {
            let viewer_id = layout.viewer.as_ref().map(|viewer| viewer.id);
            or_empty(post::timeline(pool, viewer_id).await)
        }
        None => Vec::new(),
    };
    let mut composer = Composer::new(PostPage::Home, &query);
    composer.team = team_for(&state, &layout).await;
    if layout.viewer.as_ref().is_some_and(crate::entities::user::User::is_bot) {
        composer.default_visibility = Visibility::Unlisted;
    }
    render_template(&HomeTemplate {
        title: "Home".to_string(),
        layout,
        composer,
        posts,
    })
}

/// The signed-in user's feed: their posts, and those of the accounts they
/// follow on other servers.
pub async fn feed_page(
    State(state): State<AppState>,
    session: Session,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let layout = Layout::load(&state, &session).await;
    let (Some(viewer_id), Some(pool)) = (layout.viewer.as_ref().map(|viewer| viewer.id), &state.pool)
    else {
        return Redirect::to("/login").into_response();
    };
    let posts = or_empty(post::feed(pool, viewer_id).await);
    let mut composer = Composer::new(PostPage::Feed, &query);
    composer.team = team_for(&state, &layout).await;
    if layout.viewer.as_ref().is_some_and(crate::entities::user::User::is_bot) {
        composer.default_visibility = Visibility::Unlisted;
    }
    render_template(&FeedTemplate {
        title: "Feed".to_string(),
        layout,
        composer,
        posts,
    })
}

impl Composer {
    /// The form of the Staff tab: posts and articles of the team account
    /// `team` (its username).
    pub fn for_team(query: &HashMap<String, String>, team: String) -> Self {
        Self {
            team: Some(team),
            team_only: true,
            ..Self::new(PostPage::Staff, query)
        }
    }
}

/// The team account's username, when the viewer is of the staff and it's on.
async fn team_for(state: &AppState, layout: &Layout) -> Option<String> {
    let pool = state.pool.as_ref()?;
    if !layout.viewer.as_ref().is_some_and(crate::entities::user::User::is_staff) {
        return None;
    }
    crate::entities::user::team_account(pool).await.ok().flatten().map(|team| team.username)
}

/// What the post form sent: `multipart/form-data`, for the photos.
#[derive(Default)]
struct PostForm {
    page: String,
    /// `article` from the Article tab.
    kind: String,
    title: String,
    body: String,
    visibility: String,
    /// `team` to post as the team account (the staff only).
    author: String,
    sensitive: bool,
    /// Content warning, made with AI, sponsored: see `post::Labels`.
    fields: HashMap<String, String>,
    photos: Vec<Vec<u8>>,
    /// Alt texts, in the order of the photos.
    alts: Vec<String>,
    /// The originals of the cropped photos (`original-N`, N their index),
    /// kept to crop them again.
    originals: HashMap<usize, Vec<u8>>,
}

async fn read_post_form(multipart: &mut Multipart) -> Result<PostForm, PostError> {
    let mut form = PostForm::default();
    while let Some(field) = multipart.next_field().await.map_err(|_| PostError::PhotoTooLarge)? {
        let name = field.name().unwrap_or_default().to_string();
        if let Some(index) = name.strip_prefix("original-").and_then(|index| index.parse::<usize>().ok()).filter(|index| *index < MAX_PHOTOS) {
            let data = field.bytes().await.map_err(|_| PostError::PhotoTooLarge)?;
            if data.len() > MAX_IMAGE_BYTES {
                return Err(PostError::PhotoTooLarge);
            }
            if !data.is_empty() {
                form.originals.insert(index, data.to_vec());
            }
            continue;
        }
        if name == "photos" {
            // Fails when the body goes over `MAX_POST_BYTES`.
            let data = field.bytes().await.map_err(|_| PostError::PhotoTooLarge)?;
            // The empty field sent when no photo is chosen.
            if data.is_empty() {
                continue;
            }
            if data.len() > MAX_IMAGE_BYTES {
                return Err(PostError::PhotoTooLarge);
            }
            if form.photos.len() == MAX_PHOTOS {
                return Err(PostError::TooManyPhotos);
            }
            form.photos.push(data.to_vec());
            continue;
        }

        let text = field.text().await.map_err(|_| PostError::Server)?;
        match name.as_str() {
            "page" => form.page = text,
            "kind" => form.kind = text,
            "title" => form.title = text,
            "body" => form.body = text,
            "visibility" => form.visibility = text,
            "author" => form.author = text,
            "sensitive" => form.sensitive = !text.is_empty(),
            "alt" => form.alts.push(text),
            "content_warning" | "ai_generated" | "sponsored" => {
                form.fields.insert(name, text);
            }
            _ => {}
        }
    }
    Ok(form)
}

/// Cleans and saves the photos (see `photo`) in the author's folder.
async fn save_photos(user_id: i32, form: &PostForm) -> Result<Vec<NewAttachment>, PostError> {
    let mut cleaned = Vec::new();
    for data in &form.photos {
        cleaned.push(crate::photo::prepare(data).await.map_err(|_| PostError::PhotoFormat)?);
    }
    let mut saved: Vec<NewAttachment> = Vec::new();
    for (index, photo) in cleaned.into_iter().enumerate() {
        match image::save_file(ImageOwner::User(user_id), "post", photo.extension, &photo.data).await {
            Ok(url) => {
                let mut attachment = NewAttachment::new(
                    url,
                    photo.media_type.to_string(),
                    form.alts.get(index).map(String::as_str).unwrap_or(""),
                    i32::try_from(photo.width).ok(),
                    i32::try_from(photo.height).ok(),
                );
                // Cropped in the browser: its original, to crop it again.
                if let Some(original) = form.originals.get(&index) {
                    attachment.original_url = image::save_original(ImageOwner::User(user_id), original).await.ok();
                }
                saved.push(attachment);
            }
            Err(err) => {
                tracing::error!("failed to save a photo: {:?}", err);
                remove_saved(&saved).await;
                return Err(PostError::Server);
            }
        }
    }
    Ok(saved)
}

async fn remove_saved(photos: &[NewAttachment]) {
    let urls: Vec<String> = photos.iter().flat_map(|photo| std::iter::once(photo.url.clone()).chain(photo.original_url.clone())).collect();
    crate::entities::media::remove_local_files(&urls).await;
}

pub async fn create_post(State(state): State<AppState>, session: Session, mut multipart: Multipart) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let form = read_post_form(&mut multipart).await;
    let page = PostPage::parse(form.as_ref().map(|form| form.page.as_str()).unwrap_or(""));
    let is_article = form.as_ref().is_ok_and(|form| form.kind == "article");
    let labels_form = form.as_ref().map(|form| form.fields.clone()).map_err(|_| ());

    let result = async {
        let mut form = form?;
        // Posting as the team account: the staff only, while it's on.
        let author_id = if form.author == "team" {
            let team = crate::entities::user::team_account(&pool).await.map_err(|err| {
                tracing::error!("failed to load the team account: {}", err);
                PostError::Server
            })?;
            match team {
                Some(team) if me.is_staff() => {
                    tracing::info!("@{} posts as @{}", me.username, team.username);
                    team.id
                }
                _ => return Err(PostError::Server),
            }
        } else {
            me.id
        };
        // Only what others see counts (Server rules, 7): private notes don't.
        let private = Visibility::parse(&form.visibility) == Some(Visibility::Private);
        if author_id == me.id && !private && bot_over_limit(&pool, &me).await {
            return Err(PostError::BotLimit);
        }
        if !crate::security::hit(crate::security::Limit::Post, &author_id.to_string()) {
            return Err(PostError::TooMany);
        }
        // The same text again: refused, see `entities::spam`.
        if !private {
            let text = form.body.clone();
            let sender = crate::entities::spam::Sender::Local(author_id);
            if crate::entities::spam::is_repeated(&pool, sender, crate::entities::spam::Kind::Post, &text, None).await.unwrap_or_else(|err| {
                tracing::warn!("failed to check repeats: {}", err);
                false
            }) {
                if let Err(err) = crate::entities::spam::record(&pool, sender, crate::entities::spam::Kind::Post, &text, &format!("@{}", me.username)).await {
                    tracing::warn!("failed to record a repeat of @{}: {}", me.username, err);
                }
                return Err(PostError::Duplicate);
            }
        }
        if is_article {
            if !crate::settings::services().publishing {
                return Err(PostError::Server);
            }
            let (title, body) = validate_article(&form.title, &form.body)?;
            let visibility = Visibility::parse(&form.visibility).ok_or(PostError::InvalidVisibility)?;
            return post::create_article(&pool, author_id, &title, &body, visibility).await.map_err(|err| {
                tracing::error!("failed to create article: {}", err);
                PostError::Server
            });
        }
        let services = crate::settings::services();
        if !services.image_sharing {
            form.photos.clear();
        }
        // Without `BLOGGING`, posts are photos with a caption, like on
        // Pixelfed (and none at all without photos either).
        if !services.blogging && form.photos.is_empty() {
            return Err(if services.image_sharing { PostError::NeedsPhoto } else { PostError::Server });
        }
        // A photo can go without a caption, like on Pixelfed.
        let body = match post::validate_body(&form.body) {
            Err(BodyError::Empty) if !form.photos.is_empty() => String::new(),
            body => body?,
        };
        check_post_length(&body)?;
        let visibility = Visibility::parse(&form.visibility).ok_or(PostError::InvalidVisibility)?;
        let failed = |err: sqlx::Error| {
            tracing::error!("failed to create post: {}", err);
            PostError::Server
        };
        if form.photos.is_empty() {
            return post::create(&pool, author_id, &body, visibility, None).await.map_err(failed);
        }
        let photos = save_photos(author_id, &form).await?;
        match post::create_with_photos(&pool, author_id, &body, visibility, form.sensitive, &photos).await {
            Ok(post_id) => Ok(post_id),
            Err(err) => {
                remove_saved(&photos).await;
                Err(failed(err))
            }
        }
    }
    .await;

    if let (Ok(post_id), Ok(form)) = (&result, &labels_form) {
        save_labels(&pool, *post_id, form).await;
    }
    // Mentions of posts (not articles, in Markdown), before the post goes to
    // other servers: they get them as tags.
    if let Ok(post_id) = &result
        && !is_article
        && crate::settings::services().blogging
    {
        save_mentions(&state, &pool, *post_id).await;
    }
    if let Ok(post_id) = &result {
        notifier::post_created(&state, &pool, *post_id).await;
        state.previews.notify_one();
    }
    let location = match result {
        // An article opens on its own page, like on WriteFreely.
        Ok(post_id) if is_article => format!("/posts/{post_id}"),
        Ok(_) if page == PostPage::Staff => format!("{}?published=1#team-composer", page.url()),
        Ok(_) => page.url().to_string(),
        Err(error) if is_article => format!("{}?error={}&compose=article", page.url(), error.code()),
        Err(error) => format!("{}?error={}", page.url(), error.code()),
    };
    Redirect::to(&location).into_response()
}

/// Keeps the mentions of a post written here and tells the people
/// mentioned, see `handlers::mention`. Its author may be the team account.
async fn save_mentions(state: &AppState, pool: &sqlx::PgPool, post_id: i64) {
    let found: Result<Option<(Option<i32>, String)>, sqlx::Error> = sqlx::query_as("SELECT user_id, body FROM posts WHERE id = $1")
        .bind(post_id)
        .fetch_optional(pool)
        .await;
    let (author_id, body) = match found {
        Ok(Some((Some(author_id), body))) => (author_id, body),
        Ok(_) => return,
        Err(err) => {
            tracing::error!("failed to load post {} for its mentions: {}", post_id, err);
            return;
        }
    };
    match crate::entities::user::find_by_id(pool, author_id).await {
        Ok(Some(author)) => crate::handlers::mention::save(state, pool, &author, post_id, &body).await,
        Ok(None) => {}
        Err(err) => tracing::error!("failed to load the author of post {}: {}", post_id, err),
    }
}

/// A post at most `BLOGGING-MAX-CHARS` long, counted like Mastodon (see
/// `mention::mastodon_length`).
fn check_post_length(body: &str) -> Result<(), PostError> {
    match crate::mention::mastodon_length(body) > crate::settings::services().blogging_max_chars {
        true => Err(PostError::TooLong),
        false => Ok(()),
    }
}

/// The title (one line) and Markdown text of an article.
fn validate_article(title: &str, body: &str) -> Result<(String, String), PostError> {
    let title = match post::validate_text(title, TITLE_MAX_CHARS) {
        Ok(title) if title.contains('\n') => return Err(PostError::TitleTooLong),
        Ok(title) => title,
        Err(BodyError::Empty) => return Err(PostError::NoTitle),
        Err(BodyError::TooLong) => return Err(PostError::TitleTooLong),
        Err(BodyError::InvalidCharacters) => return Err(PostError::InvalidCharacters),
    };
    let body = match post::validate_text(body, ARTICLE_MAX_CHARS) {
        Err(BodyError::TooLong) => return Err(PostError::ArticleTooLong),
        body => body?,
    };
    Ok((title, body))
}

/// The page editing a post or a comment.
#[derive(Template)]
#[template(path = "edit.html")]
pub struct EditTemplate {
    pub title: String,
    pub layout: Layout,
    pub action: String,
    pub back: String,
    pub body: String,
    pub body_max_chars: usize,
    /// The title of an article, `None` for posts and comments.
    pub article_title: Option<String>,
    pub title_max_chars: usize,
    pub error: Option<String>,
    /// Other servers may have it: they get the edit.
    pub federated: bool,
    /// The thumbnail form, for a video.
    pub video_thumbnail: Option<VideoThumbnail>,
    /// A video's category now (`Some(None)`: none), `None` for the others.
    pub video_category: Option<Option<i16>>,
    /// The photos of a photo post, to crop again (see `crop_photo`): shown,
    /// and their original.
    pub photos: Vec<(String, String)>,
    /// A post's content warning and labels, `None` for comments.
    pub labels: Option<post::Labels>,
    /// For a post (not an article, a video nor a comment): its longest
    /// length counted like Mastodon, shown by a counter.
    pub post_max_chars: Option<usize>,
}

/// The thumbnail of a video being edited (`/posts/{id}/thumbnail`).
pub struct VideoThumbnail {
    pub post_id: i64,
    /// The picture shown now.
    pub url: Option<String>,
    pub error: Option<String>,
    pub saved: bool,
    pub width: u32,
    pub height: u32,
}

/// The user's own post, written here.
async fn own_post(state: &AppState, session: &Session, post_id: i64) -> Result<(sqlx::PgPool, crate::entities::user::User, Post), Response> {
    let (pool, me) = signed_in_user(state, session).await?;
    match post::find(&pool, post_id, Some(me.id)).await {
        Ok(Some(post)) if post.can_edit() => Ok((pool, me, post)),
        Ok(_) => Err(error_page(Layout::load(state, session).await, StatusCode::NOT_FOUND, "This post does not exist or isn't yours.")),
        Err(err) => {
            tracing::error!("failed to load post {}: {}", post_id, err);
            Err(error_page(Layout::load(state, session).await, StatusCode::INTERNAL_SERVER_ERROR, "Failed to load this post."))
        }
    }
}

pub async fn edit_page(
    State(state): State<AppState>,
    session: Session,
    Path(post_id): Path<i64>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let (_, _, post) = match own_post(&state, &session, post_id).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    // An event has its own form: its date, place and how to join.
    if post.is_event() {
        return Redirect::to(&format!("/events/{post_id}/edit")).into_response();
    }
    let video_thumbnail = post.is_video().then(|| VideoThumbnail {
        post_id,
        url: post.poster(),
        error: query
            .get("thumbnail-error")
            .and_then(|code| crate::handlers::video::MediaError::from_code(code))
            .map(|error| error.message(image::MAX_IMAGE_BYTES)),
        saved: query.get("thumbnail").is_some_and(|value| value == "saved"),
        width: crate::av::THUMBNAIL_SIZE.0,
        height: crate::av::THUMBNAIL_SIZE.1,
    });
    let video_category = post.is_video().then_some(post.category);
    // Each shown cropped, cropped again from its original.
    let photos = match post.kind == post::Kind::Post && post.title.is_none() {
        true => post
            .attachments
            .iter()
            .filter(|attachment| attachment.is_image() && crate::entities::media::is_local(&attachment.url))
            .map(|attachment| (attachment.url.clone(), attachment.original_url.clone().unwrap_or_else(|| attachment.url.clone())))
            .collect(),
        false => Vec::new(),
    };
    let labels = post.labels();
    let post_max_chars = (post.kind == post::Kind::Post && post.title.is_none()).then_some(crate::settings::services().blogging_max_chars);
    render_template(&EditTemplate {
        post_max_chars,
        title: if post.is_article() {
            "Edit the article"
        } else if post.is_video() {
            "Edit the video"
        } else {
            "Edit the post"
        }
        .to_string(),
        layout: Layout::load(&state, &session).await,
        action: format!("/posts/{post_id}/edit"),
        back: format!("/posts/{post_id}"),
        body_max_chars: if post.is_article() { ARTICLE_MAX_CHARS } else { BODY_MAX_CHARS },
        title_max_chars: TITLE_MAX_CHARS,
        article_title: post.title,
        error: query
            .get("error")
            .and_then(|code| PostError::from_code(code))
            .map(PostError::message),
        federated: state.federation.is_some() && matches!(post.visibility, Visibility::Public | Visibility::Unlisted),
        video_thumbnail,
        video_category,
        photos,
        labels: Some(labels),
        body: post.body,
    })
}

pub async fn update_post(
    State(state): State<AppState>,
    session: Session,
    Path(post_id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let (pool, me, post) = match own_post(&state, &session, post_id).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let field = |name: &str| form.get(name).map(String::as_str).unwrap_or("");
    let result = async {
        // An article stays one, with a title; so do videos, episodes and
        // live streams, whose description may be empty.
        let (title, body) = match (post.is_article(), post.title.is_some()) {
            (true, _) => validate_article(field("title"), field("body")).map(|(title, body)| (Some(title), body))?,
            (false, true) => {
                let media_error = |_| PostError::TitleTooLong;
                let title = match crate::handlers::video::validate_title(field("title"), crate::entities::video::TITLE_MAX_CHARS) {
                    Err(crate::handlers::video::MediaError::NoTitle) => return Err(PostError::NoTitle),
                    title => title.map_err(media_error)?,
                };
                let body = crate::handlers::video::validate_description(field("body"), BODY_MAX_CHARS).map_err(|_| PostError::TooLong)?;
                (Some(title), body)
            }
            (false, false) => {
                let body = post::validate_body(field("body"));
                // A photo post may have no caption.
                let body = match body {
                    Err(BodyError::Empty) if !post.attachments.is_empty() => String::new(),
                    body => body?,
                };
                check_post_length(&body)?;
                (None, body)
            }
        };
        // The staff edits the posts of the team account too, see `post_select!`.
        let author_id = post.author.local().map_or(me.id, |author| author.id);
        if post.is_video() {
            let category = crate::entities::video::parse_category(field("category"));
            post::set_category(&pool, post_id, category).await.map_err(|err| {
                tracing::error!("failed to change the category of {}: {}", post_id, err);
                PostError::Server
            })?;
        }
        post::update_own(&pool, post_id, author_id, title.as_deref(), &body).await.map_err(|err| {
            tracing::error!("failed to update post {}: {}", post_id, err);
            PostError::Server
        })
    }
    .await;
    match result {
        Ok(true) => {
            save_labels(&pool, post_id, &form).await;
            if post.kind == post::Kind::Post && post.title.is_none() && crate::settings::services().blogging {
                save_mentions(&state, &pool, post_id).await;
            }
            notifier::post_updated(&state, &pool, post_id).await;
            state.previews.notify_one();
            Redirect::to(&format!("/posts/{post_id}")).into_response()
        }
        Ok(false) => Redirect::to(&format!("/posts/{post_id}/edit?error={}", PostError::Server.code())).into_response(),
        Err(error) => Redirect::to(&format!("/posts/{post_id}/edit?error={}", error.code())).into_response(),
    }
}

/// Deletes the user's own post, or any post for the site's staff.
pub async fn delete_post(
    State(state): State<AppState>,
    session: Session,
    Path(post_id): Path<i64>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let allowed = match post::find(&pool, post_id, Some(me.id)).await {
        Ok(Some(post)) => post.can_edit() || me.is_staff(),
        Ok(None) => false,
        Err(err) => {
            tracing::error!("failed to load post {}: {}", post_id, err);
            false
        }
    };
    if !allowed {
        return error_page(Layout::load(&state, &session).await, StatusCode::NOT_FOUND, "This post does not exist or isn't yours.");
    }
    let deletions = notifier::post_deleting(&state, &pool, post_id).await;
    match post::delete(&pool, post_id).await {
        Ok(_) => {
            deletions.send(&state, &pool).await;
            tracing::info!("@{} deleted post {}", me.username, post_id);
            // Not back to the post's own page, which is gone.
            let back = local_path(form.get("return"), "/your-feed");
            let back = if back.starts_with(&format!("/posts/{post_id}")) { "/your-feed".to_string() } else { back };
            Redirect::to(&back).into_response()
        }
        Err(err) => {
            tracing::error!("failed to delete post {}: {}", post_id, err);
            error_page(Layout::load(&state, &session).await, StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong, try again later.")
        }
    }
}

#[derive(Template)]
#[template(path = "tag.html")]
struct TagTemplate {
    title: String,
    layout: Layout,
    tag: String,
    posts: Vec<Post>,
    /// Whether the viewer follows it, `None` for visitors.
    following: Option<bool>,
}

/// Posts shown on a hashtag's page.
const TAG_PAGE_SIZE: i64 = 50;

/// `/tags/{tag}`: the posts with this hashtag, of here and of other servers
/// already received, like Mastodon's page of a hashtag.
pub async fn tag_page(State(state): State<AppState>, session: Session, Path(raw): Path<String>) -> Response {
    let layout = Layout::load(&state, &session).await;
    let Some(tag) = crate::hashtag::normalize(&raw) else {
        return error_page(layout, StatusCode::NOT_FOUND, "This isn't a hashtag.");
    };
    let posts = match &state.pool {
        Some(pool) => or_empty(post::by_tag(pool, layout.viewer.as_ref().map(|viewer| viewer.id), &tag, TAG_PAGE_SIZE).await),
        None => Vec::new(),
    };
    let following = match (&state.pool, &layout.viewer) {
        (Some(pool), Some(viewer)) => Some(crate::entities::hashtag_follow::is_following(pool, viewer.id, &tag).await.unwrap_or(false)),
        _ => None,
    };
    render_template(&TagTemplate { title: format!("#{tag}"), layout, tag, posts, following })
}

/// Follows (`follow`) or unfollows the hashtag: its public posts show in
/// the feed, see `entities::hashtag_follow`. Then back to `return`.
pub async fn follow_tag(
    State(state): State<AppState>,
    session: Session,
    Path((raw, action)): Path<(String, String)>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let Some(tag) = crate::hashtag::normalize(&raw) else {
        return error_page(Layout::load(&state, &session).await, StatusCode::NOT_FOUND, "This isn't a hashtag.");
    };
    let back = local_path(form.get("return"), &crate::hashtag::path(&tag));
    let result = match action.as_str() {
        "follow" => crate::entities::hashtag_follow::follow(&pool, me.id, &tag).await,
        _ => crate::entities::hashtag_follow::unfollow(&pool, me.id, &tag).await.map(|()| true),
    };
    match result {
        Ok(true) => Redirect::to(&back).into_response(),
        Ok(false) => error_page(
            Layout::load(&state, &session).await,
            StatusCode::BAD_REQUEST,
            &crate::i18n::tf("You can follow at most {} hashtags.", crate::entities::hashtag_follow::MAX_PER_USER),
        ),
        Err(err) => {
            tracing::error!("failed to change the follow of #{} by @{}: {}", tag, me.username, err);
            error_page(Layout::load(&state, &session).await, StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong, try again later.")
        }
    }
}

/// A photo of a post cropped again from its original (in the browser, see
/// `setupCropInputs`): `position` from 0, its alt text and original kept.
/// Other servers get the edited post.
pub async fn crop_photo(
    State(state): State<AppState>,
    session: Session,
    Path((post_id, position)): Path<(i64, i64)>,
    mut multipart: Multipart,
) -> Response {
    let (pool, me, post) = match own_post(&state, &session, post_id).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let back = |done: &str| Redirect::to(&format!("/posts/{post_id}/edit?{done}#photos")).into_response();
    let is_local_photo = usize::try_from(position)
        .ok()
        .and_then(|index| post.attachments.get(index))
        .is_some_and(|attachment| attachment.is_image() && crate::entities::media::is_local(&attachment.url));
    if !is_local_photo {
        return back("error=photo-format");
    }
    let photo = match image::read_image_field(&mut multipart).await {
        Ok(data) => match crate::photo::prepare(&data).await {
            Ok(photo) => photo,
            Err(_) => return back("error=photo-format"),
        },
        Err(_) => return back("error=photo-too-large"),
    };
    let author_id = post.author.local().map_or(me.id, |author| author.id);
    let Ok(url) = image::save_file(ImageOwner::User(author_id), "post", photo.extension, &photo.data).await else {
        return back("error=server");
    };
    let size = |value: u32| i32::try_from(value).ok();
    match crate::entities::media::replace_photo(&pool, post_id, position, &url, photo.media_type, size(photo.width), size(photo.height)).await {
        Ok(Some(unused)) => crate::entities::media::remove_local_files(&unused.into_iter().collect::<Vec<_>>()).await,
        Ok(None) => {
            crate::entities::media::remove_local_files(&[url]).await;
            return back("error=photo-format");
        }
        Err(err) => {
            tracing::error!("failed to crop a photo of post {}: {}", post_id, err);
            crate::entities::media::remove_local_files(&[url]).await;
            return back("error=server");
        }
    }
    // Shown as edited, and sent again to the servers that have it.
    if let Err(err) = post::mark_edited(&pool, post_id).await {
        tracing::warn!("failed to mark post {} edited: {}", post_id, err);
    }
    notifier::post_updated(&state, &pool, post_id).await;
    back("photo-saved")
}

/// The bot `user` published `settings::bot_posts_per_hour` posts (not
/// private) in the last hour (Server rules, 7). Always `false` for people.
pub(crate) async fn bot_over_limit(pool: &sqlx::PgPool, user: &crate::entities::user::User) -> bool {
    if !user.is_bot() {
        return false;
    }
    match crate::entities::user::posts_last_hour(pool, user.id).await {
        Ok(count) => count >= i64::from(crate::settings::bot_posts_per_hour()),
        Err(err) => {
            tracing::warn!("failed to count the posts of the bot @{}: {}", user.username, err);
            false
        }
    }
}

pub(crate) fn bot_limit_message() -> String {
    crate::i18n::plural(
        crate::settings::bot_posts_per_hour(),
        "A bot can publish at most one post an hour: try again later.",
        "A bot can publish at most {} posts an hour: try again later.",
    )
}

/// The error page of a bot over its limit, for the forms without a message.
pub(crate) fn bot_limit_page(layout: Layout) -> Response {
    error_page(layout, StatusCode::TOO_MANY_REQUESTS, &bot_limit_message())
}

/// The content warning and labels sent with a post's form (new or edited),
/// see `post::Labels`.
pub(crate) async fn save_labels(pool: &sqlx::PgPool, post_id: i64, fields: &HashMap<String, String>) {
    let labels = post::Labels::from_form(|name| fields.get(name).cloned());
    if let Err(err) = post::set_labels(pool, post_id, &labels).await {
        tracing::error!("failed to save the labels of post {}: {}", post_id, err);
    }
}

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

/// Pages with the post form. Sent back by the form so the author returns
/// where they posted from, only to these fixed URLs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PostPage {
    Home,
    Feed,
}

impl PostPage {
    fn as_str(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Feed => "feed",
        }
    }

    fn url(self) -> &'static str {
        match self {
            Self::Home => "/",
            Self::Feed => "/your-feed",
        }
    }

    fn parse(value: &str) -> Self {
        if value == Self::Feed.as_str() {
            Self::Feed
        } else {
            Self::Home
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
    Server,
}

impl PostError {
    const ALL: [Self; 11] = [
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
            Self::Server => "server",
        }
    }

    fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|error| error.code() == code)
    }

    fn message(self) -> String {
        match self {
            Self::Empty => "Write something or add a photo before publishing.".to_string(),
            Self::TooLong => format!("A post can be at most {BODY_MAX_CHARS} characters."),
            Self::InvalidCharacters => "The post contains characters that are not allowed.".to_string(),
            Self::InvalidVisibility => "Choose who can see the post.".to_string(),
            Self::TooManyPhotos => format!("A post can have at most {MAX_PHOTOS} photos."),
            Self::PhotoTooLarge => format!("A photo can be at most {} MB.", MAX_IMAGE_BYTES / 1024 / 1024),
            Self::PhotoFormat => "Photos must be JPEG, PNG, GIF or WebP.".to_string(),
            Self::NoTitle => "Give the article a title.".to_string(),
            Self::TitleTooLong => format!("A title can be at most {TITLE_MAX_CHARS} characters, on one line."),
            Self::ArticleTooLong => format!("An article can be at most {ARTICLE_MAX_CHARS} characters."),
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
    page: &'static str,
    error: Option<String>,
    visibilities: [Visibility; 2],
    body_max_chars: usize,
    /// `IMAGESHARING` in `configs/services.conf`.
    photos: bool,
    max_photos: usize,
    /// The Article tab, with `PUBLISHING` in `configs/services.conf`.
    articles: bool,
    /// Opened on the Article tab (`?compose=article`), after an error in it.
    article_open: bool,
    title_max_chars: usize,
    article_max_chars: usize,
}

impl Composer {
    fn new(page: PostPage, query: &HashMap<String, String>) -> Self {
        Self {
            page: page.as_str(),
            error: query
                .get("error")
                .and_then(|code| PostError::from_code(code))
                .map(PostError::message),
            visibilities: Visibility::ALL,
            body_max_chars: BODY_MAX_CHARS,
            photos: crate::settings::services().image_sharing,
            max_photos: MAX_PHOTOS,
            articles: crate::settings::services().publishing,
            article_open: crate::settings::services().publishing && query.get("compose").is_some_and(|kind| kind == "article"),
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
    render_template(&HomeTemplate {
        title: "Home".to_string(),
        layout,
        composer: Composer::new(PostPage::Home, &query),
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
    render_template(&FeedTemplate {
        title: "Feed".to_string(),
        layout,
        composer: Composer::new(PostPage::Feed, &query),
        posts,
    })
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
    sensitive: bool,
    photos: Vec<Vec<u8>>,
    /// Alt texts, in the order of the photos.
    alts: Vec<String>,
}

async fn read_post_form(multipart: &mut Multipart) -> Result<PostForm, PostError> {
    let mut form = PostForm::default();
    while let Some(field) = multipart.next_field().await.map_err(|_| PostError::PhotoTooLarge)? {
        let name = field.name().unwrap_or_default().to_string();
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
            "sensitive" => form.sensitive = !text.is_empty(),
            "alt" => form.alts.push(text),
            _ => {}
        }
    }
    Ok(form)
}

/// Cleans and saves the photos (see `photo`) in the author's folder.
async fn save_photos(user_id: i32, form: &PostForm) -> Result<Vec<NewAttachment>, PostError> {
    let mut cleaned = Vec::new();
    for data in &form.photos {
        cleaned.push(crate::photo::clean(data).map_err(|_| PostError::PhotoFormat)?);
    }
    let mut saved: Vec<NewAttachment> = Vec::new();
    for (index, photo) in cleaned.into_iter().enumerate() {
        match image::save_file(ImageOwner::User(user_id), "post", photo.extension, &photo.data).await {
            Ok(url) => saved.push(NewAttachment::new(
                url,
                photo.media_type.to_string(),
                form.alts.get(index).map(String::as_str).unwrap_or(""),
                i32::try_from(photo.width).ok(),
                i32::try_from(photo.height).ok(),
            )),
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
    let urls: Vec<String> = photos.iter().map(|photo| photo.url.clone()).collect();
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

    let result = async {
        let mut form = form?;
        if is_article {
            if !crate::settings::services().publishing {
                return Err(PostError::Server);
            }
            let (title, body) = validate_article(&form.title, &form.body)?;
            let visibility = Visibility::parse(&form.visibility).ok_or(PostError::InvalidVisibility)?;
            return post::create_article(&pool, me.id, &title, &body, visibility).await.map_err(|err| {
                tracing::error!("failed to create article: {}", err);
                PostError::Server
            });
        }
        if !crate::settings::services().image_sharing {
            form.photos.clear();
        }
        // A photo can go without a caption, like on Pixelfed.
        let body = match post::validate_body(&form.body) {
            Err(BodyError::Empty) if !form.photos.is_empty() => String::new(),
            body => body?,
        };
        let visibility = Visibility::parse(&form.visibility).ok_or(PostError::InvalidVisibility)?;
        let failed = |err: sqlx::Error| {
            tracing::error!("failed to create post: {}", err);
            PostError::Server
        };
        if form.photos.is_empty() {
            return post::create(&pool, me.id, &body, visibility, None).await.map_err(failed);
        }
        let photos = save_photos(me.id, &form).await?;
        match post::create_with_photos(&pool, me.id, &body, visibility, form.sensitive, &photos).await {
            Ok(post_id) => Ok(post_id),
            Err(err) => {
                remove_saved(&photos).await;
                Err(failed(err))
            }
        }
    }
    .await;

    if let Ok(post_id) = &result {
        notifier::post_created(&state, &pool, *post_id).await;
        state.previews.notify_one();
    }
    let location = match result {
        // An article opens on its own page, like on WriteFreely.
        Ok(post_id) if is_article => format!("/posts/{post_id}"),
        Ok(_) => page.url().to_string(),
        Err(error) if is_article => format!("{}?error={}&compose=article", page.url(), error.code()),
        Err(error) => format!("{}?error={}", page.url(), error.code()),
    };
    Redirect::to(&location).into_response()
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
    render_template(&EditTemplate {
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
        federated: state.federation.is_some() && post.visibility == Visibility::Public,
        video_thumbnail,
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
            (false, false) => (None, post::validate_body(field("body"))?),
        };
        post::update_own(&pool, post_id, me.id, title.as_deref(), &body).await.map_err(|err| {
            tracing::error!("failed to update post {}: {}", post_id, err);
            PostError::Server
        })
    }
    .await;
    match result {
        Ok(_) => {
            notifier::post_updated(&state, &pool, post_id).await;
            state.previews.notify_one();
            Redirect::to(&format!("/posts/{post_id}")).into_response()
        }
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

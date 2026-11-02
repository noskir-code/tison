//! Videos, like PeerTube (`VIDEO` in `configs/services.conf`), and live
//! streams from OBS, like Owncast (`LIVE-STREAMING`). Uploads are converted
//! by `av::jobs`; streams are received by `av::rtmp`.

use std::{collections::HashMap, path::PathBuf};

use askama::Template;
use axum::{
    Form,
    extract::{
        Multipart, Path, Query, State, WebSocketUpgrade,
        multipart::Field,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use tokio::io::AsyncWriteExt;
use tower_sessions::Session;

use crate::{
    AppState,
    av::jobs,
    entities::{
        post::{self, BODY_MAX_CHARS, Kind, MediaList, NewMedia, Post, Visibility},
        video::{self, LiveSettings},
    },
    handlers::image::{ImageOwner, MAX_IMAGE_BYTES},
    handlers::system::{Layout, error_page, render_template},
    handlers::user::signed_in_user,
};
/// `|t` and `|tf` in the templates, see `i18n`.
use crate::i18n::filters;

/// Errors of the upload and live forms, sent back as `?error=<code>`.
#[derive(Clone, Copy, Debug)]
pub enum MediaError {
    NoFile,
    TooLarge,
    ThumbnailTooLarge,
    ThumbnailFormat,
    NoTitle,
    TitleTooLong,
    TooLong,
    InvalidCharacters,
    InvalidVisibility,
    Server,
}

impl MediaError {
    const ALL: [Self; 10] = [
        Self::NoFile,
        Self::TooLarge,
        Self::ThumbnailTooLarge,
        Self::ThumbnailFormat,
        Self::NoTitle,
        Self::TitleTooLong,
        Self::TooLong,
        Self::InvalidCharacters,
        Self::InvalidVisibility,
        Self::Server,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::NoFile => "no-file",
            Self::TooLarge => "too-large",
            Self::ThumbnailTooLarge => "thumbnail-too-large",
            Self::ThumbnailFormat => "thumbnail-format",
            Self::NoTitle => "no-title",
            Self::TitleTooLong => "title-too-long",
            Self::TooLong => "too-long",
            Self::InvalidCharacters => "invalid-characters",
            Self::InvalidVisibility => "invalid-visibility",
            Self::Server => "server",
        }
    }

    pub fn message(self, max_bytes: usize) -> String {
        match self {
            Self::NoFile => "Choose a file to upload.".to_string(),
            Self::TooLarge => crate::i18n::tf("The file can be at most {} MB.", max_bytes / 1024 / 1024),
            Self::ThumbnailTooLarge => crate::i18n::tf("The thumbnail can be at most {} MB.", MAX_IMAGE_BYTES / 1024 / 1024),
            Self::ThumbnailFormat => "The thumbnail must be a JPEG, PNG, GIF or WebP picture.".to_string(),
            Self::NoTitle => "Give it a title.".to_string(),
            Self::TitleTooLong => format!("A title can be at most {} characters, on one line.", video::TITLE_MAX_CHARS),
            Self::TooLong => "The description is too long.".to_string(),
            Self::InvalidCharacters => "The text contains characters that are not allowed.".to_string(),
            Self::InvalidVisibility => "Choose who can see it.".to_string(),
            Self::Server => "Something went wrong, try again later.".to_string(),
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|error| error.code() == code)
    }

    /// The message of `?error=`, if any.
    pub fn from_query(query: &HashMap<String, String>, max_bytes: usize) -> Option<String> {
        query.get("error").and_then(|code| Self::from_code(code)).map(|error| error.message(max_bytes))
    }
}

/// A one-line title.
pub fn validate_title(raw: &str, max_chars: usize) -> Result<String, MediaError> {
    match post::validate_text(raw, max_chars) {
        Ok(title) if title.contains('\n') => Err(MediaError::TitleTooLong),
        Ok(title) => Ok(title),
        Err(post::BodyError::Empty) => Err(MediaError::NoTitle),
        Err(post::BodyError::TooLong) => Err(MediaError::TitleTooLong),
        Err(post::BodyError::InvalidCharacters) => Err(MediaError::InvalidCharacters),
    }
}

/// A description that may be empty.
pub fn validate_description(raw: &str, max_chars: usize) -> Result<String, MediaError> {
    match post::validate_text(raw, max_chars) {
        Ok(text) => Ok(text),
        Err(post::BodyError::Empty) => Ok(String::new()),
        Err(post::BodyError::TooLong) => Err(MediaError::TooLong),
        Err(post::BodyError::InvalidCharacters) => Err(MediaError::InvalidCharacters),
    }
}

/// A form with large files, written to `jobs::SOURCES_DIR` as they arrive
/// instead of in memory. The other fields are kept as text.
pub struct Upload {
    pub fields: HashMap<String, String>,
    /// By field name.
    pub files: HashMap<String, PathBuf>,
}

impl Upload {
    pub fn field(&self, name: &str) -> &str {
        self.fields.get(name).map(String::as_str).unwrap_or_default()
    }

    pub fn file(&self, name: &str) -> Option<&PathBuf> {
        self.files.get(name)
    }

    /// Removes the files, when the form is refused or they were used.
    pub async fn discard(self) {
        for file in self.files.into_values() {
            let _ = tokio::fs::remove_file(file).await;
        }
    }
}

/// `files` are the file fields and their largest size. A file too large is
/// `TooLarge`, or `ThumbnailTooLarge` for the field `thumbnail`.
pub async fn read_upload(multipart: &mut Multipart, files: &[(&str, usize)]) -> Result<Upload, MediaError> {
    let mut upload = Upload { fields: HashMap::new(), files: HashMap::new() };
    let result = async {
        while let Some(field) = multipart.next_field().await.map_err(|_| MediaError::TooLarge)? {
            let name = field.name().unwrap_or_default().to_string();
            if let Some((_, max_bytes)) = files.iter().find(|(file_field, _)| *file_field == name) {
                if !upload.files.contains_key(&name) {
                    // Kept even on failure, so `discard` removes the file.
                    let mut written = None;
                    let saved = save_field(field, *max_bytes, &mut written).await;
                    if let Some(path) = written {
                        upload.files.insert(name.clone(), path);
                    }
                    saved.map_err(|error| match (error, name.as_str()) {
                        (MediaError::TooLarge, "thumbnail") => MediaError::ThumbnailTooLarge,
                        (error, _) => error,
                    })?;
                }
                continue;
            }
            let text = field.text().await.map_err(|_| MediaError::Server)?;
            upload.fields.insert(name, text);
        }
        Ok(())
    }
    .await;
    match result {
        Ok(()) => Ok(upload),
        Err(error) => {
            upload.discard().await;
            Err(error)
        }
    }
}

/// Writes the file to `written`, left `None` for the empty field sent
/// without a file.
async fn save_field(mut field: Field<'_>, max_bytes: usize, written: &mut Option<PathBuf>) -> Result<(), MediaError> {
    tokio::fs::create_dir_all(jobs::SOURCES_DIR).await.map_err(|_| MediaError::Server)?;
    let path = PathBuf::from(jobs::SOURCES_DIR).join(format!("{}.upload", crate::handlers::session::random_part()));
    let mut file = tokio::fs::File::create(&path).await.map_err(|_| MediaError::Server)?;
    *written = Some(path.clone());
    let mut size = 0;
    while let Some(chunk) = field.chunk().await.map_err(|_| MediaError::TooLarge)? {
        size += chunk.len();
        if size > max_bytes {
            return Err(MediaError::TooLarge);
        }
        file.write_all(&chunk).await.map_err(|_| MediaError::Server)?;
    }
    file.flush().await.map_err(|_| MediaError::Server)?;
    if size == 0 {
        let _ = tokio::fs::remove_file(&path).await;
        *written = None;
    }
    Ok(())
}

fn or_empty(result: Result<Vec<Post>, sqlx::Error>, what: &str) -> Vec<Post> {
    result.unwrap_or_else(|err| {
        tracing::error!("failed to load {}: {}", what, err);
        Vec::new()
    })
}

#[derive(Template)]
#[template(path = "videos.html")]
struct VideosTemplate {
    title: String,
    layout: Layout,
    /// The viewer's uploads still converting, with their progress.
    converting: Vec<Post>,
    live_now: Vec<Post>,
    videos: Vec<Post>,
    /// `LIVE-STREAMING`: the "Go live" button.
    live_streaming: bool,
}

/// The newest videos and the streams going on, like PeerTube's "Discover".
pub async fn videos_page(State(state): State<AppState>, session: Session) -> Response {
    let layout = Layout::load(&state, &session).await;
    let Some(pool) = &state.pool else {
        return error_page(layout, StatusCode::SERVICE_UNAVAILABLE, "The database is not configured.");
    };
    let viewer_id = layout.viewer.as_ref().map(|viewer| viewer.id);
    let services = crate::settings::services();
    let live_now = match services.live_streaming {
        true => or_empty(post::media_list(pool, viewer_id, MediaList::LiveNow).await, "live streams"),
        false => Vec::new(),
    };
    let videos = match services.video {
        true => or_empty(post::media_list(pool, viewer_id, MediaList::Videos).await, "videos"),
        false => Vec::new(),
    };
    let converting = match viewer_id {
        Some(viewer_id) => or_empty(post::media_list(pool, Some(viewer_id), MediaList::ConvertingBy(viewer_id)).await, "conversions"),
        None => Vec::new(),
    };
    render_template(&VideosTemplate {
        title: "Videos".to_string(),
        layout,
        converting,
        live_now,
        videos,
        live_streaming: services.live_streaming,
    })
}

#[derive(Template)]
#[template(path = "video_upload.html")]
struct VideoUploadTemplate {
    title: String,
    layout: Layout,
    error: Option<String>,
    visibilities: [Visibility; 2],
    max_mb: usize,
    title_max_chars: usize,
    description_max_chars: usize,
}

pub async fn upload_page(State(state): State<AppState>, session: Session, Query(query): Query<HashMap<String, String>>) -> Response {
    let layout = Layout::load(&state, &session).await;
    if layout.viewer.is_none() {
        return Redirect::to("/login").into_response();
    }
    render_template(&VideoUploadTemplate {
        title: "Upload a video".to_string(),
        layout,
        error: MediaError::from_query(&query, state.max_video_bytes),
        visibilities: Visibility::ALL,
        max_mb: state.max_video_bytes / 1024 / 1024,
        title_max_chars: video::TITLE_MAX_CHARS,
        description_max_chars: BODY_MAX_CHARS,
    })
}

/// Saves the video and converts it in the background: its page says
/// "converting" until it's ready.
pub async fn upload(State(state): State<AppState>, session: Session, mut multipart: Multipart) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let files = [("video", state.max_video_bytes), ("thumbnail", MAX_IMAGE_BYTES)];
    let upload = match read_upload(&mut multipart, &files).await {
        Ok(upload) => upload,
        Err(error) => return Redirect::to(&format!("/videos/upload?error={}", error.code())).into_response(),
    };
    let checked = (|| {
        let source = upload.file("video").ok_or(MediaError::NoFile)?;
        let title = validate_title(upload.field("title"), video::TITLE_MAX_CHARS)?;
        let body = validate_description(upload.field("description"), BODY_MAX_CHARS)?;
        let visibility = Visibility::parse(upload.field("visibility")).ok_or(MediaError::InvalidVisibility)?;
        Ok::<_, MediaError>((source.to_string_lossy().into_owned(), title, body, visibility))
    })();
    let (source, title, body, visibility) = match checked {
        Ok(checked) => checked,
        Err(error) => {
            upload.discard().await;
            return Redirect::to(&format!("/videos/upload?error={}", error.code())).into_response();
        }
    };
    // The thumbnail chosen with it, made at once: the video waits for ffmpeg.
    let poster_url = match upload.file("thumbnail") {
        Some(picture) => {
            let saved = save_thumbnail(ImageOwner::User(me.id), picture).await;
            if let Some(picture) = upload.files.get("thumbnail") {
                let _ = tokio::fs::remove_file(picture).await;
            }
            match saved {
                Ok(url) => Some(url),
                Err(error) => {
                    upload.discard().await;
                    return Redirect::to(&format!("/videos/upload?error={}", error.code())).into_response();
                }
            }
        }
        None => None,
    };
    let media = NewMedia {
        kind: Kind::Video,
        title: &title,
        body: &body,
        visibility,
        sensitive: !upload.field("sensitive").is_empty(),
        podcast_id: None,
        source: &source,
        poster_url: poster_url.as_deref(),
    };
    match post::create_media(&pool, me.id, &media).await {
        Ok(post_id) => {
            tracing::info!("@{} uploaded video {}", me.username, post_id);
            jobs::enqueue(state.clone(), post_id);
            Redirect::to(&format!("/posts/{post_id}")).into_response()
        }
        Err(err) => {
            tracing::error!("failed to save video: {}", err);
            upload.discard().await;
            crate::entities::media::remove_local_files(&poster_url.into_iter().collect::<Vec<_>>()).await;
            Redirect::to(&format!("/videos/upload?error={}", MediaError::Server.code())).into_response()
        }
    }
}

/// An uploaded picture as a video's thumbnail (see `av::thumbnail`): its
/// public URL. Its metadata are removed first, like photos.
async fn save_thumbnail(owner: ImageOwner, picture: &std::path::Path) -> Result<String, MediaError> {
    let tools = crate::av::tools().ok_or(MediaError::Server)?;
    let data = tokio::fs::read(picture).await.map_err(|_| MediaError::Server)?;
    let photo = crate::photo::clean(&data).map_err(|_| MediaError::ThumbnailFormat)?;
    let cleaned = PathBuf::from(jobs::SOURCES_DIR).join(format!("{}.{}", crate::handlers::session::random_part(), photo.extension));
    tokio::fs::write(&cleaned, &photo.data).await.map_err(|_| MediaError::Server)?;
    let made = async {
        let (output, url) = crate::handlers::image::new_file(owner, "thumbnail", "jpg").await.map_err(|_| MediaError::Server)?;
        match crate::av::thumbnail(tools, &cleaned, &output).await {
            Ok(()) => Ok(url),
            Err(err) => {
                tracing::info!("refused a thumbnail: {}", err);
                let _ = tokio::fs::remove_file(&output).await;
                Err(MediaError::ThumbnailFormat)
            }
        }
    }
    .await;
    let _ = tokio::fs::remove_file(&cleaned).await;
    made
}

/// Changes the thumbnail of the author's video, from the edit page: an
/// uploaded picture, or a frame of the video again (`reset`).
pub async fn change_thumbnail(
    State(state): State<AppState>,
    session: Session,
    Path(post_id): Path<i64>,
    mut multipart: Multipart,
) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let back = |result: Result<(), MediaError>| {
        let query = match result {
            Ok(()) => "thumbnail=saved".to_string(),
            Err(error) => format!("thumbnail-error={}", error.code()),
        };
        Redirect::to(&format!("/posts/{post_id}/edit?{query}#thumbnail")).into_response()
    };
    let post = match post::find(&pool, post_id, Some(me.id)).await {
        Ok(Some(post)) if post.can_edit() && post.is_video() => post,
        _ => return error_page(Layout::load(&state, &session).await, StatusCode::NOT_FOUND, "This video does not exist or isn't yours."),
    };
    let upload = match read_upload(&mut multipart, &[("thumbnail", MAX_IMAGE_BYTES)]).await {
        Ok(upload) => upload,
        Err(error) => return back(Err(error)),
    };
    let owner = ImageOwner::User(me.id);
    let made = match (upload.file("thumbnail"), upload.field("reset").is_empty()) {
        (Some(picture), _) => save_thumbnail(owner, picture).await,
        // A frame of the converted video, like at upload.
        (None, false) => {
            let video = post.attachments.iter().find(|attachment| attachment.is_video()).map(|video| video.url.clone());
            let (Some(tools), Some(path)) = (crate::av::tools(), video.as_deref().and_then(|url| url.strip_prefix('/'))) else {
                upload.discard().await;
                return back(Err(MediaError::NoFile));
            };
            let seconds = f64::from(post.duration_seconds.unwrap_or(0));
            jobs::make_poster(tools, owner, std::path::Path::new(path), seconds).await.ok_or(MediaError::Server)
        }
        (None, true) => Err(MediaError::NoFile),
    };
    upload.discard().await;
    let url = match made {
        Ok(url) => url,
        Err(error) => return back(Err(error)),
    };
    match post::set_poster(&pool, post_id, me.id, &url).await {
        Ok(Some(old)) => {
            crate::entities::media::remove_local_files(&old.into_iter().collect::<Vec<_>>()).await;
            if !post.is_processing() {
                crate::handlers::notifier::post_updated(&state, &pool, post_id).await;
            }
            back(Ok(()))
        }
        Ok(None) => {
            crate::entities::media::remove_local_files(&[url]).await;
            back(Err(MediaError::Server))
        }
        Err(err) => {
            tracing::error!("failed to save the thumbnail of {}: {}", post_id, err);
            crate::entities::media::remove_local_files(&[url]).await;
            back(Err(MediaError::Server))
        }
    }
}

#[derive(Template)]
#[template(path = "live.html")]
struct LiveTemplate {
    title: String,
    layout: Layout,
    username: String,
    settings: LiveSettings,
    rtmp_url: String,
    error: Option<String>,
    saved: bool,
    title_max_chars: usize,
    description_max_chars: usize,
}

/// The streamer's page, like Owncast's admin: where OBS sends, the stream
/// key, and the title of the next stream.
pub async fn studio_page(State(state): State<AppState>, session: Session, Query(query): Query<HashMap<String, String>>) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let layout = Layout::load(&state, &session).await;
    let settings = match video::settings(&pool, me.id).await {
        Ok(settings) => settings,
        Err(err) => {
            tracing::error!("failed to load live settings of {}: {}", me.username, err);
            return error_page(layout, StatusCode::INTERNAL_SERVER_ERROR, "Failed to load your live settings.");
        }
    };
    render_template(&LiveTemplate {
        title: "Go live".to_string(),
        layout,
        username: me.username,
        settings,
        rtmp_url: state.rtmp_url.clone(),
        error: MediaError::from_query(&query, state.max_video_bytes),
        saved: query.contains_key("saved"),
        title_max_chars: video::TITLE_MAX_CHARS,
        description_max_chars: video::DESCRIPTION_MAX_CHARS,
    })
}

pub async fn update_studio(State(state): State<AppState>, session: Session, Form(form): Form<HashMap<String, String>>) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let field = |name: &str| form.get(name).map(String::as_str).unwrap_or_default();
    let result = async {
        // An empty title is "<name> is live".
        let title = match validate_title(field("title"), video::TITLE_MAX_CHARS) {
            Err(MediaError::NoTitle) => String::new(),
            title => title?,
        };
        let description = validate_description(field("description"), video::DESCRIPTION_MAX_CHARS)?;
        video::settings(&pool, me.id).await.map_err(|_| MediaError::Server)?;
        video::update_settings(&pool, me.id, &title, &description, !field("keep_replay").is_empty())
            .await
            .map_err(|err| {
                tracing::error!("failed to save live settings: {}", err);
                MediaError::Server
            })
    }
    .await;
    match result {
        Ok(()) => Redirect::to("/live?saved").into_response(),
        Err(error) => Redirect::to(&format!("/live?error={}", error.code())).into_response(),
    }
}

pub async fn reset_key(State(state): State<AppState>, session: Session) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    if let Err(err) = video::reset_key(&pool, me.id).await {
        tracing::error!("failed to reset the stream key of {}: {}", me.username, err);
        return Redirect::to(&format!("/live?error={}", MediaError::Server.code())).into_response();
    }
    tracing::info!("@{} reset their stream key", me.username);
    Redirect::to("/live?saved").into_response()
}

/// `/live/{username}`: their stream going on, a link to share.
pub async fn watch(State(state): State<AppState>, session: Session, Path(username): Path<String>) -> Response {
    if let Some(pool) = &state.pool
        && let Ok(Some(post_id)) = video::live_post_of(pool, &username).await
    {
        return Redirect::to(&format!("/posts/{post_id}")).into_response();
    }
    let layout = Layout::load(&state, &session).await;
    error_page(layout, StatusCode::NOT_FOUND, &format!("@{username} isn't live right now."))
}

/// Proxies close WebSockets that stay silent, usually after a minute: a
/// conversion waiting its turn sends its state again this often.
const PROGRESS_PING: std::time::Duration = std::time::Duration::from_secs(30);

/// The conversion of the author's video or episode, live like the
/// notifications: `{"state": "waiting" | "converting" | "ready" |
/// "failed", "percent": n}` each time it moves, then the socket closes.
pub async fn progress(
    State(state): State<AppState>,
    session: Session,
    headers: HeaderMap,
    Path(post_id): Path<i64>,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !crate::handlers::notification::same_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(pool) = state.pool.clone() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let Ok(Some(me)) = crate::handlers::session::signed_in(&pool, &session).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    // Only its author sees a conversion.
    let status: Option<Option<String>> = sqlx::query_scalar("SELECT media_status FROM posts WHERE id = $1 AND user_id = $2")
        .bind(post_id)
        .bind(me.id)
        .fetch_optional(&pool)
        .await
        .unwrap_or(None);
    let Some(status) = status else {
        return StatusCode::NOT_FOUND.into_response();
    };
    // Still waiting while nothing converts it (the server stopped in the
    // middle): it's queued again, like at start (see `jobs::resume`).
    if status.as_deref() == Some("processing") && jobs::watch(post_id).is_none() {
        jobs::enqueue(state.clone(), post_id);
    }
    upgrade.on_upgrade(move |socket| send_progress(socket, post_id, status))
}

async fn send_progress(mut socket: WebSocket, post_id: i64, status: Option<String>) {
    let send = |progress: jobs::Progress| Message::Text(progress.to_json().to_string().into());
    let Some(mut changes) = jobs::watch(post_id) else {
        // Finished already, or never converted.
        let progress = match status.as_deref() {
            Some("failed") => jobs::Progress::Failed,
            _ => jobs::Progress::Ready,
        };
        let _ = socket.send(send(progress)).await;
        return;
    };
    loop {
        let progress = *changes.borrow_and_update();
        if socket.send(send(progress)).await.is_err() || progress.is_finished() {
            return;
        }
        tokio::select! {
            changed = changes.changed() => {
                // The conversion was forgotten without finishing (the post
                // was deleted): its page reloads.
                if changed.is_err() && !changes.borrow().is_finished() {
                    let _ = socket.send(send(jobs::Progress::Ready)).await;
                    return;
                }
            }
            _ = tokio::time::sleep(PROGRESS_PING) => {}
            // The page closed.
            message = socket.recv() => {
                if !matches!(message, Some(Ok(_))) {
                    return;
                }
            }
        }
    }
}

//! Podcasts, like Funkwhale's channels and Castopod (`AUDIOSTREAMING` in
//! `configs/services.conf`): a cover, a description, episodes converted to
//! MP3 by `av::jobs`, and an RSS feed any podcast app can follow.

use std::collections::HashMap;

use askama::Template;
use axum::{
    extract::{Multipart, Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use sqlx::types::chrono::{DateTime, Utc};
use tower_sessions::Session;

use crate::{
    AppState,
    av::jobs,
    entities::{
        media,
        podcast::{self, CATEGORIES, NewPodcast, Podcast},
        post::{self, BODY_MAX_CHARS, Kind, MediaList, NewMedia, Post, Visibility},
    },
    handlers::{
        image::{self, ImageOwner, MAX_IMAGE_BYTES},
        notifier,
        system::{Layout, error_page, render_template},
        user::signed_in_user,
        video::{MediaError, read_upload, validate_description, validate_title},
    },
    linkify::escape,
};

#[derive(Template)]
#[template(path = "podcasts.html")]
struct PodcastsTemplate {
    title: String,
    layout: Layout,
    podcasts: Vec<Podcast>,
    episodes: Vec<Post>,
}

/// Every podcast, and the newest episodes, here and from Funkwhale and
/// other servers.
pub async fn podcasts_page(State(state): State<AppState>, session: Session) -> Response {
    let layout = Layout::load(&state, &session).await;
    let Some(pool) = &state.pool else {
        return error_page(layout, StatusCode::SERVICE_UNAVAILABLE, "The database is not configured.");
    };
    let podcasts = podcast::all(pool).await.unwrap_or_else(|err| {
        tracing::error!("failed to list podcasts: {}", err);
        Vec::new()
    });
    let viewer_id = layout.viewer.as_ref().map(|viewer| viewer.id);
    let episodes = post::media_list(pool, viewer_id, MediaList::LatestEpisodes).await.unwrap_or_else(|err| {
        tracing::error!("failed to list episodes: {}", err);
        Vec::new()
    });
    render_template(&PodcastsTemplate { title: "Podcasts".to_string(), layout, podcasts, episodes })
}

#[derive(Template)]
#[template(path = "podcast_new.html")]
struct NewPodcastTemplate {
    title: String,
    layout: Layout,
    error: Option<String>,
    categories: [&'static str; 19],
    title_max_chars: usize,
    description_max_chars: usize,
}

pub async fn new_page(State(state): State<AppState>, session: Session, Query(query): Query<HashMap<String, String>>) -> Response {
    let layout = Layout::load(&state, &session).await;
    if layout.viewer.is_none() {
        return Redirect::to("/login").into_response();
    }
    render_template(&NewPodcastTemplate {
        title: "New podcast".to_string(),
        layout,
        error: MediaError::from_query(&query, MAX_IMAGE_BYTES),
        categories: CATEGORIES,
        title_max_chars: podcast::TITLE_MAX_CHARS,
        description_max_chars: podcast::DESCRIPTION_MAX_CHARS,
    })
}

/// A language code like `en` or `pt-BR`, else `en`.
fn language(raw: &str) -> String {
    let raw = raw.trim();
    let valid = (2..=8).contains(&raw.len()) && raw.chars().all(|letter| letter.is_ascii_alphabetic() || letter == '-');
    if valid { raw.to_string() } else { "en".to_string() }
}

pub async fn create(State(state): State<AppState>, session: Session, mut multipart: Multipart) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let back = |error: MediaError| Redirect::to(&format!("/podcasts/new?error={}", error.code())).into_response();
    let upload = match read_upload(&mut multipart, &[("cover", MAX_IMAGE_BYTES)]).await {
        Ok(upload) => upload,
        Err(error) => return back(error),
    };
    let checked = (|| {
        let title = validate_title(upload.field("title"), podcast::TITLE_MAX_CHARS)?;
        let description = validate_description(upload.field("description"), podcast::DESCRIPTION_MAX_CHARS)?;
        Ok::<_, MediaError>((title, description))
    })();
    let category = CATEGORIES.into_iter().find(|category| *category == upload.field("category")).unwrap_or("Society & Culture");
    let language = language(upload.field("language"));
    let explicit = !upload.field("explicit").is_empty();
    let cover_file = upload.file("cover").cloned();
    // The cover, without its metadata like photos.
    let cover = match &cover_file {
        Some(file) => {
            let data = tokio::fs::read(file).await.unwrap_or_default();
            match crate::photo::clean(&data) {
                Ok(photo) => image::save_file(ImageOwner::User(me.id), "cover", photo.extension, &photo.data).await.ok(),
                Err(_) => {
                    upload.discard().await;
                    return back(MediaError::NoFile);
                }
            }
        }
        None => None,
    };
    upload.discard().await;
    let (title, description) = match checked {
        Ok(fields) => fields,
        Err(error) => {
            media::remove_local_files(&cover.into_iter().collect::<Vec<_>>()).await;
            return back(error);
        }
    };
    let new = NewPodcast {
        title: &title,
        description: &description,
        cover_url: cover.as_deref(),
        category,
        language: &language,
        explicit,
    };
    match podcast::create(&pool, me.id, &new).await {
        Ok(podcast_id) => {
            tracing::info!("@{} created podcast {}", me.username, podcast_id);
            Redirect::to(&format!("/podcasts/{podcast_id}")).into_response()
        }
        Err(err) => {
            tracing::error!("failed to create podcast: {}", err);
            media::remove_local_files(&cover.into_iter().collect::<Vec<_>>()).await;
            back(MediaError::Server)
        }
    }
}

#[derive(Template)]
#[template(path = "podcast.html")]
struct PodcastTemplate {
    title: String,
    layout: Layout,
    podcast: Podcast,
    episodes: Vec<Post>,
    is_owner: bool,
    feed_url: String,
    error: Option<String>,
    visibilities: [Visibility; 2],
    max_mb: usize,
    title_max_chars: usize,
    description_max_chars: usize,
    return_to: String,
}

async fn find_podcast(state: &AppState, layout: Layout, podcast_id: i64) -> Result<(Layout, Podcast), Response> {
    let Some(pool) = &state.pool else {
        return Err(error_page(layout, StatusCode::SERVICE_UNAVAILABLE, "The database is not configured."));
    };
    match podcast::find(pool, podcast_id).await {
        Ok(Some(podcast)) => Ok((layout, podcast)),
        Ok(None) => Err(error_page(layout, StatusCode::NOT_FOUND, "This podcast does not exist.")),
        Err(err) => {
            tracing::error!("failed to load podcast {}: {}", podcast_id, err);
            Err(error_page(layout, StatusCode::INTERNAL_SERVER_ERROR, "Failed to load this podcast."))
        }
    }
}

/// The podcast with its episodes, and for its author the episode form.
pub async fn podcast_page(
    State(state): State<AppState>,
    session: Session,
    Path(podcast_id): Path<i64>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let layout = Layout::load(&state, &session).await;
    let (layout, podcast) = match find_podcast(&state, layout, podcast_id).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let viewer_id = layout.viewer.as_ref().map(|viewer| viewer.id);
    let episodes = match &state.pool {
        Some(pool) => post::media_list(pool, viewer_id, MediaList::Episodes(podcast_id)).await.unwrap_or_else(|err| {
            tracing::error!("failed to list episodes of podcast {}: {}", podcast_id, err);
            Vec::new()
        }),
        None => Vec::new(),
    };
    render_template(&PodcastTemplate {
        title: podcast.title.clone(),
        is_owner: viewer_id == Some(podcast.user_id),
        layout,
        feed_url: format!("{}/podcasts/{podcast_id}/feed.xml", state.public_base_url),
        episodes,
        podcast,
        error: MediaError::from_query(&query, state.max_video_bytes),
        visibilities: Visibility::ALL,
        max_mb: state.max_video_bytes / 1024 / 1024,
        title_max_chars: crate::entities::video::TITLE_MAX_CHARS,
        description_max_chars: BODY_MAX_CHARS,
        return_to: format!("/podcasts/{podcast_id}"),
    })
}

/// A new episode, converted to MP3 in the background.
pub async fn upload_episode(
    State(state): State<AppState>,
    session: Session,
    Path(podcast_id): Path<i64>,
    mut multipart: Multipart,
) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let back = |error: MediaError| Redirect::to(&format!("/podcasts/{podcast_id}?error={}#new-episode", error.code())).into_response();
    match podcast::find(&pool, podcast_id).await {
        Ok(Some(podcast)) if podcast.user_id == me.id => {}
        _ => return error_page(Layout::load(&state, &session).await, StatusCode::NOT_FOUND, "This podcast does not exist or isn't yours."),
    }
    let upload = match read_upload(&mut multipart, &[("audio", state.max_video_bytes)]).await {
        Ok(upload) => upload,
        Err(error) => return back(error),
    };
    let checked = (|| {
        let source = upload.file("audio").ok_or(MediaError::NoFile)?;
        let title = validate_title(upload.field("title"), crate::entities::video::TITLE_MAX_CHARS)?;
        let body = validate_description(upload.field("description"), BODY_MAX_CHARS)?;
        let visibility = Visibility::parse(upload.field("visibility")).ok_or(MediaError::InvalidVisibility)?;
        Ok::<_, MediaError>((source.to_string_lossy().into_owned(), title, body, visibility))
    })();
    let (source, title, body, visibility) = match checked {
        Ok(checked) => checked,
        Err(error) => {
            upload.discard().await;
            return back(error);
        }
    };
    let media = NewMedia {
        kind: Kind::Episode,
        title: &title,
        body: &body,
        visibility,
        sensitive: false,
        podcast_id: Some(podcast_id),
        source: &source,
        poster_url: None,
    };
    match post::create_media(&pool, me.id, &media).await {
        Ok(post_id) => {
            tracing::info!("@{} uploaded episode {} of podcast {}", me.username, post_id, podcast_id);
            jobs::enqueue(state.clone(), post_id);
            Redirect::to(&format!("/podcasts/{podcast_id}")).into_response()
        }
        Err(err) => {
            tracing::error!("failed to save episode: {}", err);
            upload.discard().await;
            back(MediaError::Server)
        }
    }
}

/// The podcast and its episodes, files included.
pub async fn delete(State(state): State<AppState>, session: Session, Path(podcast_id): Path<i64>) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let podcast = match podcast::find(&pool, podcast_id).await {
        Ok(Some(podcast)) if podcast.user_id == me.id || me.is_staff() => podcast,
        _ => return error_page(Layout::load(&state, &session).await, StatusCode::NOT_FOUND, "This podcast does not exist or isn't yours."),
    };
    for episode_id in podcast::episode_ids(&pool, podcast_id).await.unwrap_or_default() {
        let deletions = notifier::post_deleting(&state, &pool, episode_id).await;
        if post::delete(&pool, episode_id).await.is_ok() {
            deletions.send(&state, &pool).await;
        }
    }
    if let Err(err) = podcast::delete(&pool, podcast_id).await {
        tracing::error!("failed to delete podcast {}: {}", podcast_id, err);
        return error_page(Layout::load(&state, &session).await, StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong, try again later.");
    }
    media::remove_local_files(&podcast.cover_url.into_iter().collect::<Vec<_>>()).await;
    tracing::info!("@{} deleted podcast {}", me.username, podcast_id);
    Redirect::to("/podcasts").into_response()
}

/// `Tue, 05 Nov 2026 10:00:00 +0000`, the dates of RSS.
fn rss_date(time: &DateTime<Utc>) -> String {
    time.format("%a, %d %b %Y %H:%M:%S +0000").to_string()
}

/// The RSS feed of a podcast, with the tags of Apple Podcasts that every
/// podcast app reads: what Funkwhale and Castopod give too.
pub async fn feed(State(state): State<AppState>, Path(podcast_id): Path<i64>) -> Response {
    let Some(pool) = &state.pool else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let podcast = match podcast::find(pool, podcast_id).await {
        Ok(Some(podcast)) => podcast,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(err) => {
            tracing::error!("failed to load podcast {}: {}", podcast_id, err);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    // What visitors see: public episodes, ready to play.
    let episodes = post::media_list(pool, None, MediaList::Episodes(podcast_id)).await.unwrap_or_default();
    let base = &state.public_base_url;
    let absolute = |url: &str| if url.starts_with('/') { format!("{base}{url}") } else { url.to_string() };
    let page = format!("{base}/podcasts/{podcast_id}");

    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str("<rss version=\"2.0\" xmlns:itunes=\"http://www.itunes.com/dtds/podcast-1.0.dtd\" xmlns:atom=\"http://www.w3.org/2005/Atom\" xmlns:content=\"http://purl.org/rss/1.0/modules/content/\">\n<channel>\n");
    xml.push_str(&format!("<title>{}</title>\n<link>{}</link>\n", escape(&podcast.title), escape(&page)));
    xml.push_str(&format!("<description>{}</description>\n", escape(&podcast.description)));
    xml.push_str(&format!("<language>{}</language>\n", escape(&podcast.language)));
    xml.push_str(&format!("<atom:link href=\"{}/feed.xml\" rel=\"self\" type=\"application/rss+xml\"/>\n", escape(&page)));
    xml.push_str(&format!("<itunes:author>{}</itunes:author>\n", escape(&podcast.author_name)));
    xml.push_str(&format!("<itunes:summary>{}</itunes:summary>\n", escape(&podcast.description)));
    if let Some(cover) = &podcast.cover_url {
        let cover = escape(&absolute(cover));
        xml.push_str(&format!("<itunes:image href=\"{cover}\"/>\n<image><url>{cover}</url><title>{}</title><link>{}</link></image>\n", escape(&podcast.title), escape(&page)));
    }
    xml.push_str(&format!("<itunes:category text=\"{}\"/>\n", escape(&podcast.category)));
    xml.push_str(&format!("<itunes:explicit>{}</itunes:explicit>\n<itunes:type>episodic</itunes:type>\n", podcast.explicit));
    xml.push_str("<generator>Tison</generator>\n");
    for episode in &episodes {
        let Some(audio) = episode.attachments.iter().find(|attachment| attachment.is_audio()) else {
            continue;
        };
        let length = match audio.url.strip_prefix('/') {
            Some(path) if media::is_local(&audio.url) => tokio::fs::metadata(path).await.map(|meta| meta.len()).unwrap_or(0),
            _ => 0,
        };
        let link = format!("{base}/posts/{}", episode.id);
        xml.push_str("<item>\n");
        xml.push_str(&format!("<title>{}</title>\n", escape(episode.title_text())));
        xml.push_str(&format!("<description>{}</description>\n", escape(&episode.body)));
        xml.push_str(&format!("<content:encoded>{}</content:encoded>\n", escape(&crate::API::activities::text_to_html(&episode.body))));
        xml.push_str(&format!("<link>{}</link>\n<guid isPermaLink=\"false\">{}</guid>\n", escape(&link), escape(&link)));
        xml.push_str(&format!("<pubDate>{}</pubDate>\n", rss_date(&episode.created_at)));
        xml.push_str(&format!(
            "<enclosure url=\"{}\" length=\"{length}\" type=\"{}\"/>\n",
            escape(&absolute(&audio.url)),
            escape(&audio.media_type)
        ));
        if let Some(seconds) = episode.duration_seconds {
            xml.push_str(&format!("<itunes:duration>{seconds}</itunes:duration>\n"));
        }
        xml.push_str(&format!("<itunes:explicit>{}</itunes:explicit>\n", podcast.explicit));
        xml.push_str("</item>\n");
    }
    xml.push_str("</channel>\n</rss>\n");
    ([(header::CONTENT_TYPE, "application/rss+xml; charset=utf-8")], xml).into_response()
}

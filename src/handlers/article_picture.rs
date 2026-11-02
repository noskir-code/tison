//! Writing an article: pictures uploaded into it (the toolbar's "Upload
//! picture" button, or a picture pasted or dropped in the text, see
//! `assets/system.js`), cleaned like the photos of posts (see `photo`): what
//! the camera wrote in them is removed. And its preview, shown under the
//! text while writing.

use axum::{
    Json,
    extract::{Multipart, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use tower_sessions::Session;

use crate::{
    AppState,
    entities::article_picture,
    handlers::{
        image::{self, IMAGE_FIELD, ImageOwner, MAX_IMAGE_BYTES},
        user::signed_in_user,
    },
};

/// `{"body": "..."}` → the article as it will be shown, in HTML: made like
/// `Post::article_html`, pictures of the web through the media proxy.
pub async fn preview(State(state): State<AppState>, session: Session, Json(form): Json<serde_json::Value>) -> Response {
    let (pool, _) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let body = form["body"].as_str().unwrap_or_default();
    if body.chars().count() > crate::entities::post::ARTICLE_MAX_CHARS {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }
    // Pictures of the web are shown through the proxy, which only serves known addresses.
    if let Err(err) = crate::entities::media::register_article_pictures(&pool, body).await {
        tracing::warn!("failed to register the pictures of a preview: {}", err);
    }
    let html = crate::markdown::to_html(body, &|address| match crate::entities::media::is_local(address) {
        true => address.to_string(),
        false => crate::entities::media::proxy_url(address),
    });
    ([(axum::http::header::CACHE_CONTROL, "no-store")], Json(json!({ "html": html }))).into_response()
}

fn refused(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

/// The picture in the field `image` → `{"url": ..., "width": ..., "height": ...}`,
/// or `{"error": "..."}` to show.
pub async fn upload(State(state): State<AppState>, session: Session, mut multipart: Multipart) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    if !crate::settings::services().image_sharing {
        return refused(StatusCode::FORBIDDEN, "Pictures are turned off on this server.");
    }
    let mut data = None;
    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some(IMAGE_FIELD) {
            // Fails when the body goes over the global `DefaultBodyLimit`.
            data = Some(field.bytes().await.map_err(|_| ()));
            break;
        }
    }
    let data = match data {
        Some(Ok(data)) if !data.is_empty() => data,
        Some(Err(())) => return refused(StatusCode::PAYLOAD_TOO_LARGE, "This picture is too big: 10 MB at most."),
        _ => return refused(StatusCode::BAD_REQUEST, "No picture was sent."),
    };
    if data.len() > MAX_IMAGE_BYTES {
        return refused(StatusCode::PAYLOAD_TOO_LARGE, "This picture is too big: 10 MB at most.");
    }
    let photo = match crate::photo::clean(&data) {
        Ok(photo) => photo,
        Err(_) => return refused(StatusCode::UNSUPPORTED_MEDIA_TYPE, "Only JPEG, PNG, GIF and WebP pictures can be added."),
    };
    let url = match image::save_file(ImageOwner::User(me.id), "article", photo.extension, &photo.data).await {
        Ok(url) => url,
        Err(err) => {
            tracing::error!("failed to save an article picture of {}: {:?}", me.username, err);
            return refused(StatusCode::INTERNAL_SERVER_ERROR, "Failed to save the picture, try again later.");
        }
    };
    if let Err(err) = article_picture::add(&pool, me.id, &url).await {
        tracing::error!("failed to register article picture {}: {}", url, err);
        crate::entities::media::remove_local_files(std::slice::from_ref(&url)).await;
        return refused(StatusCode::INTERNAL_SERVER_ERROR, "Failed to save the picture, try again later.");
    }
    Json(json!({ "url": url, "width": photo.width, "height": photo.height })).into_response()
}

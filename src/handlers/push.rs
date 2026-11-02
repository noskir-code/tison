//! Turning browser notifications on and off (see `push`), called by
//! `assets/system.js` with what the browser's `PushManager` gives.

use axum::{
    Json,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use tower_sessions::Session;

use crate::{AppState, API::base64, entities::push as subscriptions, handlers::user::signed_in_user};

/// Push services' addresses are about 200 letters.
const MAX_ENDPOINT_LENGTH: usize = 1000;

/// The server's public key, which browsers subscribe with.
pub async fn key(State(state): State<AppState>) -> Response {
    match &state.push {
        Some(push) => ([(header::CACHE_CONTROL, "no-cache")], push.public_key.clone()).into_response(),
        None => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

fn endpoint_of(body: &Value) -> Option<&str> {
    body["endpoint"].as_str().filter(|endpoint| endpoint.len() <= MAX_ENDPOINT_LENGTH)
}

/// `{"endpoint": ..., "keys": {"p256dh": ..., "auth": ...}}`, the JSON of a
/// `PushSubscription`: this browser now receives the user's notifications.
pub async fn subscribe(State(state): State<AppState>, session: Session, Json(body): Json<Value>) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let Some(push) = &state.push else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let (Some(endpoint), Some(p256dh), Some(auth)) =
        (endpoint_of(&body), body["keys"]["p256dh"].as_str(), body["keys"]["auth"].as_str())
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if let Err(err) = push.check_endpoint(endpoint) {
        tracing::info!("refused browser notifications address of {}: {}", me.username, err);
        return StatusCode::BAD_REQUEST.into_response();
    }
    // An uncompressed P-256 key, and a 16-byte secret.
    let valid_key = base64::decode_url(p256dh).is_some_and(|key| key.len() == 65 && key[0] == 4);
    let valid_auth = base64::decode_url(auth).is_some_and(|auth| auth.len() == 16);
    if !valid_key || !valid_auth {
        return StatusCode::BAD_REQUEST.into_response();
    }
    match subscriptions::subscribe(&pool, me.id, endpoint, p256dh, auth).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => {
            tracing::error!("failed to save browser notifications of {}: {}", me.username, err);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// `{"endpoint": ...}`: this browser stops receiving them.
pub async fn unsubscribe(State(state): State<AppState>, session: Session, Json(body): Json<Value>) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let Some(endpoint) = endpoint_of(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    match subscriptions::unsubscribe(&pool, me.id, endpoint).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => {
            tracing::error!("failed to remove browser notifications of {}: {}", me.username, err);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Sends a test to the user's browsers → `{"accepted": n, "browsers": n}`.
pub async fn test(State(state): State<AppState>, session: Session) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let Some(push) = &state.push else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match push.send_test(&pool, me.id).await {
        Ok((accepted, browsers)) => Json(json!({ "accepted": accepted, "browsers": browsers })).into_response(),
        Err(err) => {
            tracing::error!("failed to send a test notification to {}: {}", me.username, err);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// `{"endpoint": ...}` → `{"on": bool}`: whether this browser receives the
/// user's notifications (it may have been taken by another account since).
pub async fn status(State(state): State<AppState>, session: Session, Json(body): Json<Value>) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let Some(endpoint) = endpoint_of(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    match subscriptions::is_subscribed(&pool, me.id, endpoint).await {
        Ok(on) => Json(json!({ "on": on && state.push.is_some() })).into_response(),
        Err(err) => {
            tracing::error!("failed to check browser notifications of {}: {}", me.username, err);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

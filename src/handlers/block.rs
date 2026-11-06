//! Blocking people (Server rules, 2), see `entities::block`: from their
//! profile, and unblocking from the settings too.

use std::collections::HashMap;

use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use tower_sessions::Session;

use crate::{
    AppState,
    entities::{block, federation as store, user},
    handlers::{
        notifier,
        system::{Layout, error_page, local_path},
        user::signed_in_user,
    },
};

/// Blocks or unblocks the user `username`, then back to `return`.
async fn change_user(state: AppState, session: Session, username: String, form: HashMap<String, String>, blocked: bool) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let back = local_path(form.get("return"), &format!("/user/{username}"));
    let other = match user::find_by_username(&pool, &username).await {
        Ok(Some(other)) if other.id != me.id => other,
        Ok(_) => return error_page(Layout::load(&state, &session).await, StatusCode::NOT_FOUND, "This user does not exist."),
        Err(err) => {
            tracing::error!("failed to load @{}: {}", username, err);
            return error_page(Layout::load(&state, &session).await, StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong, try again later.");
        }
    };
    let result = if blocked { block::block_user(&pool, me.id, other.id).await } else { block::unblock_user(&pool, me.id, other.id).await };
    if let Err(err) = result {
        tracing::error!("failed to change the block of @{} by @{}: {}", other.username, me.username, err);
        return error_page(Layout::load(&state, &session).await, StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong, try again later.");
    }
    tracing::info!("@{} {} @{}", me.username, if blocked { "blocked" } else { "unblocked" }, other.username);
    Redirect::to(&back).into_response()
}

pub async fn block_user(State(state): State<AppState>, session: Session, Path(username): Path<String>, Form(form): Form<HashMap<String, String>>) -> Response {
    change_user(state, session, username, form, true).await
}

pub async fn unblock_user(State(state): State<AppState>, session: Session, Path(username): Path<String>, Form(form): Form<HashMap<String, String>>) -> Response {
    change_user(state, session, username, form, false).await
}

/// Blocks or unblocks an account of another server (a row of
/// `federation_actors`), which is told, then back to `return`.
async fn change_actor(state: AppState, session: Session, actor_id: i64, form: HashMap<String, String>, blocked: bool) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let back = local_path(form.get("return"), &format!("/remote/{actor_id}"));
    let actor = match store::actor_profile(&pool, actor_id).await {
        Ok(Some((author, _))) => store::find_actor(&pool, &author.actor_id).await.ok().flatten().map(|(actor, _)| actor),
        _ => None,
    };
    let Some(actor) = actor else {
        return error_page(Layout::load(&state, &session).await, StatusCode::NOT_FOUND, "This account is unknown here.");
    };
    let result = if blocked { block::block_actor(&pool, me.id, actor.id).await } else { block::unblock_actor(&pool, me.id, actor.id).await };
    if let Err(err) = result {
        tracing::error!("failed to change the block of {} by @{}: {}", actor.actor_id, me.username, err);
        return error_page(Layout::load(&state, &session).await, StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong, try again later.");
    }
    if let Some(federation) = state.federation.as_deref() {
        notifier::block_changed(federation, &pool, &me, &actor, blocked).await;
    }
    tracing::info!("@{} {} {}", me.username, if blocked { "blocked" } else { "unblocked" }, actor.actor_id);
    Redirect::to(&back).into_response()
}

pub async fn block_actor(State(state): State<AppState>, session: Session, Path(actor_id): Path<i64>, Form(form): Form<HashMap<String, String>>) -> Response {
    change_actor(state, session, actor_id, form, true).await
}

pub async fn unblock_actor(State(state): State<AppState>, session: Session, Path(actor_id): Path<i64>, Form(form): Form<HashMap<String, String>>) -> Response {
    change_actor(state, session, actor_id, form, false).await
}

//! Moderation notices and appeals (Server rules, enforcement), see
//! `entities::notice`: `/notices` for the person, even suspended or banned
//! (signed in for that page only, see `handlers::auth::login`), and
//! `/moderation/appeals` for the staff.

use std::collections::HashMap;

use askama::Template;
use axum::{
    Form,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use sqlx::PgPool;
use tower_sessions::Session;

use crate::{
    AppState,
    entities::{
        comment,
        notice::{self, Action, Notice},
        post,
        user::{self, User},
    },
    handlers::{
        moderation::{Section, sections},
        notifier,
        system::{Layout, error_page, render_template},
        user::signed_in_user,
    },
};
/// `|t` and `|tf` in the templates, see `i18n`.
use crate::i18n::filters;

/// A suspended or banned account that gave its password at login: it can
/// read its notices and appeal, nothing else.
pub const APPEAL_USER_KEY: &str = "appeal_user_id";

/// The person whose notices are shown: signed in, or suspended and come
/// from the login page.
async fn person(state: &AppState, session: &Session) -> Option<(PgPool, User)> {
    let pool = state.pool.clone()?;
    if let Ok((pool, me)) = signed_in_user(state, session).await {
        return Some((pool, me));
    }
    let user_id = session.get::<i32>(APPEAL_USER_KEY).await.ok().flatten()?;
    match user::find_by_id(&pool, user_id).await {
        Ok(Some(user)) if user.is_suspended() => Some((pool, user)),
        _ => None,
    }
}

#[derive(Template)]
#[template(path = "notices.html")]
struct NoticesTemplate {
    title: String,
    layout: Layout,
    user: User,
    notices: Vec<Notice>,
    message: Option<&'static str>,
    error: bool,
    text_max_chars: usize,
}

pub async fn notices_page(State(state): State<AppState>, session: Session, Query(query): Query<HashMap<String, String>>) -> Response {
    let layout = Layout::load(&state, &session).await;
    let Some((pool, user)) = person(&state, &session).await else {
        return Redirect::to("/login").into_response();
    };
    let notices = match notice::for_user(&pool, user.id).await {
        Ok(notices) => notices,
        Err(err) => {
            tracing::error!("failed to load the notices of @{}: {}", user.username, err);
            return error_page(layout, StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong, try again later.");
        }
    };
    if let Err(err) = notice::mark_read(&pool, user.id).await {
        tracing::warn!("failed to mark the notices of @{} read: {}", user.username, err);
    }
    let (message, error) = match query.get("appeal").map(String::as_str) {
        Some("sent") => (Some("Your appeal is sent: a moderator will answer it here."), false),
        Some("empty") => (Some("Explain why the decision should change."), true),
        Some(_) => (Some("This notice can't be appealed anymore."), true),
        None => (None, false),
    };
    render_template(&NoticesTemplate {
        title: "Moderation notices".to_string(),
        layout,
        user,
        notices,
        message,
        error,
        text_max_chars: notice::TEXT_MAX_CHARS,
    })
}

pub async fn appeal(State(state): State<AppState>, session: Session, Path(id): Path<i64>, Form(form): Form<HashMap<String, String>>) -> Response {
    let Some((pool, user)) = person(&state, &session).await else {
        return Redirect::to("/login").into_response();
    };
    let key = user.id.to_string();
    if !crate::security::hit(crate::security::Limit::Appeal, &key) {
        return error_page(Layout::load(&state, &session).await, StatusCode::TOO_MANY_REQUESTS, crate::security::Limit::Appeal.message());
    }

    let text = notice::clean_text(form.get("appeal").map(String::as_str).unwrap_or_default());
    if text.is_empty() {
        return Redirect::to(&format!("/notices?appeal=empty#notice-{id}")).into_response();
    }
    match notice::appeal(&pool, id, user.id, &text).await {
        Ok(true) => {
            tracing::info!("@{} appealed the moderation notice {}", user.username, id);
            Redirect::to(&format!("/notices?appeal=sent#notice-{id}")).into_response()
        }
        Ok(false) => Redirect::to(&format!("/notices?appeal=closed#notice-{id}")).into_response(),
        Err(err) => {
            tracing::error!("failed to save the appeal of @{}: {}", user.username, err);
            error_page(Layout::load(&state, &session).await, StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong, try again later.")
        }
    }
}

#[derive(Template)]
#[template(path = "moderation_appeals.html")]
struct AppealsTemplate {
    title: String,
    layout: Layout,
    sections: Vec<Section>,
    appeals: Vec<Notice>,
    message: Option<&'static str>,
    text_max_chars: usize,
}

/// The staff, else the page to show.
async fn staff(state: &AppState, session: &Session) -> Result<(PgPool, User), Response> {
    let (pool, me) = signed_in_user(state, session).await?;
    if !me.is_staff() {
        return Err(error_page(Layout::load(state, session).await, StatusCode::FORBIDDEN, "This page is for the site's staff."));
    }
    Ok((pool, me))
}

pub async fn appeals_page(State(state): State<AppState>, session: Session, Query(query): Query<HashMap<String, String>>) -> Response {
    let (pool, me) = match staff(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let appeals = notice::pending_appeals(&pool).await.unwrap_or_else(|err| {
        tracing::error!("failed to load the appeals: {}", err);
        Vec::new()
    });
    render_template(&AppealsTemplate {
        title: "Appeals".to_string(),
        sections: sections(&pool, "Appeals", &me).await,
        layout: Layout::load(&state, &session).await,
        appeals,
        message: match query.get("done").map(String::as_str) {
            Some("accepted") => Some("Accepted: the decision is reversed, and the person is told."),
            Some("accepted_erased") => Some("Accepted, and the person is told, but the content was already erased: it can't come back."),
            Some("rejected") => Some("Rejected: the person is told."),
            Some(_) => Some("This appeal was already answered."),
            None => None,
        },
        text_max_chars: notice::TEXT_MAX_CHARS,
    })
}

/// Accepts (`decision=accept`: what can be is reversed) or rejects an appeal,
/// with an answer for the person.
pub async fn answer(State(state): State<AppState>, session: Session, Path(id): Path<i64>, Form(form): Form<HashMap<String, String>>) -> Response {
    let (pool, me) = match staff(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let accepted = form.get("decision").is_some_and(|decision| decision == "accept");
    let text = form.get("answer").map(String::as_str).unwrap_or_default();
    let result = async {
        let Some(found) = notice::find(&pool, id).await? else {
            return Ok(None);
        };
        if !notice::answer(&pool, id, accepted, text, me.id).await? {
            return Ok(None);
        }
        // Reversed before the email, which links to what came back.
        let reversed = !accepted || reverse(&state, &pool, &found).await?;
        email(&state, &pool, id, true);
        Ok::<_, sqlx::Error>(Some(reversed))
    }
    .await;
    match result {
        Ok(Some(reversed)) => {
            tracing::info!("moderator @{} {} the appeal {}", me.username, if accepted { "accepted" } else { "rejected" }, id);
            let done = match (accepted, reversed) {
                (true, true) => "accepted",
                (true, false) => "accepted_erased",
                _ => "rejected",
            };
            Redirect::to(&format!("/moderation/appeals?done={done}")).into_response()
        }
        Ok(None) => Redirect::to("/moderation/appeals?done=closed").into_response(),
        Err(err) => {
            tracing::error!("failed to answer the appeal {}: {}", id, err);
            error_page(Layout::load(&state, &session).await, StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong, try again later.")
        }
    }
}

/// Undoes what a notice says was done, when it can be: a removed post or
/// comment comes back while it isn't erased yet (see
/// `settings::removed_content_days`), a warning has nothing to undo. `false`
/// when what was removed is already erased.
async fn reverse(state: &AppState, pool: &PgPool, found: &Notice) -> Result<bool, sqlx::Error> {
    match found.action {
        Action::Suspended | Action::Banned => {
            user::reinstate(pool, found.user_id).await?;
            notifier::account_status_changed(state, pool, found.user_id).await;
        }
        Action::Limited => user::set_limited(pool, found.user_id, false).await?,
        Action::MarkedSensitive => match (found.comment_id, found.post_id) {
            (Some(comment_id), _) => {
                comment::set_sensitive(pool, comment_id, false).await?;
                notifier::comment_updated(state, pool, comment_id).await;
            }
            (None, Some(post_id)) => {
                post::set_moderated_warning(pool, post_id, None).await?;
                notifier::post_updated(state, pool, post_id).await;
            }
            (None, None) => {}
        },
        Action::Removed => match (found.comment_id, found.post_id) {
            // Sent again to other servers. Some (Mastodon) remember it was
            // deleted and don't show it again.
            (Some(comment_id), _) => {
                if !comment::restore(pool, comment_id).await? {
                    return Ok(false);
                }
                notifier::comment_created(state, pool, comment_id).await;
            }
            (None, Some(post_id)) => {
                if !post::restore(pool, post_id).await? {
                    return Ok(false);
                }
                notifier::post_created(state, pool, post_id).await;
            }
            // A message, or erased since (the ids are gone with it).
            (None, None) => return Ok(false),
        },
        Action::Warning => {}
    }
    Ok(true)
}

#[derive(Template)]
#[template(path = "emails/notice.txt")]
struct NoticeEmailText<'a> {
    username: &'a str,
    notice: &'a Notice,
    answered: bool,
    link: &'a str,
}

#[derive(Template)]
#[template(path = "emails/notice.html")]
struct NoticeEmailHtml<'a> {
    site_url: &'a str,
    notice: &'a Notice,
    answered: bool,
    link: &'a str,
}

/// Emails the person a new notice, or the answer to their appeal
/// (`answered`), in their language: suspended, they may not come back to
/// the site to see it. In the background: the moderator doesn't wait.
pub fn email(state: &AppState, pool: &PgPool, notice_id: i64, answered: bool) {
    let Some(mailer) = state.mailer.clone() else {
        return;
    };
    let pool = pool.clone();
    let site_url = state.public_base_url.trim_end_matches('/').to_string();
    tokio::spawn(async move {
        let found = async {
            let Some(found) = notice::find(&pool, notice_id).await? else {
                return Ok(None);
            };
            let address = user::email_and_language(&pool, found.user_id).await?;
            Ok::<_, sqlx::Error>(address.map(|address| (found, address)))
        }
        .await;
        let (found, (address, language)) = match found {
            Ok(Some(found)) => found,
            Ok(None) => return,
            Err(err) => {
                tracing::error!("failed to load the notice {} to email: {}", notice_id, err);
                return;
            }
        };
        let lang = crate::i18n::Lang::parse(&language).unwrap_or_default();
        let link = format!("{site_url}/notices");
        let result = crate::i18n::scoped(lang, async {
            let subject = if answered {
                crate::i18n::tf("{}: your appeal was answered", crate::settings::instance_name())
            } else {
                crate::i18n::tf("{}: a moderation notice", crate::settings::instance_name())
            };
            let email = crate::mail::Email::render(
                &subject,
                &NoticeEmailText { username: &found.username, notice: &found, answered, link: &link },
                &NoticeEmailHtml { site_url: &site_url, notice: &found, answered, link: &link },
            )?;
            mailer.send(&address, email).await
        })
        .await;
        if let Err(err) = result {
            tracing::error!("failed to email the notice {}: {}", notice_id, err);
        }
    });
}

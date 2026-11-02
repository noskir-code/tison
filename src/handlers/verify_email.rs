//! Verifying the email of an account: a link sent when it's created, and
//! again on demand. Only the SHA-256 of the link's token is stored (in
//! `users.email_verification_token`): the token is random and long, a slow
//! hash like for passwords isn't needed. Opening the link verifies the
//! address, signed in or not, on any device.

use std::collections::HashMap;

use askama::Template;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use rsa::sha2::{Digest, Sha256};
use sqlx::PgPool;
use tower_sessions::Session;

use crate::{
    AppState,
    entities::user::{self, VERIFICATION_VALID_HOURS},
    handlers::{
        session::new_token,
        system::{Layout, error_page, render_template},
        user::signed_in_user,
    },
    mail::Email,
};
/// `|t` and `|tf` in the templates, see `i18n`.
use crate::i18n::filters;

#[derive(Template)]
#[template(path = "emails/verify_email.txt")]
struct VerifyEmailText<'a> {
    username: &'a str,
    link: &'a str,
    valid_hours: i32,
}

#[derive(Template)]
#[template(path = "emails/verify_email.html")]
struct VerifyEmailHtml<'a> {
    site_url: &'a str,
    username: &'a str,
    link: &'a str,
    valid_hours: i32,
}

#[derive(Template)]
#[template(path = "email_verified.html")]
struct VerifiedTemplate {
    title: String,
    layout: Layout,
    username: String,
}

fn token_hash(token: &str) -> String {
    Sha256::digest(token.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect()
}

/// What happened when asking for a link.
#[derive(Debug, PartialEq, Eq)]
pub enum Sent {
    Sent,
    /// One was sent less than `VERIFICATION_RESEND_SECONDS` ago, or it's verified.
    TooSoon,
    /// No mail server, or it failed: see the log.
    Failed,
}

/// Sends a new link to the user's email, the previous one stopping to work.
pub async fn send_link(state: &AppState, pool: &PgPool, user_id: i32) -> Sent {
    let Some(mailer) = &state.mailer else {
        return Sent::Failed;
    };
    let loaded = async {
        let Some(found) = user::find_by_id(pool, user_id).await? else {
            return Ok(None);
        };
        Ok::<_, sqlx::Error>(user::email_status(pool, user_id).await?.map(|(address, _)| (found, address)))
    };
    let (found, address) = match loaded.await {
        Ok(Some(found)) => found,
        Ok(None) => return Sent::Failed,
        Err(err) => {
            tracing::error!("failed to load the email of user {}: {}", user_id, err);
            return Sent::Failed;
        }
    };
    let (selector, validator) = new_token();
    let token = format!("{selector}{validator}");
    match user::start_verification(pool, user_id, &token_hash(&token)).await {
        Ok(true) => {}
        Ok(false) => return Sent::TooSoon,
        Err(err) => {
            tracing::error!("failed to store the email verification of user {}: {}", user_id, err);
            return Sent::Failed;
        }
    }
    let link = format!("{}/verify-email?token={token}", state.public_base_url);
    let email = Email::render(
        &crate::i18n::t("Verify your Tison email"),
        &VerifyEmailText {
            username: &found.username,
            link: &link,
            valid_hours: VERIFICATION_VALID_HOURS,
        },
        &VerifyEmailHtml {
            site_url: &state.public_base_url,
            username: &found.username,
            link: &link,
            valid_hours: VERIFICATION_VALID_HOURS,
        },
    );
    let result = match email {
        Ok(email) => mailer.send(&address, email).await,
        Err(err) => Err(err),
    };
    match result {
        Ok(()) => {
            tracing::info!("email verification link sent to user {}", user_id);
            Sent::Sent
        }
        Err(err) => {
            tracing::error!("failed to send the email verification to user {}: {}", user_id, err);
            Sent::Failed
        }
    }
}

/// A new account: its link is sent in the background, the signup goes on.
pub fn after_signup(state: &AppState, user_id: i32) {
    let Some(pool) = state.pool.clone() else {
        return;
    };
    let state = state.clone();
    // In the language of the browser that signed up.
    crate::i18n::spawn(async move {
        send_link(&state, &pool, user_id).await;
    });
}

/// The link of the email: `?token=...`.
pub async fn verify(State(state): State<AppState>, session: Session, Query(query): Query<HashMap<String, String>>) -> Response {
    let Some(pool) = &state.pool else {
        let layout = Layout::load(&state, &session).await;
        return error_page(layout, StatusCode::SERVICE_UNAVAILABLE, "The database is not configured.");
    };
    let token = query.get("token").map(|token| token.trim()).unwrap_or_default();
    let verified = match token.is_empty() {
        true => Ok(None),
        false => user::verify_email(pool, &token_hash(token)).await,
    };
    match verified {
        Ok(Some(verified)) => {
            tracing::info!("@{} verified their email", verified.username);
            // Loaded after: the banner asking to verify is gone.
            let layout = Layout::load(&state, &session).await;
            render_template(&VerifiedTemplate {
                title: "Email verified".to_string(),
                layout,
                username: verified.username,
            })
        }
        Ok(None) => {
            let layout = Layout::load(&state, &session).await;
            let message = match layout.viewer.as_ref().map(|viewer| viewer.email_verified) {
                Some(true) => "Your email is already verified.",
                _ => "This link is not valid anymore: it expired, was already used, or a newer one was sent. Ask for a new one from your settings.",
            };
            error_page(layout, StatusCode::BAD_REQUEST, message)
        }
        Err(err) => {
            tracing::error!("failed to verify an email: {}", err);
            let layout = Layout::load(&state, &session).await;
            error_page(layout, StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong, try again later.")
        }
    }
}

/// "Send the link again", from the banner or the settings: back to where it
/// was asked, with `?verification=sent`, `wait` or `failed`.
pub async fn resend(State(state): State<AppState>, session: Session, axum::Form(form): axum::Form<HashMap<String, String>>) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let outcome = match me.email_verified {
        true => "verified",
        false => match send_link(&state, &pool, me.id).await {
            Sent::Sent => "sent",
            Sent::TooSoon => "wait",
            Sent::Failed => "failed",
        },
    };
    let path = crate::handlers::system::local_path(form.get("return"), "/settings");
    // `local_path` drops the `#...`: kept when it's only a name, like `#email`.
    let fragment = form
        .get("return")
        .and_then(|back| back.split_once('#'))
        .map(|(_, fragment)| fragment)
        .filter(|fragment| !fragment.is_empty() && fragment.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'))
        .map(|fragment| format!("#{fragment}"))
        .unwrap_or_default();
    let separator = if path.contains('?') { '&' } else { '?' };
    Redirect::to(&format!("{path}{separator}verification={outcome}{fragment}")).into_response()
}

/// The message after asking again, from `?verification=`.
pub fn resend_message(value: Option<&String>) -> Option<&'static str> {
    Some(match value?.as_str() {
        "sent" => "A new link was sent: check your inbox, and your spam folder.",
        "wait" => "A link was sent less than two minutes ago: check your inbox, or wait a moment.",
        "verified" => "Your email is already verified.",
        "failed" => "The email couldn't be sent. Try again later.",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_stored_hashed() {
        let hash = token_hash("abc");
        assert_eq!(hash, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_ne!(token_hash("abd"), hash);
    }

    #[tokio::test]
    async fn emails_follow_the_language() {
        let render = || {
            let text = VerifyEmailText { username: "ana", link: "https://example.org/v", valid_hours: 48 };
            let html = VerifyEmailHtml { site_url: "https://example.org", username: "ana", link: "https://example.org/v", valid_hours: 48 };
            (text.render().unwrap(), html.render().unwrap())
        };
        let (text, html) = render();
        assert!(text.starts_with("Hello @ana,") && text.contains("within 48 hours"));
        assert!(html.contains(r#"<html lang="en">"#));
        let (text, html) = crate::i18n::scoped(crate::i18n::Lang::Fr, async move { render() }).await;
        assert!(text.starts_with("Bonjour @ana,") && text.contains("dans les 48 heures"));
        assert!(html.contains(r#"<html lang="fr">"#) && html.contains("Bienvenue sur Tison, @ana"));
    }
}

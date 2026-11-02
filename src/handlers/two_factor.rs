//! Two-factor authentication: turned on in the settings with an
//! authenticator app (a QR code, see `qr` and `totp`), then asked after the
//! password when signing in. Ten recovery codes replace the app once each.

use std::collections::HashMap;

use askama::Template;
use axum::{
    Form,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use sqlx::PgPool;
use tower_sessions::Session;

use crate::{
    AppState,
    entities::{
        two_factor::{self, TwoFactor},
        user::{self, User},
    },
    handlers::{
        auth::{hash_password_blocking, verify_password_blocking},
        session::sign_in,
        system::{Layout, error_page, render_template},
        user::signed_in_user,
    },
    mail::Email,
    qr::QrCode,
    totp,
};
/// `|t` and `|tf` in the templates, see `i18n`.
use crate::i18n::filters;

/// The secret being set up, until its first code is confirmed.
const SETUP_KEY: &str = "two_factor_setup";
/// After a right password: the user, whether to stay signed in, since when,
/// and the codes tried.
const PENDING_USER_KEY: &str = "two_factor_user";
const PENDING_REMEMBER_KEY: &str = "two_factor_remember";
const PENDING_SINCE_KEY: &str = "two_factor_since";
const PENDING_TRIES_KEY: &str = "two_factor_tries";
/// Minutes to type the code after the password.
const PENDING_MINUTES: i64 = 10;
/// Wrong codes before starting over with the password.
const MAX_TRIES: i32 = 5;
const RECOVERY_CODES: usize = 10;

/// Errors of the settings page, as `?error=`.
fn error_message(code: &str) -> Option<&'static str> {
    Some(match code {
        "password" => "Wrong password.",
        "code" => "Wrong code: check the 6 digits of your app (they change every 30 seconds).",
        "expired" => "The setup took too long: start again.",
        "server" => "Something went wrong, try again later.",
        _ => return None,
    })
}

fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

/// `abcde-fghij`: 50 random bits, easy to copy.
fn new_recovery_code() -> String {
    let letters = totp::base32_encode(&totp::new_secret()[..7]).to_lowercase();
    format!("{}-{}", &letters[..5], &letters[5..10])
}

/// What people type: without spaces, dashes or capitals.
fn normalize_recovery(code: &str) -> String {
    code.chars().filter(|letter| letter.is_ascii_alphanumeric()).collect::<String>().to_lowercase()
}

/// New recovery codes, and their hashes to store.
async fn recovery_codes() -> Option<(Vec<String>, Vec<String>)> {
    let codes: Vec<String> = (0..RECOVERY_CODES).map(|_| new_recovery_code()).collect();
    let mut hashes = Vec::with_capacity(codes.len());
    for code in &codes {
        hashes.push(hash_password_blocking(normalize_recovery(code)).await?);
    }
    Some((codes, hashes))
}

/// A code of the app, or a recovery code (used up). `true` when right.
async fn check_code(pool: &PgPool, user_id: i32, factor: &TwoFactor, typed: &str) -> bool {
    if let Some(step) = totp::verify(&factor.secret_bytes(), typed, totp::current_step(), factor.last_step) {
        return two_factor::use_step(pool, user_id, step).await.unwrap_or(false);
    }
    let recovery = normalize_recovery(typed);
    if recovery.len() != 10 {
        return false;
    }
    for (code_id, hash) in two_factor::unused_recovery(pool, user_id).await.unwrap_or_default() {
        if verify_password_blocking(recovery.clone(), Some(hash)).await {
            return two_factor::use_recovery(pool, code_id).await.unwrap_or(false);
        }
    }
    false
}

#[derive(Template)]
#[template(path = "two_factor.html")]
struct TwoFactorTemplate {
    title: String,
    layout: Layout,
    /// On: since when, recovery codes left.
    factor: Option<TwoFactor>,
    /// Being set up: the QR code (SVG) and the secret to type instead.
    setup: Option<(String, String)>,
    /// Just made: shown once.
    recovery: Vec<String>,
    error: Option<&'static str>,
    saved: Option<&'static str>,
}

fn page(layout: Layout, factor: Option<TwoFactor>, setup: Option<(String, String)>, recovery: Vec<String>, query: &HashMap<String, String>) -> Response {
    render_template(&TwoFactorTemplate {
        title: "Two-factor authentication".to_string(),
        layout,
        factor,
        setup,
        recovery,
        error: query.get("error").and_then(|code| error_message(code)),
        saved: match query.get("saved").map(String::as_str) {
            Some("off") => Some("Two-factor authentication is off."),
            _ => None,
        },
    })
}

fn back(error: &str) -> Response {
    Redirect::to(&format!("/settings/two-factor?error={error}")).into_response()
}

/// The QR code of `secret`, and the secret grouped by 4 for typing.
fn setup_view(state: &AppState, user: &User, secret: &str) -> Option<(String, String)> {
    let issuer = crate::entities::author::host_of(&state.public_base_url);
    let issuer = issuer.split(':').next().unwrap_or("Tison");
    let bytes = totp::base32_decode(secret)?;
    let qr = QrCode::encode(totp::setup_url(issuer, &user.username, &bytes).as_bytes())?;
    let grouped = secret.as_bytes().chunks(4).map(|chunk| String::from_utf8_lossy(chunk).into_owned()).collect::<Vec<_>>().join(" ");
    Some((qr.to_svg(), grouped))
}

pub async fn settings_page(State(state): State<AppState>, session: Session, Query(query): Query<HashMap<String, String>>) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let layout = Layout::load(&state, &session).await;
    let factor = match two_factor::find(&pool, me.id).await {
        Ok(factor) => factor,
        Err(err) => {
            tracing::error!("failed to load the two-factor authentication of {}: {}", me.username, err);
            return error_page(layout, StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong, try again later.");
        }
    };
    let setup = match (&factor, session.get::<String>(SETUP_KEY).await.ok().flatten()) {
        (None, Some(secret)) => setup_view(&state, &me, &secret),
        _ => None,
    };
    page(layout, factor, setup, Vec::new(), &query)
}

/// Turning it on starts with the password: a stolen session can't lock
/// its owner out.
pub async fn start(State(state): State<AppState>, session: Session, Form(form): Form<HashMap<String, String>>) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let password = form.get("password").cloned().unwrap_or_default();
    let hash = user::password_hash(&pool, me.id).await.ok().flatten();
    if !verify_password_blocking(password, hash).await {
        return back("password");
    }
    let secret = totp::base32_encode(&totp::new_secret());
    if session.insert(SETUP_KEY, secret).await.is_err() {
        return back("server");
    }
    Redirect::to("/settings/two-factor#setup").into_response()
}

/// The first code from the app: it's on, and the recovery codes are shown
/// (once: they're stored hashed).
pub async fn confirm(State(state): State<AppState>, session: Session, Form(form): Form<HashMap<String, String>>) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let Some(secret) = session.get::<String>(SETUP_KEY).await.ok().flatten() else {
        return back("expired");
    };
    let bytes = totp::base32_decode(&secret).unwrap_or_default();
    let Some(step) = totp::verify(&bytes, form.get("code").map(String::as_str).unwrap_or_default(), totp::current_step(), None) else {
        return Redirect::to("/settings/two-factor?error=code#setup").into_response();
    };
    let Some((codes, hashes)) = recovery_codes().await else {
        return back("server");
    };
    if let Err(err) = two_factor::enable(&pool, me.id, &secret, step, &hashes).await {
        tracing::error!("failed to turn on two-factor authentication for {}: {}", me.username, err);
        return back("server");
    }
    let _ = session.remove::<String>(SETUP_KEY).await;
    tracing::info!("@{} turned on two-factor authentication", me.username);
    tell_by_email(&state, &pool, &me, true).await;
    let layout = Layout::load(&state, &session).await;
    let factor = two_factor::find(&pool, me.id).await.ok().flatten();
    page(layout, factor, None, codes, &HashMap::new())
}

/// New recovery codes, with a code of the app (or a recovery code).
pub async fn new_recovery(State(state): State<AppState>, session: Session, Form(form): Form<HashMap<String, String>>) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let Ok(Some(factor)) = two_factor::find(&pool, me.id).await else {
        return back("server");
    };
    if !check_code(&pool, me.id, &factor, form.get("code").map(String::as_str).unwrap_or_default()).await {
        return back("code");
    }
    let Some((codes, hashes)) = recovery_codes().await else {
        return back("server");
    };
    if let Err(err) = two_factor::set_recovery(&pool, me.id, &hashes).await {
        tracing::error!("failed to make recovery codes for {}: {}", me.username, err);
        return back("server");
    }
    let layout = Layout::load(&state, &session).await;
    let factor = two_factor::find(&pool, me.id).await.ok().flatten();
    page(layout, factor, None, codes, &HashMap::new())
}

/// Turning it off: the password and a code.
pub async fn disable(State(state): State<AppState>, session: Session, Form(form): Form<HashMap<String, String>>) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let Ok(Some(factor)) = two_factor::find(&pool, me.id).await else {
        return back("server");
    };
    let password = form.get("password").cloned().unwrap_or_default();
    let hash = user::password_hash(&pool, me.id).await.ok().flatten();
    if !verify_password_blocking(password, hash).await {
        return back("password");
    }
    if !check_code(&pool, me.id, &factor, form.get("code").map(String::as_str).unwrap_or_default()).await {
        return back("code");
    }
    if let Err(err) = two_factor::disable(&pool, me.id).await {
        tracing::error!("failed to turn off two-factor authentication for {}: {}", me.username, err);
        return back("server");
    }
    tracing::info!("@{} turned off two-factor authentication", me.username);
    tell_by_email(&state, &pool, &me, false).await;
    Redirect::to("/settings/two-factor?saved=off").into_response()
}

#[derive(Template)]
#[template(path = "emails/two_factor.txt")]
struct TwoFactorEmailText<'a> {
    enabled: bool,
    settings_link: &'a str,
}

#[derive(Template)]
#[template(path = "emails/two_factor.html")]
struct TwoFactorEmailHtml<'a> {
    site_url: &'a str,
    enabled: bool,
    settings_link: &'a str,
}

/// An email when it's turned on or off: off is a sign of a stolen account.
async fn tell_by_email(state: &AppState, pool: &PgPool, user: &User, enabled: bool) {
    let Some(mailer) = &state.mailer else {
        return;
    };
    let address: Option<String> = sqlx::query_scalar("SELECT email FROM users WHERE id = $1")
        .bind(user.id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .flatten();
    let Some(address) = address else {
        return;
    };
    let link = format!("{}/settings/two-factor", state.public_base_url);
    let subject = if enabled { "Two-factor authentication is on" } else { "Two-factor authentication was turned off" };
    let email = Email::render(
        &crate::i18n::t(subject),
        &TwoFactorEmailText { enabled, settings_link: &link },
        &TwoFactorEmailHtml { site_url: &state.public_base_url, enabled, settings_link: &link },
    );
    let result = match email {
        Ok(email) => mailer.send(&address, email).await,
        Err(err) => Err(err),
    };
    if let Err(err) = result {
        tracing::error!("failed to send the two-factor email to user {}: {}", user.id, err);
    }
}

/*
Signing in
*/

/// After the right password of a user with two-factor authentication: the
/// code is asked before signing in.
pub async fn ask_code(session: &Session, user_id: i32, stay_signed_in: bool) -> Response {
    let saved = async {
        session.cycle_id().await?;
        session.insert(PENDING_USER_KEY, user_id).await?;
        session.insert(PENDING_REMEMBER_KEY, stay_signed_in).await?;
        session.insert(PENDING_SINCE_KEY, now_seconds()).await?;
        session.insert(PENDING_TRIES_KEY, 0).await
    }
    .await;
    match saved {
        Ok(()) => Redirect::to("/login/two-factor").into_response(),
        Err(err) => {
            tracing::error!("failed to start the two-factor step: {}", err);
            Redirect::to("/login").into_response()
        }
    }
}

async fn forget_pending(session: &Session) {
    for key in [PENDING_USER_KEY, PENDING_REMEMBER_KEY, PENDING_SINCE_KEY, PENDING_TRIES_KEY] {
        let _ = session.remove_value(key).await;
    }
}

/// The user waiting for their code, if not too long ago.
async fn pending_user(session: &Session) -> Option<i32> {
    let user_id = session.get::<i32>(PENDING_USER_KEY).await.ok().flatten()?;
    let since = session.get::<i64>(PENDING_SINCE_KEY).await.ok().flatten()?;
    if now_seconds() - since > PENDING_MINUTES * 60 {
        forget_pending(session).await;
        return None;
    }
    Some(user_id)
}

#[derive(Template)]
#[template(path = "two_factor_login.html")]
struct CodeTemplate {
    title: String,
    layout: Layout,
    error: Option<&'static str>,
}

pub async fn login_page(State(state): State<AppState>, session: Session) -> Response {
    if pending_user(&session).await.is_none() {
        return Redirect::to("/login").into_response();
    }
    render_template(&CodeTemplate {
        title: "Two-factor authentication".to_string(),
        layout: Layout::load(&state, &session).await,
        error: None,
    })
}

pub async fn login_code(State(state): State<AppState>, session: Session, Form(form): Form<HashMap<String, String>>) -> Response {
    let Some(pool) = state.pool.clone() else {
        return Redirect::to("/login").into_response();
    };
    let Some(user_id) = pending_user(&session).await else {
        return Redirect::to("/login").into_response();
    };
    let tries = session.get::<i32>(PENDING_TRIES_KEY).await.ok().flatten().unwrap_or(0) + 1;
    let _ = session.insert(PENDING_TRIES_KEY, tries).await;
    let (factor, found) = match (two_factor::find(&pool, user_id).await, user::find_by_id(&pool, user_id).await) {
        (Ok(factor), Ok(Some(found))) if !found.is_suspended() => (factor, found),
        _ => {
            forget_pending(&session).await;
            return Redirect::to("/login").into_response();
        }
    };
    let typed = form.get("code").map(String::as_str).unwrap_or_default();
    // Turned off meanwhile: the password was enough.
    let right = match &factor {
        Some(factor) => check_code(&pool, user_id, factor, typed).await,
        None => true,
    };
    if !right {
        if tries >= MAX_TRIES {
            forget_pending(&session).await;
            tracing::warn!("too many wrong two-factor codes for user {}", user_id);
            return Redirect::to("/login?two_factor=failed").into_response();
        }
        let page = render_template(&CodeTemplate {
            title: "Two-factor authentication".to_string(),
            layout: Layout::load(&state, &session).await,
            error: Some("Wrong code. Type the 6 digits your app shows now, or a recovery code."),
        });
        return (StatusCode::UNAUTHORIZED, page).into_response();
    }
    let stay_signed_in = session.get::<bool>(PENDING_REMEMBER_KEY).await.ok().flatten().unwrap_or(false);
    forget_pending(&session).await;
    if let Err(err) = sign_in(&session, &found).await {
        tracing::error!("failed to sign in user {}: {}", user_id, err);
        return Redirect::to("/login").into_response();
    }
    crate::handlers::auth::signed_in_redirect(&state, user_id, stay_signed_in, "/").await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_codes_are_easy_to_type() {
        let code = new_recovery_code();
        assert_eq!(code.len(), 11);
        assert_eq!(&code[5..6], "-");
        assert_eq!(normalize_recovery(&code.to_uppercase().replace('-', " ")), normalize_recovery(&code));
        assert_ne!(new_recovery_code(), code);
    }
}

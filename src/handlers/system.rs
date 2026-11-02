use askama::Template;
use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use tower_sessions::Session;

use crate::{
    AppState,
    entities::{notification, site_page, user::User},
    handlers::session::signed_in,
};
/// `|t` and `|tf` in the templates, see `i18n`.
use crate::i18n::filters;

/// Data needed by `base.html` on every page.
#[derive(Default)]
pub struct Layout {
    /// Signed-in user, `None` for visitors.
    pub viewer: Option<User>,
    /// Shown next to "Notifications" in the menu, 0 for visitors.
    pub unread_notifications: i64,
    /// Conversations with an unread message, next to "Messenger".
    pub unread_messages: i64,
    /// Open reports, next to "Moderation": the site's staff only.
    pub open_reports: i64,
    /// Kept in the search bar of every page, set by the search page.
    pub search: String,
    /// The viewer's email isn't verified yet, and a link can be sent: a
    /// banner asks to, see `handlers::verify_email`.
    pub email_unverified: bool,
}

impl Layout {
    pub async fn load(state: &AppState, session: &Session) -> Self {
        let Some(pool) = &state.pool else {
            return Self::default();
        };
        let viewer = match signed_in(pool, session).await {
            Ok(viewer) => viewer,
            Err(err) => {
                tracing::warn!("failed to load signed-in user: {}", err);
                return Self::default();
            }
        };
        let unread_messages = match &viewer {
            Some(viewer) if crate::settings::services().dfrn => crate::entities::message::unread_count(pool, viewer.id).await.unwrap_or_else(|err| {
                tracing::warn!("failed to count unread messages: {}", err);
                0
            }),
            _ => 0,
        };
        let open_reports = match &viewer {
            Some(viewer) if viewer.is_staff() => crate::entities::report::open_count(pool).await.unwrap_or_else(|err| {
                tracing::warn!("failed to count reports: {}", err);
                0
            }),
            _ => 0,
        };
        let unread_notifications = match &viewer {
            Some(viewer) => notification::unread_count(pool, viewer.id).await.unwrap_or_else(|err| {
                tracing::warn!("failed to count notifications: {}", err);
                0
            }),
            None => 0,
        };
        let email_unverified = state.mailer.is_some() && viewer.as_ref().is_some_and(|viewer| !viewer.email_verified);
        Self {
            viewer,
            unread_notifications,
            unread_messages,
            open_reports,
            search: String::new(),
            email_unverified,
        }
    }

    pub fn unread_messages_label(&self) -> String {
        match self.unread_messages {
            count if count > 99 => "99+".to_string(),
            count => count.to_string(),
        }
    }

    /// The count as shown in the menu: "99+" past 99.
    pub fn unread_label(&self) -> String {
        match self.unread_notifications {
            count if count > 99 => "99+".to_string(),
            count => count.to_string(),
        }
    }
}

#[derive(Template)]
#[template(path = "error.html")]
struct ErrorTemplate<'a> {
    title: String,
    layout: Layout,
    message: &'a str,
}

/// Error page keeping the site layout.
pub fn error_page(layout: Layout, status: StatusCode, message: &str) -> Response {
    let template = ErrorTemplate {
        title: status.canonical_reason().unwrap_or("Error").to_string(),
        layout,
        message,
    };
    (status, render_template(&template)).into_response()
}

/// Added to the addresses of `style.css` and `system.js` (`?v=`): a new
/// one at each start, so browsers load the new files instead of their copy.
pub fn assets_version() -> &'static str {
    static VERSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    VERSION.get_or_init(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs().to_string())
            .unwrap_or_default()
    })
}

pub fn render_template<T: Template>(template: &T) -> Response {
    match template.render() {
        Ok(html) => Html(html).into_response(),
        Err(err) => {
            tracing::error!("template rendering failed: {}", err);
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
        }
    }
}

/// `raw` without its `#fragment` if it's a path on this site, else `fallback`.
/// Used for the "return to" field of forms, so a forged form can't send
/// users to another website (`//evil.example` or `https://...`).
pub fn local_path(raw: Option<&String>, fallback: &str) -> String {
    let path = raw
        .map(|raw| raw.split('#').next().unwrap_or_default())
        .unwrap_or_default();
    let safe = path.starts_with('/')
        && !path.starts_with("//")
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && path.len() <= 512;
    if safe { path.to_string() } else { fallback.to_string() }
}

/// Any route that does not exist.
pub async fn not_found(State(state): State<AppState>, session: Session) -> Response {
    let layout = Layout::load(&state, &session).await;
    error_page(layout, StatusCode::NOT_FOUND, "This page does not exist.")
}

#[derive(Template)]
#[template(path = "site_page.html")]
struct SitePageTemplate {
    title: String,
    layout: Layout,
    page: site_page::SitePage,
    /// The page's file as HTML, see `SitePage::html`.
    html: String,
    can_edit: bool,
}

/// Terms of Use, Privacy Policy, Cookies or Server rules: a file of `legal/`
/// (see `entities::site_page`), written by the creator.
pub async fn site_page(State(state): State<AppState>, session: Session, uri: axum::http::Uri) -> Response {
    let layout = Layout::load(&state, &session).await;
    let Some(kind) = site_page::Kind::parse(uri.path().trim_start_matches('/')) else {
        return error_page(layout, StatusCode::NOT_FOUND, "This page doesn't exist.");
    };
    let page = site_page::read(kind);
    render_template(&SitePageTemplate {
        title: kind.title().to_string(),
        html: page.html(&state.instance_name, &state.public_base_url),
        can_edit: layout.viewer.as_ref().is_some_and(User::is_admin),
        layout,
        page,
    })
}

/// The language picker of the menu: `lang` kept a year in a cookie, then
/// back to the page it was chosen on (from `Referer`, this site only).
pub async fn set_language(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    axum::Form(form): axum::Form<std::collections::HashMap<String, String>>,
) -> Response {
    let back = headers
        .get(axum::http::header::REFERER)
        .and_then(|referer| referer.to_str().ok())
        .and_then(|referer| referer.strip_prefix(state.public_base_url.trim_end_matches('/')).or(Some(referer)))
        .map(str::to_string);
    let back = local_path(back.as_ref(), "/");
    let mut response = axum::response::Redirect::to(&back).into_response();
    if let Some(lang) = form.get("lang").and_then(|lang| crate::i18n::Lang::parse(lang)) {
        let secure = if state.secure_cookies { "; Secure" } else { "" };
        let cookie = format!("lang={}; Max-Age=31536000; Path=/; SameSite=Lax{secure}", lang.code());
        if let Ok(value) = axum::http::HeaderValue::from_str(&cookie) {
            response.headers_mut().append(axum::http::header::SET_COOKIE, value);
        }
    }
    response
}

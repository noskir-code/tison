//! Reports and the moderation dashboard.
//!
//! Users report a post, a comment or an account (`/report?post=12`,
//! `?comment=`, `?user=`, `?remote=`). Reports also come from other servers
//! (Fediverse `Flag`, see `API::activities`). The site's staff (`moderator`
//! and `admin` roles) handle them on `/moderation`: delete what was
//! reported, suspend a local user, block an account of another server, or
//! close the report. The admin, shown as the site's creator, also names the
//! moderators (`/moderation/staff`).

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
        author::Author,
        comment,
        federation as store,
        post,
        report::{self, COMMENT_MAX_CHARS, NewReport, Reason, Report, Status, Target},
        user::{self, Role, User},
    },
    handlers::{
        notifier,
        system::{Layout, error_page, render_template},
        user::signed_in_user,
    },
};

// --- Reporting ---

#[derive(Template)]
#[template(path = "report.html")]
struct ReportTemplate {
    title: String,
    layout: Layout,
    /// The form's hidden field, like `post=12`.
    target_field: (&'static str, String),
    /// "Post by @bob", "@bob@mastodon.social"...
    target_label: String,
    /// The reported text, empty for an account.
    excerpt: String,
    reasons: [Reason; 6],
    comment_max_chars: usize,
    error: Option<String>,
    done: bool,
}

/// What `?post=`, `?comment=`, `?user=` or `?remote=` points to, as the
/// viewer can see it. `Err` with the page to show otherwise.
async fn resolve_target(pool: &PgPool, me: &User, query: &HashMap<String, String>) -> Result<(NewReport, (&'static str, String)), &'static str> {
    let number = |name: &str| query.get(name).and_then(|value| value.parse::<i64>().ok());
    let account_of = |author: &Author| match author {
        Author::Local(user) => Target::User(user.id),
        Author::Remote(remote) => Target::Remote(remote.id),
    };
    let new = |target, account, snapshot: &str, label: String| NewReport {
        target,
        account,
        snapshot: report::snapshot(snapshot),
        target_label: label,
        reason: Reason::Other,
        comment: String::new(),
    };
    if let Some(post_id) = number("post") {
        let post = post::find(pool, post_id, Some(me.id)).await.ok().flatten().ok_or("This post does not exist.")?;
        if post.is_own {
            return Err("You can't report your own post.");
        }
        let label = format!("Post by {}", post.author.handle());
        return Ok((new(Target::Post(post_id), Some(account_of(&post.author)), &post.body, label), ("post", post_id.to_string())));
    }
    if let Some(comment_id) = number("comment") {
        let (found, post_id) = comment::find_any(pool, comment_id).await.ok().flatten().ok_or("This comment does not exist.")?;
        // Only comments of posts the viewer can see.
        post::find(pool, post_id, Some(me.id)).await.ok().flatten().ok_or("This comment does not exist.")?;
        if found.author.local().is_some_and(|author| author.id == me.id) {
            return Err("You can't report your own comment.");
        }
        let label = format!("Comment by {}", found.author.handle());
        return Ok((
            new(Target::Comment(comment_id), Some(account_of(&found.author)), &found.body, label),
            ("comment", comment_id.to_string()),
        ));
    }
    if let Some(username) = query.get("user") {
        let found = user::find_by_username(pool, username).await.ok().flatten().ok_or("This user does not exist.")?;
        if found.id == me.id {
            return Err("You can't report yourself.");
        }
        let label = format!("@{}", found.username);
        return Ok((new(Target::User(found.id), None, &found.bio, label), ("user", found.username)));
    }
    if let Some(actor_id) = number("remote") {
        let (author, summary) = store::actor_profile(pool, actor_id).await.ok().flatten().ok_or("This account is unknown here.")?;
        return Ok((new(Target::Remote(actor_id), None, &summary, author.handle()), ("remote", actor_id.to_string())));
    }
    Err("Nothing to report.")
}

pub async fn report_page(
    State(state): State<AppState>,
    session: Session,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let layout = Layout::load(&state, &session).await;
    let (target, field) = match resolve_target(&pool, &me, &query).await {
        Ok(found) => found,
        Err(message) => return error_page(layout, StatusCode::NOT_FOUND, message),
    };
    render_template(&ReportTemplate {
        title: "Report".to_string(),
        layout,
        target_field: field,
        excerpt: target.snapshot,
        target_label: target.target_label,
        reasons: Reason::ALL,
        comment_max_chars: COMMENT_MAX_CHARS,
        error: query.get("error").map(|_| format!("Choose a reason, and write at most {COMMENT_MAX_CHARS} characters.")),
        done: query.contains_key("done"),
    })
}

pub async fn send_report(State(state): State<AppState>, session: Session, Form(form): Form<HashMap<String, String>>) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let (mut target, (field, value)) = match resolve_target(&pool, &me, &form).await {
        Ok(found) => found,
        Err(message) => return error_page(Layout::load(&state, &session).await, StatusCode::NOT_FOUND, message),
    };
    let back = format!("/report?{field}={}", crate::handlers::search::form_urlencode(&value));
    // Not `comment`: that's the reported comment's id.
    let comment = form.get("details").map(|details| details.trim().replace("\r\n", "\n")).unwrap_or_default();
    let reason = form.get("reason").and_then(|reason| Reason::parse(reason));
    let (Some(reason), true) = (reason, comment.chars().count() <= COMMENT_MAX_CHARS) else {
        return Redirect::to(&format!("{back}&error=1")).into_response();
    };
    target.reason = reason;
    target.comment = comment.chars().filter(|letter| !letter.is_control() || *letter == '\n').collect();
    match report::create(&pool, me.id, &target).await {
        Ok(id) => {
            tracing::info!("@{} reported {} ({})", me.username, target.target_label, id);
            Redirect::to(&format!("{back}&done=1")).into_response()
        }
        Err(err) => {
            tracing::error!("failed to save a report of @{}: {}", me.username, err);
            error_page(Layout::load(&state, &session).await, StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong, try again later.")
        }
    }
}

// --- Dashboard ---

struct StatusTab {
    label: &'static str,
    url: String,
    count: i64,
    active: bool,
}

#[derive(Template)]
#[template(path = "moderation.html")]
struct ModerationTemplate {
    title: String,
    layout: Layout,
    sections: Vec<Section>,
    status: Status,
    tabs: Vec<StatusTab>,
    reports: Vec<Report>,
    /// `?before=` of the next page, when there is one.
    next_before: Option<i64>,
    message: Option<&'static str>,
}

/// The signed-in member of the site's staff, else the page to show.
async fn staff(state: &AppState, session: &Session) -> Result<(PgPool, User), Response> {
    let (pool, me) = signed_in_user(state, session).await?;
    if !me.is_staff() {
        return Err(error_page(
            Layout::load(state, session).await,
            StatusCode::FORBIDDEN,
            "Only the site's moderators can see this page.",
        ));
    }
    Ok((pool, me))
}

pub async fn dashboard(State(state): State<AppState>, session: Session, Query(query): Query<HashMap<String, String>>) -> Response {
    let (pool, _) = match staff(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let status = Status::parse(query.get("status").map(String::as_str));
    let before = query.get("before").and_then(|id| id.parse::<i64>().ok());
    let loaded = async { Ok::<_, sqlx::Error>((report::list(&pool, status, before).await?, report::counts(&pool).await?)) }.await;
    let (reports, counts) = match loaded {
        Ok(loaded) => loaded,
        Err(err) => {
            tracing::error!("failed to load reports: {}", err);
            return error_page(Layout::load(&state, &session).await, StatusCode::INTERNAL_SERVER_ERROR, "Failed to load the reports.");
        }
    };
    let next_before = (reports.len() as i64 == report::PAGE_SIZE)
        .then(|| reports.last().map(|last| last.id))
        .flatten();
    let tabs = Status::ALL
        .into_iter()
        .zip(counts)
        .map(|(tab, count)| StatusTab {
            label: tab.label(),
            url: format!("/moderation?status={}", tab.as_str()),
            count,
            active: tab == status,
        })
        .collect();
    let message = query.get("done").map(|done| match done.as_str() {
        "deleted" => "Deleted, and the report resolved.",
        "suspended" => "The account is suspended, and the report resolved.",
        "unsuspended" => "The account is reinstated.",
        "blocked" => "The account is blocked, and the report resolved.",
        "unblocked" => "The account is unblocked.",
        "staff" => "Accounts of the staff can't be suspended here.",
        _ => "Saved.",
    });
    render_template(&ModerationTemplate {
        title: "Moderation".to_string(),
        sections: sections(&pool, "Reports").await,
        layout: Layout::load(&state, &session).await,
        status,
        tabs,
        reports,
        next_before,
        message,
    })
}

/// Acts on a report: `resolve`, `dismiss`, `reopen`, `delete` (what it's
/// about), `suspend` / `unsuspend` (a local user), `block` / `unblock` (an
/// account of another server).
pub async fn act(
    State(state): State<AppState>,
    session: Session,
    Path((id, action)): Path<(i64, String)>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let (pool, me) = match staff(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let back_status = Status::parse(form.get("status").map(String::as_str)).as_str();
    let back = |done: &str| Redirect::to(&format!("/moderation?status={back_status}&done={done}#report-{id}")).into_response();
    let (post_id, comment_id, user_id, actor_id) = match report::targets(&pool, id).await {
        Ok(Some(targets)) => targets,
        Ok(None) => return error_page(Layout::load(&state, &session).await, StatusCode::NOT_FOUND, "This report does not exist."),
        Err(err) => return failed(&state, &session, err).await,
    };
    let result: Result<&str, sqlx::Error> = async {
        Ok(match action.as_str() {
            "resolve" => {
                report::set_status(&pool, id, Status::Resolved, me.id).await?;
                "saved"
            }
            "dismiss" => {
                report::set_status(&pool, id, Status::Dismissed, me.id).await?;
                "saved"
            }
            "reopen" => {
                report::set_status(&pool, id, Status::Open, me.id).await?;
                "saved"
            }
            "delete" => {
                // Other servers are told, like when the author deletes it.
                if let Some(comment_id) = comment_id {
                    let deletions = notifier::comment_deleting(&state, &pool, comment_id).await;
                    comment::delete(&pool, comment_id).await?;
                    deletions.send(&state, &pool).await;
                } else if let Some(post_id) = post_id {
                    let deletions = notifier::post_deleting(&state, &pool, post_id).await;
                    post::delete(&pool, post_id).await?;
                    deletions.send(&state, &pool).await;
                }
                report::set_status(&pool, id, Status::Resolved, me.id).await?;
                "deleted"
            }
            "suspend" | "unsuspend" => {
                let Some(user_id) = user_id else {
                    return Ok("saved");
                };
                let target = user::find_by_id(&pool, user_id).await?;
                if target.as_ref().is_some_and(User::is_staff) {
                    return Ok("staff");
                }
                let suspend = action == "suspend";
                user::set_suspended(&pool, user_id, suspend).await?;
                if suspend {
                    report::set_status(&pool, id, Status::Resolved, me.id).await?;
                    "suspended"
                } else {
                    "unsuspended"
                }
            }
            "block" | "unblock" => {
                let Some(actor_id) = actor_id else {
                    return Ok("saved");
                };
                let block = action == "block";
                store::set_blocked(&pool, actor_id, block).await?;
                if block {
                    report::set_status(&pool, id, Status::Resolved, me.id).await?;
                    "blocked"
                } else {
                    "unblocked"
                }
            }
            _ => "saved",
        })
    }
    .await;
    match result {
        Ok(done) => {
            tracing::info!("moderator @{} did {} on report {}", me.username, action, id);
            back(done)
        }
        Err(err) => failed(&state, &session, err).await,
    }
}

async fn failed(state: &AppState, session: &Session, err: sqlx::Error) -> Response {
    tracing::error!("moderation action failed: {}", err);
    error_page(Layout::load(state, session).await, StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong, try again later.")
}

// --- Staff ---

#[derive(Template)]
#[template(path = "staff.html")]
struct StaffTemplate {
    title: String,
    layout: Layout,
    sections: Vec<Section>,
    staff: Vec<User>,
    /// The admin: can name moderators.
    can_manage: bool,
    error: Option<&'static str>,
    saved: bool,
}

/// The site's staff, and for the admin, the form naming moderators.
pub async fn staff_page(State(state): State<AppState>, session: Session, Query(query): Query<HashMap<String, String>>) -> Response {
    let (pool, me) = match staff(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let members = match user::staff(&pool).await {
        Ok(members) => members,
        Err(err) => return failed(&state, &session, err).await,
    };
    render_template(&StaffTemplate {
        title: "Staff".to_string(),
        sections: sections(&pool, "Staff").await,
        layout: Layout::load(&state, &session).await,
        staff: members,
        can_manage: me.is_admin(),
        error: query.get("error").map(|error| match error.as_str() {
            "nobody" => "Nobody has this username.",
            "suspended" => "This account is suspended: reinstate it first.",
            _ => "Something went wrong, try again later.",
        }),
        saved: query.contains_key("saved"),
    })
}

/// The admin, else the page to show: only they change the staff.
async fn admin(state: &AppState, session: &Session) -> Result<(PgPool, User), Response> {
    let (pool, me) = staff(state, session).await?;
    if !me.is_admin() {
        return Err(error_page(
            Layout::load(state, session).await,
            StatusCode::FORBIDDEN,
            "Only the creator of the site names moderators.",
        ));
    }
    Ok((pool, me))
}

/// Makes a user a moderator, by username.
pub async fn add_moderator(State(state): State<AppState>, session: Session, Form(form): Form<HashMap<String, String>>) -> Response {
    let (pool, me) = match admin(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let username = form.get("username").map(|name| name.trim().trim_start_matches('@')).unwrap_or_default();
    let found = match user::find_by_username(&pool, username).await {
        Ok(found) => found,
        Err(err) => return failed(&state, &session, err).await,
    };
    let Some(target) = found else {
        return Redirect::to("/moderation/staff?error=nobody").into_response();
    };
    if target.is_suspended() {
        return Redirect::to("/moderation/staff?error=suspended").into_response();
    }
    // The admin stays the admin.
    if target.role == Role::User {
        if let Err(err) = user::set_role(&pool, target.id, Role::Moderator).await {
            return failed(&state, &session, err).await;
        }
        tracing::info!("@{} made @{} a moderator", me.username, target.username);
    }
    Redirect::to("/moderation/staff?saved=1").into_response()
}

/// Takes the moderator role back. The admin can't be removed here.
pub async fn remove_moderator(State(state): State<AppState>, session: Session, Path(user_id): Path<i32>) -> Response {
    let (pool, me) = match admin(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    match user::find_by_id(&pool, user_id).await {
        Ok(Some(target)) if target.role == Role::Moderator => {
            if let Err(err) = user::set_role(&pool, target.id, Role::User).await {
                return failed(&state, &session, err).await;
            }
            tracing::info!("@{} removed @{} from the moderators", me.username, target.username);
        }
        Ok(_) => {}
        Err(err) => return failed(&state, &session, err).await,
    }
    Redirect::to("/moderation/staff?saved=1").into_response()
}

// --- Tabs ---

/// A tab of the moderation area: Reports, Federation, Staff.
pub struct Section {
    label: &'static str,
    url: &'static str,
    active: bool,
    /// Open reports, on the Reports tab.
    count: Option<i64>,
}

/// The tabs, `active` being the label of the current one.
async fn sections(pool: &PgPool, active: &str) -> Vec<Section> {
    let open = report::open_count(pool).await.unwrap_or(0);
    [("Reports", "/moderation"), ("Federation", "/moderation/federation"), ("Staff", "/moderation/staff")]
        .into_iter()
        .map(|(label, url)| Section {
            label,
            url,
            active: label == active,
            count: (label == "Reports" && open > 0).then_some(open),
        })
        .collect()
}

// --- Federation ---

#[derive(Template)]
#[template(path = "moderation_federation.html")]
struct FederationTemplate {
    title: String,
    layout: Layout,
    sections: Vec<Section>,
    instances: Vec<store::Instance>,
    search: String,
    message: Option<&'static str>,
    /// Off: there are no other servers to show.
    federation_on: bool,
}

/// The servers met, with what's known of them and their rule.
pub async fn federation_page(State(state): State<AppState>, session: Session, Query(query): Query<HashMap<String, String>>) -> Response {
    let (pool, _) = match staff(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let search = query.get("q").map(|search| search.trim().to_string()).unwrap_or_default();
    let instances = match store::instances(&pool, Some(&search).filter(|search| !search.is_empty()).map(String::as_str)).await {
        Ok(instances) => instances,
        Err(err) => return failed(&state, &session, err).await,
    };
    let message = query.get("done").map(|done| match done.as_str() {
        "limited" => "Limited: its posts and notifications only reach the people following its accounts.",
        "banned" => "Banned: what came from it is deleted, and nothing goes to it anymore.",
        "cleared" => "The server is treated normally again.",
        "invalid" => "Write a domain, like mastodon.social.",
        "own" => "This is the domain of this site.",
        _ => "Saved.",
    });
    render_template(&FederationTemplate {
        title: "Federation".to_string(),
        sections: sections(&pool, "Federation").await,
        layout: Layout::load(&state, &session).await,
        instances,
        search,
        message,
        federation_on: state.federation.is_some(),
    })
}

/// A domain as written by the staff: `mastodon.social`, from an address
/// too. `None` when it can't be one.
fn clean_domain(raw: &str) -> Option<String> {
    let raw = raw.trim().trim_start_matches('@');
    let domain = match raw.split_once("://") {
        Some(_) => store::host_of(raw),
        None => raw.rsplit('@').next().unwrap_or_default().split('/').next().unwrap_or_default().to_ascii_lowercase(),
    };
    let valid = domain.contains('.') || domain.contains(':');
    let valid = valid
        && domain.len() <= 253
        && domain.chars().all(|letter| letter.is_ascii_alphanumeric() || matches!(letter, '.' | '-' | ':'))
        && !domain.starts_with(['.', '-']);
    valid.then_some(domain)
}

/// Limits, bans or clears a server (`policy`: `limited`, `banned`, `none`).
pub async fn set_instance_policy(State(state): State<AppState>, session: Session, Form(form): Form<HashMap<String, String>>) -> Response {
    let (pool, me) = match staff(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let back = |done: &str| Redirect::to(&format!("/moderation/federation?done={done}")).into_response();
    let Some(domain) = form.get("domain").and_then(|domain| clean_domain(domain)) else {
        return back("invalid");
    };
    if state.federation.as_ref().is_some_and(|federation| federation.domain == domain) {
        return back("own");
    }
    let policy = form.get("policy").and_then(|policy| store::Policy::parse(policy));
    let reason: String = form
        .get("reason")
        .map(|reason| reason.trim().chars().filter(|letter| !letter.is_control()).take(500).collect())
        .unwrap_or_default();
    if let Err(err) = store::set_policy(&pool, &domain, policy, &reason, me.id).await {
        return failed(&state, &session, err).await;
    }
    tracing::info!("moderator @{} set {} to {:?}", me.username, domain, policy);
    back(match policy {
        Some(store::Policy::Limited) => "limited",
        Some(store::Policy::Banned) => "banned",
        None => "cleared",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domains_are_cleaned() {
        assert_eq!(clean_domain(" Mastodon.Social ").as_deref(), Some("mastodon.social"));
        assert_eq!(clean_domain("https://pleroma.example/users/a").as_deref(), Some("pleroma.example"));
        assert_eq!(clean_domain("@bob@misskey.io").as_deref(), Some("misskey.io"));
        assert_eq!(clean_domain("127.0.0.1:9100").as_deref(), Some("127.0.0.1:9100"));
        assert_eq!(clean_domain("localhost"), None);
        assert_eq!(clean_domain("bad domain.com"), None);
        assert_eq!(clean_domain("x.org'; DROP TABLE"), None);
    }
}

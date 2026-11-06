//! Reports and the moderation dashboard.
//!
//! Users report a post, a comment, a message they received or an account
//! (`/report?post=12`, `?comment=`, `?message=`, `?user=`, `?remote=`). Reports also come from other servers
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
        notice,
        post,
        site_page::{self, SitePage},
        report::{self, COMMENT_MAX_CHARS, NewReport, Reason, Report, Status, Target},
        statistics::{self, StatGroup},
        user::{self, Role, User},
    },
    handlers::{
        notifier,
        system::{Layout, error_page, render_template},
        user::signed_in_user,
    },
};
/// `|t` and `|tf` in the templates, see `i18n`.
use crate::i18n::filters;

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
    reasons: [Reason; 10],
    comment_max_chars: usize,
    error: Option<String>,
    done: bool,
    /// Where "Back" and "Cancel" lead without JavaScript (with it, back in
    /// the history, see `data-history-back` in system.js).
    back_url: String,
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
        if post.author.local().is_some_and(|author| author.team) {
            return Err("The posts of the moderation team can't be reported.");
        }
        let label = crate::i18n::tf("Post by {}", post.author.handle());
        return Ok((new(Target::Post(post_id), Some(account_of(&post.author)), &post.body, label), ("post", post_id.to_string())));
    }
    if let Some(comment_id) = number("comment") {
        let (found, post_id) = comment::find_any(pool, comment_id).await.ok().flatten().filter(|(found, _)| !found.removed).ok_or("This comment does not exist.")?;
        // Only comments of posts the viewer can see.
        post::find(pool, post_id, Some(me.id)).await.ok().flatten().ok_or("This comment does not exist.")?;
        if found.author.local().is_some_and(|author| author.id == me.id) {
            return Err("You can't report your own comment.");
        }
        let label = crate::i18n::tf("Comment by {}", found.author.handle());
        return Ok((
            new(Target::Comment(comment_id), Some(account_of(&found.author)), &found.body, label),
            ("comment", comment_id.to_string()),
        ));
    }
    if let Some(message_id) = number("message") {
        // Only a message of a conversation the viewer is in.
        let found = crate::entities::message::find_visible(pool, message_id, me.id).await.ok().flatten().ok_or("This message does not exist.")?;
        if found.is_mine {
            return Err("You can't report your own message.");
        }
        if found.author.local().is_some_and(|author| author.is_staff()) {
            return Err("The staff of the site can't be reported.");
        }
        let label = crate::i18n::tf("Message from {}", found.author.handle());
        return Ok((
            new(Target::Message(message_id), Some(account_of(&found.author)), &found.body, label),
            ("message", message_id.to_string()),
        ));
    }
    if let Some(username) = query.get("user") {
        let found = user::find_by_username(pool, username).await.ok().flatten().ok_or("This user does not exist.")?;
        if found.id == me.id {
            return Err("You can't report yourself.");
        }
        // The creator, the moderators and the team account.
        if found.is_staff() {
            return Err("The staff of the site can't be reported.");
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
    let back_url = match field.0 {
        "post" => format!("/posts/{}", field.1),
        "comment" => comment::post_of(&pool, field.1.parse().unwrap_or_default())
            .await
            .ok()
            .flatten()
            .map_or_else(|| "/".to_string(), |post_id| format!("/posts/{post_id}#comment-{}", field.1)),
        "message" => "/messenger".to_string(),
        "user" => format!("/user/{}", field.1),
        "remote" => format!("/remote/{}", field.1),
        _ => "/".to_string(),
    };
    render_template(&ReportTemplate {
        title: "Report".to_string(),
        layout,
        back_url,
        target_field: field,
        excerpt: target.snapshot,
        target_label: target.target_label,
        reasons: Reason::ALL,
        comment_max_chars: COMMENT_MAX_CHARS,
        error: query.get("error").map(|_| crate::i18n::tf("Choose a reason, and write at most {} characters.", COMMENT_MAX_CHARS)),
        done: query.contains_key("done"),
    })
}

pub async fn send_report(State(state): State<AppState>, session: Session, Form(form): Form<HashMap<String, String>>) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let key = me.id.to_string();
    if !crate::security::hit(crate::security::Limit::Report, &key) {
        return error_page(Layout::load(&state, &session).await, StatusCode::TOO_MANY_REQUESTS, crate::security::Limit::Report.message());
    }
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
    /// The choices of a suspension, see `SUSPENSION_DAYS`.
    suspension_days: [i32; 7],
    /// The rules a notice can name, see `entities::notice`.
    rules: [Reason; 10],
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
    let (pool, me) = match staff(&state, &session).await {
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
        "deleted" => "Removed, and the report resolved.",
        "banned" => "The account is banned, and the report resolved.",
        "suspended" => "The account is suspended, and the report resolved.",
        "unsuspended" => "The account is reinstated.",
        "report_removed" => "The report is deleted.",
        "account_deleted" => "The account and everything it posted are deleted, and the report resolved.",
        "blocked" => "The account is blocked, and the report resolved.",
        "unblocked" => "The account is unblocked.",
        "warned" => "The warning is sent, and the report resolved.",
        "marked_sensitive" => "It's behind a content warning now, and the report resolved.",
        "limited" => "The account is limited, and the report resolved.",
        "unlimited" => "The account isn't limited anymore.",
        "staff" => "Accounts of the staff can't be warned, limited, banned, suspended or deleted here.",
        _ => "Saved.",
    });
    render_template(&ModerationTemplate {
        title: "Moderation".to_string(),
        sections: sections(&pool, "Reports", &me).await,
        layout: Layout::load(&state, &session).await,
        status,
        tabs,
        reports,
        next_before,
        message,
        suspension_days: SUSPENSION_DAYS,
        rules: Reason::ALL,
    })
}

/// Acts on a report: `remove` (the report itself), `resolve`, `dismiss`,
/// `reopen`, `warn`, `sensitive` (a content warning: `warning`), `delete`
/// (what it's about), `limit` / `unlimit`, `ban`, `suspend` (for `days`),
/// `unsuspend` (reinstates) and `delete-account` (a local user), `block` /
/// `unblock` (an account of another server). The person is told what was
/// done and why (`rule`, `note`), see `entities::notice`.
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
    let report::Targets { post_id, comment_id, message_id, user_id, actor_id } = match report::targets(&pool, id).await {
        Ok(Some(targets)) => targets,
        Ok(None) => return error_page(Layout::load(&state, &session).await, StatusCode::NOT_FOUND, "This report does not exist."),
        Err(err) => return failed(&state, &session, err).await,
    };
    // The rule told to the person (the report's reason unless chosen) and
    // the moderator's note, see `entities::notice`.
    let (report_reason, snapshot) = match report::reason_and_snapshot(&pool, id).await {
        Ok(found) => found.unwrap_or((Reason::Other, String::new())),
        Err(err) => return failed(&state, &session, err).await,
    };
    let rule = form.get("rule").and_then(|rule| Reason::parse(rule)).unwrap_or(report_reason);
    let note = form.get("note").cloned().unwrap_or_default();
    // A local account, not of the staff: it gets the notice.
    let target = match user_id {
        Some(user_id) => match user::find_by_id(&pool, user_id).await {
            Ok(found) => found,
            Err(err) => return failed(&state, &session, err).await,
        },
        None => None,
    };
    // The post (and comment) it's about, for the notice's link.
    let about = match (post_id, comment_id) {
        (Some(post_id), comment_id) => Some((post_id, comment_id)),
        (None, Some(comment_id)) => comment::post_of(&pool, comment_id).await.ok().flatten().map(|post_id| (post_id, Some(comment_id))),
        (None, None) => None,
    };
    let tell = |action: notice::Action, about: Option<(i64, Option<i64>)>, until| {
        let pool = pool.clone();
        let target = target.clone();
        let note = note.clone();
        let snapshot = snapshot.clone();
        let moderator = me.id;
        let state = state.clone();
        async move {
            let Some(target) = target.filter(|target| !target.is_staff()) else {
                return Ok::<(), sqlx::Error>(());
            };
            let (post_id, comment_id) = about.map_or((None, None), |(post_id, comment_id)| (Some(post_id), comment_id));
            let new = notice::NewNotice { user_id: target.id, action, reason: rule, note: &note, post_id, comment_id, excerpt: &snapshot, until, created_by: moderator };
            let id = notice::create(&pool, &new).await?;
            // Suspended, they may not come back to see it.
            crate::handlers::notice::email(&state, &pool, id, false);
            Ok(())
        }
    };
    let result: Result<&str, sqlx::Error> = async {
        Ok(match action.as_str() {
            "remove" => {
                report::delete(&pool, id).await?;
                "report_removed"
            }
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
            // Nothing done but telling the person the rule (Server rules,
            // enforcement: "Reminder / warning").
            "warn" => {
                if target.as_ref().is_none_or(User::is_staff) {
                    return Ok("saved");
                }
                tell(notice::Action::Warning, about, None).await?;
                report::set_status(&pool, id, Status::Resolved, me.id).await?;
                "warned"
            }
            // Behind a content warning (posts) or blurred (comments), here
            // and on other servers.
            "sensitive" => {
                if let Some(comment_id) = comment_id {
                    comment::set_sensitive(&pool, comment_id, true).await?;
                    notifier::comment_updated(&state, &pool, comment_id).await;
                } else if let Some(post_id) = post_id {
                    let warning = form.get("warning").map(|warning| warning.trim()).filter(|warning| !warning.is_empty());
                    let warning = warning.map(str::to_string).unwrap_or_else(|| crate::i18n::t(rule.label()));
                    post::set_moderated_warning(&pool, post_id, Some(&warning)).await?;
                    notifier::post_updated(&state, &pool, post_id).await;
                }
                tell(notice::Action::MarkedSensitive, about, None).await?;
                report::set_status(&pool, id, Status::Resolved, me.id).await?;
                "marked_sensitive"
            }
            "delete" => {
                // A private message goes for both people. Posts and comments
                // are hidden at once and erased after
                // `settings::removed_content_days`, unless an accepted appeal
                // brings them back. Other servers are told at once, like
                // when the author deletes it.
                if let Some(message_id) = message_id {
                    crate::handlers::messenger::remove_reported(&state, &pool, message_id).await?;
                    tell(notice::Action::Removed, None, None).await?;
                } else {
                    if let Some(comment_id) = comment_id {
                        let deletions = notifier::comment_deleting(&state, &pool, comment_id).await;
                        comment::remove(&pool, comment_id).await?;
                        deletions.send(&state, &pool).await;
                    } else if let Some(post_id) = post_id {
                        let deletions = notifier::post_deleting(&state, &pool, post_id).await;
                        post::remove(&pool, post_id).await?;
                        deletions.send(&state, &pool).await;
                    }
                    // The notice keeps what was removed, to bring it back.
                    tell(notice::Action::Removed, about, None).await?;
                }
                report::set_status(&pool, id, Status::Resolved, me.id).await?;
                "deleted"
            }
            "limit" | "unlimit" => {
                let Some(target) = target.as_ref().filter(|target| !target.is_staff()) else {
                    return Ok("staff");
                };
                let limit = action == "limit";
                user::set_limited(&pool, target.id, limit).await?;
                if limit {
                    tell(notice::Action::Limited, None, None).await?;
                    report::set_status(&pool, id, Status::Resolved, me.id).await?;
                    "limited"
                } else {
                    "unlimited"
                }
            }
            "ban" | "suspend" | "unsuspend" | "delete-account" => {
                let Some(user_id) = user_id else {
                    return Ok("saved");
                };
                let Some(target) = target.clone() else {
                    return Ok("saved");
                };
                if target.is_staff() {
                    return Ok("staff");
                }
                match action.as_str() {
                    "unsuspend" => {
                        user::reinstate(&pool, user_id).await?;
                        notifier::account_status_changed(&state, &pool, user_id).await;
                        return Ok("unsuspended");
                    }
                    "ban" => {
                        user::restrict(&pool, user_id, None).await?;
                        tell(notice::Action::Banned, None, None).await?;
                    }
                    "suspend" => {
                        let days = form.get("days").and_then(|days| days.parse::<i32>().ok()).filter(|days| SUSPENSION_DAYS.contains(days));
                        let Some(days) = days else {
                            return Ok("saved");
                        };
                        user::restrict(&pool, user_id, Some(days)).await?;
                        let until = user::find_by_id(&pool, user_id).await?.and_then(|user| user.suspended_until);
                        tell(notice::Action::Suspended, None, until).await?;
                    }
                    _ => {
                        report::set_status(&pool, id, Status::Resolved, me.id).await?;
                        notifier::user_deleting(&state, &pool, &target).await;
                        user::delete(&pool, user_id).await?;
                        crate::handlers::image::remove_folder(crate::handlers::image::ImageOwner::User(user_id)).await;
                        tracing::info!("moderator @{} deleted the account @{}", me.username, target.username);
                        return Ok("account_deleted");
                    }
                }
                // Other servers hide it too, see `notifier::account_status_changed`.
                notifier::account_status_changed(&state, &pool, user_id).await;
                report::set_status(&pool, id, Status::Resolved, me.id).await?;
                if action == "ban" { "banned" } else { "suspended" }
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

/// The lengths of a suspension offered, in days; longer is a ban.
pub const SUSPENSION_DAYS: [i32; 7] = [1, 3, 7, 14, 30, 90, 365];

/// Erases, every hour, the posts and comments removed by the moderators
/// more than `settings::removed_content_days` ago, with their files: past
/// that, an appeal can't bring them back.
pub async fn run_removed_purge(pool: PgPool) {
    let mut every = tokio::time::interval(std::time::Duration::from_secs(60 * 60));
    loop {
        every.tick().await;
        let days = crate::settings::removed_content_days();
        let purged = async {
            let mut count = 0;
            for comment_id in comment::removed_before(&pool, days).await? {
                count += usize::from(comment::delete(&pool, comment_id).await?);
            }
            for post_id in post::removed_before(&pool, days).await? {
                count += usize::from(post::delete(&pool, post_id).await?);
            }
            Ok::<_, sqlx::Error>(count)
        }
        .await;
        match purged {
            Ok(0) => {}
            Ok(count) => tracing::info!("erased {} post(s) and comment(s) removed by the moderators more than {} days ago", count, days),
            Err(err) => tracing::warn!("failed to erase the removed posts and comments: {}", err),
        }
    }
}

/// Ends the suspensions whose time is over, every minute, and tells other
/// servers.
pub async fn run_suspension_ends(state: AppState, pool: PgPool) {
    let mut every = tokio::time::interval(std::time::Duration::from_secs(60));
    loop {
        every.tick().await;
        match user::end_suspensions(&pool).await {
            Ok(ended) => {
                if !ended.is_empty() {
                    tracing::info!("{} suspension(s) ended", ended.len());
                }
                for user_id in ended {
                    notifier::account_status_changed(&state, &pool, user_id).await;
                }
            }
            Err(err) => tracing::warn!("failed to end suspensions: {}", err),
        }
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
    /// The form posting as the team account, when it's on (`MULTI-MODERATOR`).
    composer: Option<crate::handlers::post::Composer>,
    /// The team account's latest posts, under its form.
    team_posts: Vec<post::Post>,
    /// A post was just published as the team.
    published: bool,
}

/// The team account's posts shown under its form in the Staff tab.
const TEAM_POSTS_SHOWN: usize = 5;

/// The site's staff, and for the admin, the form naming moderators. With
/// `MULTI-MODERATOR`, the forms writing posts and articles as the team.
pub async fn staff_page(State(state): State<AppState>, session: Session, Query(query): Query<HashMap<String, String>>) -> Response {
    let (pool, me) = match staff(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let loaded = async {
        let members = user::staff(&pool).await?;
        let team = user::team_account(&pool).await?;
        let team_posts = match &team {
            Some(team) => post::by_author(&pool, team.id, Some(me.id)).await?.into_iter().take(TEAM_POSTS_SHOWN).collect(),
            None => Vec::new(),
        };
        Ok::<_, sqlx::Error>((members, team, team_posts))
    }
    .await;
    let (members, team, team_posts) = match loaded {
        Ok(loaded) => loaded,
        Err(err) => return failed(&state, &session, err).await,
    };
    render_template(&StaffTemplate {
        title: "Staff".to_string(),
        sections: sections(&pool, "Staff", &me).await,
        layout: Layout::load(&state, &session).await,
        staff: members,
        can_manage: me.is_admin(),
        // Other codes are the team composer's, shown in it.
        error: query.get("error").and_then(|error| match error.as_str() {
            "nobody" => Some("Nobody has this username."),
            "suspended" => Some("This account is suspended: reinstate it first."),
            _ => None,
        }),
        saved: query.contains_key("saved"),
        composer: team.map(|team| crate::handlers::post::Composer::for_team(&query, team.username)),
        team_posts,
        published: query.contains_key("published"),
    })
}

/// The admin, else the page to show: only they change the staff and see the configuration.
async fn admin(state: &AppState, session: &Session) -> Result<(PgPool, User), Response> {
    let (pool, me) = staff(state, session).await?;
    if !me.is_admin() {
        return Err(error_page(
            Layout::load(state, session).await,
            StatusCode::FORBIDDEN,
            "Only the creator of the site can do this.",
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
        // The team account stays a moderator, see `user::sync_team_account`.
        Ok(Some(target)) if target.role == Role::Moderator && !target.team => {
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

/// A tab of the moderation area: Reports, Federation, Statistics, Staff, and
/// Configuration for the creator.
pub struct Section {
    pub(crate) label: &'static str,
    pub(crate) url: &'static str,
    pub(crate) active: bool,
    /// Open reports, on the Reports tab; appeals waiting, on Appeals.
    pub(crate) count: Option<i64>,
}

/// The tabs, `active` being the label of the current one.
pub(crate) async fn sections(pool: &PgPool, active: &str, me: &User) -> Vec<Section> {
    let open = report::open_count(pool).await.unwrap_or(0);
    let appeals = notice::pending_count(pool).await.unwrap_or(0);
    let relay_requests = crate::entities::relay::pending_subscribers(pool).await.unwrap_or(0);
    [
        ("Reports", "/moderation"),
        ("Appeals", "/moderation/appeals"),
        ("Users", "/moderation/users"),
        ("Federation", "/moderation/federation"),
        ("Relays", "/moderation/relays"),
        ("Statistics", "/moderation/statistics"),
        ("Staff", "/moderation/staff"),
        ("Incidents", "/moderation/incidents"),
        ("Configuration", "/moderation/configuration"),
    ]
        .into_iter()
        .filter(|(label, _)| !matches!(*label, "Configuration" | "Incidents" | "Relays") || me.is_admin())
        .map(|(label, url)| Section {
            label,
            url,
            active: label == active,
            count: match label {
                "Reports" => (open > 0).then_some(open),
                "Appeals" => (appeals > 0).then_some(appeals),
                "Relays" => (relay_requests > 0).then_some(relay_requests),
                _ => None,
            },
        })
        .collect()
}

// --- Relays ---

#[derive(Template)]
#[template(path = "moderation_relays.html")]
struct RelaysTemplate {
    title: String,
    layout: Layout,
    sections: Vec<Section>,
    relays: Vec<crate::entities::relay::Relay>,
    subscribers: Vec<crate::entities::relay::Subscriber>,
    /// `RELAY-SERVER`: other servers can follow this one.
    relay_server: bool,
    /// Where other servers subscribe: the instance actor's inbox, and its
    /// address for LitePub relays.
    inbox_url: String,
    actor_url: String,
    federation_on: bool,
    message: Option<String>,
    error: bool,
}

/// The relays followed, and the servers following this one as a relay, see
/// `API::relay`.
pub async fn relays_page(State(state): State<AppState>, session: Session, Query(query): Query<HashMap<String, String>>) -> Response {
    let (pool, me) = match admin(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let loaded = async { Ok::<_, sqlx::Error>((crate::entities::relay::list(&pool).await?, crate::entities::relay::subscribers(&pool).await?)) }.await;
    let (relays, subscribers) = match loaded {
        Ok(loaded) => loaded,
        Err(err) => return failed(&state, &session, err).await,
    };
    let base = state.public_base_url.trim_end_matches('/');
    let error = query.contains_key("error");
    render_template(&RelaysTemplate {
        title: "Relays".to_string(),
        sections: sections(&pool, "Relays", &me).await,
        layout: Layout::load(&state, &session).await,
        relays,
        subscribers,
        relay_server: crate::settings::relay_server(),
        inbox_url: format!("{base}{}", crate::API::relay::INBOX_PATH),
        actor_url: format!("{base}/ap/actor"),
        federation_on: state.federation.is_some(),
        message: query.get("error").cloned().or_else(|| {
            query.get("done").map(|done| match done.as_str() {
                "added" => "The relay is asked: it shows as connected once it accepts.".to_string(),
                "removed" => "The relay is removed, and told.".to_string(),
                "paused" => "Paused: nothing is sent to it nor read from it.".to_string(),
                "resumed" => "The relay is asked again.".to_string(),
                "accepted" => "The server is accepted: it gets the public posts relayed here.".to_string(),
                "rejected" => "The server is refused, and told.".to_string(),
                _ => "Saved.".to_string(),
            })
        }),
        error,
    })
}

/// Follows a relay (`url`).
pub async fn add_relay(State(state): State<AppState>, session: Session, Form(form): Form<HashMap<String, String>>) -> Response {
    let (pool, me) = match admin(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let Some(federation) = state.federation.as_deref() else {
        return Redirect::to("/moderation/relays").into_response();
    };
    let url = form.get("url").map(|url| url.trim().to_string()).unwrap_or_default();
    match crate::API::relay::follow(federation, &pool, &url).await {
        Ok(_) => {
            tracing::info!("@{} added the relay {}", me.username, url);
            Redirect::to("/moderation/relays?done=added").into_response()
        }
        Err(message) => Redirect::to(&format!("/moderation/relays?error={}", crate::handlers::search::form_urlencode(&crate::i18n::t(&message)))).into_response(),
    }
}

/// `remove`, `pause` or `resume` a relay followed.
pub async fn relay_action(State(state): State<AppState>, session: Session, Path((id, action)): Path<(i64, String)>) -> Response {
    let (pool, me) = match admin(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let Some(federation) = state.federation.as_deref() else {
        return Redirect::to("/moderation/relays").into_response();
    };
    let relay = match crate::entities::relay::find(&pool, id).await {
        Ok(Some(relay)) => relay,
        Ok(None) => return Redirect::to("/moderation/relays").into_response(),
        Err(err) => return failed(&state, &session, err).await,
    };
    let done = match action.as_str() {
        "remove" => {
            if let Err(err) = crate::API::relay::unfollow(federation, &pool, &relay).await {
                tracing::warn!("failed to tell the relay {}: {}", relay.url, err);
            }
            if let Err(err) = crate::entities::relay::remove(&pool, id).await {
                return failed(&state, &session, err).await;
            }
            "removed"
        }
        "pause" => {
            if let Err(err) = crate::entities::relay::set_state(&pool, id, crate::entities::relay::State::Paused).await {
                return failed(&state, &session, err).await;
            }
            "paused"
        }
        _ => {
            if let Err(message) = crate::API::relay::follow(federation, &pool, &relay.url).await {
                return Redirect::to(&format!("/moderation/relays?error={}", crate::handlers::search::form_urlencode(&crate::i18n::t(&message)))).into_response();
            }
            "resumed"
        }
    };
    tracing::info!("@{} did {} on the relay {}", me.username, action, relay.url);
    Redirect::to(&format!("/moderation/relays?done={done}")).into_response()
}

/// `accept`, `reject` or `remove` a server following this one as a relay.
pub async fn subscriber_action(State(state): State<AppState>, session: Session, Path((id, action)): Path<(i64, String)>) -> Response {
    let (pool, me) = match admin(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let Some(federation) = state.federation.as_deref() else {
        return Redirect::to("/moderation/relays").into_response();
    };
    let subscriber = match crate::entities::relay::find_subscriber(&pool, id).await {
        Ok(Some(subscriber)) => subscriber,
        Ok(None) => return Redirect::to("/moderation/relays").into_response(),
        Err(err) => return failed(&state, &session, err).await,
    };
    let accepted = action == "accept";
    if let Err(err) = crate::API::relay::answer_subscriber(federation, &pool, &subscriber, accepted).await {
        tracing::warn!("failed to answer {}: {}", subscriber.actor_id, err);
    }
    let saved = match action.as_str() {
        "accept" => crate::entities::relay::set_subscriber_state(&pool, id, crate::entities::relay::State::Accepted).await,
        "reject" => crate::entities::relay::set_subscriber_state(&pool, id, crate::entities::relay::State::Rejected).await,
        _ => crate::entities::relay::remove_subscriber(&pool, Some(id), None).await,
    };
    if let Err(err) = saved {
        return failed(&state, &session, err).await;
    }
    tracing::info!("@{} did {} on the relay subscriber {}", me.username, action, subscriber.actor_id);
    Redirect::to(&format!("/moderation/relays?done={}", if accepted { "accepted" } else { "rejected" })).into_response()
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
    /// What this site met of the Fediverse.
    statistics: Vec<StatGroup>,
}

/// The servers met, with what's known of them and their rule.
pub async fn federation_page(State(state): State<AppState>, session: Session, Query(query): Query<HashMap<String, String>>) -> Response {
    let (pool, me) = match staff(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let search = query.get("q").map(|search| search.trim().to_string()).unwrap_or_default();
    let instances = match store::instances(&pool, Some(&search).filter(|search| !search.is_empty()).map(String::as_str)).await {
        Ok(instances) => instances,
        Err(err) => return failed(&state, &session, err).await,
    };
    let statistics = match statistics::federation(&pool).await {
        Ok(statistics) => statistics,
        Err(err) => return failed(&state, &session, err).await,
    };
    let message = query.get("done").map(|done| match done.as_str() {
        "limited" => "Limited: its posts and notifications only reach the people following its accounts.",
        "banned" => "Banned: what came from it is deleted, and nothing goes to it anymore.",
        "cleared" => "The server is treated normally again.",
        "media" => "Its media are rejected: its posts come without their pictures, videos and sounds.",
        "invalid" => "Write a domain, like mastodon.social.",
        "own" => "This is the domain of this site.",
        _ => "Saved.",
    });
    render_template(&FederationTemplate {
        title: "Federation".to_string(),
        sections: sections(&pool, "Federation", &me).await,
        layout: Layout::load(&state, &session).await,
        instances,
        search,
        message,
        federation_on: state.federation.is_some(),
        statistics,
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
    store::is_valid_domain(&domain).then_some(domain)
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
        Some(store::Policy::RejectMedia) => "media",
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

// --- Statistics ---

#[derive(Template)]
#[template(path = "moderation_statistics.html")]
struct StatisticsTemplate {
    title: String,
    layout: Layout,
    sections: Vec<Section>,
    statistics: Vec<StatGroup>,
}

/// What was made on this site: people, posts, pictures, videos...
pub async fn statistics_page(State(state): State<AppState>, session: Session) -> Response {
    let (pool, me) = match staff(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let statistics = match statistics::local(&pool).await {
        Ok(statistics) => statistics,
        Err(err) => return failed(&state, &session, err).await,
    };
    render_template(&StatisticsTemplate {
        title: "Statistics".to_string(),
        sections: sections(&pool, "Statistics", &me).await,
        layout: Layout::load(&state, &session).await,
        statistics,
    })
}

// --- Configuration ---

#[derive(Template)]
#[template(path = "moderation_configuration.html")]
struct ConfigurationTemplate {
    title: String,
    layout: Layout,
    sections: Vec<Section>,
    files: Vec<crate::settings::shown::ShownFile>,
    /// Terms of Use, Privacy Policy, Cookies and Server rules, see `site_page`.
    pages: Vec<SitePage>,
    message: Option<String>,
    error: Option<String>,
}

async fn configuration(state: &AppState, session: &Session, pool: &PgPool, me: &User, message: Option<String>, error: Option<String>) -> Response {
    let pages = site_page::Kind::ALL.map(site_page::read).into();
    let status = if error.is_some() { StatusCode::BAD_REQUEST } else { StatusCode::OK };
    let page = render_template(&ConfigurationTemplate {
        title: "Configuration".to_string(),
        sections: sections(pool, "Configuration", me).await,
        layout: Layout::load(state, session).await,
        files: crate::settings::shown::files(),
        pages,
        message,
        error,
    });
    (status, page).into_response()
}

/// The configuration files, for the creator only, secrets hidden.
/// `?saved=runtime&changed=APP_PORT,VIDEO` after a save.
pub async fn configuration_page(State(state): State<AppState>, session: Session, Query(query): Query<HashMap<String, String>>) -> Response {
    let (pool, me) = match admin(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    if let Some(kind) = query.get("page").and_then(|slug| site_page::Kind::parse(slug)) {
        let message = crate::i18n::tf("“{}” saved.", crate::i18n::t(kind.title()));
        return configuration(&state, &session, &pool, &me, Some(message), None).await;
    }
    let message = query.get("saved").map(|file| {
        // Only names of settings: the address can't make the page say anything else.
        let changed = query
            .get("changed")
            .map(|keys| keys.split(',').filter(|key| !key.is_empty() && key.bytes().all(|byte| byte == b'_' || byte == b'-' || byte.is_ascii_alphanumeric())).collect::<Vec<_>>().join(", "))
            .unwrap_or_default();
        let file = if matches!(file.as_str(), "runtime" | "services" | "settings") { file.as_str() } else { "the file" };
        match changed.is_empty() {
            true => format!("Nothing changed in {file}.conf."),
            false => format!("Saved in {file}.conf: {changed}. Restart Tison to apply them."),
        }
    });
    configuration(&state, &session, &pool, &me, message, None).await
}

/// Saves one file from its form: the creator's password, the file's version
/// shown, and `line-N` for each setting.
pub async fn save_configuration(State(state): State<AppState>, session: Session, Form(form): Form<HashMap<String, String>>) -> Response {
    let (pool, me) = match admin(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let field = |name: &str| form.get(name).cloned().unwrap_or_default();
    let stored_hash = match user::password_hash(&pool, me.id).await {
        Ok(hash) => hash,
        Err(err) => return failed(&state, &session, err).await,
    };
    if !crate::handlers::auth::verify_password_blocking(field("password"), stored_hash).await {
        let error = "Wrong password: nothing was saved.".to_string();
        return configuration(&state, &session, &pool, &me, None, Some(error)).await;
    }
    let file = field("file");
    let saved = crate::settings::shown::save(&file, &field("version"), &form);
    match saved {
        Ok(changed) => {
            if !changed.is_empty() {
                // Names only: values may be secret.
                tracing::info!("@{} changed {} in {}.conf", me.username, changed.join(", "), file);
            }
            Redirect::to(&format!("/moderation/configuration?saved={file}&changed={}#{file}", changed.join(","))).into_response()
        }
        Err(err) => {
            tracing::warn!("@{} failed to save {}.conf: {:?}", me.username, file, err);
            configuration(&state, &session, &pool, &me, None, Some(err.message())).await
        }
    }
}

// --- Pages of the site ---

#[derive(Template)]
#[template(path = "moderation_site_page.html")]
struct SitePageEditTemplate {
    title: String,
    layout: Layout,
    sections: Vec<Section>,
    page: SitePage,
    body_max_chars: usize,
    /// `{{INSTANCE_NAME}}`... filled in on the site, see `site_page::PLACEHOLDERS`.
    placeholders: String,
    /// The languages it can be written in, each its own file.
    languages: [crate::i18n::Lang; 3],
    error: Option<String>,
}

/// Writing one of the pages linked under the menu, for the creator only,
/// in English or another language (`?lang=fr`: `Terms.fr.md`).
pub async fn site_page_edit(State(state): State<AppState>, session: Session, Path(slug): Path<String>, Query(query): Query<HashMap<String, String>>) -> Response {
    let (pool, me) = match admin(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let Some(kind) = site_page::Kind::parse(&slug) else {
        return error_page(Layout::load(&state, &session).await, StatusCode::NOT_FOUND, "This page doesn't exist.");
    };
    let lang = query.get("lang").and_then(|code| crate::i18n::Lang::parse(code)).unwrap_or_default();
    site_page_form(&state, &session, &pool, &me, site_page::read_language(kind, lang), None).await
}

async fn site_page_form(state: &AppState, session: &Session, pool: &PgPool, me: &User, page: SitePage, error: Option<String>) -> Response {
    let status = if error.is_some() { StatusCode::BAD_REQUEST } else { StatusCode::OK };
    let form = render_template(&SitePageEditTemplate {
        title: page.kind.title().to_string(),
        sections: sections(pool, "Configuration", me).await,
        layout: Layout::load(state, session).await,
        page,
        body_max_chars: site_page::BODY_MAX_CHARS,
        placeholders: site_page::PLACEHOLDERS.map(|name| format!("{{{{{name}}}}}")).join(", "),
        languages: crate::i18n::Lang::ALL,
        error,
    });
    (status, form).into_response()
}

/// Saves a page to its file: `body`, in Markdown, and `version`, of the file
/// shown. Empty, the page says it isn't written yet.
pub async fn save_site_page(State(state): State<AppState>, session: Session, Path(slug): Path<String>, Form(form): Form<HashMap<String, String>>) -> Response {
    let (pool, me) = match admin(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let Some(kind) = site_page::Kind::parse(&slug) else {
        return error_page(Layout::load(&state, &session).await, StatusCode::NOT_FOUND, "This page doesn't exist.");
    };
    let lang = form.get("lang").and_then(|code| crate::i18n::Lang::parse(code)).unwrap_or_default();
    let path = kind.path_in(lang);
    let body = form.get("body").cloned().unwrap_or_default();
    let refused = if body.chars().count() > site_page::BODY_MAX_CHARS {
        Some(crate::i18n::tf("The page is too long: at most {} characters.", site_page::BODY_MAX_CHARS))
    } else {
        match site_page::save(kind, lang, form.get("version").map(String::as_str).unwrap_or_default(), &body) {
            Ok(()) => None,
            Err(site_page::SaveError::Changed) => Some(crate::i18n::tf(
                "{} changed since you opened it (in an editor, or another tab): nothing was saved. Your text is below: save again to replace the file with it.",
                &path,
            )),
            Err(site_page::SaveError::Io(err)) => {
                tracing::error!("failed to write {}: {}", path, err);
                Some(crate::i18n::tf("{} couldn't be written: check that Tison may write in this folder.", &path))
            }
        }
    };
    if let Some(error) = refused {
        // The text sent stays in the form, with the version of the file as
        // it is now: saving again replaces it.
        let page = SitePage { body, ..site_page::read_language(kind, lang) };
        return site_page_form(&state, &session, &pool, &me, page, Some(error)).await;
    }
    // Pictures of the web are shown through the proxy, which only serves known addresses.
    if let Err(err) = crate::entities::media::register_article_pictures(&pool, &body).await {
        tracing::warn!("failed to register the pictures of {}: {}", path, err);
    }
    tracing::info!("@{} changed {}", me.username, path);
    Redirect::to(&format!("/moderation/configuration?page={}#site-pages", kind.slug())).into_response()
}

/// `{"body": "..."}` → the page as visitors will see it, in HTML, for the
/// preview of the editor (see `SitePage::html`).
pub async fn site_page_preview(State(state): State<AppState>, session: Session, axum::Json(form): axum::Json<serde_json::Value>) -> Response {
    let (pool, _) = match admin(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let body = form["body"].as_str().unwrap_or_default();
    if body.chars().count() > site_page::BODY_MAX_CHARS {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }
    if let Err(err) = crate::entities::media::register_article_pictures(&pool, body).await {
        tracing::warn!("failed to register the pictures of a preview: {}", err);
    }
    let page = SitePage { kind: site_page::Kind::Terms, lang: crate::i18n::Lang::En, body: body.to_string(), updated_at: None, version: String::new() };
    let html = page.html(&state.instance_name, &state.public_base_url);
    ([(axum::http::header::CACHE_CONTROL, "no-store")], axum::Json(serde_json::json!({ "html": html }))).into_response()
}

// --- Users ---

#[derive(Template)]
#[template(path = "moderation_users.html")]
struct UsersTemplate {
    title: String,
    layout: Layout,
    sections: Vec<Section>,
    users: Vec<user::ListedUser>,
    search: String,
    /// `?before=` of the next page, when there is one.
    next_before: Option<i32>,
    domains: Vec<crate::entities::email_domain::BlockedDomain>,
    message: Option<&'static str>,
}

/// The site's accounts with their email, and the email domains blocked at
/// signup. `?q=` searches, `?before=` pages.
pub async fn users_page(State(state): State<AppState>, session: Session, Query(query): Query<HashMap<String, String>>) -> Response {
    let (pool, me) = match staff(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let search = query.get("q").map(|search| search.trim().to_string()).unwrap_or_default();
    let before = query.get("before").and_then(|before| before.parse::<i32>().ok());
    let loaded = async {
        Ok::<_, sqlx::Error>((
            user::list_for_staff(&pool, &search, before).await?,
            crate::entities::email_domain::list(&pool).await?,
        ))
    }
    .await;
    let (users, domains) = match loaded {
        Ok(loaded) => loaded,
        Err(err) => return failed(&state, &session, err).await,
    };
    let next_before = (users.len() as i64 == user::LIST_SIZE).then(|| users.last().map(|last| last.user.id)).flatten();
    let message = query.get("done").map(|done| match done.as_str() {
        "blocked" => "The domain is blocked: no new account can use it.",
        "unblocked" => "The domain is unblocked.",
        "invalid" => "This isn't a domain, like spam.example.",
        _ => "Saved.",
    });
    render_template(&UsersTemplate {
        title: "Users".to_string(),
        sections: sections(&pool, "Users", &me).await,
        layout: Layout::load(&state, &session).await,
        users,
        search,
        next_before,
        domains,
        message,
    })
}

/// Blocks (`domain`) or unblocks (`unblock`) an email domain for new accounts.
pub async fn email_domains(State(state): State<AppState>, session: Session, Form(form): Form<HashMap<String, String>>) -> Response {
    let (pool, me) = match staff(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let back = |done: &str| Redirect::to(&format!("/moderation/users?done={done}#email-domains")).into_response();
    let result = if let Some(domain) = form.get("unblock") {
        crate::entities::email_domain::remove(&pool, domain).await.map(|()| "unblocked")
    } else {
        let Some(domain) = crate::entities::email_domain::normalize(form.get("domain").map(String::as_str).unwrap_or_default()) else {
            return back("invalid");
        };
        tracing::info!("@{} blocked the email domain {}", me.username, domain);
        crate::entities::email_domain::add(&pool, &domain, me.id).await.map(|_| "blocked")
    };
    match result {
        Ok(done) => back(done),
        Err(err) => failed(&state, &session, err).await,
    }
}

// --- Confidentiality incidents ---

#[derive(Template)]
#[template(path = "moderation_incidents.html")]
struct IncidentsTemplate {
    title: String,
    layout: Layout,
    sections: Vec<Section>,
    incidents: Vec<crate::entities::incident::Incident>,
    today: String,
    text_max_chars: usize,
    error: Option<&'static str>,
    saved: bool,
}

/// The register of confidentiality incidents (Law 25 art. 3.8, GDPR art.
/// 33.5), for the creator only: see `entities::incident`.
pub async fn incidents_page(State(state): State<AppState>, session: Session, Query(query): Query<HashMap<String, String>>) -> Response {
    let (pool, me) = match admin(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let incidents = match crate::entities::incident::all(&pool).await {
        Ok(incidents) => incidents,
        Err(err) => return failed(&state, &session, err).await,
    };
    render_template(&IncidentsTemplate {
        title: "Incidents".to_string(),
        sections: sections(&pool, "Incidents", &me).await,
        layout: Layout::load(&state, &session).await,
        incidents,
        today: sqlx::types::chrono::Utc::now().format("%Y-%m-%d").to_string(),
        text_max_chars: crate::entities::incident::TEXT_MAX_CHARS,
        error: query.get("error").map(|_| "Write what happened and when it was discovered."),
        saved: query.contains_key("saved"),
    })
}

/// Adds an incident, or changes one (`id`): its notices are added later.
pub async fn save_incident(State(state): State<AppState>, session: Session, Form(form): Form<HashMap<String, String>>) -> Response {
    let (pool, me) = match admin(&state, &session).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let date = |name: &str| form.get(name).and_then(|value| sqlx::types::chrono::NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d").ok());
    let text = |name: &str| form.get(name).map(|value| value.trim().chars().take(crate::entities::incident::TEXT_MAX_CHARS).collect::<String>()).unwrap_or_default();
    let id = form.get("id").and_then(|id| id.parse::<i64>().ok());
    let (Some(discovered_on), description) = (date("discovered_on"), text("description")) else {
        return Redirect::to("/moderation/incidents?error=1#new").into_response();
    };
    if description.is_empty() {
        return Redirect::to("/moderation/incidents?error=1#new").into_response();
    }
    let incident = crate::entities::incident::IncidentForm {
        occurred_on: date("occurred_on"),
        discovered_on,
        description,
        data_concerned: text("data_concerned"),
        people_affected: text("people_affected"),
        serious_risk: form.get("serious_risk").is_some_and(|value| value == "yes"),
        measures: text("measures"),
        authority_notified_on: date("authority_notified_on"),
        people_notified_on: date("people_notified_on"),
    };
    if let Err(err) = crate::entities::incident::save(&pool, id, &incident, me.id).await {
        return failed(&state, &session, err).await;
    }
    tracing::info!("@{} recorded a confidentiality incident", me.username);
    Redirect::to("/moderation/incidents?saved").into_response()
}

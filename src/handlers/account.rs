//! What users decide about their own data, in Settings > Account: a copy of
//! it (GDPR art. 15 and 20, Québec's Law 25 art. 27, PIPEDA principle 9),
//! deleting the account (GDPR art. 17, Law 25 art. 28), accepting the
//! Terms and Privacy Policy when they change, and whether search engines may
//! show their profile.

use std::{collections::HashMap, path::PathBuf};

use axum::{
    Form,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use sqlx::PgPool;
use tower::ServiceExt;
use tower_sessions::Session;

use crate::{
    AppState,
    entities::{data_export, site_page, user},
    handlers::{
        auth::verify_password_blocking,
        image::{self, ImageOwner},
        notifier,
        system::{Layout, error_page, local_path},
        user::signed_in_user,
    },
};

/// The password, checked again before giving out or deleting everything.
async fn password_ok(pool: &PgPool, user_id: i32, form: &HashMap<String, String>) -> bool {
    let hash = user::password_hash(pool, user_id).await.ok().flatten();
    verify_password_blocking(form.get("password").cloned().unwrap_or_default(), hash).await
}

fn back(done: &str) -> Response {
    Redirect::to(&format!("/settings?tab=account&{done}#your-data")).into_response()
}

// --- A copy of the data ---

/// Asks for a copy: made in the background, one every
/// `data_export::EVERY_HOURS`.
pub async fn request_export(State(state): State<AppState>, session: Session, Form(form): Form<HashMap<String, String>>) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    if !password_ok(&pool, me.id, &form).await {
        return back("data=password");
    }
    match data_export::latest(&pool, me.id).await {
        Ok(Some(last)) if last.too_recent() => return back("data=too-soon"),
        Ok(_) => {}
        Err(err) => {
            tracing::error!("failed to check the exports of @{}: {}", me.username, err);
            return back("data=server");
        }
    }
    let token = format!("{}{}", crate::handlers::session::random_part(), crate::handlers::session::random_part());
    if let Err(err) = data_export::create(&pool, me.id, &token).await {
        tracing::error!("failed to start the export of @{}: {}", me.username, err);
        return back("data=server");
    }
    let user_id = me.id;
    tokio::spawn(async move {
        let size = match build_export(&pool, user_id, &token).await {
            Ok(size) => Some(size),
            Err(err) => {
                tracing::error!("failed to make the export of user {}: {}", user_id, err);
                None
            }
        };
        if let Err(err) = data_export::finish(&pool, &token, size).await {
            tracing::error!("failed to save the export of user {}: {}", user_id, err);
        }
    });
    tracing::info!("@{} asked for a copy of their data", me.username);
    back("data=preparing")
}

/// One JSON document from the database, indented: `query` gives one row
/// and column, read as text.
async fn json(pool: &PgPool, query: &'static str, user_id: i32) -> Result<String, sqlx::Error> {
    let sql = format!("SELECT ({query})::text");
    let text: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql)).bind(user_id).fetch_one(pool).await?;
    let value: serde_json::Value = text.as_deref().and_then(|text| serde_json::from_str(text).ok()).unwrap_or(serde_json::Value::Null);
    Ok(serde_json::to_string_pretty(&value).unwrap_or_default())
}

/// What goes in the copy: everything the account wrote, chose or uploaded.
/// Messages of the others aren't theirs to take: only those sent.
const EXPORTED: [(&str, &str); 12] = [
    (
        "profile.json",
        "SELECT row_to_json(u) FROM (SELECT username, displayname, firstname, lastname, names_visibility, birthday, birthday_public,
                gender, bio, email, email_verified, created_at, role, federation_enabled, search_indexing, photo_url, background_url,
                terms_accepted_at AS consent_given_at, terms_version AS consent_to_version
            FROM users WHERE id = $1) u",
    ),
    (
        "posts.json",
        "SELECT COALESCE(json_agg(p ORDER BY p.created_at), '[]'::json) FROM (
            SELECT p.id, p.kind, p.title, p.body, p.visibility, p.sensitive, p.created_at, p.edited_at, p.tags, p.category,
                   (SELECT slug FROM communities c WHERE c.id = p.community_id) AS community,
                   (SELECT COALESCE(json_agg(json_build_object('url', a.url, 'type', a.media_type, 'description', a.description)
                                    ORDER BY a.position), '[]'::json)
                    FROM attachments a WHERE a.post_id = p.id) AS attachments
            FROM posts p WHERE p.user_id = $1) p",
    ),
    (
        "comments.json",
        "SELECT COALESCE(json_agg(c ORDER BY c.created_at), '[]'::json) FROM (
            SELECT id, post_id, parent_id, body, created_at, edited_at FROM comments WHERE user_id = $1) c",
    ),
    (
        "messages_sent.json",
        "SELECT COALESCE(json_agg(m ORDER BY m.created_at), '[]'::json) FROM (
            SELECT m.id, m.created_at, m.body,
                   COALESCE((SELECT '@' || o.username FROM users o WHERE o.id = CASE WHEN c.user_id = $1 THEN c.other_user_id ELSE c.user_id END),
                            (SELECT '@' || ra.username || '@' || ra.host FROM federation_actors ra WHERE ra.id = c.remote_actor_id)) AS to_account,
                   (SELECT COALESCE(json_agg(a.url), '[]'::json) FROM attachments a WHERE a.message_id = m.id) AS attachments
            FROM messages m JOIN conversations c ON c.id = m.conversation_id WHERE m.sender_user_id = $1) m",
    ),
    (
        "following.json",
        "SELECT json_build_object(
            'here', (SELECT COALESCE(json_agg(json_build_object('account', '@' || u.username, 'since', f.created_at)), '[]'::json)
                     FROM user_follows f JOIN users u ON u.id = f.followed_id WHERE f.follower_id = $1),
            'other_servers', (SELECT COALESCE(json_agg(json_build_object('account', '@' || a.username || '@' || a.host, 'id', a.actor_id,
                                                                         'accepted', f.accepted, 'since', f.created_at)), '[]'::json)
                              FROM federation_following f JOIN federation_actors a ON a.id = f.actor_id WHERE f.user_id = $1))",
    ),
    (
        "followers.json",
        "SELECT json_build_object(
            'here', (SELECT COALESCE(json_agg(json_build_object('account', '@' || u.username, 'since', f.created_at)), '[]'::json)
                     FROM user_follows f JOIN users u ON u.id = f.follower_id WHERE f.followed_id = $1),
            'other_servers', (SELECT COALESCE(json_agg(json_build_object('account', '@' || a.username || '@' || a.host, 'id', a.actor_id,
                                                                         'since', f.created_at)), '[]'::json)
                              FROM federation_followers f JOIN federation_actors a ON a.id = f.actor_id WHERE f.user_id = $1))",
    ),
    (
        "friends.json",
        "SELECT COALESCE(json_agg(json_build_object('account', '@' || u.username, 'accepted', f.accepted, 'since', f.created_at)), '[]'::json)
         FROM friendships f JOIN users u ON u.id = CASE WHEN f.requester_id = $1 THEN f.addressee_id ELSE f.requester_id END
         WHERE f.requester_id = $1 OR f.addressee_id = $1",
    ),
    (
        "likes.json",
        "SELECT json_build_object(
            'posts', (SELECT COALESCE(json_agg(json_build_object('post', post_id, 'at', created_at)), '[]'::json) FROM post_likes WHERE user_id = $1),
            'comments', (SELECT COALESCE(json_agg(json_build_object('comment', comment_id, 'at', created_at)), '[]'::json) FROM comment_likes WHERE user_id = $1))",
    ),
    (
        "communities.json",
        "SELECT json_build_object(
            'member_of', (SELECT COALESCE(json_agg(json_build_object('community', c.slug, 'since', m.joined_at)), '[]'::json)
                          FROM community_members m JOIN communities c ON c.id = m.community_id WHERE m.user_id = $1),
            'owner_of', (SELECT COALESCE(json_agg(json_build_object('community', slug, 'name', name, 'description', description)), '[]'::json)
                         FROM communities WHERE owner_user_id = $1))",
    ),
    (
        "events.json",
        "SELECT COALESCE(json_agg(json_build_object('event', p.post_id, 'role', p.role)), '[]'::json)
         FROM event_participants p WHERE p.user_id = $1",
    ),
    (
        "reports_made.json",
        "SELECT COALESCE(json_agg(json_build_object('about', target_label, 'reason', reason, 'comment', comment, 'status', status,
                                                     'at', created_at)), '[]'::json)
         FROM reports WHERE reporter_user_id = $1",
    ),
    (
        "security.json",
        "SELECT json_build_object(
            'two_factor_since', (SELECT enabled_at FROM two_factor WHERE user_id = $1),
            'browsers_receiving_notifications', (SELECT COUNT(*) FROM push_subscriptions WHERE user_id = $1),
            'stay_signed_in_tokens', (SELECT COUNT(*) FROM remember_tokens WHERE user_id = $1))",
    ),
];

const README: &str = "This is a copy of your data, as you asked for it.

profile.json         your profile, email, birthday, and when you accepted the Terms and Privacy Policy
posts.json           your posts, articles, videos, episodes and events, with their files
comments.json        your comments
messages_sent.json   the private messages you sent (those of others are theirs)
following.json       who you follow, here and on other servers
followers.json       who follows you
friends.json         your friends
likes.json           what you liked
communities.json     the communities you joined or created
events.json          the events you take part in
reports_made.json    the reports you made
security.json        two-factor authentication and signed-in browsers (no secret)
files/               what you uploaded: photos, videos (their best quality), pictures, files

Your passwords and secret keys are never included.
";

/// Makes the archive in `data_export::DIR`, returns its size.
async fn build_export(pool: &PgPool, user_id: i32, token: &str) -> Result<u64, String> {
    let mut documents = Vec::new();
    for (name, query) in EXPORTED {
        documents.push((name, json(pool, query, user_id).await.map_err(|err| format!("{name}: {err}"))?));
    }
    // The files of the account: of the qualities of a video, only the best.
    let lower_qualities: Vec<String> = sqlx::query_scalar(
        "SELECT v.url FROM video_files v JOIN posts p ON p.id = v.post_id
         WHERE p.user_id = $1 AND v.lines < (SELECT MAX(w.lines) FROM video_files w WHERE w.post_id = v.post_id)",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|err| err.to_string())?;
    let folder = PathBuf::from("public/uploads/users").join(user_id.to_string());
    let path = PathBuf::from(data_export::DIR).join(format!("{token}.zip"));
    let now = sqlx::types::chrono::Utc::now().format("%Y %m %d %H %M %S").to_string();
    let when: Vec<u16> = now.split(' ').filter_map(|part| part.parse().ok()).collect();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(data_export::DIR).map_err(|err| err.to_string())?;
        let temporary = path.with_extension("zip.tmp");
        let file = std::io::BufWriter::new(std::fs::File::create(&temporary).map_err(|err| err.to_string())?);
        let mut zip = crate::zip::ZipWriter::new(file, (when[0], when[1], when[2], when[3], when[4], when[5]));
        let result = (|| {
            zip.add_bytes("README.txt", README.as_bytes())?;
            for (name, document) in &documents {
                zip.add_bytes(name, document.as_bytes())?;
            }
            if let Ok(entries) = std::fs::read_dir(&folder) {
                let mut entries: Vec<_> = entries.flatten().collect();
                entries.sort_by_key(|entry| entry.file_name());
                for entry in entries {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let url = format!("/public/uploads/users/{user_id}/{name}");
                    // Originals of cropped pictures are the user's too.
                    if entry.path().is_file() && !lower_qualities.contains(&url) {
                        zip.add_file(&format!("files/{name}"), &entry.path())?;
                    }
                }
            }
            Ok::<_, std::io::Error>(())
        })();
        if let Err(err) = result {
            let _ = std::fs::remove_file(&temporary);
            return Err(err.to_string());
        }
        zip.finish().map_err(|err| err.to_string())?;
        std::fs::rename(&temporary, &path).map_err(|err| err.to_string())?;
        std::fs::metadata(&path).map(|metadata| metadata.len()).map_err(|err| err.to_string())
    })
    .await
    .map_err(|err| err.to_string())?
}

/// The archive, to its owner only, as a download.
pub async fn download_export(State(state): State<AppState>, session: Session, Path(token): Path<String>, request: axum::extract::Request) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let found = data_export::find(&pool, me.id, &token).await.ok().flatten().filter(|export| export.is_ready());
    let Some(export) = found else {
        return error_page(Layout::load(&state, &session).await, StatusCode::NOT_FOUND, "This copy doesn't exist anymore: ask for a new one.");
    };
    let name = format!("{}-{}-{}.zip", crate::settings::instance_name().replace(|letter: char| !letter.is_ascii_alphanumeric(), "-"), me.username, export.created_at.format("%Y-%m-%d"));
    match tower_http::services::ServeFile::new(export.path()).oneshot(request).await {
        Ok(response) => {
            let mut response = response.map(axum::body::Body::new).into_response();
            if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"{name}\"")) {
                response.headers_mut().insert(header::CONTENT_DISPOSITION, value);
            }
            response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
            response
        }
        Err(err) => match err {},
    }
}

/// Removes the copies past `data_export::KEEP_DAYS`, every hour.
pub async fn run_export_cleaning(pool: PgPool) {
    let mut every = tokio::time::interval(std::time::Duration::from_secs(3600));
    loop {
        every.tick().await;
        match data_export::remove(&pool, None).await {
            Ok(0) => {}
            Ok(removed) => tracing::info!("{} old copies of data removed", removed),
            Err(err) => tracing::warn!("failed to remove old copies of data: {}", err),
        }
    }
}

// --- Deleting the account ---

/// Deletes the signed-in account, after its password and a confirmation:
/// what it posted, its files, its copies, and other servers are told. The
/// creator of the site can't: someone must run it.
pub async fn delete_account(State(state): State<AppState>, session: Session, headers: HeaderMap, Form(form): Form<HashMap<String, String>>) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    if me.is_admin() {
        return back("delete=creator");
    }
    if form.get("confirm").is_none_or(|confirm| confirm.is_empty()) {
        return back("delete=confirm");
    }
    if !password_ok(&pool, me.id, &form).await {
        return back("delete=password");
    }
    notifier::user_deleting(&state, &pool, &me).await;
    if let Err(err) = data_export::remove(&pool, Some(me.id)).await {
        tracing::warn!("failed to remove the copies of @{}: {}", me.username, err);
    }
    if let Err(err) = user::delete(&pool, me.id).await {
        tracing::error!("failed to delete the account of @{}: {}", me.username, err);
        return back("delete=server");
    }
    image::remove_folder(ImageOwner::User(me.id)).await;
    tracing::info!("@{} deleted their account", me.username);
    // Signed out everywhere: the "stay signed in" cookie goes too.
    crate::handlers::auth::logout(State(state), session, headers).await
}

// --- Consent and search engines ---

/// Accepts the current Terms and Privacy Policy (the banner shown when
/// they changed), back to the page it was shown on.
pub async fn accept_terms(State(state): State<AppState>, session: Session, headers: HeaderMap) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    if let Err(err) = user::accept_terms(&pool, me.id, &site_page::consent_version()).await {
        tracing::error!("failed to save the consent of @{}: {}", me.username, err);
    }
    let back = headers
        .get(header::REFERER)
        .and_then(|referer| referer.to_str().ok())
        .and_then(|referer| referer.strip_prefix(state.public_base_url.trim_end_matches('/')).or(Some(referer)))
        .map(str::to_string);
    Redirect::to(&local_path(back.as_ref(), "/")).into_response()
}

/// Whether the public posts may be found by the search, here and on other
/// servers (Mastodon's `indexable`). Other servers get the profile again.
pub async fn indexable(State(state): State<AppState>, session: Session, Form(form): Form<HashMap<String, String>>) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let indexable = form.get("indexable").is_some_and(|value| value == "on");
    if let Err(err) = user::set_indexable(&pool, me.id, indexable).await {
        tracing::error!("failed to save the indexable choice of @{}: {}", me.username, err);
    } else if let Ok(Some(updated)) = user::find_by_id(&pool, me.id).await {
        crate::handlers::notifier::profile_updated(&state, &pool, &updated).await;
    }
    Redirect::to("/settings?tab=profile&saved#indexable").into_response()
}

/// Whether search engines may show the profile and posts.
pub async fn search_indexing(State(state): State<AppState>, session: Session, Form(form): Form<HashMap<String, String>>) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let allowed = form.get("search_indexing").is_some_and(|value| value == "on");
    if let Err(err) = user::set_search_indexing(&pool, me.id, allowed).await {
        tracing::error!("failed to save the indexing choice of @{}: {}", me.username, err);
    }
    Redirect::to("/settings?tab=profile&saved#search-engines").into_response()
}

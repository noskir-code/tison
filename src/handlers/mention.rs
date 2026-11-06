//! Mentions of the posts written here (`BLOGGING`, see `crate::mention`):
//! who `@alice` and `@bob@mastodon.social` are, kept with the post
//! (`entities::mention`) for other servers, and told by a notification.

use sqlx::PgPool;

use crate::{
    AppState,
    API::linker,
    entities::{
        federation::Signer,
        mention::{self, MAX_PER_POST},
        notification, post,
        user::{self, User},
    },
};

/// How long asking another server who `@bob@server` is may take, for each.
const LOOKUP_SECONDS: u64 = 5;

/// The users here and the accounts of other servers the text mentions,
/// at most `MAX_PER_POST`. Accounts not known yet are asked to their server
/// (WebFinger), when the author is on the Fediverse; unknown ones are left
/// out (they stay plain text for other servers).
async fn resolve(state: &AppState, pool: &PgPool, author: &User, text: &str) -> (Vec<i32>, Vec<i64>) {
    let site_domain = crate::settings::site_domain();
    let (mut users, mut actors) = (Vec::new(), Vec::new());
    for handle in crate::mention::find(text).into_iter().take(MAX_PER_POST) {
        if handle.is_local(site_domain) {
            match user::find_by_username(pool, &handle.user).await {
                Ok(Some(found)) if found.id != author.id && !found.is_suspended() => users.push(found.id),
                Ok(_) => {}
                Err(err) => tracing::warn!("failed to find the mentioned @{}: {}", handle.user, err),
            }
            continue;
        }
        let domain = handle.domain.clone().unwrap_or_default();
        let known: Result<Option<i64>, sqlx::Error> = sqlx::query_scalar(
            "SELECT id FROM federation_actors WHERE LOWER(username) = LOWER($1) AND host = $2 AND blocked_at IS NULL LIMIT 1",
        )
        .bind(&handle.user)
        .bind(&domain)
        .fetch_optional(pool)
        .await;
        match known {
            Ok(Some(id)) => actors.push(id),
            Ok(None) => {
                if let Some(id) = look_up(state, pool, author, &handle.full()).await {
                    actors.push(id);
                }
            }
            Err(err) => tracing::warn!("failed to find the mentioned {}: {}", handle.full(), err),
        }
    }
    (users, actors)
}

/// Asks the server of `@bob@server` who it is, and saves the account.
async fn look_up(state: &AppState, pool: &PgPool, author: &User, handle: &str) -> Option<i64> {
    let federation = state.federation.as_deref()?;
    if !author.is_federated() {
        return None;
    }
    let found = async {
        let url = linker::webfinger(federation, handle).await?;
        let key = linker::local_key(federation, pool, Signer::User(author.id)).await?;
        linker::fetch_actor(federation, pool, &url, &key).await
    };
    match tokio::time::timeout(std::time::Duration::from_secs(LOOKUP_SECONDS), found).await {
        Ok(Ok(actor)) if !actor.blocked => Some(actor.id),
        Ok(Ok(_)) => None,
        Ok(Err(err)) => {
            tracing::info!("mentioned {} not found: {}", handle, err);
            None
        }
        Err(_) => {
            tracing::info!("mentioned {}: its server took too long", handle);
            None
        }
    }
}

/// Keeps the mentions of the post `post_id` written by `author`, and tells
/// the users here mentioned for the first time who can see it (not those
/// blocking the author, nor those a followers-only post isn't for). Before
/// the post is sent to other servers: the mentions go with it.
pub async fn save(state: &AppState, pool: &PgPool, author: &User, post_id: i64, text: &str) {
    let (users, actors) = resolve(state, pool, author, text).await;
    let new = match mention::replace(pool, post_id, &users, &actors).await {
        Ok(new) => new,
        Err(err) => {
            tracing::error!("failed to save the mentions of post {}: {}", post_id, err);
            return;
        }
    };
    let mut told = Vec::new();
    for user_id in new {
        if post::find(pool, post_id, Some(user_id)).await.ok().flatten().is_some() {
            told.push(user_id);
        }
    }
    if let Err(err) = notification::mentioned(pool, post_id, &told, Some(author.id), None).await {
        tracing::error!("failed to notify the mentions of post {}: {}", post_id, err);
    }
}

/// The users here a post received from another server mentions (its
/// `Mention` tags), told when they can see it.
pub async fn save_received(pool: &PgPool, post_id: i64, remote_actor_id: i64, users: &[i32]) {
    let users: Vec<i32> = users.iter().copied().take(MAX_PER_POST).collect();
    let new = match mention::replace(pool, post_id, &users, &[]).await {
        Ok(new) => new,
        Err(err) => {
            tracing::error!("failed to save the mentions of post {}: {}", post_id, err);
            return;
        }
    };
    let mut told = Vec::new();
    for user_id in new {
        if post::find(pool, post_id, Some(user_id)).await.ok().flatten().is_some() {
            told.push(user_id);
        }
    }
    if let Err(err) = notification::mentioned(pool, post_id, &told, None, Some(remote_actor_id)).await {
        tracing::error!("failed to notify the mentions of post {}: {}", post_id, err);
    }
}

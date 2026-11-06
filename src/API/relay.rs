//! Relays: servers forwarding public posts between the instances following
//! them, so small sites get content and discover accounts.
//!
//! - **Following relays.** An admin adds a relay (Moderation → Relays). The
//!   site's own actor (`/ap/actor`, an `Application`) follows it: Mastodon's
//!   relays by their inbox (the Follow is of the public collection),
//!   LitePub's (Pleroma's) by their actor. What they relay is fetched again
//!   from its own server and stored (see `activities::store_relayed`); the
//!   site's public posts go to them too (see `handlers::notifier`).
//! - **Acting as a relay** (`RELAY-SERVER`). Other servers follow this
//!   actor, each accepted by an admin. The public posts they send are
//!   forwarded to the others: as they are to Mastodon's, as Announces to
//!   LitePub's.
//!
//! Everything relays send comes to `/ap/actor/inbox`, apart from what users
//! receive: this module handles that inbox alone.

use axum::{
    extract::State,
    http::{HeaderMap, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::{
    AppState,
    API::{
        Federation,
        activities::{self, PUBLIC, object_id},
        linker::{self, KeySource},
    },
    entities::{
        federation::{self as store, Signer},
        relay::{self, State as RelayState},
    },
};

/// The path of the instance actor's inbox, see the module.
pub const INBOX_PATH: &str = "/ap/actor/inbox";

fn is_public_collection(id: &str) -> bool {
    matches!(id, PUBLIC | "as:Public" | "Public")
}

/// The site's actor, an `Application` like Mastodon's instance actor.
pub async fn instance_actor(State(state): State<AppState>) -> Response {
    let (Some(federation), Some(pool)) = (state.federation.as_deref(), state.pool.as_ref()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let public_pem = match linker::keys(pool, Signer::Instance).await {
        Ok((_, public_pem, _)) => public_pem,
        Err(err) => {
            tracing::error!("failed to load the instance keys: {}", err);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let id = federation.instance_actor_url();
    let actor = json!({
        "@context": ["https://www.w3.org/ns/activitystreams", "https://w3id.org/security/v1"],
        "id": id,
        "type": "Application",
        "preferredUsername": federation.domain,
        "name": crate::settings::instance_name(),
        "url": federation.base_url,
        // No shared inbox: relays deliver here, see the module.
        "inbox": format!("{}{INBOX_PATH}", federation.base_url),
        "followers": format!("{id}/followers"),
        // Servers following it as a relay are accepted by an admin.
        "manuallyApprovesFollowers": true,
        "publicKey": { "id": federation.instance_key_id(), "owner": id, "publicKeyPem": public_pem },
    });
    super::federation::activity_json(&actor)
}

/// Its followers: the servers it relays for, only counted.
pub async fn instance_followers(State(state): State<AppState>) -> Response {
    let (Some(federation), Some(pool)) = (state.federation.as_deref(), state.pool.as_ref()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let count = relay::subscriber_inboxes(pool, None).await.map(|inboxes| inboxes.len()).unwrap_or(0);
    super::federation::activity_json(&activities::counted_collection(&format!("{}/followers", federation.instance_actor_url()), count as i64))
}

/// Handles what arrives at `INBOX_PATH`, after the banned servers are left
/// out: Follows and Undos of servers wanting this site as their relay,
/// answers of the relays it follows, and what relays and subscribers send.
pub async fn receive(federation: &Federation, pool: &PgPool, method: &Method, uri: &Uri, headers: &HeaderMap, body: &[u8], activity: &Value) -> Response {
    let key = match linker::local_key(federation, pool, Signer::Instance).await {
        Ok(key) => key,
        Err(err) => {
            tracing::error!("failed to load the instance key: {}", err);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let sender = match linker::verify_request(federation, pool, method, uri, headers, body, KeySource::Fetch(&key)).await {
        Ok(sender) => sender,
        Err(err) => {
            tracing::info!("refused a relay activity: {}", err);
            return StatusCode::UNAUTHORIZED.into_response();
        }
    };
    if sender.blocked {
        return StatusCode::ACCEPTED.into_response();
    }
    let host = store::host_of(&sender.actor_id);
    let result = handle(federation, pool, &sender, &host, body, activity).await;
    if let Err(err) = result {
        tracing::warn!("failed to handle a relay activity from {}: {}", sender.actor_id, err);
    }
    StatusCode::ACCEPTED.into_response()
}

async fn handle(federation: &Federation, pool: &PgPool, sender: &store::RemoteActor, host: &str, body: &[u8], activity: &Value) -> Result<(), String> {
    let kind = activity["type"].as_str().unwrap_or_default();
    let instance = federation.instance_actor_url();
    let object = &activity["object"];
    let own_actor = object_id(&activity["actor"]) == Some(sender.actor_id.as_str());
    match kind {
        // A server wants this one as its relay.
        "Follow" if own_actor => {
            let target = object_id(object).unwrap_or_default();
            if target != instance && !is_public_collection(target) {
                return Ok(());
            }
            let follow_id = activity["id"].as_str().unwrap_or_default();
            if !crate::settings::relay_server() {
                let reject = activities::reject_follow(&instance, &sender.actor_id, follow_id);
                activities::send(federation, pool, Signer::Instance, std::slice::from_ref(&sender.inbox), &reject).await?;
                tracing::info!("refused {} as a relay subscriber: RELAY-SERVER is off", sender.actor_id);
                return Ok(());
            }
            let kind = if is_public_collection(target) { "mastodon" } else { "litepub" };
            let subscriber = relay::add_subscriber(pool, &sender.actor_id, &sender.inbox, host, kind, follow_id)
                .await
                .map_err(|err| format!("failed to save the subscriber: {err}"))?;
            match subscriber.state {
                // Asked again: told again.
                RelayState::Accepted => answer_subscriber(federation, pool, &subscriber, true).await?,
                _ => tracing::info!("{} asks to use this server as a relay: waiting for an admin", sender.actor_id),
            }
        }
        "Undo" if own_actor && object["type"] == "Follow" => {
            relay::remove_subscriber(pool, None, Some(&sender.actor_id)).await.map_err(|err| format!("failed to remove the subscriber: {err}"))?;
            tracing::info!("{} stopped using this server as a relay", sender.actor_id);
        }
        // The answer of a relay this site follows.
        "Accept" | "Reject" if own_actor => {
            let follow_id = object_id(object).unwrap_or_default();
            let accepted = kind == "Accept";
            if relay::answered(pool, follow_id, host, accepted).await.map_err(|err| format!("failed to save the answer: {err}"))? {
                tracing::info!("relay {} {} the subscription", host, if accepted { "accepted" } else { "refused" });
            }
        }
        _ => receive_content(federation, pool, sender, host, body, activity).await?,
    }
    Ok(())
}

/// What a followed relay or a subscriber sends: posts to store (fetched
/// again from their own server), and for a subscriber, to forward.
async fn receive_content(federation: &Federation, pool: &PgPool, sender: &store::RemoteActor, host: &str, body: &[u8], activity: &Value) -> Result<(), String> {
    let kind = activity["type"].as_str().unwrap_or_default();
    let from_relay = relay::is_followed_relay(pool, host).await.map_err(|err| format!("failed to check the relay: {err}"))?;
    let subscriber = match crate::settings::relay_server() {
        true => relay::accepted_subscriber(pool, &sender.actor_id).await.map_err(|err| format!("failed to check the subscriber: {err}"))?,
        false => None,
    };
    // A subscriber's users send their posts too: from its server.
    let subscriber = match subscriber {
        Some(subscriber) => Some(subscriber),
        None if crate::settings::relay_server() => subscriber_of_host(pool, host).await?,
        None => None,
    };
    if !from_relay && subscriber.is_none() {
        tracing::debug!("ignored {} sent to the instance actor by {}", kind, sender.actor_id);
        return Ok(());
    }
    // A Create or an Announce gives the post; a Delete, what's gone.
    let object = &activity["object"];
    let object_url = match kind {
        "Announce" | "Create" | "Update" | "Delete" => object_id(object),
        _ => None,
    };
    let Some(object_url) = object_url else {
        return Ok(());
    };
    if kind == "Delete" {
        activities::delete_relayed(federation, pool, object_url).await?;
    } else if let Some(post_id) = activities::store_relayed(federation, pool, object_url).await? {
        tracing::info!("stored relayed post {} as {}", object_url, post_id);
    }
    if let Some(subscriber) = subscriber {
        forward(federation, pool, &subscriber.host, kind, object_url, body).await?;
    }
    Ok(())
}

/// The accepted subscriber on `host`, when a user of that server sends.
async fn subscriber_of_host(pool: &PgPool, host: &str) -> Result<Option<relay::Subscriber>, String> {
    let all = relay::subscribers(pool).await.map_err(|err| format!("failed to load the subscribers: {err}"))?;
    Ok(all.into_iter().find(|subscriber| subscriber.host == host && subscriber.state == RelayState::Accepted))
}

/// Forwards what a subscriber sent to the other subscribers: as it is to
/// Mastodon's (they check its own signature), an Announce of the post to
/// LitePub's.
async fn forward(federation: &Federation, pool: &PgPool, from_host: &str, kind: &str, object_url: &str, body: &[u8]) -> Result<(), String> {
    let inboxes = relay::subscriber_inboxes(pool, Some(from_host)).await.map_err(|err| format!("failed to load the subscribers: {err}"))?;
    let (mastodon, litepub): (Vec<_>, Vec<_>) = inboxes.into_iter().partition(|(_, kind)| kind == "mastodon");
    let mastodon: Vec<String> = mastodon.into_iter().map(|(inbox, _)| inbox).collect();
    let litepub: Vec<String> = litepub.into_iter().map(|(inbox, _)| inbox).collect();
    let raw = String::from_utf8_lossy(body);
    if !mastodon.is_empty() {
        store::enqueue(pool, Signer::Instance, &mastodon, &raw).await.map_err(|err| format!("failed to queue: {err}"))?;
    }
    if !litepub.is_empty() {
        let instance = federation.instance_actor_url();
        let activity = match kind {
            "Create" | "Announce" => activities::with_context(json!({
                "id": format!("{instance}/announces/{}", sqlx::types::chrono::Utc::now().timestamp_micros()),
                "type": "Announce",
                "actor": instance,
                "object": object_url,
                "to": [format!("{instance}/followers")],
                "cc": [PUBLIC],
            }))
            .to_string(),
            _ => raw.to_string(),
        };
        store::enqueue(pool, Signer::Instance, &litepub, &activity).await.map_err(|err| format!("failed to queue: {err}"))?;
    }
    federation.wake_deliveries();
    Ok(())
}

/// Accepts (or refuses) a server asking to use this one as its relay.
pub async fn answer_subscriber(federation: &Federation, pool: &PgPool, subscriber: &relay::Subscriber, accepted: bool) -> Result<(), String> {
    let instance = federation.instance_actor_url();
    let follow = json!({
        "id": subscriber.follow_id,
        "type": "Follow",
        "actor": subscriber.actor_id,
        "object": if subscriber.kind == "mastodon" { PUBLIC.to_string() } else { instance.clone() },
    });
    let answer = match accepted {
        true => activities::accept(&instance, &follow),
        false => activities::reject_follow(&instance, &subscriber.actor_id, &subscriber.follow_id),
    };
    activities::send(federation, pool, Signer::Instance, std::slice::from_ref(&subscriber.inbox), &answer).await
}

/// Follows the relay at `url`: an inbox (Mastodon's relays, ending with
/// `/inbox`) or an actor (LitePub's). Returns its id.
pub async fn follow(federation: &Federation, pool: &PgPool, url: &str) -> Result<i64, String> {
    let url = url.trim();
    if !(url.starts_with("https://") || (federation.allow_insecure && url.starts_with("http://"))) {
        return Err("A relay's address starts with https://.".to_string());
    }
    if store::is_banned(pool, url).await.map_err(|err| format!("failed to check {url}: {err}"))? {
        return Err("This server is banned here.".to_string());
    }
    let key = linker::local_key(federation, pool, Signer::Instance).await?;
    let (inbox, actor_id) = if url.trim_end_matches('/').ends_with("/inbox") {
        (url.to_string(), None)
    } else {
        let actor = linker::fetch_actor(federation, pool, url, &key).await.map_err(|err| {
            tracing::info!("failed to fetch the relay {}: {}", url, err);
            "No relay found at this address.".to_string()
        })?;
        (actor.inbox.clone(), Some(actor.actor_id))
    };
    let instance = federation.instance_actor_url();
    let follow_id = format!("{instance}/follows/{}", sqlx::types::chrono::Utc::now().timestamp_micros());
    let follow = activities::with_context(json!({
        "id": follow_id,
        "type": "Follow",
        "actor": instance,
        "object": actor_id.clone().unwrap_or_else(|| PUBLIC.to_string()),
        "to": [actor_id.clone().unwrap_or_else(|| PUBLIC.to_string())],
    }));
    let host = store::host_of(actor_id.as_deref().unwrap_or(url));
    let id = relay::add(pool, url, &inbox, actor_id.as_deref(), &host, &follow_id)
        .await
        .map_err(|err| format!("failed to save the relay: {err}"))?;
    activities::send(federation, pool, Signer::Instance, std::slice::from_ref(&inbox), &follow).await?;
    tracing::info!("following the relay {}", url);
    Ok(id)
}

/// Stops following a relay: it's told (Undo of the Follow).
pub async fn unfollow(federation: &Federation, pool: &PgPool, relay: &relay::Relay) -> Result<(), String> {
    let instance = federation.instance_actor_url();
    let follow = json!({
        "id": relay.follow_id,
        "type": "Follow",
        "actor": instance,
        "object": relay.actor_id.clone().unwrap_or_else(|| PUBLIC.to_string()),
    });
    let undo = activities::undo(&follow);
    activities::send(federation, pool, Signer::Instance, std::slice::from_ref(&relay.inbox), &undo).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_public_collection_has_several_names() {
        assert!(is_public_collection(PUBLIC) && is_public_collection("as:Public") && is_public_collection("Public"));
        assert!(!is_public_collection("https://relay.example/actor"));
    }
}

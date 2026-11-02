//! Browser notifications (Web Push): each new notification and message is also sent to
//! the browsers where the user turned them on, through their browser's push
//! service (Mozilla, Google, Apple, Microsoft...), and shown by the service
//! worker `assets/sw.js` even when no Tison page is open.
//!
//! What is sent is encrypted for the browser (RFC 8291, aes128gcm): the push
//! service only sees where it goes. Requests are signed with this server's
//! key (VAPID, RFC 8292), the one browsers subscribed with.

use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use reqwest::Url;
use ring::{
    aead, agreement,
    error::Unspecified,
    hkdf,
    rand::{SecureRandom, SystemRandom},
    signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair},
};
use serde_json::json;
use sqlx::{PgPool, postgres::PgListener};

use crate::{
    API::{base64, linker},
    entities::{
        message::{self, Message},
        notification::{self, Change, Notification},
        push::{self as subscriptions, Subscription},
    },
};

/// Name of the key in `server_keys`.
const KEY_NAME: &str = "vapid";
/// How long the push service keeps a notification for a browser that is off.
const TTL_SECONDS: u32 = 2 * 24 * 3600;
/// Size of the encrypted record: one is enough, push services take 4 KB.
const RECORD_SIZE: u32 = 4096;
/// How long a signature lasts (24 hours at most).
const SIGNATURE_SECONDS: u64 = 12 * 3600;
/// The longest text shown, in letters.
const BODY_CHARS: usize = 200;

pub struct Push {
    key: EcdsaKeyPair,
    /// The public key, base64url: browsers subscribe with it.
    pub public_key: String,
    /// Who sends, for push services to contact: the site, or an address.
    subject: String,
    allow_insecure: bool,
    client: reqwest::Client,
    rng: SystemRandom,
}

impl Push {
    /// Loads the key, made the first time.
    pub async fn load(pool: &PgPool, base_url: &str, allow_insecure: bool) -> Result<Self, String> {
        let rng = SystemRandom::new();
        let fresh = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng)
            .map_err(|_| "failed to make the browser notifications key".to_string())?;
        let stored = subscriptions::server_key(pool, KEY_NAME, || base64::encode(fresh.as_ref()))
            .await
            .map_err(|err| format!("failed to load the browser notifications key: {err}"))?;
        let pkcs8 = base64::decode(&stored).ok_or("the browser notifications key is not base64")?;
        let key = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &pkcs8, &rng)
            .map_err(|err| format!("invalid browser notifications key: {err}"))?;
        let base_url = base_url.trim_end_matches('/');
        // Apple only takes https:// or mailto:.
        let subject = if base_url.starts_with("https://") {
            base_url.to_string()
        } else {
            let host = crate::entities::author::host_of(base_url);
            format!("mailto:admin@{}", host.split(':').next().unwrap_or_default())
        };
        Ok(Self {
            public_key: base64::encode_url(key.public_key().as_ref()),
            key,
            subject,
            allow_insecure,
            client: linker::client(base_url, allow_insecure, Duration::from_secs(15))?,
            rng,
        })
    }

    /// Whether browsers may give this address: their push service, never
    /// this machine or its network.
    pub fn check_endpoint(&self, endpoint: &str) -> Result<Url, String> {
        let url = Url::parse(endpoint).map_err(|err| format!("invalid address: {err}"))?;
        linker::check_url(&url, self.allow_insecure)?;
        Ok(url)
    }

    /// `Authorization` for a push service: `vapid t=<JWT>, k=<public key>`.
    fn authorization(&self, endpoint: &Url) -> Result<String, Unspecified> {
        let expires = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| Unspecified)?.as_secs() + SIGNATURE_SECONDS;
        let claims = json!({
            "aud": endpoint.origin().ascii_serialization(),
            "exp": expires,
            "sub": self.subject,
        });
        let signed = format!(
            "{}.{}",
            base64::encode_url(br#"{"typ":"JWT","alg":"ES256"}"#),
            base64::encode_url(claims.to_string().as_bytes())
        );
        let signature = self.key.sign(&self.rng, signed.as_bytes())?;
        Ok(format!("vapid t={signed}.{}, k={}", base64::encode_url(signature.as_ref()), self.public_key))
    }

    /// Sends a test to the user's browsers: (accepted by their push service, browsers).
    pub async fn send_test(&self, pool: &PgPool, user_id: i32) -> Result<(usize, usize), sqlx::Error> {
        let browsers = subscriptions::of_user(pool, user_id).await?;
        let payload = json!({
            "title": "Tison",
            "body": "Browser notifications work in this browser.",
            "url": "/settings",
            "tag": "test",
        })
        .to_string();
        let mut accepted = 0;
        for browser in &browsers {
            if self.send(pool, browser, payload.as_bytes()).await {
                accepted += 1;
            }
        }
        Ok((accepted, browsers.len()))
    }

    /// Sends `payload` to one browser: `true` when its push service took it.
    /// Forgets the browser when its push service doesn't know it anymore.
    async fn send(&self, pool: &PgPool, subscription: &Subscription, payload: &[u8]) -> bool {
        let prepared = (|| {
            let url = self.check_endpoint(&subscription.endpoint)?;
            let browser_key = base64::decode_url(&subscription.p256dh).ok_or("invalid browser key")?;
            let auth = base64::decode_url(&subscription.auth).ok_or("invalid browser secret")?;
            let body = encrypt(&browser_key, &auth, payload, &self.rng).map_err(|_| "failed to encrypt")?;
            let authorization = self.authorization(&url).map_err(|_| "failed to sign")?;
            Ok::<_, String>((url, body, authorization))
        })();
        let (url, body, authorization) = match prepared {
            Ok(prepared) => prepared,
            Err(err) => {
                tracing::warn!("browser notifications: forgetting subscription {}: {}", subscription.id, err);
                forget(pool, subscription.id).await;
                return false;
            }
        };
        let response = self
            .client
            .post(url)
            .header("TTL", TTL_SECONDS)
            .header("Urgency", "normal")
            .header("Content-Encoding", "aes128gcm")
            .header("Content-Type", "application/octet-stream")
            .header("Authorization", authorization)
            .body(body)
            .send()
            .await;
        match response {
            Ok(response) if response.status().is_success() => return true,
            // Turned off in the browser, or expired.
            Ok(response) if matches!(response.status().as_u16(), 404 | 410) => forget(pool, subscription.id).await,
            Ok(response) => {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                tracing::warn!(
                    "browser notifications: push service refused ({}): {}",
                    status,
                    text.chars().take(200).collect::<String>()
                );
            }
            Err(err) => tracing::warn!("browser notifications: failed to reach the push service: {}", err),
        }
        false
    }
}

async fn forget(pool: &PgPool, id: i64) {
    if let Err(err) = subscriptions::delete(pool, id).await {
        tracing::warn!("browser notifications: failed to delete subscription {}: {}", id, err);
    }
}

/// Length of what HKDF makes, for `ring`.
struct Length(usize);

impl hkdf::KeyType for Length {
    fn len(&self) -> usize {
        self.0
    }
}

fn expand(prk: &hkdf::Prk, info: &[&[u8]], out: &mut [u8]) -> Result<(), Unspecified> {
    prk.expand(info, Length(out.len()))?.fill(out)
}

/// `payload` encrypted for the browser with key `browser_key` (P-256,
/// uncompressed) and secret `auth`, with a new key of ours.
fn encrypt(browser_key: &[u8], auth: &[u8], payload: &[u8], rng: &SystemRandom) -> Result<Vec<u8>, Unspecified> {
    let private = agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, rng)?;
    let public = private.compute_public_key()?;
    let mut salt = [0u8; 16];
    rng.fill(&mut salt)?;
    let browser = agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, browser_key);
    agreement::agree_ephemeral(private, &browser, |secret| {
        encrypt_with(secret, auth, browser_key, public.as_ref(), &salt, payload)
    })?
}

/// RFC 8291 from the shared secret: the body sent to the push service.
fn encrypt_with(
    shared_secret: &[u8],
    auth: &[u8],
    browser_key: &[u8],
    our_key: &[u8],
    salt: &[u8; 16],
    payload: &[u8],
) -> Result<Vec<u8>, Unspecified> {
    let auth_prk = hkdf::Salt::new(hkdf::HKDF_SHA256, auth).extract(shared_secret);
    let mut ikm = [0u8; 32];
    expand(&auth_prk, &[b"WebPush: info\0", browser_key, our_key], &mut ikm)?;
    let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, salt).extract(&ikm);
    let mut content_key = [0u8; 16];
    expand(&prk, &[b"Content-Encoding: aes128gcm\0"], &mut content_key)?;
    let mut nonce = [0u8; 12];
    expand(&prk, &[b"Content-Encoding: nonce\0"], &mut nonce)?;

    // A single record: the payload, then 2 for "last record".
    let mut record = payload.to_vec();
    record.push(2);
    let key = aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_128_GCM, &content_key)?);
    key.seal_in_place_append_tag(aead::Nonce::assume_unique_for_key(nonce), aead::Aad::empty(), &mut record)?;

    let mut body = Vec::with_capacity(16 + 4 + 1 + our_key.len() + record.len());
    body.extend_from_slice(salt);
    body.extend_from_slice(&RECORD_SIZE.to_be_bytes());
    body.push(our_key.len() as u8);
    body.extend_from_slice(our_key);
    body.extend_from_slice(&record);
    Ok(body)
}

/// Cuts `text` to `BODY_CHARS` letters.
fn shorten(text: String) -> String {
    if text.chars().count() > BODY_CHARS {
        text.chars().take(BODY_CHARS - 1).collect::<String>() + "…"
    } else {
        text
    }
}

/// What `assets/sw.js` shows for a message. One per conversation: a new
/// message replaces the one before (`renotify` still alerts).
fn message_payload(conversation_id: i64, message: &Message) -> String {
    json!({
        "title": message.author.display_name(),
        "body": shorten(message.body.clone()),
        "icon": message.author.photo_url(),
        "url": format!("/messenger/{conversation_id}"),
        // Not shown while this page is in front: it shows the message already.
        "page": format!("/messenger/{conversation_id}"),
        "tag": format!("conversation-{conversation_id}"),
        "renotify": true,
    })
    .to_string()
}

/// What `assets/sw.js` shows: who, what happened, and where clicking leads.
fn payload(notification: &Notification) -> String {
    let action = notification.kind.action().trim_start_matches("· ");
    let mut text = match &notification.community {
        Some(community) => format!("{action} {}", community.name),
        None => action.to_string(),
    };
    if let Some(first) = text.get(..1) {
        text = first.to_uppercase() + &text[1..];
    }
    if let Some(comment) = &notification.comment_excerpt {
        text = format!("{text}\n“{comment}”");
    } else if let Some(post) = &notification.post_excerpt {
        text = format!("{text}\n{post}{}", if notification.post_truncated { "…" } else { "" });
    }
    json!({
        "title": notification.actor_name(),
        "body": shorten(text),
        "icon": notification.actor_photo(),
        "url": format!("/notifications/{}/open", notification.id),
        "page": "/notifications",
        "tag": format!("notification-{}", notification.id),
    })
    .to_string()
}

/// What to send to a user's browsers.
#[derive(Clone, Copy)]
enum Pushed {
    Notification(i64),
    Message(i64),
}

/// Sends a new notification or message to the user's browsers.
async fn deliver(push: Arc<Push>, pool: PgPool, user_id: i32, pushed: Pushed) {
    let result = async {
        let browsers = subscriptions::of_user(&pool, user_id).await?;
        if browsers.is_empty() {
            return Ok(());
        }
        let payload = match pushed {
            Pushed::Notification(id) => {
                if !notification::claim_push(&pool, user_id, id).await? {
                    return Ok(());
                }
                // Muted, or deleted meanwhile.
                let Some(found) = notification::find(&pool, user_id, id).await? else {
                    return Ok(());
                };
                payload(&found)
            }
            Pushed::Message(id) => {
                let Some(conversation_id) = message::claim_push(&pool, id, user_id).await? else {
                    return Ok(());
                };
                let Some(found) = message::find_one(&pool, id, user_id).await? else {
                    return Ok(());
                };
                message_payload(conversation_id, &found)
            }
        };
        for browser in &browsers {
            push.send(&pool, browser, payload.as_bytes()).await;
        }
        Ok::<_, sqlx::Error>(())
    };
    if let Err(err) = result.await {
        let (what, id) = match pushed {
            Pushed::Notification(id) => ("notification", id),
            Pushed::Message(id) => ("message", id),
        };
        tracing::warn!("browser notifications: failed to send {} {}: {}", what, id, err);
    }
}

/// Sends each new notification, forever, like `handlers::notification::listen`.
pub async fn run(push: Arc<Push>, pool: PgPool) {
    loop {
        let mut listener = match PgListener::connect_with(&pool).await {
            Ok(listener) => listener,
            Err(err) => {
                tracing::warn!("browser notifications: failed to connect, retrying: {}", err);
                tokio::time::sleep(Duration::from_secs(10)).await;
                continue;
            }
        };
        if let Err(err) = listener.listen(notification::CHANNEL).await {
            tracing::warn!("browser notifications: failed to listen, retrying: {}", err);
            tokio::time::sleep(Duration::from_secs(10)).await;
            continue;
        }
        loop {
            match listener.recv().await {
                Ok(signal) => {
                    let pushed = match Change::parse(signal.payload()) {
                        Some((user_id, Change::Added(id))) => Some((user_id, Pushed::Notification(id))),
                        Some((user_id, Change::NewMessage(id))) => Some((user_id, Pushed::Message(id))),
                        _ => None,
                    };
                    if let Some((user_id, pushed)) = pushed {
                        tokio::spawn(deliver(push.clone(), pool.clone(), user_id, pushed));
                    }
                }
                // Reconnects by itself: notifications made meanwhile aren't sent.
                Err(err) => {
                    tracing::warn!("browser notifications: connection lost: {}", err);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(text: &str) -> Vec<u8> {
        base64::decode_url(text).unwrap()
    }

    #[test]
    fn encryption_matches_rfc_8291() {
        // Its example, appendix A.
        let body = encrypt_with(
            &bytes("kyrL1jIIOHEzg3sM2ZWRHDRB62YACZhhSlknJ672kSs"),
            &bytes("BTBZMqHH6r4Tts7J_aSIgg"),
            &bytes("BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4"),
            &bytes("BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8"),
            &bytes("DGv6ra1nlYgDCS1FRnbzlw").try_into().unwrap(),
            b"When I grow up, I want to be a watermelon",
        )
        .unwrap();
        assert_eq!(
            base64::encode_url(&body),
            "DGv6ra1nlYgDCS1FRnbzlwAAEABBBP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A_yl95bQpu6cVPTpK4Mqgkf1CXztLVBSt2Ks3oZwbuwXPXLWyouBWLVWGNWQexSgSxsj_Qulcy4a-fN"
        );
    }

    #[test]
    fn browsers_can_decrypt() {
        // Plays the browser: its key, then decrypting what was sent.
        let rng = SystemRandom::new();
        let browser_private = agreement::EphemeralPrivateKey::generate(&agreement::ECDH_P256, &rng).unwrap();
        let browser_key = browser_private.compute_public_key().unwrap().as_ref().to_vec();
        let auth = [7u8; 16];
        let body = encrypt(&browser_key, &auth, b"hello", &rng).unwrap();

        let salt: [u8; 16] = body[..16].try_into().unwrap();
        assert_eq!(u32::from_be_bytes(body[16..20].try_into().unwrap()), RECORD_SIZE);
        let our_key = &body[21..21 + usize::from(body[20])];
        let server = agreement::UnparsedPublicKey::new(&agreement::ECDH_P256, our_key);
        let (content_key, nonce) = agreement::agree_ephemeral(browser_private, &server, |secret| {
            let auth_prk = hkdf::Salt::new(hkdf::HKDF_SHA256, &auth).extract(secret);
            let mut ikm = [0u8; 32];
            expand(&auth_prk, &[b"WebPush: info\0", &browser_key, our_key], &mut ikm).unwrap();
            let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, &salt).extract(&ikm);
            let (mut content_key, mut nonce) = ([0u8; 16], [0u8; 12]);
            expand(&prk, &[b"Content-Encoding: aes128gcm\0"], &mut content_key).unwrap();
            expand(&prk, &[b"Content-Encoding: nonce\0"], &mut nonce).unwrap();
            (content_key, nonce)
        })
        .unwrap();
        let key = aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_128_GCM, &content_key).unwrap());
        let mut record = body[21 + our_key.len()..].to_vec();
        let plain = key.open_in_place(aead::Nonce::assume_unique_for_key(nonce), aead::Aad::empty(), &mut record).unwrap();
        assert_eq!(plain, b"hello\x02");
    }

    #[test]
    fn signatures_verify() {
        // reqwest needs it, like in `main`.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap();
        let key = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &rng).unwrap();
        let push = Push {
            public_key: base64::encode_url(key.public_key().as_ref()),
            key,
            subject: "mailto:admin@example.com".to_string(),
            allow_insecure: false,
            client: reqwest::Client::new(),
            rng,
        };
        let endpoint = Url::parse("https://updates.push.services.mozilla.com/wpush/v2/abc").unwrap();
        let header = push.authorization(&endpoint).unwrap();
        let (token, public_key) = header.strip_prefix("vapid t=").unwrap().split_once(", k=").unwrap();
        assert_eq!(public_key, push.public_key);
        let (signed, signature) = token.rsplit_once('.').unwrap();
        let claims = String::from_utf8(bytes(signed.split_once('.').unwrap().1)).unwrap();
        assert!(claims.contains(r#""aud":"https://updates.push.services.mozilla.com""#), "{claims}");
        ring::signature::UnparsedPublicKey::new(&ring::signature::ECDSA_P256_SHA256_FIXED, bytes(public_key))
            .verify(signed.as_bytes(), &bytes(signature))
            .unwrap();
        assert!(push.check_endpoint("http://127.0.0.1/push").is_err());
    }
}

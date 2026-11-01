//! ActivityPub JSON: the actors of local users and communities, posts and
//! comments as notes, the activities sent when something happens, and the
//! handling of the activities other servers send.
//!
//! Built with `json!` rather than typed structs: the format has many optional
//! fields and Tison only reads a few of them.

use serde_json::{Value, json};
use sqlx::{
    PgPool,
    types::chrono::{DateTime, Utc},
};

use super::{
    Federation,
    linker::{self, LocalKey},
};
use crate::entities::{
    author::{Author, RemoteAuthor},
    comment::{self, Comment},
    community::{self, Community},
    federation::{self as store, Reacted, RemoteActor, RemoteReaction, Signer},
    media::{self, NewAttachment},
    message,
    notification,
    post::{self, BODY_MAX_CHARS, Kind, Post, RemoteObject, RemotePost, Visibility},
    user::{self, User},
};

/// Addressing a post to this makes it public.
pub const PUBLIC: &str = "https://www.w3.org/ns/activitystreams#Public";
const ACTIVITY_STREAMS: &str = "https://www.w3.org/ns/activitystreams";
const SECURITY: &str = "https://w3id.org/security/v1";

/// Like `2026-10-30T17:21:00Z`, the format Mastodon expects.
fn iso(time: &DateTime<Utc>) -> String {
    time.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Adds `@context`, only needed on the outermost object.
pub fn with_context(mut object: Value) -> Value {
    if let Value::Object(fields) = &mut object {
        fields.insert("@context".to_string(), json!(ACTIVITY_STREAMS));
    }
    object
}

/// Tison text is plain: escaped, addresses as links (see `linkify`), blank
/// lines become paragraphs and line breaks `<br>`, which every Fediverse
/// server displays.
pub fn text_to_html(text: &str) -> String {
    text.split("\n\n")
        .map(str::trim)
        .filter(|paragraph| !paragraph.is_empty())
        .map(|paragraph| format!("<p>{}</p>", crate::linkify::to_html(paragraph).replace('\n', "<br>")))
        .collect()
}

/// The reverse, for notes of other servers: line breaks and paragraphs kept,
/// other tags removed (links keep their text), entities decoded. Tison
/// shows the result as plain text, escaped, so nothing here can inject HTML.
pub fn html_to_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut rest = html;
    // The open link: its address, and where its text starts.
    let mut link: Option<(String, usize)> = None;
    while let Some(start) = rest.find('<') {
        text.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('>') else {
            rest = "";
            break;
        };
        let original = rest[start + 1..start + end].trim();
        let tag = original.to_ascii_lowercase();
        let name: String = tag.trim_start_matches('/').chars().take_while(char::is_ascii_alphanumeric).collect();
        match (name.as_str(), tag.starts_with('/')) {
            ("br", _) => text.push('\n'),
            ("p" | "div" | "blockquote" | "li", true) => text.push_str("\n\n"),
            ("a", false) => link = link_address(original).map(|href| (href, text.len())),
            // A link hiding its address behind words (Friendica's
            // "[url=...]photos[/url]"): the address is kept after them.
            ("a", true) => {
                if let Some((href, text_start)) = link.take() {
                    let shown = text[text_start..].trim();
                    let hides_address = !shown.is_empty()
                        && !shown.contains("://")
                        && !shown.starts_with("www.")
                        && !shown.starts_with(['@', '#']);
                    if hides_address {
                        text.push_str(&format!(" ({href})"));
                    }
                }
            }
            _ => {}
        }
        rest = &rest[start + end + 1..];
    }
    text.push_str(rest);
    let decoded = decode_entities(&text);
    // At most one blank line in a row.
    let mut cleaned = String::with_capacity(decoded.len());
    for line in decoded.split('\n') {
        if line.trim().is_empty() && cleaned.ends_with("\n\n") {
            continue;
        }
        cleaned.push_str(line.trim_end());
        cleaned.push('\n');
    }
    cleaned.trim().to_string()
}

/// The web address of an `<a href="...">` tag, from inside its `<` and `>`.
fn link_address(tag: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let after = &tag[lower.find("href=")? + 5..];
    let href = match after.chars().next()? {
        quote @ ('"' | '\'') => after[1..].split(quote).next()?,
        _ => after.split(|letter: char| letter.is_whitespace()).next()?,
    };
    let href = href.trim();
    (href.starts_with("https://") || href.starts_with("http://")).then(|| href.to_string())
}

pub(crate) fn decode_entities(text: &str) -> String {
    let mut decoded = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        decoded.push_str(&rest[..start]);
        rest = &rest[start..];
        let entity = rest[1..].find(';').filter(|&end| end <= 10).map(|end| &rest[1..end + 1]);
        let letter = entity.and_then(|entity| match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|number| number.parse().ok()))
                .and_then(char::from_u32),
        });
        match (entity, letter) {
            (Some(entity), Some(letter)) => {
                decoded.push(letter);
                rest = &rest[entity.len() + 2..];
            }
            _ => {
                decoded.push('&');
                rest = &rest[1..];
            }
        }
    }
    decoded.push_str(rest);
    decoded
}

/// Ids in `to`, `cc`, `audience` and mentions of an object: who it's for.
fn audience(object: &Value) -> Vec<&str> {
    let mut ids = Vec::new();
    for field in ["to", "cc", "audience"] {
        match &object[field] {
            Value::String(id) => ids.push(id.as_str()),
            Value::Array(items) => ids.extend(items.iter().filter_map(object_id)),
            _ => {}
        }
    }
    if let Value::Array(tags) = &object["tag"] {
        ids.extend(
            tags.iter()
                .filter(|tag| tag["type"] == "Mention")
                .filter_map(|tag| tag["href"].as_str()),
        );
    }
    ids
}

/// Public, or "home" on Misskey (unlisted): addressed to everyone.
/// Followers-only and direct notes are never published in a community.
fn is_public(object: &Value) -> bool {
    audience(object)
        .iter()
        .any(|id| matches!(*id, PUBLIC | "as:Public" | "Public"))
}

/// Addressed to the followers of `sender` (Mastodon's "followers only",
/// Misskey's "followers", Friendica's and Hubzilla's restricted posts),
/// rather than only to the people it mentions: its followers collection
/// (from its profile), else an address of its server looking like one
/// (`/users/alice/followers` on Mastodon, `/followers/alice` on Friendica).
fn is_for_followers(object: &Value, sender: &RemoteActor) -> bool {
    audience(object).iter().any(|id| {
        sender.followers_url.as_deref() == Some(*id)
            || ((id.ends_with("/followers") || id.contains("/followers/")) && linker::same_host(id, &sender.actor_id))
    })
}

/// A private message: Pleroma's chat message, or a note neither public nor
/// for the sender's followers (Mastodon's "direct", Hubzilla's private mail,
/// Misskey's "specified").
fn is_message(object: &Value, sender: &RemoteActor) -> bool {
    object["type"] == "ChatMessage" || (is_note(object) && !is_public(object) && !is_for_followers(object, sender))
}

/// Types of objects Tison shows as posts: Mastodon's polls are Questions,
/// Lemmy's posts Pages.
fn is_note(object: &Value) -> bool {
    matches!(object["type"].as_str(), Some("Note" | "Page" | "Article" | "Question" | "Video" | "Audio" | "Event"))
}

/// Who wrote an object: one address, or the first of several (PeerTube lists
/// the channel too).
fn attributed_to(object: &Value) -> Option<&str> {
    match &object["attributedTo"] {
        Value::Array(items) => items
            .iter()
            .find(|item| item["type"] == "Person")
            .or_else(|| items.first())
            .and_then(object_id),
        other => object_id(other),
    }
}

// --- Objects ---

/// The actor of a local user: what other servers show as their profile.
pub fn person(federation: &Federation, user: &User, public_key_pem: &str) -> Value {
    let id = federation.actor_url(user.public_id);
    let mut person = json!({
        "@context": [
            ACTIVITY_STREAMS,
            SECURITY,
            { "manuallyApprovesFollowers": "as:manuallyApprovesFollowers" },
        ],
        "id": id,
        "type": "Person",
        "preferredUsername": user.username,
        "name": user.display_name(),
        "summary": text_to_html(&user.bio),
        "url": federation.profile_url(&user.username),
        "inbox": format!("{id}/inbox"),
        "outbox": format!("{id}/outbox"),
        "followers": format!("{id}/followers"),
        "following": format!("{id}/following"),
        "endpoints": { "sharedInbox": federation.shared_inbox_url() },
        // Every follow request is accepted.
        "manuallyApprovesFollowers": false,
        "published": iso(&user.created_at),
        "publicKey": {
            "id": federation.key_id(user.public_id),
            "owner": id,
            "publicKeyPem": public_key_pem,
        },
    });
    if let Some(photo) = &user.photo_url {
        person["icon"] = json!({ "type": "Image", "url": federation.absolute_url(photo) });
    }
    if let Some(background) = &user.background_url {
        person["image"] = json!({ "type": "Image", "url": federation.absolute_url(background) });
    }
    person
}

/// The actor of a public community: a Group, like Lemmy's communities.
pub fn group(federation: &Federation, community: &Community, public_key_pem: &str) -> Value {
    let id = federation.community_url(community.public_id);
    let mut group = json!({
        "@context": [
            ACTIVITY_STREAMS,
            SECURITY,
            {
                "manuallyApprovesFollowers": "as:manuallyApprovesFollowers",
                "lemmy": "https://join-lemmy.org/ns#",
                "postingRestrictedToMods": "lemmy:postingRestrictedToMods",
            },
        ],
        "id": id,
        "type": "Group",
        "preferredUsername": community.slug,
        "name": community.name,
        "summary": text_to_html(&community.description),
        "url": federation.community_page_url(&community.slug),
        "inbox": format!("{id}/inbox"),
        "outbox": format!("{id}/outbox"),
        "followers": format!("{id}/followers"),
        "attributedTo": format!("{id}/moderators"),
        "endpoints": { "sharedInbox": federation.shared_inbox_url() },
        "manuallyApprovesFollowers": false,
        "postingRestrictedToMods": false,
        "published": iso(&community.created_at),
        "publicKey": {
            "id": federation.community_key_id(community.public_id),
            "owner": id,
            "publicKeyPem": public_key_pem,
        },
    });
    if let Some(photo) = &community.photo_url {
        group["icon"] = json!({ "type": "Image", "url": federation.absolute_url(photo) });
    }
    group
}

/// The Group of a community that left the Fediverse: its key, nothing about
/// the community.
pub fn group_without_details(federation: &Federation, community: &Community, public_key_pem: &str) -> Value {
    let id = federation.community_url(community.public_id);
    json!({
        "@context": [ACTIVITY_STREAMS, SECURITY],
        "id": id,
        "type": "Group",
        "preferredUsername": format!("community{}", community.public_id),
        "inbox": format!("{id}/inbox"),
        "manuallyApprovesFollowers": true,
        "discoverable": false,
        "publicKey": {
            "id": federation.community_key_id(community.public_id),
            "owner": id,
            "publicKeyPem": public_key_pem,
        },
    })
}

/// The Note of a post: its own address for posts written here, the one of its
/// server for posts received.
pub fn note_id(federation: &Federation, post: &Post) -> String {
    match &post.ap_id {
        Some(ap_id) => ap_id.clone(),
        None => federation.post_object_url(post.id),
    }
}

/// The Note of a comment: same rule as `note_id`.
pub fn comment_note_id(federation: &Federation, comment: &Comment) -> String {
    match &comment.ap_id {
        Some(ap_id) => ap_id.clone(),
        None => federation.comment_object_url(comment.id),
    }
}

/// A public post written here as a Note. `None` for posts of other servers.
pub fn post_note(federation: &Federation, post: &Post) -> Option<Value> {
    let author = post.author.local()?;
    let mut cc = vec![federation.followers_url(author.public_id)];
    let mut note = json!({
        "id": federation.post_object_url(post.id),
        "type": "Note",
        "attributedTo": federation.actor_url(author.public_id),
        "content": text_to_html(&post.body),
        "url": federation.post_page_url(post.id),
        "published": iso(&post.created_at),
        "to": [PUBLIC],
        // Blurs the photos, like Pixelfed's "sensitive".
        "sensitive": post.sensitive,
        // Counts shown by servers that read them, like Mastodon 4.3+.
        "likes": { "type": "Collection", "totalItems": post.like_count },
        "shares": { "type": "Collection", "totalItems": post.share_count },
    });
    // Its photos: Images as Pixelfed sends them, which Mastodon reads too.
    if post.kind == Kind::Post && !post.attachments.is_empty() {
        let photos: Vec<Value> = post
            .attachments
            .iter()
            .map(|photo| {
                let url = match photo.url.starts_with('/') {
                    true => format!("{}{}", federation.base_url, photo.url),
                    false => photo.url.clone(),
                };
                let mut image = json!({
                    "type": "Image",
                    "mediaType": photo.media_type,
                    "url": url,
                    "name": if photo.description.is_empty() { Value::Null } else { json!(photo.description) },
                });
                if let (Some(width), Some(height)) = (photo.width, photo.height) {
                    image["width"] = json!(width);
                    image["height"] = json!(height);
                }
                image
            })
            .collect();
        note["attachment"] = json!(photos);
        // Pixelfed hides the comment box when it's false.
        note["commentsEnabled"] = json!(true);
    }
    if let Some(edited_at) = &post.edited_at {
        note["updated"] = json!(iso(edited_at));
    }
    if let Some(community) = &post.community {
        // Lemmy's way (FEP-1b12) to say which group it's in.
        let group = federation.community_url(community.public_id);
        cc.push(group.clone());
        note["audience"] = json!(group);
    }
    if let Some(title) = post.title.as_ref().filter(|_| post.is_article()) {
        // An article, as WriteFreely sends them: Mastodon shows its title
        // and a link to its page, WriteFreely-like readers all of it.
        let base = &federation.base_url;
        note["type"] = json!("Article");
        note["name"] = json!(title);
        note["content"] = json!(crate::markdown::to_html(&post.body, &|address| match address.starts_with('/') {
            true => format!("{base}{address}"),
            false => address.to_string(),
        }));
        note["mediaType"] = json!("text/html");
        note["source"] = json!({ "content": post.body, "mediaType": "text/markdown" });
        note["wordCount"] = json!(post.body.split_whitespace().count());
        note["tag"] = json!([]);
    }
    if post.kind != Kind::Post {
        media_object(federation, post, &mut note);
    }
    note["cc"] = json!(cc);
    Some(note)
}

/// A video as PeerTube sends them (`Video`), an episode as Funkwhale does
/// (`Audio`), a live stream as Owncast's "is live" notes. Mastodon shows the
/// first two with their title and link, and plays the attached file.
fn media_object(federation: &Federation, post: &Post, note: &mut Value) {
    let base = &federation.base_url;
    let absolute = |url: &str| if url.starts_with('/') { format!("{base}{url}") } else { url.to_string() };
    let page = federation.post_page_url(post.id);
    let title = post.title_text();
    if let Some(event) = post.event.as_ref().filter(|_| post.kind == Kind::Event) {
        event_object(post, event, note, &absolute);
        return;
    }
    if post.kind == Kind::Live {
        let state = if post.is_on_air() { "🔴 Live now" } else { "Was live" };
        let link = crate::linkify::escape(&page);
        note["content"] = json!(format!(
            "<p>{state}: <strong>{}</strong></p>{}<p><a href=\"{link}\">{link}</a></p>",
            crate::linkify::escape(title),
            text_to_html(&post.body)
        ));
        return;
    }
    let file = post.attachments.iter().find(|attachment| attachment.is_video() || attachment.is_audio());
    note["type"] = json!(if post.kind == Kind::Video { "Video" } else { "Audio" });
    note["name"] = json!(title);
    note["mediaType"] = json!("text/html");
    let mut links = vec![json!({ "type": "Link", "mediaType": "text/html", "href": page })];
    // Every quality, like PeerTube lists its resolutions.
    for quality in &post.video_files {
        links.push(json!({
            "type": "Link",
            "mediaType": "video/mp4",
            "href": absolute(&quality.url),
            "width": quality.width,
            "height": quality.height,
            "size": quality.size_bytes,
        }));
    }
    if let Some(file) = file {
        if post.video_files.is_empty() {
            let mut link = json!({ "type": "Link", "mediaType": file.media_type, "href": absolute(&file.url) });
            if let (Some(width), Some(height)) = (file.width, file.height) {
                link["width"] = json!(width);
                link["height"] = json!(height);
            }
            links.push(link);
        }
        note["attachment"] = json!([{
            "type": "Document",
            "mediaType": file.media_type,
            "url": absolute(&file.url),
            "name": title,
            "width": file.width,
            "height": file.height,
        }]);
    }
    note["url"] = json!(links);
    if let Some(seconds) = post.duration_seconds {
        note["duration"] = json!(crate::av::iso_duration(seconds));
    }
    let picture = post.poster_url.clone().or_else(|| post.podcast.as_ref().and_then(|podcast| podcast.cover_url.clone()));
    if let Some(picture) = picture {
        let media_type = match picture.rsplit('.').next().map(str::to_ascii_lowercase).as_deref() {
            Some("png") => "image/png",
            Some("webp") => "image/webp",
            Some("gif") => "image/gif",
            _ => "image/jpeg",
        };
        let image = json!({ "type": "Image", "url": absolute(&picture), "mediaType": media_type });
        note["icon"] = json!([image.clone()]);
        note["image"] = image;
    }
    note["commentsEnabled"] = json!(true);
}

/// An event as Mobilizon sends them: its time, place or link online, how to
/// join and how many can come. Mastodon shows its title and a link.
fn event_object(post: &Post, event: &crate::entities::event::EventDetails, note: &mut Value, absolute: &dyn Fn(&str) -> String) {
    note["type"] = json!("Event");
    note["name"] = json!(post.title_text());
    note["mediaType"] = json!("text/html");
    note["startTime"] = json!(event.starts_at.to_rfc3339());
    if let Some(end) = &event.ends_at {
        note["endTime"] = json!(end.to_rfc3339());
    }
    note["timezone"] = json!(event.timezone);
    note["category"] = json!(event.category);
    note["joinMode"] = json!(event.join_mode);
    note["ical:status"] = json!(event.status.to_uppercase());
    note["participantCount"] = json!(event.going);
    if let Some(capacity) = event.capacity {
        note["maximumAttendeeCapacity"] = json!(capacity);
        note["remainingAttendeeCapacity"] = json!(event.places_left().unwrap_or(0));
    }
    note["isOnline"] = json!(event.is_online());
    note["anonymousParticipationEnabled"] = json!(false);
    note["commentsEnabled"] = json!(true);
    note["repliesModerationOption"] = json!("allow_all");
    note["draft"] = json!(false);
    if !event.place_name.trim().is_empty() || !event.address.trim().is_empty() {
        note["location"] = json!({
            "type": "Place",
            "name": if event.place_name.trim().is_empty() { &event.address } else { &event.place_name },
            "address": { "type": "PostalAddress", "streetAddress": event.address },
        });
        if let (Some(latitude), Some(longitude)) = (event.latitude, event.longitude) {
            note["location"]["latitude"] = json!(latitude);
            note["location"]["longitude"] = json!(longitude);
        }
    }
    let mut attachments = Vec::new();
    if let Some(banner) = &post.poster_url {
        attachments.push(json!({ "type": "Document", "mediaType": "image/jpeg", "url": absolute(banner), "name": "Banner" }));
    }
    // Only participants see a link kept for them.
    if let Some(online) = event.online_url.as_ref().filter(|_| event.online_public) {
        attachments.push(json!({ "type": "Link", "mediaType": "text/html", "name": "Website", "href": online }));
    }
    if !attachments.is_empty() {
        note["attachment"] = json!(attachments);
    }
}

/// The details of an Event of another server, see `remote_event`.
struct RemoteEvent {
    starts_at: DateTime<Utc>,
    ends_at: Option<DateTime<Utc>>,
    timezone: String,
    place_name: String,
    address: String,
    online_url: Option<String>,
    category: &'static str,
    capacity: Option<i32>,
    join_mode: &'static str,
    status: &'static str,
    participants: Option<i32>,
    banner: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
}

impl RemoteEvent {
    fn details(&self) -> crate::entities::event::NewEvent<'_> {
        crate::entities::event::NewEvent {
            starts_at: self.starts_at,
            ends_at: self.ends_at,
            timezone: &self.timezone,
            place_name: &self.place_name,
            address: &self.address,
            online_url: self.online_url.as_deref(),
            category: self.category,
            capacity: self.capacity,
            join_mode: self.join_mode,
            status: self.status,
            remote_participants: self.participants,
            // Given by its server for everyone.
            online_public: true,
            latitude: self.latitude,
            longitude: self.longitude,
            series_id: None,
        }
    }
}

/// An Event of another server (Mobilizon, Gancio...): its details, `None`
/// for anything else.
fn remote_event(note: &Value) -> Option<RemoteEvent> {
    if note["type"].as_str() != Some("Event") {
        return None;
    }
    let time = |name: &str| {
        note[name].as_str().and_then(|value| DateTime::parse_from_rfc3339(value).ok()).map(|time| time.with_timezone(&Utc))
    };
    let starts_at = time("startTime")?;
    let ends_at = time("endTime").filter(|end| *end >= starts_at);
    let text = |value: &Value, max: usize| -> String { clean_text(value.as_str().unwrap_or_default().trim()).chars().take(max).collect() };
    let location = &note["location"];
    let address = match &location["address"] {
        Value::String(address) => clean_text(address.trim()),
        Value::Object(_) => ["streetAddress", "postalCode", "addressLocality", "addressRegion", "addressCountry"]
            .iter()
            .filter_map(|part| location["address"][part].as_str().map(str::trim).filter(|part| !part.is_empty()))
            .collect::<Vec<_>>()
            .join(", "),
        _ => String::new(),
    };
    let attachments = match &note["attachment"] {
        Value::Array(items) => items.clone(),
        item @ Value::Object(_) => vec![item.clone()],
        _ => Vec::new(),
    };
    let web = |url: &str| url.starts_with("https://") || url.starts_with("http://");
    let online_url = attachments
        .iter()
        .filter(|item| item["type"] == "Link")
        .filter_map(|item| item["href"].as_str())
        .find(|url| web(url))
        .map(str::to_string);
    let banner = attachments
        .iter()
        .filter(|item| item["mediaType"].as_str().is_some_and(|kind| kind.starts_with("image/")))
        .find_map(|item| attachment_url(&item["url"]).map(|(url, _)| url))
        .or_else(|| attachment_url(&note["image"]["url"]).map(|(url, _)| url))
        .filter(|url| web(url));
    let category = note["category"].as_str().unwrap_or_default().to_ascii_uppercase();
    let category = crate::entities::event::CATEGORIES.iter().find(|(code, _)| *code == category).map_or("MEETING", |(code, _)| code);
    let status = note["ical:status"].as_str().or(note["status"].as_str()).unwrap_or("CONFIRMED").to_ascii_lowercase();
    let status = match status.as_str() {
        "cancelled" => "cancelled",
        "tentative" => "tentative",
        _ => "confirmed",
    };
    let count = |name: &str| note[name].as_i64().and_then(|value| i32::try_from(value).ok()).filter(|value| *value >= 0);
    let timezone = text(&note["timezone"], 64);
    Some(RemoteEvent {
        starts_at,
        ends_at,
        timezone: if timezone.is_empty() { "UTC".to_string() } else { timezone },
        place_name: text(&location["name"], crate::entities::event::PLACE_MAX_CHARS),
        address: address.chars().take(crate::entities::event::ADDRESS_MAX_CHARS).collect(),
        online_url,
        category,
        capacity: count("maximumAttendeeCapacity").filter(|capacity| *capacity > 0),
        join_mode: if note["joinMode"].as_str() == Some("free") || note["joinMode"].is_null() { "free" } else { "restricted" },
        status,
        participants: count("participantCount"),
        banner,
        // Mobilizon gives the place's coordinates: its map.
        latitude: location["latitude"].as_f64().filter(|latitude| (-90.0..=90.0).contains(latitude)),
        longitude: location["longitude"].as_f64().filter(|longitude| (-180.0..=180.0).contains(longitude)),
    })
}

/// A video or episode of another server: its kind, file, picture and
/// length. PeerTube lists its MP4 files in `url`, by resolution (and inside
/// its HLS link), Funkwhale its sound.
struct RemoteMedia {
    kind: Kind,
    file: Option<NewAttachment>,
    poster: Option<String>,
    duration: Option<i32>,
}

/// Links of `url`, those inside other links (`tag`) too.
fn links(value: &Value, found: &mut Vec<Value>) {
    match value {
        Value::Array(items) => items.iter().for_each(|item| links(item, found)),
        Value::Object(fields) => {
            if fields.contains_key("href") {
                found.push(value.clone());
            }
            if let Some(tags) = fields.get("tag") {
                links(tags, found);
            }
        }
        Value::String(url) => found.push(json!({ "href": url })),
        _ => {}
    }
}

fn remote_media(note: &Value) -> Option<RemoteMedia> {
    let kind = match note["type"].as_str()? {
        "Video" => Kind::Video,
        "Audio" => Kind::Episode,
        _ => return None,
    };
    let prefix = if kind == Kind::Video { "video/" } else { "audio/" };
    let mut found = Vec::new();
    links(&note["url"], &mut found);
    let usable = |link: &&Value| {
        let href = link["href"].as_str().unwrap_or_default();
        (href.starts_with("https://") || href.starts_with("http://"))
            && link["mediaType"].as_str().is_some_and(|media_type| media_type.starts_with(prefix))
    };
    // What the media proxy can bring: the largest file up to 720p of known
    // size under its limit, else the smallest.
    let fits = |link: &&Value| link["size"].as_u64().is_none_or(|size| size <= crate::handlers::media::MAX_MEDIA_BYTES as u64);
    let height = |link: &Value| link["height"].as_i64().unwrap_or(0);
    let candidates: Vec<&Value> = found.iter().filter(usable).filter(fits).collect();
    let chosen = candidates
        .iter()
        .filter(|link| height(link) <= 720)
        .max_by_key(|link| height(link))
        .or_else(|| candidates.iter().min_by_key(|link| height(link)))
        .copied();
    let size = |link: &Value, name: &str| link[name].as_i64().and_then(|value| i32::try_from(value).ok()).filter(|value| *value > 0);
    let file = chosen
        .map(|link| {
            NewAttachment::new(
                link["href"].as_str().unwrap_or_default().to_string(),
                link["mediaType"].as_str().unwrap_or_default().to_string(),
                "",
                size(link, "width"),
                size(link, "height"),
            )
        })
        // Or an attachment, as Mastodon-like servers send them.
        .or_else(|| attachments(note).into_iter().find(|attachment| attachment.media_type.starts_with(prefix)));
    // The widest picture up to 1280 pixels, else any.
    let mut pictures = Vec::new();
    for field in ["icon", "image", "thumbnail"] {
        match &note[field] {
            Value::Array(items) => pictures.extend(items.iter().cloned()),
            item @ (Value::Object(_) | Value::String(_)) => pictures.push(item.clone()),
            _ => {}
        }
    }
    let picture_url = |picture: &Value| match picture {
        Value::String(url) => Some(url.clone()),
        other => attachment_url(&other["url"]).map(|(url, _)| url).or_else(|| other["href"].as_str().map(str::to_string)),
    };
    let width = |picture: &Value| picture["width"].as_i64().unwrap_or(0);
    let poster = pictures
        .iter()
        .filter(|picture| width(picture) <= 1280)
        .max_by_key(|picture| width(picture))
        .or_else(|| pictures.first())
        .and_then(picture_url)
        .filter(|url| url.starts_with("https://") || url.starts_with("http://"));
    Some(RemoteMedia { kind, file, poster, duration: crate::av::parse_iso_duration(&note["duration"]) })
}

/// A comment written here as a Note replying to `in_reply_to` (the post's
/// Note, or the comment it answers). `None` for comments of other servers.
pub fn comment_note(federation: &Federation, comment: &Comment, post: &Post, in_reply_to: &str) -> Option<Value> {
    let author = comment.author.local()?;
    let post_author = match &post.author {
        Author::Local(post_author) => federation.actor_url(post_author.public_id),
        Author::Remote(remote) => remote.actor_id.clone(),
    };
    let mut note = json!({
        "id": federation.comment_object_url(comment.id),
        "type": "Note",
        "attributedTo": federation.actor_url(author.public_id),
        "inReplyTo": in_reply_to,
        "content": text_to_html(&comment.body),
        "url": format!("{}#comment-{}", federation.post_page_url(post.id), comment.id),
        "published": iso(&comment.created_at),
        "sensitive": false,
    });
    if let Some(edited_at) = &comment.edited_at {
        note["updated"] = json!(iso(edited_at));
    }
    // On a followers-only post of another server: only its author is told,
    // like a reply on Mastodon to such a post.
    if post.is_followers_only() {
        note["to"] = json!([post_author]);
        note["cc"] = json!([]);
        return Some(note);
    }
    let mut cc = vec![federation.followers_url(author.public_id), post_author];
    if let Some(community) = &post.community {
        let group = federation.community_url(community.public_id);
        cc.push(group.clone());
        note["audience"] = json!(group);
    }
    note["to"] = json!([PUBLIC]);
    note["cc"] = json!(cc);
    Some(note)
}

/// A collection only giving its size: enough for other servers to show counts.
pub fn counted_collection(id: &str, total: i64) -> Value {
    with_context(json!({ "id": id, "type": "OrderedCollection", "totalItems": total }))
}

// --- Activities ---

/// Publishes a Note, addressed like the note itself.
pub fn create(note: &Value) -> Value {
    with_context(json!({
        "id": format!("{}/activity", note["id"].as_str().unwrap_or_default()),
        "type": "Create",
        "actor": note["attributedTo"],
        "published": note["published"],
        "to": note["to"],
        "cc": note["cc"],
        "object": note,
    }))
}

/// A private message of `sender` to an account of another server: a direct
/// Note mentioning it (Mastodon, Hubzilla, Misskey, Pleroma...), or a
/// ChatMessage for a Pleroma chat (`chat`). The Create wraps it.
pub fn direct_message(
    federation: &Federation,
    sender: &User,
    to: &RemoteAuthor,
    message_id: i64,
    body: &str,
    sent_at: &DateTime<Utc>,
    chat: bool,
) -> Value {
    let actor = federation.actor_url(sender.public_id);
    let id = federation.message_object_url(message_id);
    let object = if chat {
        json!({
            "id": id,
            "type": "ChatMessage",
            "attributedTo": actor,
            "to": [to.actor_id],
            "content": crate::linkify::to_html(body).replace('\n', "<br>"),
            "published": iso(sent_at),
        })
    } else {
        let handle = to.handle();
        // The mention starts the text, as Mastodon writes it.
        let mention = format!(
            "<p><span class=\"h-card\"><a href=\"{}\" class=\"u-url mention\">@<span>{}</span></a></span></p>",
            crate::linkify::escape(&to.external_url),
            crate::linkify::escape(&to.username)
        );
        json!({
            "id": id,
            "type": "Note",
            "attributedTo": actor,
            "to": [to.actor_id],
            "cc": [],
            "content": format!("{mention}{}", text_to_html(body)),
            "published": iso(sent_at),
            "sensitive": false,
            "directMessage": true,
            "tag": [{ "type": "Mention", "href": to.actor_id, "name": handle }],
        })
    };
    with_context(json!({
        "id": format!("{id}/activity"),
        "type": "Create",
        "actor": actor,
        "published": iso(sent_at),
        "to": [to.actor_id],
        "cc": [],
        "object": object,
    }))
}

/// An edit of a Note, addressed like the note itself.
pub fn update(note: &Value) -> Value {
    with_context(json!({
        "id": format!("{}/updates/{}", note["id"].as_str().unwrap_or_default(), Utc::now().timestamp_micros()),
        "type": "Update",
        "actor": note["attributedTo"],
        "published": iso(&Utc::now()),
        "to": note["to"],
        "cc": note["cc"],
        "object": note,
    }))
}

/// Deletes the Note `note_id` of `actor`, which other servers replace with a
/// Tombstone.
pub fn delete(actor: &str, note_id: &str, to: &Value, cc: &Value) -> Value {
    with_context(json!({
        "id": format!("{note_id}/delete"),
        "type": "Delete",
        "actor": actor,
        "to": to,
        "cc": cc,
        "object": { "id": note_id, "type": "Tombstone" },
    }))
}

/// A local user following `target` (an actor of another server).
pub fn follow(federation: &Federation, user: &User, target: &str, follow_id: &str) -> Value {
    with_context(json!({
        "id": follow_id,
        "type": "Follow",
        "actor": federation.actor_url(user.public_id),
        "object": target,
    }))
}

/// Ends the follow `follow_id` of `follower` on the local actor `actor`:
/// Mastodon and Misskey remove a follower this way.
pub fn reject_follow(actor: &str, follower: &str, follow_id: &str) -> Value {
    with_context(json!({
        "id": format!("{actor}/rejects/{}", Utc::now().timestamp_micros()),
        "type": "Reject",
        "actor": actor,
        "object": {
            "id": follow_id,
            "type": "Follow",
            "actor": follower,
            "object": actor,
        },
    }))
}

/// Deletes a local community's Group, for servers to forget it.
pub fn delete_group(federation: &Federation, community: &Community) -> Value {
    let actor = federation.community_url(community.public_id);
    with_context(json!({
        "id": format!("{actor}#delete"),
        "type": "Delete",
        "actor": actor,
        "to": [PUBLIC],
        "object": actor,
    }))
}

/// Where a reaction is addressed: the post's author.
fn author_id(federation: &Federation, post: &Post) -> String {
    match &post.author {
        Author::Local(author) => federation.actor_url(author.public_id),
        Author::Remote(remote) => remote.actor_id.clone(),
    }
}

/// A like of a post. `liked_at` makes the id unique: liking again after
/// removing the like is a new activity.
pub fn like(federation: &Federation, liker: &User, post: &Post, liked_at: &DateTime<Utc>) -> Value {
    let actor = federation.actor_url(liker.public_id);
    json!({
        "id": format!("{actor}/likes/{}/{}", post.id, liked_at.timestamp_micros()),
        "type": "Like",
        "actor": actor,
        "object": note_id(federation, post),
        "to": [author_id(federation, post)],
        "cc": [PUBLIC],
    })
}

/// A like of a comment: its Note, written here or on another server.
pub fn like_comment(federation: &Federation, liker: &User, comment: &Comment, liked_at: &DateTime<Utc>) -> Value {
    let actor = federation.actor_url(liker.public_id);
    let author = match &comment.author {
        Author::Local(author) => federation.actor_url(author.public_id),
        Author::Remote(remote) => remote.actor_id.clone(),
    };
    json!({
        "id": format!("{actor}/comment-likes/{}/{}", comment.id, liked_at.timestamp_micros()),
        "type": "Like",
        "actor": actor,
        "object": comment_note_id(federation, comment),
        "to": [author],
        "cc": [PUBLIC],
    })
}

/// A share (boost, renote) of a post, shown on the sharer's profile.
pub fn announce(federation: &Federation, sharer: &User, post: &Post, shared_at: &DateTime<Utc>) -> Value {
    let actor = federation.actor_url(sharer.public_id);
    json!({
        "id": format!("{actor}/shares/{}/{}", post.id, shared_at.timestamp_micros()),
        "type": "Announce",
        "actor": actor,
        "published": iso(shared_at),
        "object": note_id(federation, post),
        "to": [PUBLIC],
        "cc": [author_id(federation, post), federation.followers_url(sharer.public_id)],
    })
}

/// A community sharing one of its posts with its followers: how they see the
/// community's posts in their timeline (a renote on Misskey, a boost on
/// Mastodon). The Note itself is announced, not its Create: Misskey and
/// Mastodon only show announced notes.
pub fn community_announce(federation: &Federation, community_public_id: i64, note_id: &str, post_id: i64) -> Value {
    let actor = federation.community_url(community_public_id);
    with_context(json!({
        "id": format!("{actor}/announces/{post_id}"),
        "type": "Announce",
        "actor": actor,
        "published": iso(&Utc::now()),
        "object": note_id,
        "to": [PUBLIC],
        "cc": [federation.community_followers_url(community_public_id)],
    }))
}

/// Cancels `activity` (a Like, an Announce or a Follow), addressed the same way.
pub fn undo(activity: &Value) -> Value {
    with_context(json!({
        "id": format!("{}/undo", activity["id"].as_str().unwrap_or_default()),
        "type": "Undo",
        "actor": activity["actor"],
        "to": activity.get("to").cloned().unwrap_or_else(|| json!([activity["object"]])),
        "cc": activity.get("cc").cloned().unwrap_or_else(|| json!([])),
        "object": activity,
    }))
}

/// A user here joins an event of another server (Mobilizon's participation).
pub fn join(federation: &Federation, user: &User, event_id: &str, join_id: &str, organizer: &str) -> Value {
    with_context(json!({
        "id": join_id,
        "type": "Join",
        "actor": federation.actor_url(user.public_id),
        "object": event_id,
        "to": [organizer],
        "participationMessage": "",
    }))
}

/// They leave it.
pub fn leave(federation: &Federation, user: &User, event_id: &str, join_id: &str, organizer: &str) -> Value {
    let actor = federation.actor_url(user.public_id);
    with_context(json!({
        "id": format!("{actor}/leaves/{}", Utc::now().timestamp_micros()),
        "type": "Leave",
        "actor": actor,
        "object": event_id,
        "to": [organizer],
        // Mobilizon finds the participation by its Join too.
        "participation": join_id,
    }))
}

/// The organizer here accepts a Join of their event.
pub fn accept_join(actor: &str, join: &Value) -> Value {
    accept(actor, join)
}

/// Or refuses it.
pub fn reject_join(actor: &str, join: &Value) -> Value {
    with_context(json!({
        "id": format!("{actor}/rejects/{}", Utc::now().timestamp_micros()),
        "type": "Reject",
        "actor": actor,
        "object": join,
    }))
}

fn accept(actor: &str, follow: &Value) -> Value {
    with_context(json!({
        "id": format!("{actor}/accepts/{}", Utc::now().timestamp_micros()),
        "type": "Accept",
        "actor": actor,
        "object": follow,
    }))
}

// --- Receiving ---

/// Id of an object given either as an address or as an embedded object.
pub fn object_id(value: &Value) -> Option<&str> {
    value.as_str().or_else(|| value.get("id")?.as_str())
}

/// Who an incoming activity is for, when it's one Tison acts on.
pub enum Recipient {
    User(User),
    Community(Community),
}

impl Recipient {
    /// Signs the requests made while handling it, like fetching the sender's key.
    pub fn signer(&self) -> Signer {
        match self {
            Self::User(user) => Signer::User(user.id),
            Self::Community(community) => Signer::Community(community.id),
        }
    }
}

/// The local user or community with actor address `url`, if other servers can
/// see it.
async fn local_actor(federation: &Federation, pool: &PgPool, url: &str) -> Option<Recipient> {
    if let Some(public_id) = federation.local_public_id(url) {
        return match user::find_by_public_id(pool, public_id).await {
            Ok(user) => user.filter(|user| user.is_federated()).map(Recipient::User),
            Err(err) => {
                tracing::warn!("failed to load user {}: {}", public_id, err);
                None
            }
        };
    }
    let public_id = federation.local_community_public_id(url)?;
    match community::find_by_public_id(pool, public_id).await {
        Ok(community) => community.filter(Community::is_federated).map(Recipient::Community),
        Err(err) => {
            tracing::warn!("failed to load community {}: {}", public_id, err);
            None
        }
    }
}

/// A post or comment stored here, from its ActivityPub address.
#[derive(Clone, Copy, Debug)]
enum Target {
    Post(i64),
    Comment { id: i64, post_id: i64 },
}

impl Target {
    fn post_id(self) -> i64 {
        match self {
            Self::Post(id) | Self::Comment { post_id: id, .. } => id,
        }
    }

    fn reacted(self) -> Reacted {
        match self {
            Self::Post(id) => Reacted::Post(id),
            Self::Comment { id, .. } => Reacted::Comment(id),
        }
    }
}

/// A post or comment written here (`/ap/posts/{id}`...), or received from
/// another server and stored here.
async fn find_target(federation: &Federation, pool: &PgPool, url: &str) -> Option<Target> {
    if let Some(post_id) = federation.local_post_id(url) {
        return Some(Target::Post(post_id));
    }
    if let Some(comment_id) = federation.local_comment_id(url) {
        let owners = comment::owners(pool, comment_id).await.ok()??;
        return Some(Target::Comment { id: comment_id, post_id: owners.post_id });
    }
    if let Ok(Some(post_id)) = post::find_by_ap_id(pool, url).await {
        return Some(Target::Post(post_id));
    }
    let (comment_id, post_id) = comment::find_by_ap_id(pool, url).await.ok()??;
    Some(Target::Comment { id: comment_id, post_id })
}

/// A local user following the account `actor_id` of another server: their key
/// fetches what it sends.
async fn follower_of(pool: &PgPool, actor_id: &str) -> Option<Recipient> {
    let user_id = store::a_local_follower(pool, actor_id).await.ok()??;
    user::find_by_id(pool, user_id)
        .await
        .ok()?
        .filter(|user| user.is_federated())
        .map(Recipient::User)
}

/// Who receives activities about a post: its community, else its local
/// author, else a local follower of its remote author.
async fn post_recipient(pool: &PgPool, post: &Post) -> Option<Recipient> {
    if let Some(community) = &post.community {
        return community::find_by_id(pool, community.id)
            .await
            .ok()?
            .filter(Community::is_federated)
            .map(Recipient::Community);
    }
    match &post.author {
        Author::Local(user) => Some(user.clone()).filter(|user| user.is_federated()).map(Recipient::User),
        Author::Remote(remote) => follower_of(pool, &remote.actor_id).await,
    }
}

/// Who receives a reaction to `target`: the comment's local author, else the
/// recipient of the post, see `post_recipient`.
async fn target_recipient(pool: &PgPool, target: Target) -> Option<Recipient> {
    if let Target::Comment { id, .. } = target
        && let Some(author_id) = comment::owners(pool, id).await.ok()?.and_then(|owners| owners.author_id)
        && let Some(user) = user::find_by_id(pool, author_id).await.ok()?.filter(|user| user.is_federated())
    {
        return Some(Recipient::User(user));
    }
    let post = post::find_federable(pool, target.post_id()).await.ok()??;
    post_recipient(pool, &post).await
}

/// The local user who sent the Follow an Accept or Reject answers.
async fn follow_sender(federation: &Federation, pool: &PgPool, follow: &Value) -> Option<Recipient> {
    let public_id = object_id(follow)
        .and_then(|id| federation.follow_owner(id).or_else(|| federation.join_owner(id)))
        .or_else(|| federation.local_public_id(object_id(&follow["actor"])?))?;
    user::find_by_public_id(pool, public_id)
        .await
        .ok()?
        .filter(|user| user.is_federated())
        .map(Recipient::User)
}

/// The recipient of an incoming activity, `None` when Tison ignores it:
/// - a Follow of a local user or community, or its Undo,
/// - the Accept or Reject of a Follow sent by a local user,
/// - a like, reaction or share of a post or comment stored here, or its Undo,
/// - a public Create of a Note mentioning a local community, or replying to
///   a post or comment stored here,
/// - anything sent by an account a local user follows.
pub async fn recipient(federation: &Federation, pool: &PgPool, activity: &Value) -> Option<Recipient> {
    let object = &activity["object"];
    let found = match activity["type"].as_str()? {
        "Follow" => local_actor(federation, pool, object_id(object)?).await,
        "Undo" => match object["type"].as_str() {
            Some("Follow") => local_actor(federation, pool, object_id(&object["object"])?).await,
            Some("Like" | "EmojiReact" | "Announce" | "Join") => match find_target(federation, pool, object_id(&object["object"])?).await {
                Some(target) => target_recipient(pool, target).await,
                None => None,
            },
            _ => None,
        },
        "Accept" | "Reject" => follow_sender(federation, pool, object).await,
        // Mobilizon's participation: for the event's organizer here.
        "Join" | "Leave" => match find_target(federation, pool, object_id(object)?).await {
            Some(target) => target_recipient(pool, target).await,
            None => None,
        },
        // A report: the first local account or content it's about.
        "Flag" => {
            let mut found = None;
            for url in flagged(object) {
                if let Some(public_id) = federation.local_public_id(url) {
                    found = user::find_by_public_id(pool, public_id).await.ok().flatten().map(Recipient::User);
                } else if let Some(target) = find_target(federation, pool, url).await {
                    found = target_recipient(pool, target).await;
                }
                if found.is_some() {
                    break;
                }
            }
            found
        }
        "Like" | "EmojiReact" | "Announce" => match find_target(federation, pool, object_id(object)?).await {
            Some(target) => target_recipient(pool, target).await,
            None => None,
        },
        "Create" if object["type"] == "ChatMessage" || (is_note(object) && !is_public(object)) && local_recipients(federation, pool, object).await.first().is_some() => {
            local_recipients(federation, pool, object).await.into_iter().next().map(Recipient::User)
        }
        "Create" if is_note(object) => {
            let mut found = None;
            for id in audience(object) {
                if federation.local_community_public_id(id).is_some()
                    && let Some(recipient) = local_actor(federation, pool, id).await
                {
                    found = Some(recipient);
                    break;
                }
            }
            // A reply: for the community of the post, else its author.
            if found.is_none()
                && let Some(in_reply_to) = object["inReplyTo"].as_str()
                && let Some(target) = find_target(federation, pool, in_reply_to).await
                && let Ok(Some(post)) = post::find(pool, target.post_id(), None).await
            {
                found = post_recipient(pool, &post).await;
            }
            found
        }
        _ => None,
    };
    match found {
        Some(found) => Some(found),
        None => follower_of(pool, object_id(&activity["actor"])?).await,
    }
}

/// The local users an object is addressed to (`to`, `cc`, mentions), who can
/// receive messages from other servers.
async fn local_recipients(federation: &Federation, pool: &PgPool, object: &Value) -> Vec<User> {
    let mut found: Vec<User> = Vec::new();
    for id in audience(object) {
        let Some(public_id) = federation.local_public_id(id) else {
            continue;
        };
        if let Ok(Some(user)) = user::find_by_public_id(pool, public_id).await
            && user.is_federated()
            && !found.iter().any(|known| known.id == user.id)
        {
            found.push(user);
        }
    }
    found
}

/// The text of a note of another server. Misskey sends its original text
/// (MFM, close to plain text), which reads better than the HTML. In a
/// community, the mention of the community starting the text is removed,
/// see `strip_mention`.
fn note_text(note: &Value, community: Option<(&str, &str)>) -> String {
    // The original text only when it reads well as plain text: Friendica's
    // is BBCode (`text/bbcode`, "[b]...[/b]"), its HTML is better then.
    let readable_source = note["source"]["mediaType"]
        .as_str()
        .is_some_and(|kind| matches!(kind, "text/plain" | "text/markdown" | "text/x.misskeymarkdown"));
    let source = note["_misskey_content"]
        .as_str()
        .map(str::to_string)
        .or_else(|| {
            note["source"]["content"]
                .as_str()
                .filter(|_| readable_source)
                .map(str::to_string)
        })
        .unwrap_or_else(|| html_to_text(note["content"].as_str().unwrap_or_default()));
    let source = match community {
        Some((slug, domain)) => strip_mention(&source, slug, domain),
        None => source,
    };
    // Lemmy's posts (Page) and Friendica's or WriteFreely's titled posts
    // (Article) have their title in `name`.
    let titled = matches!(note["type"].as_str(), Some("Page" | "Article"));
    let source = match note["name"].as_str().map(str::trim).filter(|name| !name.is_empty() && titled) {
        Some(title) => format!("{title}\n\n{source}"),
        None => source,
    };
    let text = match note["summary"].as_str().map(str::trim).filter(|summary| !summary.is_empty()) {
        // A content warning.
        Some(summary) => format!("⚠ {summary}\n\n{source}"),
        None => source,
    };
    // Control characters other than line breaks and tabs are dropped,
    // like for posts written here.
    let text: String = text
        .replace("\r\n", "\n")
        .chars()
        .filter(|letter| !letter.is_control() || matches!(letter, '\n' | '\t'))
        .collect();
    let text = text.trim();
    match text.char_indices().nth(BODY_MAX_CHARS) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_string(),
    }
}

/// The title and Markdown text of an article of another server: an
/// Article with a title, as WriteFreely and Friendica send them, when
/// `PUBLISHING` is on (otherwise it's a post, see `note_text`). Its
/// Markdown source when it sends one, or else its HTML read back.
fn article(note: &Value) -> Option<(String, String)> {
    if !crate::settings::services().publishing || note["type"].as_str() != Some("Article") {
        return None;
    }
    let title: String = clean_text(note["name"].as_str()?.trim()).chars().take(post::TITLE_MAX_CHARS).collect();
    if title.is_empty() {
        return None;
    }
    let source = note["source"]["content"]
        .as_str()
        .filter(|_| note["source"]["mediaType"].as_str() == Some("text/markdown"))
        .map(str::to_string)
        .unwrap_or_else(|| crate::markdown::from_html(note["content"].as_str().unwrap_or_default()));
    let body = clean_text(source.trim());
    let body = match body.char_indices().nth(post::ARTICLE_MAX_CHARS) {
        Some((end, _)) => format!("{}…", &body[..end]),
        None => body,
    };
    Some((title, body))
}

/// Without control characters other than line breaks and tabs, like texts
/// written here.
fn clean_text(text: &str) -> String {
    text.replace("\r\n", "\n")
        .chars()
        .filter(|letter| !letter.is_control() || matches!(letter, '\n' | '\t'))
        .collect()
}

/// An address of an attachment: a string, a Link, or several Links (the
/// first one, or the one with a media type, is taken).
fn attachment_url(value: &Value) -> Option<(String, Option<String>)> {
    match value {
        Value::String(url) => Some((url.clone(), None)),
        Value::Object(fields) => {
            let url = fields.get("href").or_else(|| fields.get("url"))?.as_str()?.to_string();
            Some((url, fields.get("mediaType").and_then(Value::as_str).map(str::to_string)))
        }
        Value::Array(items) => items
            .iter()
            .filter_map(attachment_url)
            .max_by_key(|(_, media_type)| media_type.is_some()),
        _ => None,
    }
}

/// The pictures, videos and sounds of a note: `attachment`, as Mastodon,
/// Misskey and Pixelfed send them. Only web addresses are kept.
fn attachments(note: &Value) -> Vec<NewAttachment> {
    let items = match &note["attachment"] {
        Value::Array(items) => items.as_slice(),
        item @ Value::Object(_) => std::slice::from_ref(item),
        _ => &[],
    };
    items
        .iter()
        .filter(|item| matches!(item["type"].as_str(), Some("Document" | "Image" | "Video" | "Audio")))
        .filter_map(|item| {
            let (url, link_type) = attachment_url(&item["url"])?;
            if !url.starts_with("https://") && !url.starts_with("http://") {
                return None;
            }
            let media_type = item["mediaType"].as_str().map(str::to_string).or(link_type).unwrap_or_default();
            let size = |name: &str| item[name].as_i64().and_then(|value| i32::try_from(value).ok()).filter(|value| *value > 0);
            Some(NewAttachment::new(
                url,
                media_type,
                item["name"].as_str().unwrap_or_default(),
                size("width"),
                size("height"),
            ))
        })
        .take(media::MAX_ATTACHMENTS)
        .collect()
}

/// Removes the mention of the community at the start of a post: on Misskey
/// one writes `@cooking@example.com Hello`, the community shows `Hello`.
fn strip_mention(text: &str, slug: &str, domain: &str) -> String {
    let mut rest = text.trim_start();
    for mention in [format!("@{slug}@{domain}"), format!("@{slug}")] {
        if rest.len() >= mention.len()
            && rest.is_char_boundary(mention.len())
            && rest[..mention.len()].eq_ignore_ascii_case(&mention)
            && rest[mention.len()..].chars().next().is_none_or(char::is_whitespace)
        {
            rest = rest[mention.len()..].trim_start();
            break;
        }
    }
    rest.to_string()
}

fn published(note: &Value) -> Option<DateTime<Utc>> {
    let published = DateTime::parse_from_rfc3339(note["published"].as_str()?).ok()?.with_timezone(&Utc);
    // Never in the future: it would stay first on every page.
    Some(published.min(Utc::now()))
}

/// Queues `activity` signed by `signer` for `inboxes`, and starts sending.
async fn send(federation: &Federation, pool: &PgPool, signer: Signer, inboxes: &[String], activity: &Value) -> Result<(), String> {
    store::enqueue(pool, signer, inboxes, &activity.to_string())
        .await
        .map_err(|err| format!("failed to queue activity: {err}"))?;
    federation.wake_deliveries();
    Ok(())
}

/// Acts on a verified activity from `sender`. Activities Tison doesn't
/// handle are accepted and ignored.
pub async fn receive(
    federation: &Federation,
    pool: &PgPool,
    sender: &RemoteActor,
    activity: &Value,
    recipient: Option<(&Recipient, &LocalKey)>,
) -> Result<(), String> {
    let kind = activity["type"].as_str().unwrap_or_default();
    match (kind, recipient) {
        ("Follow", Some((Recipient::User(user), _))) => {
            let follow_id = activity["id"].as_str().unwrap_or_default();
            let is_new = store::add_follower(pool, user.id, sender, follow_id)
                .await
                .map_err(|err| format!("failed to save follower: {err}"))?;
            if is_new && let Err(err) = notification::followed(pool, user.id, sender.id).await {
                tracing::warn!("failed to notify @{} of a follower: {}", user.username, err);
            }
            // Sent to their own inbox, not the shared one: it's for them only.
            let accept = accept(&federation.actor_url(user.public_id), activity);
            send(federation, pool, Signer::User(user.id), std::slice::from_ref(&sender.inbox), &accept).await?;
            tracing::info!("{} now follows @{}", sender.actor_id, user.username);
        }
        ("Follow", Some((Recipient::Community(community), _))) => {
            let follow_id = activity["id"].as_str().unwrap_or_default();
            store::add_community_follower(pool, community.id, sender, follow_id)
                .await
                .map_err(|err| format!("failed to save community follower: {err}"))?;
            let accept = accept(&federation.community_url(community.public_id), activity);
            let signer = Signer::Community(community.id);
            send(federation, pool, signer, std::slice::from_ref(&sender.inbox), &accept).await?;
            tracing::info!("{} now follows community {}", sender.actor_id, community.slug);
        }
        // The answer to a Join sent from here: before the Accept of Follows.
        ("Accept" | "Reject", Some((Recipient::User(user), _))) if is_join_answer(federation, &activity["object"]) => {
            receive_join_answer(pool, sender, activity, user, kind == "Accept").await?
        }
        ("Join", Some(_)) => receive_join(federation, pool, sender, activity).await?,
        ("Leave", Some(_)) => receive_leave(federation, pool, sender, &activity["object"]).await?,
        ("Undo", Some(_)) if activity["object"]["type"] == "Join" => {
            receive_leave(federation, pool, sender, &activity["object"]["object"]).await?
        }
        ("Undo", recipient) => receive_undo(federation, pool, sender, &activity["object"], recipient.map(|(recipient, _)| recipient)).await?,
        ("Accept", Some((Recipient::User(user), _))) => {
            let follow_id = object_id(&activity["object"]).filter(|id| federation.follow_owner(id).is_some());
            let accepted = store::accept_following(pool, sender, follow_id, user.id)
                .await
                .map_err(|err| format!("failed to accept follow: {err}"))?;
            if accepted {
                tracing::info!("@{} now follows {}", user.username, sender.actor_id);
            }
        }
        // A refused Follow, or a follower removed by the account.
        ("Reject", Some((Recipient::User(user), _))) => {
            store::reject_following(pool, sender, user.id)
                .await
                .map_err(|err| format!("failed to remove follow: {err}"))?;
            tracing::info!("{} refused or removed the follow of @{}", sender.actor_id, user.username);
        }
        ("Like" | "EmojiReact", Some(_)) => receive_like(federation, pool, sender, activity).await?,
        ("Announce", Some((_, key))) => receive_announce(federation, pool, sender, activity, key).await?,
        // Without the messenger (`DFRN` in `configs/services.conf`), ignored.
        ("Create", Some(_)) if is_message(&activity["object"], sender) => {
            if crate::settings::services().dfrn {
                receive_message(federation, pool, sender, &activity["object"]).await?
            }
        }
        ("Create", Some((recipient, _))) => receive_note(federation, pool, sender, &activity["object"], recipient).await?,
        // Their account was deleted.
        ("Delete", _) if object_id(&activity["object"]) == Some(sender.actor_id.as_str()) => {
            store::delete_actor(pool, &sender.actor_id)
                .await
                .map_err(|err| format!("failed to delete actor: {err}"))?;
            tracing::info!("forgot deleted account {}", sender.actor_id);
        }
        // One of their posts, comments or messages was deleted.
        ("Delete", _) => {
            let Some(id) = object_id(&activity["object"]) else {
                return Ok(());
            };
            let messages = message::find_remote(pool, sender.id, id)
                .await
                .map_err(|err| format!("failed to find deleted message: {err}"))?;
            for (message_id, conversation_id) in messages {
                message::delete(pool, message_id)
                    .await
                    .map_err(|err| format!("failed to delete message: {err}"))?;
                signal_conversation(pool, conversation_id).await;
            }
            let found = post::find_remote_object(pool, sender.id, id)
                .await
                .map_err(|err| format!("failed to find deleted note: {err}"))?;
            if let Some(object) = found {
                post::delete_remote(pool, &object)
                    .await
                    .map_err(|err| format!("failed to delete note: {err}"))?;
                tracing::info!("deleted {} on request of its author", id);
            }
        }
        // Their profile changed, maybe their key or inbox.
        ("Update", _) if object_id(&activity["object"]) == Some(sender.actor_id.as_str()) => {
            if activity["object"].get("publicKey").is_some() {
                let actor = linker::parse_actor(&activity["object"])?;
                store::save_actor(pool, &actor)
                    .await
                    .map_err(|err| format!("failed to update actor: {err}"))?;
            }
        }
        // One of their posts, comments or messages was edited.
        ("Update", _) => {
            let note = &activity["object"];
            let Some(id) = object_id(note) else {
                return Ok(());
            };
            let messages = message::find_remote(pool, sender.id, id)
                .await
                .map_err(|err| format!("failed to find edited message: {err}"))?;
            for (message_id, conversation_id) in messages {
                // Without the mention of the reader, like when it arrived.
                let mut text = note_text(note, None);
                for user_id in message::local_people(pool, conversation_id).await.unwrap_or_default() {
                    if let Ok(Some(reader)) = user::find_by_id(pool, user_id).await {
                        text = strip_mention(&text, &reader.username, &federation.domain);
                    }
                }
                if !text.is_empty() {
                    message::update_remote(pool, message_id, &text)
                        .await
                        .map_err(|err| format!("failed to update message: {err}"))?;
                    signal_conversation(pool, conversation_id).await;
                }
            }
            let found = post::find_remote_object(pool, sender.id, id)
                .await
                .map_err(|err| format!("failed to find edited note: {err}"))?;
            if let Some(object) = found {
                let community = match &object {
                    RemoteObject::Post(_, Some(community_id)) => community::find_by_id(pool, *community_id).await.ok().flatten(),
                    _ => None,
                };
                let is_post = matches!(object, RemoteObject::Post(..));
                // A video, episode or event keeps its title and file or
                // details: its `name` and `content`.
                let media = remote_media(note).filter(|_| is_post);
                let event = remote_event(note).filter(|_| is_post && crate::settings::services().events);
                let named = media.is_some() || event.is_some();
                let (title, text) = match article(note).filter(|_| is_post) {
                    _ if named => (
                        note["name"].as_str().map(|name| name.trim().chars().take(crate::entities::event::TITLE_MAX_CHARS).collect::<String>()),
                        html_to_text(note["content"].as_str().unwrap_or_default()).chars().take(BODY_MAX_CHARS).collect(),
                    ),
                    Some((title, body)) => (Some(title), body),
                    None => (None, note_text(note, community.as_ref().map(|community| (community.slug.as_str(), federation.domain.as_str())))),
                };
                let attachments = match (&media, &event) {
                    (Some(media), _) => media.file.iter().map(|file| NewAttachment::new(file.url.clone(), file.media_type.clone(), "", file.width, file.height)).collect(),
                    (None, Some(_)) => Vec::new(),
                    (None, None) => attachments(note),
                };
                if text.is_empty() && attachments.is_empty() && !named {
                    return Ok(());
                }
                let sensitive = note["sensitive"].as_bool().unwrap_or(false);
                post::update_remote(pool, &object, title.as_deref(), &text, sensitive)
                    .await
                    .map_err(|err| format!("failed to update note: {err}"))?;
                let owner = match object {
                    RemoteObject::Post(id, _) => media::Owner::Post(id),
                    RemoteObject::Comment(id) => media::Owner::Comment(id),
                };
                media::save_attachments(pool, owner, &attachments)
                    .await
                    .map_err(|err| format!("failed to update attachments: {err}"))?;
                if let (Some(event), RemoteObject::Post(post_id, _)) = (&event, &object) {
                    crate::entities::event::save(pool, *post_id, &event.details())
                        .await
                        .map_err(|err| format!("failed to update event: {err}"))?;
                }
                tracing::info!("updated {} on request of its author", id);
            }
        }
        ("Flag", Some(_)) => receive_flag(federation, pool, sender, activity).await?,
        _ => tracing::debug!("ignored {} activity from {}", kind, sender.actor_id),
    }
    Ok(())
}

/// The emoji of a Misskey or Pleroma reaction, empty for a plain like.
fn reaction_emoji(activity: &Value) -> String {
    activity["content"]
        .as_str()
        .or_else(|| activity["_misskey_reaction"].as_str())
        .unwrap_or_default()
        .chars()
        .take(64)
        .collect()
}

/// A like (or a Misskey reaction) of a post or comment stored here.
async fn receive_like(federation: &Federation, pool: &PgPool, sender: &RemoteActor, activity: &Value) -> Result<(), String> {
    let Some(target) = (match object_id(&activity["object"]) {
        Some(url) => find_target(federation, pool, url).await,
        None => None,
    }) else {
        return Ok(());
    };
    // Only what other servers may see.
    let visible = post::find_federable(pool, target.post_id())
        .await
        .map_err(|err| format!("failed to load post: {err}"))?;
    if visible.is_none() {
        return Ok(());
    }
    let activity_id = activity["id"].as_str().unwrap_or_default();
    let is_new = store::add_reaction(pool, RemoteReaction::Like, sender, target.reacted(), activity_id, &reaction_emoji(activity))
        .await
        .map_err(|err| format!("failed to save like: {err}"))?;
    if is_new {
        if let Err(err) = notification::remote_reaction_added(pool, RemoteReaction::Like, target.reacted(), sender.id).await {
            tracing::warn!("failed to notify like of {:?}: {}", target, err);
        }
        tracing::info!("{} liked {:?}", sender.actor_id, target);
    }
    Ok(())
}

/// A share (boost, renote): of a post stored here, counted; or of a post of
/// another server by an account followed here, fetched to show it in feeds.
async fn receive_announce(
    federation: &Federation,
    pool: &PgPool,
    sender: &RemoteActor,
    activity: &Value,
    key: &LocalKey,
) -> Result<(), String> {
    let Some(object_url) = object_id(&activity["object"]) else {
        return Ok(());
    };
    let activity_id = activity["id"].as_str().unwrap_or_default();
    let post_id = match find_target(federation, pool, object_url).await {
        // Comments can't be shared here.
        Some(Target::Comment { .. }) => return Ok(()),
        Some(Target::Post(post_id)) => {
            // Only public posts can be shared.
            let public = post::find(pool, post_id, None)
                .await
                .map_err(|err| format!("failed to load post: {err}"))?;
            if public.is_none() {
                return Ok(());
            }
            post_id
        }
        None => {
            let followed = store::a_local_follower(pool, &sender.actor_id)
                .await
                .map_err(|err| format!("failed to check followers: {err}"))?;
            if followed.is_none() {
                return Ok(());
            }
            match fetch_shared_post(federation, pool, object_url, key).await? {
                Some(post_id) => post_id,
                None => return Ok(()),
            }
        }
    };
    let is_new = store::add_reaction(pool, RemoteReaction::Share, sender, Reacted::Post(post_id), activity_id, "")
        .await
        .map_err(|err| format!("failed to save share: {err}"))?;
    if is_new {
        if let Err(err) = notification::remote_reaction_added(pool, RemoteReaction::Share, Reacted::Post(post_id), sender.id).await {
            tracing::warn!("failed to notify share of post {}: {}", post_id, err);
        }
        tracing::info!("{} shared post {}", sender.actor_id, post_id);
    }
    Ok(())
}

/// Fetches a shared note from its own server (never trusting a copy sent by
/// someone else) and stores it as a post. `None` when it can't be shown.
async fn fetch_shared_post(federation: &Federation, pool: &PgPool, url: &str, key: &LocalKey) -> Result<Option<i64>, String> {
    // A share of a post of a banned server: not fetched.
    if store::is_banned(pool, url).await.map_err(|err| format!("failed to check {url}: {err}"))? {
        return Ok(None);
    }
    let note = match linker::fetch(federation, url, Some(key)).await {
        Ok(note) => note,
        Err(err) => {
            tracing::info!("failed to fetch shared note {}: {}", url, err);
            return Ok(None);
        }
    };
    let (Some(ap_id), Some(author_url)) = (note["id"].as_str(), attributed_to(&note)) else {
        return Ok(None);
    };
    if !is_note(&note) || !is_public(&note) || !linker::same_host(ap_id, url) || !linker::same_host(ap_id, author_url) {
        tracing::info!("ignored shared note {}: not a public note of its server", url);
        return Ok(None);
    }
    if let Some(post_id) = post::find_by_ap_id(pool, ap_id)
        .await
        .map_err(|err| format!("failed to find post: {err}"))?
    {
        return Ok(Some(post_id));
    }
    let author = linker::fetch_actor(federation, pool, author_url, key).await?;
    store_post(pool, &author, &note, None, Visibility::Public).await?;
    post::find_by_ap_id(pool, ap_id)
        .await
        .map_err(|err| format!("failed to find post: {err}"))
}

/// Saves a note of another server as a post, with its media. `None` when it
/// was already stored, or is empty.
async fn store_post(
    pool: &PgPool,
    author: &RemoteActor,
    note: &Value,
    community: Option<(&Community, &str)>,
    visibility: Visibility,
) -> Result<Option<i64>, String> {
    let Some(ap_id) = note["id"].as_str() else {
        return Ok(None);
    };
    let media = remote_media(note).filter(|media| match media.kind {
        Kind::Video => crate::settings::services().video,
        _ => crate::settings::services().podcasts,
    });
    // Mobilizon's: without `EVENT-PLANNING`, a post like others.
    let event = remote_event(note).filter(|_| crate::settings::services().events);
    let (title, text) = match (&media, article(note)) {
        // PeerTube's and Mobilizon's description is their `content`, their
        // title `name`.
        _ if event.is_some() => (
            note["name"].as_str().map(|name| name.trim().chars().take(crate::entities::event::TITLE_MAX_CHARS).collect::<String>()),
            html_to_text(note["content"].as_str().unwrap_or_default()).chars().take(BODY_MAX_CHARS).collect(),
        ),
        (Some(_), _) => (
            note["name"].as_str().map(|name| name.trim().chars().take(crate::entities::video::TITLE_MAX_CHARS).collect::<String>()),
            html_to_text(note["content"].as_str().unwrap_or_default()).chars().take(BODY_MAX_CHARS).collect(),
        ),
        (None, Some((title, body))) => (Some(title), body),
        (None, None) => (None, note_text(note, community.map(|(community, domain)| (community.slug.as_str(), domain)))),
    };
    let attachments = match (&media, &event) {
        (Some(media), _) => media.file.iter().map(|file| NewAttachment::new(file.url.clone(), file.media_type.clone(), "", file.width, file.height)).collect(),
        // The banner is its picture, the online link is in its details.
        (None, Some(_)) => Vec::new(),
        (None, None) => attachments(note),
    };
    let poster = media.as_ref().and_then(|media| media.poster.as_deref()).or(event.as_ref().and_then(|event| event.banner.as_deref()));
    if let Some(poster) = poster {
        media::register(pool, poster).await.map_err(|err| format!("failed to register poster: {err}"))?;
    }
    if text.is_empty() && attachments.is_empty() && media.is_none() && event.is_none() {
        return Ok(None);
    }
    let page = note.get("url").and_then(|url| match url {
        Value::String(url) => Some(url.clone()),
        other => other.get("href").and_then(Value::as_str).map(str::to_string),
    });
    let remote = RemotePost {
        ap_id,
        ap_url: page.as_deref().filter(|page| page.starts_with("https://") || page.starts_with("http://")),
        title: title.as_deref(),
        body: &text,
        kind: match (&media, &event) {
            (Some(media), _) => media.kind,
            (None, Some(_)) => Kind::Event,
            (None, None) => Kind::Post,
        },
        duration_seconds: media.as_ref().and_then(|media| media.duration),
        poster_url: poster,
        visibility,
        sensitive: note["sensitive"].as_bool().unwrap_or(false),
        published: published(note),
    };
    let created = post::create_remote(pool, author.id, community.map(|(community, _)| community.id), &remote)
        .await
        .map_err(|err| format!("failed to save post: {err}"))?;
    if let Some(post_id) = created {
        media::save_attachments(pool, media::Owner::Post(post_id), &attachments)
            .await
            .map_err(|err| format!("failed to save attachments: {err}"))?;
        if let Some(event) = &event {
            crate::entities::event::save(pool, post_id, &event.details())
                .await
                .map_err(|err| format!("failed to save event: {err}"))?;
        }
    }
    Ok(created)
}

/// Whether an Accept or Reject answers a Join sent from here.
fn is_join_answer(federation: &Federation, object: &Value) -> bool {
    object["type"] == "Join" || object_id(object).is_some_and(|id| federation.join_owner(id).is_some())
}

/// The organizer of an event of another server answered a user here.
async fn receive_join_answer(pool: &PgPool, sender: &RemoteActor, activity: &Value, user: &User, accepted: bool) -> Result<(), String> {
    let Some(join_id) = object_id(&activity["object"]) else {
        return Ok(());
    };
    let role = if accepted { "going" } else { "rejected" };
    let answered = crate::entities::event::answered(pool, join_id, role)
        .await
        .map_err(|err| format!("failed to save the answer to a join: {err}"))?;
    if let Some((post_id, user_id)) = answered {
        tracing::info!("{} {} @{} at event {}", sender.actor_id, if accepted { "accepted" } else { "refused" }, user.username, post_id);
        if accepted && let Err(err) = notification::event_accepted(pool, user_id, None, Some(sender.id), post_id).await {
            tracing::warn!("failed to notify @{} of their event: {}", user.username, err);
        }
    }
    Ok(())
}

/// Someone of another server (Mobilizon) joins an event here: accepted at
/// once, or waiting for the organizer when it needs approval; refused
/// when it's cancelled, over or full.
async fn receive_join(federation: &Federation, pool: &PgPool, sender: &RemoteActor, activity: &Value) -> Result<(), String> {
    let (Some(join_id), Some(event_url)) = (activity["id"].as_str(), object_id(&activity["object"])) else {
        return Ok(());
    };
    let Some(post_id) = federation.local_post_id(event_url) else {
        return Ok(());
    };
    let post = post::find(pool, post_id, None).await.map_err(|err| format!("failed to load event: {err}"))?;
    let Some((post, event, organizer)) = post.and_then(|post| {
        let event = post.event.clone().filter(|_| post.is_event())?;
        let organizer = post.author.local()?.clone();
        Some((post, event, organizer))
    }) else {
        return Ok(());
    };
    let organizer_url = federation.actor_url(organizer.public_id);
    let inbox = std::slice::from_ref(&sender.inbox);
    if event.is_cancelled() || event.is_past() || event.is_full() || !crate::settings::services().events {
        tracing::info!("refused {} at event {}: cancelled, over or full", sender.actor_id, post.id);
        return send(federation, pool, Signer::User(organizer.id), inbox, &reject_join(&organizer_url, activity)).await;
    }
    let pending = event.needs_approval();
    let message: String = clean_text(activity["participationMessage"].as_str().unwrap_or_default()).chars().take(500).collect();
    crate::entities::event::join_remote(pool, post.id, sender.id, if pending { "pending" } else { "going" }, join_id, &message)
        .await
        .map_err(|err| format!("failed to save participant: {err}"))?;
    if let Err(err) = notification::event_joined(pool, organizer.id, post.id, None, Some(sender.id), pending).await {
        tracing::warn!("failed to notify @{} of a participant: {}", organizer.username, err);
    }
    tracing::info!("{} {} event {}", sender.actor_id, if pending { "asked to join" } else { "joined" }, post.id);
    if pending {
        return Ok(());
    }
    send(federation, pool, Signer::User(organizer.id), inbox, &accept_join(&organizer_url, activity)).await
}

/// They leave an event here (`Leave`, or an Undo of their Join).
async fn receive_leave(federation: &Federation, pool: &PgPool, sender: &RemoteActor, event: &Value) -> Result<(), String> {
    let Some(post_id) = object_id(event).and_then(|url| federation.local_post_id(url)) else {
        return Ok(());
    };
    if crate::entities::event::leave_remote(pool, post_id, sender.id)
        .await
        .map_err(|err| format!("failed to remove participant: {err}"))?
    {
        tracing::info!("{} left event {}", sender.actor_id, post_id);
    }
    Ok(())
}

/// An Undo of a Follow, like or share. `object` is the activity undone, or
/// only its id.
async fn receive_undo(
    federation: &Federation,
    pool: &PgPool,
    sender: &RemoteActor,
    object: &Value,
    recipient: Option<&Recipient>,
) -> Result<(), String> {
    let undone_kind = match object["type"].as_str() {
        Some("Follow") => {
            match recipient {
                Some(Recipient::User(user)) => {
                    store::remove_follower(pool, user.id, sender)
                        .await
                        .map_err(|err| format!("failed to remove follower: {err}"))?;
                    tracing::info!("{} stopped following @{}", sender.actor_id, user.username);
                }
                Some(Recipient::Community(community)) => {
                    store::remove_community_follower(pool, community.id, sender)
                        .await
                        .map_err(|err| format!("failed to remove community follower: {err}"))?;
                    tracing::info!("{} stopped following community {}", sender.actor_id, community.slug);
                }
                None => {}
            }
            return Ok(());
        }
        Some("Like" | "EmojiReact") => Some(RemoteReaction::Like),
        Some("Announce") => Some(RemoteReaction::Share),
        _ => None,
    };
    let activity_id = object_id(object).unwrap_or_default();
    // Only the id: a Follow of a local user, or a reaction.
    if undone_kind.is_none() && !activity_id.is_empty() {
        store::remove_follow(pool, sender, activity_id)
            .await
            .map_err(|err| format!("failed to remove follow: {err}"))?;
    }
    let fallback = match (undone_kind, object_id(&object["object"])) {
        (Some(kind), Some(url)) => find_target(federation, pool, url).await.map(|target| (kind, target.reacted())),
        _ => None,
    };
    let removed = store::remove_reaction(pool, sender, activity_id, fallback)
        .await
        .map_err(|err| format!("failed to remove reaction: {err}"))?;
    if let Some((kind, target)) = removed {
        if let Err(err) = notification::remote_reaction_removed(pool, kind, target, sender.id).await {
            tracing::warn!("failed to remove notification of {:?}: {}", target, err);
        }
        tracing::info!("{} undid {:?} of {:?}", sender.actor_id, kind, target);
    }
    Ok(())
}

/// A note of another server: a post in a community, a comment replying to a
/// post or comment stored here, or a post of an account followed here.
async fn receive_note(
    federation: &Federation,
    pool: &PgPool,
    sender: &RemoteActor,
    note: &Value,
    recipient: &Recipient,
) -> Result<(), String> {
    let Some(ap_id) = note["id"].as_str() else {
        return Ok(());
    };
    if !is_note(note) {
        return Ok(());
    }
    // Only their own notes, from their own server.
    if attributed_to(note) != Some(sender.actor_id.as_str()) || !linker::same_host(ap_id, &sender.actor_id) {
        tracing::info!("refused note {} not written by its sender {}", ap_id, sender.actor_id);
        return Ok(());
    }

    if let Some(in_reply_to) = note["inReplyTo"].as_str() {
        if !is_public(note) {
            tracing::debug!("ignored non-public reply {} from {}", ap_id, sender.actor_id);
            return Ok(());
        }
        return receive_reply(federation, pool, sender, note, ap_id, in_reply_to).await;
    }

    if let Recipient::Community(community) = recipient
        && is_public(note)
    {
        // Following the community is joining it, see `COMMUNITY_FOLLOWERS_TABLE`.
        let follows = store::follows_community(pool, community.id, sender)
            .await
            .map_err(|err| format!("failed to check follower: {err}"))?;
        if follows {
            let Some(post_id) = store_post(pool, sender, note, Some((community, &federation.domain)), Visibility::Public).await? else {
                return Ok(());
            };
            tracing::info!("published {} from {} in community {}", ap_id, sender.actor_id, community.slug);
            // Shared with the community's followers, like its other posts.
            let inboxes = store::community_follower_inboxes(pool, community.id)
                .await
                .map_err(|err| format!("failed to load community followers: {err}"))?;
            let announce = community_announce(federation, community.public_id, ap_id, post_id);
            return send(federation, pool, Signer::Community(community.id), &inboxes, &announce).await;
        }
        tracing::info!("refused post of {} in {}: not a follower", sender.actor_id, community.slug);
    }

    // A post of an account followed here, for the feeds of its followers.
    let followed = store::a_local_follower(pool, &sender.actor_id)
        .await
        .map_err(|err| format!("failed to check followers: {err}"))?;
    if followed.is_none() {
        return Ok(());
    }
    let visibility = if is_public(note) {
        Visibility::Public
    } else if is_for_followers(note, sender) {
        Visibility::Followers
    } else {
        // A direct message: Tison has no messages from other servers yet.
        tracing::debug!("ignored direct note {} from {}", ap_id, sender.actor_id);
        return Ok(());
    };
    if store_post(pool, sender, note, None, visibility).await?.is_some() {
        tracing::info!("saved {} from followed account {}", ap_id, sender.actor_id);
    }
    Ok(())
}

/// The addresses a Flag is about: one, or a list (Mastodon sends the account
/// then the reported posts).
fn flagged(object: &Value) -> Vec<&str> {
    match object {
        Value::Array(items) => items.iter().filter_map(object_id).collect(),
        other => object_id(other).into_iter().collect(),
    }
}

/// A report sent by another server about accounts, posts or comments of
/// this one: one report for each post or comment, else for the account.
/// Shown to the staff, see `handlers::moderation`.
async fn receive_flag(federation: &Federation, pool: &PgPool, sender: &RemoteActor, activity: &Value) -> Result<(), String> {
    use crate::entities::report::{self, NewReport, Reason, Target as Reported};

    let comment: String = html_to_text(activity["content"].as_str().unwrap_or_default())
        .chars()
        .take(report::COMMENT_MAX_CHARS)
        .collect();
    let mut account = None;
    let mut contents = Vec::new();
    for url in flagged(&activity["object"]) {
        if let Some(public_id) = federation.local_public_id(url) {
            if let Ok(Some(user)) = user::find_by_public_id(pool, public_id).await {
                account = Some(user);
            }
        } else if let Some(target) = find_target(federation, pool, url).await {
            contents.push(target);
        }
    }
    let mut reports = Vec::new();
    for target in contents {
        let found = match target {
            Target::Post(post_id) => post::find_federable(pool, post_id)
                .await
                .ok()
                .flatten()
                .map(|post| (Reported::Post(post_id), post.author, post.body, "Post")),
            Target::Comment { id, .. } => comment::find_any(pool, id)
                .await
                .ok()
                .flatten()
                .map(|(found, _)| (Reported::Comment(id), found.author, found.body, "Comment")),
        };
        // Only what was written here: reports about others go to their server.
        if let Some((target, Author::Local(author), body, kind)) = found {
            reports.push(NewReport {
                target,
                account: Some(Reported::User(author.id)),
                snapshot: report::snapshot(&body),
                target_label: format!("{kind} by @{}", author.username),
                reason: Reason::Other,
                comment: comment.clone(),
            });
        }
    }
    if reports.is_empty()
        && let Some(user) = account
    {
        reports.push(NewReport {
            target: Reported::User(user.id),
            account: None,
            snapshot: report::snapshot(&user.bio),
            target_label: format!("@{}", user.username),
            reason: Reason::Other,
            comment,
        });
    }
    for new in &reports {
        report::create_remote(pool, sender.id, new)
            .await
            .map_err(|err| format!("failed to save report: {err}"))?;
    }
    if !reports.is_empty() {
        tracing::info!("{} reported {} thing(s) here", sender.actor_id, reports.len());
    }
    Ok(())
}

/// Tells the open pages of a conversation that it changed.
async fn signal_conversation(pool: &PgPool, conversation_id: i64) {
    let signalled = async {
        let people = message::local_people(pool, conversation_id).await?;
        notification::signal_message(pool, &people, conversation_id).await
    }
    .await;
    if let Err(err) = signalled {
        tracing::warn!("failed to signal conversation {}: {}", conversation_id, err);
    }
}

/// A private message from another server, for each local user it's
/// addressed to: added to their conversation with the sender.
async fn receive_message(federation: &Federation, pool: &PgPool, sender: &RemoteActor, object: &Value) -> Result<(), String> {
    let Some(ap_id) = object["id"].as_str() else {
        return Ok(());
    };
    // Only their own messages, from their own server.
    if attributed_to(object) != Some(sender.actor_id.as_str()) || !linker::same_host(ap_id, &sender.actor_id) {
        tracing::info!("refused message {} not written by its sender {}", ap_id, sender.actor_id);
        return Ok(());
    }
    let kind = if object["type"] == "ChatMessage" { "chat" } else { "note" };
    for user in local_recipients(federation, pool, object).await {
        // "@alice@example.com Hello": the mention of the reader goes.
        let text = strip_mention(&note_text(object, None), &user.username, &federation.domain);
        if text.is_empty() {
            continue;
        }
        let conversation_id = message::with_remote(pool, user.id, sender.id)
            .await
            .map_err(|err| format!("failed to open conversation: {err}"))?;
        message::set_remote_kind(pool, conversation_id, kind)
            .await
            .map_err(|err| format!("failed to save conversation: {err}"))?;
        let added = message::add_remote(pool, conversation_id, sender.id, ap_id, &text, published(object))
            .await
            .map_err(|err| format!("failed to save message: {err}"))?;
        if added.is_some() {
            signal_conversation(pool, conversation_id).await;
            tracing::info!("message {} from {} to @{}", ap_id, sender.actor_id, user.username);
        }
    }
    Ok(())
}

/// A reply to a post or comment stored here: saved as a comment.
async fn receive_reply(
    federation: &Federation,
    pool: &PgPool,
    sender: &RemoteActor,
    note: &Value,
    ap_id: &str,
    in_reply_to: &str,
) -> Result<(), String> {
    let Some(target) = find_target(federation, pool, in_reply_to).await else {
        return Ok(());
    };
    let (post_id, parent_id) = match target {
        Target::Post(post_id) => (post_id, None),
        Target::Comment { id, post_id } => (post_id, Some(id)),
    };
    // Only on posts everyone can see.
    let visible = post::find(pool, post_id, None)
        .await
        .map_err(|err| format!("failed to load post: {err}"))?;
    if visible.is_none() {
        return Ok(());
    }
    let text = note_text(note, None);
    let attachments = attachments(note);
    if text.is_empty() && attachments.is_empty() {
        return Ok(());
    }
    let sensitive = note["sensitive"].as_bool().unwrap_or(false);
    let created = comment::create_remote(pool, post_id, sender.id, parent_id, ap_id, &text, sensitive, published(note))
        .await
        .map_err(|err| format!("failed to save comment: {err}"))?;
    let Some(comment_id) = created else {
        return Ok(());
    };
    media::save_attachments(pool, media::Owner::Comment(comment_id), &attachments)
        .await
        .map_err(|err| format!("failed to save attachments: {err}"))?;
    let parent_author = match parent_id {
        Some(parent_id) => comment::parent_author(pool, post_id, parent_id)
            .await
            .map_err(|err| format!("failed to load parent comment: {err}"))?
            .flatten(),
        None => None,
    };
    if let Err(err) = notification::remote_comment_added(pool, post_id, comment_id, sender.id, parent_author).await {
        tracing::warn!("failed to notify comment {}: {}", comment_id, err);
    }
    tracing::info!("saved reply {} from {} on post {}", ap_id, sender.actor_id, post_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_becomes_text() {
        assert_eq!(
            html_to_text(
                "<p><span class=\"h-card\"><a href=\"https://x\">@<span>cooking</span></a></span> Hello &amp; welcome<br>line two</p><p>Next&#33; &#x1F600; &lt;b&gt;</p>"
            ),
            "@cooking Hello & welcome\nline two\n\nNext! 😀 <b>"
        );
        assert_eq!(html_to_text("a & b &unknown; c"), "a & b &unknown; c");
        // Mastodon shows the address itself (cut with spans): nothing added.
        assert_eq!(
            html_to_text("<a href=\"https://example.com/a?b=1&amp;c=2\"><span class=\"invisible\">https://</span><span>example.com/a?b=1&amp;c=2</span></a>"),
            "https://example.com/a?b=1&c=2"
        );
        assert_eq!(
            html_to_text("<a href=\"https://m.example/tags/rust\" class=\"hashtag\">#<span>rust</span></a> and <a href='https://x.org/p?a=1&amp;b=2'>this page</a>"),
            "#rust and this page (https://x.org/p?a=1&b=2)"
        );
    }

    #[test]
    fn community_mention_is_removed() {
        assert_eq!(strip_mention("@Cooking@example.com Pasta tonight", "cooking", "example.com"), "Pasta tonight");
        assert_eq!(strip_mention("@cooking\nPasta", "cooking", "example.com"), "Pasta");
        assert_eq!(strip_mention("@cookingclub hi", "cooking", "example.com"), "@cookingclub hi");
        assert_eq!(strip_mention("Pasta @cooking", "cooking", "example.com"), "Pasta @cooking");
    }

    #[test]
    fn only_public_notes() {
        assert!(is_public(&json!({ "to": [PUBLIC] })));
        // Misskey's "home": public in cc.
        assert!(is_public(&json!({ "to": ["https://m.example/users/a/followers"], "cc": [PUBLIC] })));
        assert!(!is_public(&json!({ "to": ["https://m.example/users/a/followers"] })));
        assert!(!is_public(&json!({ "to": ["https://b.example/ap/communities/1"] })));
    }

    #[test]
    fn misskey_text_is_preferred() {
        let note = json!({
            "content": "<p>html</p>",
            "_misskey_content": "@cooking@example.com **pasta**",
            "summary": "food",
        });
        assert_eq!(note_text(&note, None), "⚠ food\n\n@cooking@example.com **pasta**");
        assert_eq!(note_text(&note, Some(("cooking", "example.com"))), "⚠ food\n\n**pasta**");
    }

    #[test]
    fn friendica_bbcode_is_not_shown() {
        // As Friendica sends a titled post.
        let note = json!({
            "type": "Article",
            "name": "Weekend",
            "content": "<p>A <strong>great</strong> trip, see <a href=\"https://example.com\">photos</a></p>",
            "source": { "content": "A [b]great[/b] trip, see [url=https://example.com]photos[/url]", "mediaType": "text/bbcode" },
        });
        assert_eq!(note_text(&note, None), "Weekend\n\nA great trip, see photos (https://example.com)");
        // Plain text and Markdown sources are still preferred.
        let note = json!({ "content": "<p>x</p>", "source": { "content": "*hello*", "mediaType": "text/markdown" } });
        assert_eq!(note_text(&note, None), "*hello*");
    }

    #[test]
    fn writefreely_articles_are_read() {
        // As WriteFreely sends them: HTML only.
        let note = json!({
            "type": "Article",
            "name": "On trains",
            "content": "<p>They are <strong>slow</strong>.</p><h2>Why</h2><p>Because.</p>",
        });
        assert_eq!(article(&note), Some(("On trains".to_string(), "They are **slow**.\n\n## Why\n\nBecause.".to_string())));
        // The Markdown source when there is one, as Tison sends it.
        let note = json!({ "type": "Article", "name": "T", "content": "<p>x</p>", "source": { "content": "*x*", "mediaType": "text/markdown" } });
        assert_eq!(article(&note).map(|(_, body)| body), Some("*x*".to_string()));
        // Not an article: a Note, or no title.
        assert_eq!(article(&json!({ "type": "Note", "name": "T", "content": "x" })), None);
        assert_eq!(article(&json!({ "type": "Article", "name": " ", "content": "x" })), None);
    }

    #[test]
    fn peertube_videos_are_read() {
        // As PeerTube sends them: MP4 files by resolution inside the HLS link.
        let video = json!({
            "type": "Video",
            "name": "Sunset",
            "duration": "PT95S",
            "url": [
                { "type": "Link", "mediaType": "text/html", "href": "https://tube.example/w/abc" },
                { "type": "Link", "mediaType": "application/x-mpegURL", "href": "https://tube.example/master.m3u8", "tag": [
                    { "type": "Link", "mediaType": "video/mp4", "href": "https://tube.example/1080.mp4", "height": 1080, "size": 900000000 },
                    { "type": "Link", "mediaType": "video/mp4", "href": "https://tube.example/720.mp4", "height": 720, "size": 50000000, "width": 1280 },
                    { "type": "Link", "mediaType": "video/mp4", "href": "https://tube.example/360.mp4", "height": 360, "size": 9000000 },
                ]},
            ],
            "icon": [
                { "type": "Image", "url": "https://tube.example/small.jpg", "width": 280 },
                { "type": "Image", "url": "https://tube.example/big.jpg", "width": 850 },
            ],
        });
        let media = remote_media(&video).unwrap();
        assert_eq!(media.kind, Kind::Video);
        assert_eq!(media.file.map(|file| (file.url, file.height)), Some(("https://tube.example/720.mp4".to_string(), Some(720))));
        assert_eq!(media.poster.as_deref(), Some("https://tube.example/big.jpg"));
        assert_eq!(media.duration, Some(95));
        // Funkwhale's sounds.
        let audio = json!({ "type": "Audio", "url": [{ "type": "Link", "mediaType": "audio/mpeg", "href": "https://fw.example/listen/1.mp3" }], "duration": 300 });
        let media = remote_media(&audio).unwrap();
        assert_eq!((media.kind, media.duration), (Kind::Episode, Some(300)));
        assert_eq!(media.file.map(|file| file.url), Some("https://fw.example/listen/1.mp3".to_string()));
        assert!(remote_media(&json!({ "type": "Note" })).is_none());
    }

    #[test]
    fn mobilizon_events_are_read() {
        // As Mobilizon sends them.
        let event = json!({
            "type": "Event",
            "name": "Picnic in the park",
            "startTime": "2026-11-14T13:00:00+01:00",
            "endTime": "2026-11-14T17:00:00+01:00",
            "timezone": "Europe/Paris",
            "category": "OUTDOORS_ADVENTURE",
            "joinMode": "restricted",
            "ical:status": "TENTATIVE",
            "maximumAttendeeCapacity": 30,
            "participantCount": 12,
            "location": {
                "type": "Place",
                "name": "Parc Monceau",
                "address": { "type": "PostalAddress", "streetAddress": "35 Boulevard de Courcelles", "postalCode": "75008", "addressLocality": "Paris", "addressCountry": "France" },
            },
            "attachment": [
                { "type": "Document", "mediaType": "image/jpeg", "url": "https://mobilizon.example/media/banner.jpg", "name": "Banner" },
                { "type": "Link", "mediaType": "text/html", "name": "Website", "href": "https://meet.example/picnic" },
            ],
        });
        let found = remote_event(&event).unwrap();
        assert_eq!(found.starts_at.to_rfc3339(), "2026-11-14T12:00:00+00:00");
        assert_eq!(found.ends_at.map(|end| end.to_rfc3339()).as_deref(), Some("2026-11-14T16:00:00+00:00"));
        assert_eq!((found.timezone.as_str(), found.category, found.join_mode, found.status), ("Europe/Paris", "OUTDOORS_ADVENTURE", "restricted", "tentative"));
        assert_eq!((found.capacity, found.participants), (Some(30), Some(12)));
        assert_eq!(found.place_name, "Parc Monceau");
        assert_eq!(found.address, "35 Boulevard de Courcelles, 75008, Paris, France");
        assert_eq!(found.online_url.as_deref(), Some("https://meet.example/picnic"));
        assert_eq!(found.banner.as_deref(), Some("https://mobilizon.example/media/banner.jpg"));
        // An end before its start is dropped; without a start, not an event.
        assert!(remote_event(&json!({ "type": "Event", "startTime": "2026-11-14T13:00:00Z", "endTime": "2026-11-14T12:00:00Z" })).unwrap().ends_at.is_none());
        assert!(remote_event(&json!({ "type": "Event", "name": "x" })).is_none());
        assert!(remote_event(&json!({ "type": "Note", "startTime": "2026-11-14T13:00:00Z" })).is_none());
    }

    #[test]
    fn attachments_are_read() {
        let note = json!({
            "attachment": [
                { "type": "Document", "mediaType": "image/png", "url": "https://m.example/a.png", "name": "A cat", "width": 640, "height": 480 },
                { "type": "Video", "url": [{ "type": "Link", "mediaType": "video/mp4", "href": "https://m.example/v.mp4" }] },
                { "type": "Document", "url": "javascript:alert(1)" },
                { "type": "PropertyValue", "name": "x", "value": "y" },
            ],
        });
        let found = attachments(&note);
        assert_eq!(found.len(), 2);
        assert_eq!((found[0].url.as_str(), found[0].description.as_str(), found[0].width), ("https://m.example/a.png", "A cat", Some(640)));
        assert_eq!((found[1].url.as_str(), found[1].media_type.as_str()), ("https://m.example/v.mp4", "video/mp4"));
    }

    #[test]
    fn followers_only_is_detected() {
        let sender = RemoteActor {
            id: 1,
            actor_id: "https://m.example/users/a".to_string(),
            inbox: String::new(),
            public_key_pem: String::new(),
            followers_url: None,
            blocked: false,
        };
        assert!(is_for_followers(&json!({ "to": ["https://m.example/users/a/followers"] }), &sender));
        // Friendica's form, and a collection named otherwise in the profile.
        assert!(is_for_followers(&json!({ "to": ["https://m.example/followers/a"] }), &sender));
        let named = RemoteActor { followers_url: Some("https://m.example/c/a/fans".to_string()), ..sender.clone() };
        assert!(is_for_followers(&json!({ "cc": ["https://m.example/c/a/fans"] }), &named));
        assert!(!is_message(&json!({ "type": "Note", "to": ["https://m.example/followers/a"] }), &sender));
        assert!(is_message(&json!({ "type": "Note", "to": ["https://b.example/ap/users/1"] }), &sender));
        assert!(!is_for_followers(&json!({ "to": ["https://b.example/ap/users/1"] }), &sender));
        assert!(!is_for_followers(&json!({ "to": ["https://evil.example/users/a/followers"] }), &sender));
    }
}

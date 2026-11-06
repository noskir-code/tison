//! The messenger: private conversations with users of this server and with
//! accounts of other Fediverse servers (see `entities::message`).
//!
//! New messages appear live: the WebSocket of the notifications says which
//! conversation changed, and the open conversation asks for what's new
//! (`/messenger/{id}/after/{message_id}`, see `assets/system.js`).

use std::collections::HashMap;

use askama::Template;
use axum::{
    Form, Json,
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use serde_json::json;
use sqlx::PgPool;
use tower_sessions::Session;

use crate::{
    AppState,
    entities::{
        author::Author,
        friend,
        media::{self, NewAttachment},
        message::{self, BODY_MAX_CHARS, Conversation, Message, Summary},
        notification,
        post::BodyError,
        user::{self, User},
    },
    handlers::{
        image::{self, ImageOwner, MAX_IMAGE_BYTES},
        notifier,
        search::{self, Found},
        system::{Layout, error_page, render_template},
        user::signed_in_user,
    },
};
/// `|t` and `|tf` in the templates, see `i18n`.
use crate::i18n::filters;

#[derive(Template)]
#[template(path = "messenger.html")]
struct MessengerTemplate {
    title: String,
    layout: Layout,
    conversations: Vec<Summary>,
    /// Friends without a message yet, after the conversations: one click
    /// opens the conversation.
    friends: Vec<User>,
    error: Option<String>,
    /// `@name@server` can be written to.
    fediverse: bool,
}

#[derive(Template)]
#[template(path = "conversation.html")]
struct ConversationTemplate {
    title: String,
    layout: Layout,
    conversation: Conversation,
    messages: Vec<Message>,
    error: Option<String>,
    body_max_chars: usize,
    max_attachments: usize,
    /// The other person can't receive messages: shown instead of the form.
    closed: Option<&'static str>,
    /// The viewer blocked the other person: the settings offer "Unblock".
    blocked_by_me: bool,
    /// The other person can be reported (not of the site's staff).
    can_report: bool,
    /// What was just changed in the settings of the conversation.
    saved: Option<&'static str>,
}

/// New messages of a conversation, for the live page.
#[derive(Template)]
#[template(
    source = "{% import \"macros/message.html\" as ui %}{% for message in messages %}{% call ui::message_item(message, conversation_id) %}{% endcall %}{% endfor %}",
    ext = "html"
)]
struct MessageItems<'a> {
    messages: &'a [Message],
    conversation_id: i64,
}

/// Results sent back as `?error=<code>`.
fn error_message(code: &str) -> Option<String> {
    Some(match code {
        "empty" => "Write something or add a photo or a file before sending.".to_string(),
        "too-many" => crate::i18n::tf("A message can have at most {} photos and files.", MAX_ATTACHMENTS),
        "photo" => crate::i18n::tf("Photos must be JPEG, PNG, GIF or WebP, of at most {} MB.", MAX_IMAGE_BYTES / 1024 / 1024),
        "file" => crate::i18n::tf(
            "Attached files must be PDF, text, office documents (Word, LibreOffice), EPUB or ZIP, of at most {} MB.",
            crate::document::MAX_BYTES / 1024 / 1024,
        ),
        "too-long" => format!("A message can be at most {BODY_MAX_CHARS} characters."),
        "invalid-characters" => "The message contains characters that are not allowed.".to_string(),
        "nobody" => "Nobody found with this name or address.".to_string(),
        "yourself" => "You can't write to yourself.".to_string(),
        "blocked" => "You can't write to this person: one of you blocked the other.".to_string(),
        "bot" => "A bot only writes to people who follow it or wrote to it first.".to_string(),
        "duplicate" => "You sent this same message several times in the last hour: repeating it looks like spam.".to_string(),
        "fediverse-off" => "Your account isn't on the Fediverse: it can only write to people of this server.".to_string(),
        "server" => "Something went wrong, try again later.".to_string(),
        _ => return None,
    })
}

fn body_error_code(err: BodyError) -> &'static str {
    match err {
        BodyError::Empty => "empty",
        BodyError::TooLong => "too-long",
        BodyError::InvalidCharacters => "invalid-characters",
    }
}

/// The conversations of the signed-in user, latest first.
pub async fn messenger_page(
    State(state): State<AppState>,
    session: Session,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let conversations = message::list(&pool, me.id).await.unwrap_or_else(|err| {
        tracing::error!("failed to load conversations of {}: {}", me.username, err);
        Vec::new()
    });
    // Friends already in a conversation appear there.
    let mut friends: Vec<User> = friend::friends(&pool, me.id)
        .await
        .unwrap_or_else(|err| {
            tracing::error!("failed to load friends of {}: {}", me.username, err);
            Vec::new()
        })
        .into_iter()
        .filter(|friend| {
            !conversations
                .iter()
                .any(|conversation| conversation.other.local().is_some_and(|other| other.id == friend.id))
        })
        .collect();
    friends.sort_by_key(|friend| friend.display_name().to_lowercase());
    render_template(&MessengerTemplate {
        title: "Messenger".to_string(),
        layout: Layout::load(&state, &session).await,
        conversations,
        friends,
        error: query.get("error").and_then(|code| error_message(code)),
        fediverse: state.federation.is_some() && me.federation_enabled,
    })
}

/// Tells the open pages of both people that the conversation changed.
async fn signal(pool: &PgPool, conversation_id: i64) {
    let signalled = async {
        let people = message::local_people(pool, conversation_id).await?;
        notification::signal_message(pool, &people, conversation_id).await
    }
    .await;
    if let Err(err) = signalled {
        tracing::warn!("failed to signal conversation {}: {}", conversation_id, err);
    }
}

/// Tells the other person here of a new message, for their browsers.
async fn signal_received(pool: &PgPool, conversation_id: i64, sender_id: i32, message_id: i64) {
    let signalled = async {
        let mut people = message::local_people(pool, conversation_id).await?;
        people.retain(|person| *person != sender_id);
        notification::signal_new_message(pool, &people, message_id).await
    }
    .await;
    if let Err(err) = signalled {
        tracing::warn!("failed to signal message {}: {}", message_id, err);
    }
}

/// Opens (or starts) the conversation with `to`: a username here, or an
/// account of another server (`@name@server` or the link of its profile).
pub async fn start(State(state): State<AppState>, session: Session, Form(form): Form<HashMap<String, String>>) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let to = form.get("to").map(|to| to.trim()).unwrap_or_default();
    let back = |code: &str| Redirect::to(&format!("/messenger?error={code}#new")).into_response();
    let local_name = match state.federation.as_deref() {
        // `@name@this-domain` is a user here.
        Some(federation) => to
            .trim_start_matches('@')
            .strip_suffix(&format!("@{}", federation.domain))
            .map(str::to_string),
        None => None,
    };
    if search::is_address(to) && local_name.is_none() {
        if state.federation.is_none() || !me.federation_enabled {
            return back("fediverse-off");
        }
        return match search::find_account(&state, &pool, &me, to).await {
            Ok(Found::Remote(account)) => open_remote(&pool, &me, account.author.id).await,
            Ok(Found::Local(_)) | Err(_) => back("nobody"),
        };
    }
    let name = local_name.unwrap_or_else(|| to.trim_start_matches('@').to_string());
    match user::find_by_username(&pool, &name).await {
        Ok(Some(other)) if other.id == me.id => back("yourself"),
        Ok(Some(other)) => open_local(&pool, &me, &other).await,
        Ok(None) => back("nobody"),
        Err(err) => {
            tracing::error!("failed to find {}: {}", name, err);
            back("server")
        }
    }
}

async fn open_local(pool: &PgPool, me: &User, other: &User) -> Response {
    if crate::entities::block::between(pool, me.id, other.id).await.unwrap_or(false) {
        return Redirect::to("/messenger?error=blocked#new").into_response();
    }
    match message::with_user(pool, me.id, other.id).await {
        Ok(id) => Redirect::to(&format!("/messenger/{id}")).into_response(),
        Err(err) => {
            tracing::error!("failed to open a conversation with {}: {}", other.username, err);
            Redirect::to("/messenger?error=server").into_response()
        }
    }
}

async fn open_remote(pool: &PgPool, me: &User, remote_actor_id: i64) -> Response {
    match message::with_remote(pool, me.id, remote_actor_id).await {
        Ok(id) => Redirect::to(&format!("/messenger/{id}")).into_response(),
        Err(err) => {
            tracing::error!("failed to open a conversation with actor {}: {}", remote_actor_id, err);
            Redirect::to("/messenger?error=server").into_response()
        }
    }
}

/// "Message" on a profile here, and the friends of the messenger's list
/// (a link: opening a conversation changes nothing until a message is sent).
pub async fn with_user(State(state): State<AppState>, session: Session, Path(username): Path<String>) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    match user::find_by_username(&pool, &username).await {
        Ok(Some(other)) if other.id != me.id => open_local(&pool, &me, &other).await,
        Ok(_) => Redirect::to("/messenger?error=nobody#new").into_response(),
        Err(err) => {
            tracing::error!("failed to find {}: {}", username, err);
            Redirect::to("/messenger?error=server").into_response()
        }
    }
}

/// "Message" on the page of an account of another server.
pub async fn with_remote(State(state): State<AppState>, session: Session, Path(actor_id): Path<i64>) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    if state.federation.is_none() || !me.federation_enabled {
        return Redirect::to("/messenger?error=fediverse-off").into_response();
    }
    open_remote(&pool, &me, actor_id).await
}

/// The conversation of the signed-in user, `Err` with the page to show
/// when it isn't theirs.
async fn load(state: &AppState, session: &Session, id: i64) -> Result<(PgPool, User, Conversation), Response> {
    let (pool, me) = signed_in_user(state, session).await?;
    match message::find(&pool, id, me.id).await {
        Ok(Some(conversation)) => Ok((pool, me, conversation)),
        Ok(None) => Err(error_page(
            Layout::load(state, session).await,
            StatusCode::NOT_FOUND,
            "This conversation does not exist.",
        )),
        Err(err) => {
            tracing::error!("failed to load conversation {}: {}", id, err);
            Err(error_page(
                Layout::load(state, session).await,
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to load this conversation.",
            ))
        }
    }
}

/// The other person and the user block each other, either way (Server
/// rules, 2): no message can go.
async fn blocked(pool: &PgPool, me: &User, other: &Author) -> bool {
    let result = match other {
        Author::Local(other) => crate::entities::block::between(pool, me.id, other.id).await,
        Author::Remote(other) => crate::entities::block::blocks_actor(pool, me.id, other.id).await,
    };
    result.unwrap_or_else(|err| {
        tracing::warn!("failed to check the blocks of @{}: {}", me.username, err);
        false
    })
}

/// Why the other person can't receive messages, if they can't.
fn closed(state: &AppState, me: &User, conversation: &Conversation) -> Option<&'static str> {
    match &conversation.other {
        Author::Remote(_) if state.federation.is_none() => Some("The Fediverse is turned off on this server."),
        Author::Remote(_) if !me.federation_enabled => {
            Some("Your account isn't on the Fediverse: it can only write to people of this server.")
        }
        _ => None,
    }
}

pub async fn conversation_page(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<i64>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let (pool, me, conversation) = match load(&state, &session, id).await {
        Ok(loaded) => loaded,
        Err(response) => return response,
    };
    let messages = match message::messages(&pool, id, me.id, None).await {
        Ok(messages) => messages,
        Err(err) => {
            tracing::error!("failed to load messages of {}: {}", id, err);
            Vec::new()
        }
    };
    if let Err(err) = message::mark_read(&pool, id, me.id).await {
        tracing::warn!("failed to mark conversation {} read: {}", id, err);
    }
    // Other tabs update their unread count.
    notification::signal_message(&pool, &[me.id], id).await.ok();
    let blocked_by_me = match &conversation.other {
        Author::Local(other) => crate::entities::block::blocks_user(&pool, me.id, other.id).await,
        Author::Remote(other) => crate::entities::block::blocks_actor(&pool, me.id, other.id).await,
    }
    .unwrap_or_else(|err| {
        tracing::warn!("failed to check the blocks of @{}: {}", me.username, err);
        false
    });
    let can_report = !matches!(&conversation.other, Author::Local(other) if other.is_staff());
    let saved = query.get("saved").and_then(|saved| match saved.as_str() {
        "muted" => Some("Muted: you won't be notified of new messages in this conversation."),
        "unmuted" => Some("You will be notified of new messages in this conversation again."),
        _ => None,
    });
    render_template(&ConversationTemplate {
        title: crate::i18n::tf("Messages with {}", conversation.other.display_name()),
        blocked_by_me,
        can_report,
        saved,
        layout: Layout::load(&state, &session).await,
        closed: match blocked(&pool, &me, &conversation.other).await {
            true => Some("You can't write to this person: one of you blocked the other."),
            false => closed(&state, &me, &conversation),
        },
        conversation,
        messages,
        error: query.get("error").and_then(|code| error_message(code)),
        body_max_chars: BODY_MAX_CHARS,
        max_attachments: MAX_ATTACHMENTS,
    })
}

/// Photos and files of one message, together.
pub const MAX_ATTACHMENTS: usize = 4;

/// The largest message form: its photos and files, and its text.
pub const MAX_SEND_BYTES: usize = MAX_ATTACHMENTS * crate::document::MAX_BYTES + 1024 * 1024;

/// A message form: its text, photos (`photos`, also from the camera) and
/// files (`files`, see `document`). `Err` with the error code.
struct SentForm {
    body: String,
    photos: Vec<Vec<u8>>,
    files: Vec<(Vec<u8>, String)>,
}

async fn read_form(multipart: &mut Multipart) -> Result<SentForm, &'static str> {
    let mut form = SentForm { body: String::new(), photos: Vec::new(), files: Vec::new() };
    while let Some(field) = multipart.next_field().await.map_err(|_| "file")? {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "photos" | "files" => {
                let file_name = crate::document::shown_name(field.file_name().unwrap_or_default());
                let data = field.bytes().await.map_err(|_| "file")?;
                if data.is_empty() {
                    continue;
                }
                if form.photos.len() + form.files.len() == MAX_ATTACHMENTS {
                    return Err("too-many");
                }
                if name == "photos" {
                    if data.len() > MAX_IMAGE_BYTES {
                        return Err("photo");
                    }
                    form.photos.push(data.to_vec());
                } else {
                    if data.len() > crate::document::MAX_BYTES {
                        return Err("file");
                    }
                    form.files.push((data.to_vec(), file_name));
                }
            }
            "body" => form.body = field.text().await.map_err(|_| "server")?,
            _ => {}
        }
    }
    Ok(form)
}

/// Saves the photos (their location and camera details removed) and files of
/// a message under random names. Nothing is kept when one fails.
async fn save_attachments(user_id: i32, form: &SentForm) -> Result<Vec<NewAttachment>, &'static str> {
    let mut cleaned = Vec::new();
    for data in &form.photos {
        cleaned.push(crate::photo::prepare(data).await.map_err(|_| "photo")?);
    }
    let documents = form
        .files
        .iter()
        .map(|(data, name)| crate::document::detect(data, name).map(|document| (data, name, document)).ok_or("file"))
        .collect::<Result<Vec<_>, _>>()?;
    let mut saved: Vec<NewAttachment> = Vec::new();
    let owner = ImageOwner::User(user_id);
    let mut failed = false;
    for photo in cleaned {
        match image::save_private_file(owner, "message", photo.extension, &photo.data).await {
            Ok(url) => saved.push(NewAttachment::new(url, photo.media_type.to_string(), "", i32::try_from(photo.width).ok(), i32::try_from(photo.height).ok())),
            Err(_) => failed = true,
        }
    }
    for (data, name, document) in documents {
        match image::save_private_file(owner, "message", document.extension, data).await {
            Ok(url) => saved.push(NewAttachment::new(url, document.media_type.to_string(), name, None, None)),
            Err(_) => failed = true,
        }
    }
    if failed {
        tracing::error!("failed to save the attachments of a message of user {}", user_id);
        for attachment in &saved {
            image::remove_image(owner, &attachment.url).await;
        }
        return Err("server");
    }
    Ok(saved)
}

pub async fn send(State(state): State<AppState>, session: Session, Path(id): Path<i64>, mut multipart: Multipart) -> Response {
    let (pool, me, conversation) = match load(&state, &session, id).await {
        Ok(loaded) => loaded,
        Err(response) => return response,
    };
    if closed(&state, &me, &conversation).is_some() || blocked(&pool, &me, &conversation.other).await {
        return Redirect::to(&format!("/messenger/{id}")).into_response();
    }
    // Bots only write to who interacts with them (Server rules, 7).
    if me.is_bot() {
        let (user_id, actor_id) = match &conversation.other {
            Author::Local(other) => (Some(other.id), None),
            Author::Remote(other) => (None, Some(other.id)),
        };
        if !crate::entities::user::bot_may_message(&pool, me.id, id, user_id, actor_id).await.unwrap_or(false) {
            return Redirect::to(&format!("/messenger/{id}?error=bot#composer")).into_response();
        }
    }
    let key = me.id.to_string();
    if !crate::security::hit(crate::security::Limit::Message, &key) {
        return error_page(Layout::load(&state, &session).await, StatusCode::TOO_MANY_REQUESTS, crate::security::Limit::Message.message());
    }

    let back = |code: &str| Redirect::to(&format!("/messenger/{id}?error={code}#composer")).into_response();
    let form = match read_form(&mut multipart).await {
        Ok(form) => form,
        Err(code) => return back(code),
    };
    let has_attachments = !form.photos.is_empty() || !form.files.is_empty();
    // A photo or a file can go alone.
    let body = match message::validate_body(&form.body) {
        Ok(body) => body,
        Err(BodyError::Empty) if has_attachments => String::new(),
        Err(err) => return back(body_error_code(err)),
    };
    // The same message again and again (to many people): refused, see
    // `entities::spam`.
    let sender = crate::entities::spam::Sender::Local(me.id);
    if crate::entities::spam::is_repeated(&pool, sender, crate::entities::spam::Kind::Message, &body, None).await.unwrap_or_else(|err| {
        tracing::warn!("failed to check repeats: {}", err);
        false
    }) {
        if let Err(err) = crate::entities::spam::record(&pool, sender, crate::entities::spam::Kind::Message, &body, &format!("@{}", me.username)).await {
            tracing::warn!("failed to record a repeat of @{}: {}", me.username, err);
        }
        return back("duplicate");
    }
    let attachments = match save_attachments(me.id, &form).await {
        Ok(attachments) => attachments,
        Err(code) => return back(code),
    };
    let added = match message::add(&pool, id, me.id, &body).await {
        Ok(added) => media::save_attachments(&pool, media::Owner::Message(added.0), &attachments).await.map(|()| added),
        Err(err) => Err(err),
    };
    match added {
        Ok((message_id, sent_at)) => {
            signal(&pool, id).await;
            signal_received(&pool, id, me.id, message_id).await;
            notifier::message_sent(&state, &pool, &me, &conversation, message_id, &body, &attachments, &sent_at).await;
            Redirect::to(&format!("/messenger/{id}#message-{message_id}")).into_response()
        }
        Err(err) => {
            tracing::error!("failed to send a message in {}: {}", id, err);
            for attachment in &attachments {
                image::remove_image(ImageOwner::User(me.id), &attachment.url).await;
            }
            back("server")
        }
    }
}

/// Deletes one of the user's messages, for the other person too.
pub async fn delete(State(state): State<AppState>, session: Session, Path((id, message_id)): Path<(i64, i64)>) -> Response {
    let (pool, me, conversation) = match load(&state, &session, id).await {
        Ok(loaded) => loaded,
        Err(response) => return response,
    };
    // Its photos and files go with it: they were private.
    let files = media::local_files_of_message(&pool, message_id).await.unwrap_or_default();
    match message::delete_own(&pool, message_id, me.id).await {
        Ok(Some(conversation_id)) if conversation_id == id => {
            for url in &files {
                image::remove_image(ImageOwner::User(me.id), url).await;
            }
            signal(&pool, id).await;
            notifier::message_deleted(&state, &pool, &me, &conversation, message_id).await;
        }
        Ok(_) => {}
        Err(err) => tracing::error!("failed to delete message {}: {}", message_id, err),
    }
    Redirect::to(&format!("/messenger/{id}")).into_response()
}

/// Mutes (`muted=on`) or unmutes the conversation for the user, see
/// `message::set_muted`.
pub async fn mute(State(state): State<AppState>, session: Session, Path(id): Path<i64>, Form(form): Form<HashMap<String, String>>) -> Response {
    let (pool, me, _) = match load(&state, &session, id).await {
        Ok(loaded) => loaded,
        Err(response) => return response,
    };
    let muted = form.get("muted").is_some_and(|value| value == "on");
    if let Err(err) = message::set_muted(&pool, id, me.id, muted).await {
        tracing::error!("failed to mute conversation {}: {}", id, err);
        return error_page(Layout::load(&state, &session).await, StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong, try again later.");
    }
    // The unread count next to "Messenger" changes.
    notification::signal_message(&pool, &[me.id], id).await.ok();
    Redirect::to(&format!("/messenger/{id}?saved={}", if muted { "muted" } else { "unmuted" })).into_response()
}

/// Deletes a reported message for the moderators, with its photos and
/// files. One written here is also deleted on the other server, like when
/// its author deletes it.
pub async fn remove_reported(state: &AppState, pool: &PgPool, message_id: i64) -> Result<(), sqlx::Error> {
    let Some(conversation_id) = message::conversation_of(pool, message_id).await? else {
        return Ok(());
    };
    let sender: Option<i32> = sqlx::query_scalar("SELECT sender_user_id FROM messages WHERE id = $1").bind(message_id).fetch_one(pool).await?;
    let files = media::local_files_of_message(pool, message_id).await.unwrap_or_default();
    let sender = match sender {
        Some(sender_id) => crate::entities::user::find_by_id(pool, sender_id).await?,
        None => None,
    };
    message::delete(pool, message_id).await?;
    if let Some(sender) = &sender {
        for url in &files {
            image::remove_image(ImageOwner::User(sender.id), url).await;
        }
        if let Some(conversation) = message::find(pool, conversation_id, sender.id).await? {
            notifier::message_deleted(state, pool, sender, &conversation, message_id).await;
        }
    }
    signal(pool, conversation_id).await;
    Ok(())
}

/// `{"html": "...", "ids": [...]}`: the messages after `message_id`, and the
/// ids of all the shown ones (to remove deleted ones). Marks it read.
pub async fn after(State(state): State<AppState>, session: Session, Path((id, message_id)): Path<(i64, i64)>) -> Response {
    let (pool, me, _) = match load(&state, &session, id).await {
        Ok(loaded) => loaded,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let loaded = async {
        let new = message::messages(&pool, id, me.id, Some(message_id)).await?;
        let all = message::messages(&pool, id, me.id, None).await?;
        message::mark_read(&pool, id, me.id).await?;
        Ok::<_, sqlx::Error>((new, all))
    }
    .await;
    let (new, all) = match loaded {
        Ok(loaded) => loaded,
        Err(err) => {
            tracing::error!("failed to load new messages of {}: {}", id, err);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let html = match (MessageItems { messages: &new, conversation_id: id }).render() {
        Ok(html) => html,
        Err(err) => {
            tracing::error!("failed to render messages: {}", err);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let ids: Vec<i64> = all.iter().map(|shown| shown.id).collect();
    let bodies: HashMap<String, String> = all.iter().map(|shown| (shown.id.to_string(), shown.body_html())).collect();
    Json(json!({ "html": html, "ids": ids, "bodies": bodies })).into_response()
}

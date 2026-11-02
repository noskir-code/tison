//! Events and meetups, like Mobilizon (`EVENT-PLANNING` in
//! `configs/services.conf`): creating and editing them, joining and
//! leaving, the organizer's approvals, and their calendar file. The event
//! itself is shown by `comment::post_page`, see `event_full` in
//! macros/post.html.

use std::collections::HashMap;

use askama::Template;
use axum::{
    extract::{Multipart, Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use sqlx::types::chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use tower_sessions::Session;

use crate::{
    AppState,
    entities::{
        event::{self, CATEGORIES, NewEvent, Participant},
        media,
        notification,
        post::{self, BODY_MAX_CHARS, EventList, Post},
        user::User,
    },
    handlers::{
        image::{self, ImageOwner, MAX_IMAGE_BYTES},
        notifier,
        system::{Layout, error_page, render_template},
        user::signed_in_user,
        video::{Upload, read_upload},
    },
};
/// `|t` and `|tf` in the templates, see `i18n`.
use crate::i18n::filters;

/// Errors of the event form, sent back as `?error=<code>`.
#[derive(Clone, Copy, Debug)]
enum EventError {
    NoTitle,
    TitleTooLong,
    NoStart,
    EndBeforeStart,
    DescriptionTooLong,
    NoPlace,
    BadLink,
    BadCapacity,
    InvalidCharacters,
    BannerFormat,
    BannerTooLarge,
    BadRepeat,
    NotMember,
    Server,
}

impl EventError {
    const ALL: [Self; 14] = [
        Self::NoTitle,
        Self::TitleTooLong,
        Self::NoStart,
        Self::EndBeforeStart,
        Self::DescriptionTooLong,
        Self::NoPlace,
        Self::BadLink,
        Self::BadCapacity,
        Self::InvalidCharacters,
        Self::BannerFormat,
        Self::BannerTooLarge,
        Self::BadRepeat,
        Self::NotMember,
        Self::Server,
    ];

    fn code(self) -> &'static str {
        match self {
            Self::NoTitle => "no-title",
            Self::TitleTooLong => "title-too-long",
            Self::NoStart => "no-start",
            Self::EndBeforeStart => "end-before-start",
            Self::DescriptionTooLong => "description-too-long",
            Self::NoPlace => "no-place",
            Self::BadLink => "bad-link",
            Self::BadCapacity => "bad-capacity",
            Self::InvalidCharacters => "invalid-characters",
            Self::BannerFormat => "banner-format",
            Self::BannerTooLarge => "banner-too-large",
            Self::BadRepeat => "bad-repeat",
            Self::NotMember => "not-member",
            Self::Server => "server",
        }
    }

    fn message(self) -> String {
        match self {
            Self::NoTitle => "Give the event a title.".to_string(),
            Self::TitleTooLong => format!("A title can be at most {} characters, on one line.", event::TITLE_MAX_CHARS),
            Self::NoStart => "Choose when it starts.".to_string(),
            Self::EndBeforeStart => "It must end after it starts.".to_string(),
            Self::DescriptionTooLong => crate::i18n::tf("The description can be at most {} characters.", BODY_MAX_CHARS),
            Self::NoPlace => "Say where it takes place: a place, an address, or a link to join online.".to_string(),
            Self::BadLink => "The online link must start with https:// or http://.".to_string(),
            Self::BadCapacity => "The number of places must be a whole number from 1.".to_string(),
            Self::InvalidCharacters => "The text contains characters that are not allowed.".to_string(),
            Self::BannerFormat => "The picture must be a JPEG, PNG, GIF or WebP.".to_string(),
            Self::BannerTooLarge => crate::i18n::tf("The picture can be at most {} MB.", MAX_IMAGE_BYTES / 1024 / 1024),
            Self::BadRepeat => format!("A repeated event needs its last date, after the first one and within {} days.", event::SERIES_MAX_DAYS),
            Self::NotMember => "You can only organize events for communities you belong to.".to_string(),
            Self::Server => "Something went wrong, try again later.".to_string(),
        }
    }

    fn from_query(query: &HashMap<String, String>) -> Option<String> {
        let code = query.get("error")?;
        Self::ALL.into_iter().find(|error| error.code() == code).map(Self::message)
    }
}

/// A one-line text, possibly empty, at most `max` characters.
fn line(raw: &str, max: usize) -> Result<String, EventError> {
    match post::validate_text(raw, max) {
        Ok(text) if text.contains('\n') => Err(EventError::TitleTooLong),
        Ok(text) => Ok(text),
        Err(post::BodyError::Empty) => Ok(String::new()),
        Err(post::BodyError::TooLong) => Err(EventError::TitleTooLong),
        Err(post::BodyError::InvalidCharacters) => Err(EventError::InvalidCharacters),
    }
}

/// `2026-11-07T18:30` written in a time zone `offset` minutes east of UTC
/// (sent by system.js; 0 without JavaScript).
fn parse_time(raw: &str, offset: &str) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    let naive = NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S"))
        .ok()?;
    let minutes: i32 = offset.trim().parse().unwrap_or(0);
    let zone = FixedOffset::east_opt(minutes.clamp(-18 * 60, 18 * 60) * 60)?;
    zone.from_local_datetime(&naive).single().map(|time| time.with_timezone(&Utc))
}

/// A time zone name like `Europe/Paris`, else `UTC`.
fn timezone(raw: &str) -> String {
    let raw = raw.trim();
    let valid = (1..=64).contains(&raw.len())
        && raw.chars().all(|letter| letter.is_ascii_alphanumeric() || matches!(letter, '/' | '_' | '-' | '+'));
    if valid { raw.to_string() } else { "UTC".to_string() }
}

/// The checked form of an event.
struct EventForm {
    title: String,
    body: String,
    starts_at: DateTime<Utc>,
    ends_at: Option<DateTime<Utc>>,
    timezone: String,
    place_name: String,
    address: String,
    online_url: Option<String>,
    category: &'static str,
    capacity: Option<i32>,
    approval: bool,
    online_public: bool,
    /// Organized by this community (FEP-1b12 groups, like Mobilizon's).
    community_id: Option<i64>,
    /// Repeats, until a last start.
    repeat: Option<(event::Frequency, DateTime<Utc>)>,
}

impl EventForm {
    fn read(upload: &Upload) -> Result<Self, EventError> {
        let title = match line(upload.field("title"), event::TITLE_MAX_CHARS)? {
            title if title.is_empty() => return Err(EventError::NoTitle),
            title => title,
        };
        let body = match post::validate_text(upload.field("description"), BODY_MAX_CHARS) {
            Ok(body) => body,
            Err(post::BodyError::Empty) => String::new(),
            Err(post::BodyError::TooLong) => return Err(EventError::DescriptionTooLong),
            Err(post::BodyError::InvalidCharacters) => return Err(EventError::InvalidCharacters),
        };
        let starts_at = parse_time(upload.field("starts_at"), upload.field("start_offset")).ok_or(EventError::NoStart)?;
        let ends_at = match upload.field("ends_at").trim() {
            "" => None,
            raw => Some(parse_time(raw, upload.field("end_offset")).ok_or(EventError::EndBeforeStart)?),
        };
        if ends_at.is_some_and(|end| end <= starts_at) {
            return Err(EventError::EndBeforeStart);
        }
        let place_name = line(upload.field("place_name"), event::PLACE_MAX_CHARS)?;
        let address = line(upload.field("address"), event::ADDRESS_MAX_CHARS)?;
        let online_url = match upload.field("online_url").trim() {
            "" => None,
            url if (url.starts_with("https://") || url.starts_with("http://")) && url.len() <= 1000 && !url.contains(char::is_whitespace) => {
                Some(url.to_string())
            }
            _ => return Err(EventError::BadLink),
        };
        if place_name.is_empty() && address.is_empty() && online_url.is_none() {
            return Err(EventError::NoPlace);
        }
        let capacity = match upload.field("capacity").trim() {
            "" | "0" => None,
            raw => Some(raw.parse::<i32>().ok().filter(|places| (1..=1_000_000).contains(places)).ok_or(EventError::BadCapacity)?),
        };
        let category = CATEGORIES
            .iter()
            .find(|(code, _)| *code == upload.field("category"))
            .map_or("MEETING", |(code, _)| code);
        let community_id = match upload.field("community").trim() {
            "" | "0" => None,
            raw => Some(raw.parse::<i64>().map_err(|_| EventError::NotMember)?),
        };
        // The last date: the end of that day, in the writer's time.
        let repeat = match event::Frequency::parse(upload.field("repeat")) {
            None => None,
            Some(frequency) => {
                let last = parse_time(&format!("{}T23:59", upload.field("repeat_until").trim()), upload.field("start_offset"))
                    .ok_or(EventError::BadRepeat)?;
                let days = (last.timestamp() - starts_at.timestamp()) / 86_400;
                if last <= starts_at || days > event::SERIES_MAX_DAYS {
                    return Err(EventError::BadRepeat);
                }
                Some((frequency, last))
            }
        };
        Ok(Self {
            title,
            body,
            starts_at,
            ends_at,
            timezone: timezone(upload.field("timezone")),
            place_name,
            address,
            online_url,
            category,
            capacity,
            approval: !upload.field("approval").is_empty(),
            online_public: !upload.field("online_public").is_empty(),
            community_id,
            repeat,
        })
    }

    /// Where to look for its map: its address, or its place's name.
    fn place_query(&self) -> String {
        match (self.address.trim(), self.place_name.trim()) {
            ("", place) => place.to_string(),
            (address, _) => address.to_string(),
        }
    }

    fn details<'a>(&'a self, status: &'a str) -> NewEvent<'a> {
        NewEvent {
            starts_at: self.starts_at,
            ends_at: self.ends_at,
            timezone: &self.timezone,
            place_name: &self.place_name,
            address: &self.address,
            online_url: self.online_url.as_deref(),
            category: self.category,
            capacity: self.capacity,
            join_mode: if self.approval { "restricted" } else { "free" },
            status,
            remote_participants: None,
            online_public: self.online_public,
            latitude: None,
            longitude: None,
            series_id: None,
        }
    }
}

/// The banner uploaded with the form: cleaned like photos (no place or
/// camera), saved in the organizer's folder. `None` without one.
async fn save_banner(user_id: i32, upload: &Upload) -> Result<Option<String>, EventError> {
    let Some(file) = upload.file("banner") else {
        return Ok(None);
    };
    let data = tokio::fs::read(file).await.map_err(|_| EventError::Server)?;
    let photo = crate::photo::clean(&data).map_err(|_| EventError::BannerFormat)?;
    image::save_file(ImageOwner::User(user_id), "event", photo.extension, &photo.data)
        .await
        .map(Some)
        .map_err(|_| EventError::Server)
}

async fn read_form(multipart: &mut Multipart) -> Result<Upload, EventError> {
    read_upload(multipart, &[("banner", MAX_IMAGE_BYTES)]).await.map_err(|error| match error {
        crate::handlers::video::MediaError::TooLarge => EventError::BannerTooLarge,
        _ => EventError::Server,
    })
}

#[derive(Template)]
#[template(path = "events.html")]
struct EventsTemplate {
    title: String,
    layout: Layout,
    /// The viewer's: organized, going or asked.
    mine: Vec<Post>,
    events: Vec<Post>,
    past: bool,
    /// The dates of one recurring event.
    series: bool,
}

/// What's coming (or what was, `?past`), and the viewer's own events.
pub async fn events_page(State(state): State<AppState>, session: Session, Query(query): Query<HashMap<String, String>>) -> Response {
    let layout = Layout::load(&state, &session).await;
    let Some(pool) = &state.pool else {
        return error_page(layout, StatusCode::SERVICE_UNAVAILABLE, "The database is not configured.");
    };
    let viewer_id = layout.viewer.as_ref().map(|viewer| viewer.id);
    let past = query.contains_key("past");
    let series = query.get("series").and_then(|id| id.parse::<i64>().ok());
    let load = |list| async move {
        post::events(pool, viewer_id, list).await.unwrap_or_else(|err| {
            tracing::error!("failed to list events: {}", err);
            Vec::new()
        })
    };
    let mut events = load(match (series, past) {
        (Some(series), _) => EventList::Series(series),
        (None, true) => EventList::Past,
        (None, false) => EventList::Upcoming,
    })
    .await;
    let mine = match (viewer_id, past, series) {
        (Some(viewer_id), false, None) => load(EventList::Mine(viewer_id)).await,
        _ => Vec::new(),
    };
    // The viewer's events are listed once, under "Your events".
    events.retain(|event| !mine.iter().any(|own| own.id == event.id));
    let title = match (&events.first(), series, past) {
        (Some(first), Some(_), _) => crate::i18n::tf("Dates of {}", first.title_text()),
        (None, Some(_), _) => "Dates".to_string(),
        (_, None, true) => "Past events".to_string(),
        (_, None, false) => "Events".to_string(),
    };
    render_template(&EventsTemplate {
        title,
        layout,
        mine,
        events,
        past,
        series: series.is_some(),
    })
}

/// Values shown in the form: empty for a new event, the event's for an edit.
struct FormValues {
    title: String,
    description: String,
    starts_at: String,
    ends_at: String,
    place_name: String,
    address: String,
    online_url: String,
    category: String,
    capacity: String,
    approval: bool,
    online_public: bool,
    banner_url: Option<String>,
    community_id: Option<i64>,
}

impl FormValues {
    /// Organized by this community (a reference, as Askama gives it).
    fn is_community(&self, id: &i64) -> bool {
        self.community_id == Some(*id)
    }
}

#[derive(Template)]
#[template(path = "event_form.html")]
struct EventFormTemplate {
    title: String,
    layout: Layout,
    action: String,
    values: FormValues,
    editing: bool,
    error: Option<String>,
    categories: [(&'static str, &'static str); 20],
    /// The communities the writer can organize for (new events only).
    communities: Vec<crate::entities::community::Community>,
    frequencies: [event::Frequency; 3],
    /// For a date of a recurring event: how it repeats.
    repeat_label: Option<String>,
    title_max_chars: usize,
    description_max_chars: usize,
}

pub async fn new_page(State(state): State<AppState>, session: Session, Query(query): Query<HashMap<String, String>>) -> Response {
    let layout = Layout::load(&state, &session).await;
    let Some(viewer_id) = layout.viewer.as_ref().map(|viewer| viewer.id) else {
        return Redirect::to("/login").into_response();
    };
    // Organized by one of their communities, from its page (`?community=`).
    let communities = match (&state.pool, crate::settings::services().content_aggregator) {
        (Some(pool), true) => crate::entities::community::joined(pool, viewer_id).await.unwrap_or_default(),
        _ => Vec::new(),
    };
    let chosen = query.get("community").and_then(|id| id.parse::<i64>().ok()).filter(|id| communities.iter().any(|community| community.id == *id));
    render_template(&EventFormTemplate {
        title: "New event".to_string(),
        layout,
        action: "/events".to_string(),
        values: FormValues {
            title: String::new(),
            description: String::new(),
            starts_at: String::new(),
            ends_at: String::new(),
            place_name: String::new(),
            address: String::new(),
            online_url: String::new(),
            category: "MEETING".to_string(),
            capacity: String::new(),
            approval: false,
            online_public: false,
            banner_url: None,
            community_id: chosen,
        },
        editing: false,
        error: EventError::from_query(&query),
        categories: CATEGORIES,
        communities,
        frequencies: event::Frequency::ALL,
        repeat_label: None,
        title_max_chars: event::TITLE_MAX_CHARS,
        description_max_chars: BODY_MAX_CHARS,
    })
}

pub async fn create(State(state): State<AppState>, session: Session, mut multipart: Multipart) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let back = |error: EventError| Redirect::to(&format!("/events/new?error={}", error.code())).into_response();
    let upload = match read_form(&mut multipart).await {
        Ok(upload) => upload,
        Err(error) => return back(error),
    };
    let form = EventForm::read(&upload);
    let banner = match &form {
        Ok(_) => save_banner(me.id, &upload).await,
        Err(_) => Ok(None),
    };
    upload.discard().await;
    let (form, banner) = match (form, banner) {
        (Ok(form), Ok(banner)) => (form, banner),
        (Err(error), _) | (_, Err(error)) => return back(error),
    };
    let fail = |error: EventError, banner: Option<String>| async move {
        media::remove_local_files(&banner.into_iter().collect::<Vec<_>>()).await;
        back(error)
    };
    // For a community: only its members organize.
    if let Some(community_id) = form.community_id {
        let member = match crate::entities::community::find_by_id(&pool, community_id).await {
            Ok(Some(community)) => crate::entities::community::membership(&pool, &community, me.id).await.is_ok_and(|membership| membership.is_member),
            _ => false,
        };
        if !member || !crate::settings::services().content_aggregator {
            return fail(EventError::NotMember, banner).await;
        }
    }
    let series_id = match form.repeat {
        Some((frequency, until)) => match event::create_series(&pool, frequency, until).await {
            Ok(id) => Some(id),
            Err(err) => {
                tracing::error!("failed to create a recurring event: {}", err);
                return fail(EventError::Server, banner).await;
            }
        },
        None => None,
    };
    let details = NewEvent { series_id, ..form.details("confirmed") };
    match post::create_event(&pool, me.id, &form.title, &form.body, banner.as_deref(), form.community_id, &details).await {
        Ok(post_id) => {
            locate(&state, &pool, post_id, form.place_query());
            tracing::info!("@{} created event {}", me.username, post_id);
            // The organizer goes, like on Mobilizon.
            if let Err(err) = event::join_local(&pool, post_id, me.id, "going", None).await {
                tracing::warn!("failed to add the organizer of event {}: {}", post_id, err);
            }
            notifier::post_created(&state, &pool, post_id).await;
            // Its next dates, made now rather than in a minute.
            if series_id.is_some() {
                extend_series(&state, &pool).await;
            }
            Redirect::to(&format!("/posts/{post_id}")).into_response()
        }
        Err(err) => {
            tracing::error!("failed to create event: {}", err);
            fail(EventError::Server, banner).await
        }
    }
}

/// Finds the place of an event for its map in the background, see
/// `geocode`; other servers then get its coordinates (Mobilizon's map).
fn locate(state: &AppState, pool: &sqlx::PgPool, post_id: i64, query: String) {
    let Some(geocoder) = state.geocoder.clone() else {
        return;
    };
    let (state, pool) = (state.clone(), pool.clone());
    tokio::spawn(async move {
        match crate::geocode::find(&state.web, &geocoder, &query).await {
            Ok(Some((latitude, longitude))) => match event::set_coordinates(&pool, post_id, latitude, longitude).await {
                Ok(()) => notifier::post_updated(&state, &pool, post_id).await,
                Err(err) => tracing::warn!("failed to save the place of event {}: {}", post_id, err),
            },
            Ok(None) => tracing::info!("no place found for event {}", post_id),
            Err(err) => tracing::warn!("failed to find the place of event {}: {}", post_id, err),
        }
    });
}

/// Whether `user` manages the event: its organizer, or a moderator of the
/// community organizing it.
async fn manages(pool: &sqlx::PgPool, user: &User, post: &Post) -> bool {
    if post.can_edit() {
        return true;
    }
    let Some(community) = &post.community else {
        return false;
    };
    match crate::entities::community::find_by_id(pool, community.id).await {
        Ok(Some(community)) => crate::entities::community::membership(pool, &community, user.id)
            .await
            .is_ok_and(|membership| membership.is_moderator),
        _ => false,
    }
}

/// An event the viewer manages (see `manages`).
async fn own_event(state: &AppState, session: &Session, post_id: i64) -> Result<(sqlx::PgPool, User, Post), Response> {
    let (pool, me) = signed_in_user(state, session).await?;
    let found = post::find(&pool, post_id, Some(me.id)).await;
    let managed = match &found {
        Ok(Some(post)) if post.is_event() => manages(&pool, &me, post).await,
        _ => false,
    };
    match found {
        Ok(Some(post)) if managed => Ok((pool, me, post)),
        Ok(_) => Err(error_page(Layout::load(state, session).await, StatusCode::NOT_FOUND, "This event does not exist or isn't yours.")),
        Err(err) => {
            tracing::error!("failed to load event {}: {}", post_id, err);
            Err(error_page(Layout::load(state, session).await, StatusCode::INTERNAL_SERVER_ERROR, "Failed to load this event."))
        }
    }
}

/// For `datetime-local`, in UTC: system.js writes it in the viewer's time.
fn input_time(time: &DateTime<Utc>) -> String {
    time.format("%Y-%m-%dT%H:%M").to_string()
}

pub async fn edit_page(
    State(state): State<AppState>,
    session: Session,
    Path(post_id): Path<i64>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let (_, _, post) = match own_event(&state, &session, post_id).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let Some(details) = post.event.clone() else {
        return Redirect::to(&format!("/posts/{post_id}")).into_response();
    };
    render_template(&EventFormTemplate {
        title: "Edit the event".to_string(),
        layout: Layout::load(&state, &session).await,
        action: format!("/events/{post_id}/edit"),
        values: FormValues {
            title: post.title_text().to_string(),
            description: post.body.clone(),
            starts_at: input_time(&details.starts_at),
            ends_at: details.ends_at.as_ref().map(input_time).unwrap_or_default(),
            place_name: details.place_name.clone(),
            address: details.address.clone(),
            online_url: details.online_url.clone().unwrap_or_default(),
            category: details.category.clone(),
            capacity: details.capacity.map(|places| places.to_string()).unwrap_or_default(),
            approval: details.needs_approval(),
            online_public: details.online_public,
            banner_url: post.poster(),
            community_id: post.community.as_ref().map(|community| community.id),
        },
        editing: true,
        error: EventError::from_query(&query),
        categories: CATEGORIES,
        communities: Vec::new(),
        frequencies: event::Frequency::ALL,
        repeat_label: details.repeat_label(),
        title_max_chars: event::TITLE_MAX_CHARS,
        description_max_chars: BODY_MAX_CHARS,
    })
}

pub async fn update(State(state): State<AppState>, session: Session, Path(post_id): Path<i64>, mut multipart: Multipart) -> Response {
    let (pool, me, post) = match own_event(&state, &session, post_id).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let back = |error: EventError| Redirect::to(&format!("/events/{post_id}/edit?error={}", error.code())).into_response();
    let upload = match read_form(&mut multipart).await {
        Ok(upload) => upload,
        Err(error) => return back(error),
    };
    let form = EventForm::read(&upload);
    let banner = match &form {
        Ok(_) => save_banner(me.id, &upload).await,
        Err(_) => Ok(None),
    };
    upload.discard().await;
    let (form, banner) = match (form, banner) {
        (Ok(form), Ok(banner)) => (form, banner),
        (Err(error), _) | (_, Err(error)) => return back(error),
    };
    let status = post.event.as_ref().map_or("confirmed", |event| event.status.as_str()).to_string();
    let author_id = post.author.local().map_or(me.id, |author| author.id);
    match post::update_event(&pool, post_id, author_id, &form.title, &form.body, banner.as_deref(), &form.details(&status)).await {
        Ok(true) => {
            locate(&state, &pool, post_id, form.place_query());
            // A new banner replaces the old one, unless other dates of the
            // same event show it.
            let shared = post.event.as_ref().is_some_and(|event| event.series.is_some());
            if banner.is_some() && !shared {
                media::remove_local_files(&post.poster_url.into_iter().collect::<Vec<_>>()).await;
            }
            notifier::post_updated(&state, &pool, post_id).await;
            Redirect::to(&format!("/posts/{post_id}")).into_response()
        }
        Ok(false) => back(EventError::Server),
        Err(err) => {
            tracing::error!("failed to update event {}: {}", post_id, err);
            media::remove_local_files(&banner.into_iter().collect::<Vec<_>>()).await;
            back(EventError::Server)
        }
    }
}

/// Cancels the event, or confirms it again; its participants hear of it.
pub async fn toggle_cancelled(State(state): State<AppState>, session: Session, Path(post_id): Path<i64>) -> Response {
    let (pool, me, post) = match own_event(&state, &session, post_id).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let cancelling = post.event.as_ref().is_some_and(|event| !event.is_cancelled());
    if let Err(err) = event::set_status(&pool, post_id, if cancelling { "cancelled" } else { "confirmed" }).await {
        tracing::error!("failed to change event {}: {}", post_id, err);
        return error_page(Layout::load(&state, &session).await, StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong, try again later.");
    }
    if cancelling {
        let participants = event::local_participant_ids(&pool, post_id).await.unwrap_or_default();
        if let Err(err) = notification::event_cancelled(&pool, &participants, me.id, post_id).await {
            tracing::warn!("failed to tell participants of event {}: {}", post_id, err);
        }
    }
    notifier::post_updated(&state, &pool, post_id).await;
    tracing::info!("@{} {} event {}", me.username, if cancelling { "cancelled" } else { "confirmed again" }, post_id);
    Redirect::to(&format!("/posts/{post_id}")).into_response()
}

/// No more dates after those already made.
pub async fn stop_repeating(State(state): State<AppState>, session: Session, Path(post_id): Path<i64>) -> Response {
    let (pool, me, post) = match own_event(&state, &session, post_id).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    if let Some((series_id, _, _)) = post.event.as_ref().and_then(|event| event.series.clone()) {
        match event::stop_series(&pool, series_id).await {
            Ok(()) => tracing::info!("@{} stopped repeating event {}", me.username, post_id),
            Err(err) => tracing::error!("failed to stop the series of event {}: {}", post_id, err),
        }
    }
    Redirect::to(&format!("/posts/{post_id}")).into_response()
}

/// Makes the next dates of recurring events, and announces them.
async fn extend_series(state: &AppState, pool: &sqlx::PgPool) {
    match event::extend_series(pool).await {
        Ok(created) => {
            for post_id in created {
                tracing::info!("next date of a recurring event: event {}", post_id);
                notifier::post_created(state, pool, post_id).await;
            }
        }
        Err(err) => tracing::warn!("failed to extend recurring events: {}", err),
    }
}

/// Every minute: the next dates of recurring events, and the reminders of
/// events starting tomorrow or within the hour.
pub async fn run_scheduler(state: AppState) {
    let Some(pool) = state.pool.clone() else {
        return;
    };
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
    loop {
        tick.tick().await;
        if !crate::settings::services().events {
            continue;
        }
        extend_series(&state, &pool).await;
        match notification::event_reminders(&pool).await {
            Ok(0) => {}
            Ok(count) => tracing::info!("sent {} event reminders", count),
            Err(err) => tracing::warn!("failed to send event reminders: {}", err),
        }
    }
}

/// An event anyone signed in can see.
async fn visible_event(state: &AppState, session: &Session, post_id: i64) -> Result<(sqlx::PgPool, User, Post), Response> {
    let (pool, me) = signed_in_user(state, session).await?;
    match post::find(&pool, post_id, Some(me.id)).await {
        Ok(Some(post)) if post.is_event() => Ok((pool, me, post)),
        _ => Err(error_page(Layout::load(state, session).await, StatusCode::NOT_FOUND, "This event does not exist.")),
    }
}

/// Going, or asking to go when the organizer approves. For an event of
/// another server, a Join is sent to its organizer, who answers.
pub async fn join(State(state): State<AppState>, session: Session, Path(post_id): Path<i64>) -> Response {
    let (pool, me, post) = match visible_event(&state, &session, post_id).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let back = Redirect::to(&format!("/posts/{post_id}#event")).into_response();
    let Some(details) = post.event.as_ref() else {
        return back;
    };
    if details.is_cancelled() || details.is_past() || details.is_full() || details.is_going() {
        return back;
    }
    let result = match post.author.local() {
        Some(organizer) => {
            let pending = details.needs_approval() && organizer.id != me.id;
            match event::join_local(&pool, post_id, me.id, if pending { "pending" } else { "going" }, None).await {
                Ok(()) => notification::event_joined(&pool, organizer.id, post_id, Some(me.id), None, pending).await,
                Err(err) => Err(err),
            }
        }
        // Waiting for the event's server, like a request.
        None => match state.federation.as_deref() {
            Some(federation) => {
                let join_id = federation.join_url(me.public_id, post_id);
                let saved = event::join_local(&pool, post_id, me.id, "pending", Some(&join_id)).await;
                if saved.is_ok() {
                    notifier::event_participation(&state, &pool, &me, &post, &join_id, true).await;
                }
                saved
            }
            None => Ok(()),
        },
    };
    if let Err(err) = result {
        tracing::error!("failed to join event {}: {}", post_id, err);
    }
    back
}

pub async fn leave(State(state): State<AppState>, session: Session, Path(post_id): Path<i64>) -> Response {
    let (pool, me, post) = match visible_event(&state, &session, post_id).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    match event::leave_local(&pool, post_id, me.id).await {
        Ok(Some(join_id)) if post.author.is_remote() => {
            notifier::event_participation(&state, &pool, &me, &post, &join_id, false).await;
        }
        Ok(_) => {}
        Err(err) => tracing::error!("failed to leave event {}: {}", post_id, err),
    }
    Redirect::to(&format!("/posts/{post_id}#event")).into_response()
}

#[derive(Template)]
#[template(path = "event_participants.html")]
struct ParticipantsTemplate {
    title: String,
    layout: Layout,
    post: Post,
    participants: Vec<Participant>,
    organizer_id: i32,
}

/// The organizer's list: who goes, who waits for an answer.
pub async fn participants_page(State(state): State<AppState>, session: Session, Path(post_id): Path<i64>) -> Response {
    let (pool, me, post) = match own_event(&state, &session, post_id).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let participants = event::participants(&pool, post_id).await.unwrap_or_else(|err| {
        tracing::error!("failed to list participants of event {}: {}", post_id, err);
        Vec::new()
    });
    render_template(&ParticipantsTemplate {
        title: crate::i18n::tf("Participants of {}", post.title_text()),
        layout: Layout::load(&state, &session).await,
        post,
        participants,
        organizer_id: me.id,
    })
}

/// The organizer accepts or refuses a request, or removes a participant.
pub async fn answer(
    State(state): State<AppState>,
    session: Session,
    Path((post_id, participant_id, action)): Path<(i64, i64, String)>,
) -> Response {
    let (pool, me, post) = match own_event(&state, &session, post_id).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    let back = Redirect::to(&format!("/events/{post_id}/participants")).into_response();
    let accepted = match action.as_str() {
        "accept" => true,
        "reject" => false,
        _ => return back,
    };
    let participants = event::participants(&pool, post_id).await.unwrap_or_default();
    let Some(participant) = participants.into_iter().find(|participant| participant.id == participant_id) else {
        return back;
    };
    if accepted && post.event.as_ref().is_some_and(|event| event.is_full()) {
        return back;
    }
    if let Err(err) = event::set_role(&pool, post_id, participant_id, if accepted { "going" } else { "rejected" }).await {
        tracing::error!("failed to answer participant {}: {}", participant_id, err);
        return back;
    }
    if let Some(user_id) = participant.user_id
        && accepted
        && let Err(err) = notification::event_accepted(&pool, user_id, Some(me.id), None, post_id).await
    {
        tracing::warn!("failed to tell a participant of event {}: {}", post_id, err);
    }
    // Their server hears the answer, like Mobilizon's.
    if let (Some(remote_actor_id), Some(join_id)) = (participant.remote_actor_id, participant.join_id.as_deref()) {
        let actor: Option<String> = sqlx::query_scalar("SELECT actor_id FROM federation_actors WHERE id = $1")
            .bind(remote_actor_id)
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten();
        if let Some(actor) = actor {
            notifier::event_answer(&state, &pool, &me, &post, &actor, join_id, accepted).await;
        }
    }
    back
}

/// Folds an iCalendar line at 75 bytes, as the format asks.
fn ical_line(name: &str, value: &str) -> String {
    let line = format!("{name}:{value}");
    let mut folded = String::with_capacity(line.len() + 8);
    let mut width = 0;
    for letter in line.chars() {
        if width + letter.len_utf8() > 75 {
            folded.push_str("\r\n ");
            width = 1;
        }
        folded.push(letter);
        width += letter.len_utf8();
    }
    folded.push_str("\r\n");
    folded
}

/// Text in iCalendar: `\`, `;`, `,` and line breaks escaped.
fn ical_text(text: &str) -> String {
    text.replace('\\', "\\\\").replace(';', "\\;").replace(',', "\\,").replace("\r\n", "\\n").replace('\n', "\\n")
}

fn ical_time(time: &DateTime<Utc>) -> String {
    time.format("%Y%m%dT%H%M%SZ").to_string()
}

/// The event for a calendar app (`.ics`): Thunderbird, Apple, Google...
pub async fn calendar(State(state): State<AppState>, session: Session, Path(post_id): Path<i64>) -> Response {
    let Some(pool) = &state.pool else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let viewer = Layout::load(&state, &session).await.viewer;
    let Ok(Some(post)) = post::find(pool, post_id, viewer.as_ref().map(|viewer| viewer.id)).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(details) = post.event.as_ref().filter(|_| post.is_event()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let sees_link = details.online_public || details.is_going() || post.is_own;
    let page = format!("{}/posts/{post_id}", state.public_base_url);
    let end = details.ends_at.unwrap_or(details.starts_at);
    let mut location = details.where_label();
    if let Some(online) = details.online_url.as_ref().filter(|_| sees_link) {
        location = if location.is_empty() { online.clone() } else { format!("{location} · {online}") };
    }
    let mut ics = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Tison//Events//EN\r\nCALSCALE:GREGORIAN\r\nBEGIN:VEVENT\r\n");
    ics.push_str(&ical_line("UID", &format!("{post_id}@{}", crate::entities::author::host_of(&state.public_base_url))));
    ics.push_str(&ical_line("DTSTAMP", &ical_time(&Utc::now())));
    ics.push_str(&ical_line("DTSTART", &ical_time(&details.starts_at)));
    ics.push_str(&ical_line("DTEND", &ical_time(&end)));
    ics.push_str(&ical_line("SUMMARY", &ical_text(post.title_text())));
    ics.push_str(&ical_line("DESCRIPTION", &ical_text(&format!("{}\n\n{page}", post.body))));
    if !location.is_empty() {
        ics.push_str(&ical_line("LOCATION", &ical_text(&location)));
    }
    ics.push_str(&ical_line("URL", &page));
    ics.push_str(&ical_line("STATUS", &details.status.to_uppercase()));
    ics.push_str("END:VEVENT\r\nEND:VCALENDAR\r\n");
    (
        [
            (header::CONTENT_TYPE, "text/calendar; charset=utf-8".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"event-{post_id}.ics\"")),
        ],
        ics,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_are_read_in_the_writer_zone() {
        // 18:30 in Paris in winter (UTC+1) is 17:30 UTC.
        assert_eq!(parse_time("2026-11-07T18:30", "60").map(|time| time.to_rfc3339()), Some("2026-11-07T17:30:00+00:00".to_string()));
        assert_eq!(parse_time("2026-11-07T18:30:00", "").map(|time| time.to_rfc3339()), Some("2026-11-07T18:30:00+00:00".to_string()));
        assert!(parse_time("tomorrow", "0").is_none());
        assert_eq!(timezone("Europe/Paris"), "Europe/Paris");
        assert_eq!(timezone("<script>"), "UTC");
    }

    #[test]
    fn calendar_lines_are_folded_and_escaped() {
        assert_eq!(ical_text("a, b; c\nd"), "a\\, b\\; c\\nd");
        let folded = ical_line("SUMMARY", &"x".repeat(100));
        assert!(folded.split("\r\n").all(|line| line.len() <= 75));
        assert_eq!(folded.replace("\r\n ", ""), format!("SUMMARY:{}\r\n", "x".repeat(100)));
    }
}

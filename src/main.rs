use std::sync::Arc;

use axum::{Router, extract::DefaultBodyLimit, routing::post};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tower_sessions::{
    Expiry, MemoryStore, SessionManagerLayer,
    cookie::{SameSite, time::Duration},
};
// Named like its folder, in capitals.
#[allow(non_snake_case)]
mod API;
mod av;
mod entities;
mod geocode;
mod handlers;
mod linkify;
mod mail;
mod markdown;
mod pages;
mod photo;
mod preview;
mod settings;
use handlers::user;
use settings::Config;

/// Shared state given to every handler.
#[derive(Clone)]
pub struct AppState {
    /// `None` when no `DATABASE_URL` is configured.
    pub pool: Option<PgPool>,
    pub secure_cookies: bool,
    /// How long "stay signed in" lasts, see `handlers::session`.
    pub remember_days: i32,
    /// `None` when SMTP is not configured.
    pub mailer: Option<Arc<mail::Mailer>>,
    /// Used in links sent by email, see `Config::public_base_url`.
    pub public_base_url: String,
    /// `None` when federation is off (`FEDERATION=false`) or there is no database.
    pub federation: Option<Arc<API::Federation>>,
    /// Requests to other websites: the media proxy and link previews.
    pub web: Arc<API::Web>,
    /// Wakes the link previews when a post is written, see `preview`.
    pub previews: Arc<tokio::sync::Notify>,
    /// Open pages waiting for their unread count, see `handlers::notification`.
    pub live_notifications: Arc<handlers::notification::LiveUsers>,
    /// Where sessions live: long-lived connections reread them from here to
    /// notice a logout, see `handlers::notification::live`.
    pub session_store: MemoryStore,
    /// The address OBS sends live streams to, shown to streamers.
    pub rtmp_url: String,
    /// The largest video or episode uploaded, in bytes.
    pub max_video_bytes: usize,
    /// Finds where events take place, see `geocode`. `None`: never asked.
    pub geocoder: Option<String>,
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            tracing::warn!("failed to install CTRL+C handler: {}", err);
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(err) => {
                tracing::warn!("failed to install SIGTERM handler: {}", err);
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }

    tracing::info!("shutdown signal received");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = Config::from_runtime().map_err(io_error_from_string)?;
    settings::logger::init(&config.log_level);
    let services = config.services;
    if services.video || services.live_streaming || services.podcasts {
        if let Err(reason) = av::find(&config.ffmpeg, &config.ffprobe) {
            tracing::warn!("videos, live streams and podcasts are off: {} (install ffmpeg, or set FFMPEG and FFPROBE)", reason);
            config.services = services.without_media();
        }
    }
    config.services.install();
    av::set_video_settings(av::VideoSettings { preset: config.video_preset.clone(), max_lines: config.video_max_lines });
    av::jobs::set_parallel(config.video_conversions);
    tracing::info!(
        "services: photos {} in {} style, articles {}, videos {}, live streams {}, podcasts {}, events {}, communities {}, friends and messenger {}",
        if config.services.image_sharing { "on" } else { "off" },
        if config.services.image_system { "Pixelfed" } else { "Mastodon" },
        if config.services.publishing { "on" } else { "off" },
        if config.services.video { "on" } else { "off" },
        if config.services.live_streaming { "on" } else { "off" },
        if config.services.podcasts { "on" } else { "off" },
        if config.services.events { "on" } else { "off" },
        if config.services.content_aggregator { "on" } else { "off" },
        if config.services.dfrn { "on" } else { "off" }
    );

    // Lazy: the server starts even if Postgres is down, queries fail until it is up.
    let pool = match &config.database_url {
        Some(database_url) => Some(
            PgPoolOptions::new()
                .max_connections(config.db_max_connections)
                // The services, for the queries that hide what they turn off.
                .connect_lazy_with(
                    database_url
                        .parse::<sqlx::postgres::PgConnectOptions>()?
                        .options(config.services.sql_settings()),
                ),
        ),
        None => {
            tracing::warn!("DATABASE_URL not set; running without database");
            None
        }
    };

    if let Some(pool) = &pool
        && let Err(err) = entities::schema::sync(pool, entities::TABLES).await
    {
        tracing::error!("failed to update the database schema: {}", err);
    }

    if let Some(pool) = &pool {
        match entities::session::delete_expired(pool).await {
            Ok(0) => {}
            Ok(count) => tracing::info!("deleted {} expired remember tokens", count),
            Err(err) => tracing::warn!("failed to delete expired remember tokens: {}", err),
        }
        match entities::notification::delete_old_read(pool).await {
            Ok(0) => {}
            Ok(count) => tracing::info!("deleted {} old read notifications", count),
            Err(err) => tracing::warn!("failed to delete old notifications: {}", err),
        }
        match entities::password_reset::delete_expired(pool).await {
            Ok(0) => {}
            Ok(count) => tracing::info!("deleted {} expired password reset links", count),
            Err(err) => tracing::warn!("failed to delete expired password reset links: {}", err),
        }
        // Servers of accounts saved before they were stored.
        match entities::federation::fill_hosts(pool).await {
            Ok(0) => {}
            Ok(count) => tracing::info!("found the server of {} known accounts", count),
            Err(err) => tracing::warn!("failed to fill the servers of accounts: {}", err),
        }
        // Avatars saved before the media proxy existed.
        if let Err(err) = entities::media::register_known_avatars(pool).await {
            tracing::warn!("failed to register avatars for the media proxy: {}", err);
        }
        av::live::clean(pool).await;
    }

    // Old cached media of other servers, once a day.
    tokio::spawn(async {
        loop {
            handlers::media::clean_cache().await;
            tokio::time::sleep(std::time::Duration::from_secs(86400)).await;
        }
    });

    // A wrong SMTP setting stops the server: better than discovering it when
    // someone needs to reset their password. An unreachable server only warns,
    // it may come up later.
    let mailer = match &config.smtp {
        Some(smtp) => {
            let mailer = mail::Mailer::new(smtp).map_err(io_error_from_string)?;
            if let Err(err) = mailer.check().await {
                tracing::warn!("mail server not reachable for now ({}): {}", mailer.description(), err);
            }
            Some(mailer)
        }
        None if cfg!(debug_assertions) => Some(mail::Mailer::for_development().await),
        None => {
            tracing::warn!("SMTP_HOST not set; emails are disabled");
            None
        }
    };
    if let Some(mailer) = &mailer {
        tracing::info!("emails go to {}", mailer.description());
    }
    let mailer = mailer.map(Arc::new);

    // reqwest needs a TLS provider: ring, the one lettre's rustls already uses.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let federation = match (&pool, config.federation) {
        (Some(pool), true) => {
            let federation = Arc::new(
                API::Federation::new(&config.public_base_url, config.federation_allow_insecure)
                    .map_err(io_error_from_string)?,
            );
            if !config.public_base_url.starts_with("https://") {
                tracing::warn!(
                    "PUBLIC_BASE_URL is not https: other Fediverse servers like Mastodon won't talk to this one"
                );
            }
            if config.federation_allow_insecure {
                tracing::warn!("FEDERATION_ALLOW_INSECURE is on: only use it for local tests");
            }
            tokio::spawn(API::linker::run_deliveries(federation.clone(), pool.clone()));
            tracing::info!("Fediverse users are @username@{}", federation.domain);
            Some(federation)
        }
        _ => None,
    };

    let web = Arc::new(
        API::Web::new(&config.public_base_url, config.federation_allow_insecure).map_err(io_error_from_string)?,
    );
    let previews = Arc::new(tokio::sync::Notify::new());
    if let Some(pool) = &pool {
        tokio::spawn(preview::run(web.clone(), pool.clone(), previews.clone()));
    }

    let live_notifications = Arc::new(handlers::notification::LiveUsers::default());
    if let Some(pool) = &pool {
        tokio::spawn(handlers::notification::listen(pool.clone(), live_notifications.clone()));
    }

    let session_store = MemoryStore::default();
    let state = AppState {
        pool,
        secure_cookies: config.secure_cookies,
        remember_days: config.remember_me_days,
        mailer,
        public_base_url: config.public_base_url.clone(),
        federation,
        web,
        previews,
        live_notifications,
        session_store: session_store.clone(),
        rtmp_url: format!(
            "rtmp://{}:{}/live",
            // The site's domain, without its web port.
            entities::author::host_of(&config.public_base_url).split(':').next().unwrap_or_default(),
            config.rtmp_port
        ),
        max_video_bytes: config.max_video_mb * 1024 * 1024,
        geocoder: config.geocoder.clone(),
    };

    av::jobs::resume(&state).await;
    tokio::spawn(handlers::event::run_scheduler(state.clone()));
    if config.services.live_streaming && state.pool.is_some() {
        tokio::spawn(av::rtmp::serve(format!("{}:{}", config.rtmp_host, config.rtmp_port), state.clone()));
    }

    let session_layer =SessionManagerLayer::new(session_store)
        .with_secure(config.secure_cookies)
        .with_http_only(true)
        .with_same_site(SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(Duration::days(
            config.session_inactivity_days,
        )));

    let app = Router::new()
        .merge(pages::page_routes(config.max_video_mb * 1024 * 1024))
        .merge(API::federation::routes())
        .route("/settings/names", post(user::update_names))
        .route("/settings/bio", post(user::update_bio))
        .route("/settings/password", post(user::change_password))
        .route("/settings/username", post(user::update_username))
        .route("/settings/photo", post(user::upload_photo))
        .route("/settings/photo/remove", post(user::remove_photo))
        .route("/settings/background", post(user::upload_background))
        .route("/settings/background/remove", post(user::remove_background))
        .fallback(handlers::system::not_found);

    let app = app
        .nest_service("/assets", ServeDir::new("assets"))
        .nest_service("/public", ServeDir::new("public"))
        .layer(TraceLayer::new_for_http())
        .layer(DefaultBodyLimit::max(
            config.max_request_body_mb * 1024 * 1024,
        ))
        // Inside the session layer: it needs the session to sign users back in.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            handlers::session::restore_remembered,
        ))
        .layer(session_layer)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(config.addr()).await?;

    tracing::info!("Server running on http://{}", config.addr());

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

fn io_error_from_string(message: String) -> std::io::Error {
    std::io::Error::other(message)
}

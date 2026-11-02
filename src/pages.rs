use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::json;

use crate::{
    AppState,
    handlers::{
        article_picture, auth, comment, community, dashboard, event, media, messenger, moderation, notification, password, podcast, post, push, reaction, search,
        two_factor, user, verify_email, video,
    },
};

async fn health_live() -> Response {
    (StatusCode::OK, Json(json!({ "status": "ok" }))).into_response()
}

async fn health_ready(State(state): State<AppState>) -> Response {
    let Some(pool) = state.pool else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "no database configured" })),
        )
            .into_response();
    };

    match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&pool)
        .await
    {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "ready" }))).into_response(),
        Err(err) => {
            tracing::warn!("health readiness check failed: {}", err);
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "unavailable" })),
            )
                .into_response()
        }
    }
}

/// The pages, without those of the services turned off in
/// `configs/services.conf`: their addresses answer "not found".
/// `max_video_bytes` is the largest video or episode uploaded.
pub fn page_routes(max_video_bytes: usize) -> Router<AppState> {
    let services = crate::settings::services();
    let mut routes = base_routes();
    if services.content_aggregator {
        routes = routes.merge(community_routes());
    }
    if services.dfrn {
        routes = routes.merge(dfrn_routes());
    }
    if services.video || services.live_streaming {
        routes = routes.route("/videos", get(video::videos_page));
    }
    if services.video {
        routes = routes
            .route(
                "/videos/upload",
                get(video::upload_page).post(video::upload).layer(axum::extract::DefaultBodyLimit::max(max_video_bytes)),
            )
            .route("/posts/{post_id}/thumbnail", post(video::change_thumbnail));
    }
    if services.live_streaming {
        routes = routes
            .route("/live", get(video::studio_page).post(video::update_studio))
            .route("/live/key", post(video::reset_key))
            .route("/live/{username}", get(video::watch));
    }
    if services.podcasts {
        routes = routes.merge(podcast_routes(max_video_bytes));
    }
    if services.events {
        routes = routes.merge(event_routes());
    }
    if services.video || services.podcasts {
        // The conversion of an upload, live on its page.
        routes = routes.route("/posts/{post_id}/progress", get(video::progress));
    }
    routes
}

/// `EVENT-PLANNING`: events and meetups, like Mobilizon.
fn event_routes() -> Router<AppState> {
    Router::new()
        .route("/events", get(event::events_page).post(event::create))
        .route("/events/new", get(event::new_page))
        .route("/events/{post_id}/edit", get(event::edit_page).post(event::update))
        .route("/events/{post_id}/join", post(event::join))
        .route("/events/{post_id}/leave", post(event::leave))
        .route("/events/{post_id}/cancel", post(event::toggle_cancelled))
        .route("/events/{post_id}/stop-repeating", post(event::stop_repeating))
        .route("/events/{post_id}/participants", get(event::participants_page))
        .route("/events/{post_id}/participants/{participant_id}/{action}", post(event::answer))
        .route("/events/{post_id}/calendar.ics", get(event::calendar))
}

/// `AUDIOSTREAMING`: the podcasts.
fn podcast_routes(max_episode_bytes: usize) -> Router<AppState> {
    Router::new()
        .route("/podcasts", get(podcast::podcasts_page).post(podcast::create))
        .route("/podcasts/new", get(podcast::new_page))
        .route("/podcasts/{podcast_id}", get(podcast::podcast_page))
        .route(
            "/podcasts/{podcast_id}/episodes",
            post(podcast::upload_episode).layer(axum::extract::DefaultBodyLimit::max(max_episode_bytes)),
        )
        .route("/podcasts/{podcast_id}/delete", post(podcast::delete))
        .route("/podcasts/{podcast_id}/feed.xml", get(podcast::feed))
}

/// `CONTENT-AGGREGATOR`: the communities.
fn community_routes() -> Router<AppState> {
    Router::new()
        .route("/communities", get(community::communities_page).post(community::create_community))
        .route("/community/{slug}", get(community::community_page))
        .route("/community/{slug}/join", post(community::join))
        .route("/community/{slug}/quit", post(community::quit))
        .route("/community/{slug}/request/cancel", post(community::cancel_request))
        .route("/community/{slug}/requests/{user_id}/approve", post(community::approve_request))
        .route("/community/{slug}/requests/{user_id}/reject", post(community::reject_request))
        .route("/community/{slug}/members/{user_id}/remove", post(community::remove_member))
        .route("/community/{slug}/ignore", post(community::toggle_ignore))
        .route("/community/{slug}/posts", post(community::create_post))
        .route("/community/{slug}/posts/{post_id}/remove", post(community::remove_post))
        .route("/community/{slug}/ban", post(community::toggle_ban))
        .route("/community/{slug}/delete", post(community::delete_community))
        .route("/community/{slug}/settings", get(community::settings_page))
        .route("/community/{slug}/settings/details", post(community::update_details))
        .route("/community/{slug}/settings/photo", post(community::upload_photo))
        .route("/community/{slug}/settings/photo/remove", post(community::remove_photo))
        .route("/community/{slug}/rules", post(community::add_rule))
        .route("/community/{slug}/rules/{rule_id}", post(community::update_rule))
        .route("/community/{slug}/rules/{rule_id}/delete", post(community::delete_rule))
        .route("/community/{slug}/moderators", post(community::add_moderator))
        .route("/community/{slug}/moderators/{user_id}/remove", post(community::remove_moderator))
}

/// `DFRN`: friends and the messenger.
fn dfrn_routes() -> Router<AppState> {
    Router::new()
        .route("/messenger", get(messenger::messenger_page))
        .route("/messenger/new", post(messenger::start))
        .route("/messenger/with/{username}", get(messenger::with_user).post(messenger::with_user))
        .route("/messenger/remote/{actor_id}", post(messenger::with_remote))
        .route("/messenger/{id}", get(messenger::conversation_page).post(messenger::send))
        .route("/messenger/{id}/after/{message_id}", get(messenger::after))
        .route("/messenger/{id}/messages/{message_id}/delete", post(messenger::delete))
        .route("/user/{username}/friend", post(user::add_friend))
        .route("/user/{username}/unfriend", post(user::remove_friend))
}

fn base_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(post::home_page))
        .route("/your-feed", get(post::feed_page))
        // Up to `post::MAX_PHOTOS` photos.
        .route(
            "/posts",
            post(post::create_post).layer(axum::extract::DefaultBodyLimit::max(post::MAX_POST_BYTES)),
        )
        // One picture, with room for the form around it.
        .route(
            "/articles/pictures",
            post(article_picture::upload).layer(axum::extract::DefaultBodyLimit::max(crate::handlers::image::MAX_IMAGE_BYTES + 64 * 1024)),
        )
        .route("/articles/preview", post(article_picture::preview))
        .route("/dashboard", get(dashboard::dashboard))
        .route("/dashboard/posts/{post_id}", get(dashboard::post_dashboard))
        .route("/posts/{post_id}/view", post(dashboard::view))
        .route("/posts/{post_id}", get(comment::post_page))
        .route("/posts/{post_id}/comments", post(comment::create_comment))
        .route("/posts/{post_id}/like", post(reaction::like))
        .route("/posts/{post_id}/share", post(reaction::share))
        .route("/posts/{post_id}/edit", get(post::edit_page).post(post::update_post))
        .route("/posts/{post_id}/delete", post(post::delete_post))
        .route("/comments/{comment_id}/like", post(comment::like_comment))
        .route("/comments/{comment_id}/edit", get(comment::edit_page).post(comment::update_comment))
        .route("/comments/{comment_id}/delete", post(comment::delete_comment))
        .route("/report", get(moderation::report_page).post(moderation::send_report))
        .route("/moderation", get(moderation::dashboard))
        .route("/moderation/reports/{id}/{action}", post(moderation::act))
        .route("/moderation/staff", get(moderation::staff_page).post(moderation::add_moderator))
        .route("/moderation/statistics", get(moderation::statistics_page))
        .route("/moderation/configuration", get(moderation::configuration_page).post(moderation::save_configuration))
        .route("/moderation/configuration/pages/preview", post(moderation::site_page_preview))
        .route("/moderation/configuration/pages/{slug}", get(moderation::site_page_edit).post(moderation::save_site_page))
        .route("/moderation/federation", get(moderation::federation_page).post(moderation::set_instance_policy))
        .route("/moderation/staff/{user_id}/remove", post(moderation::remove_moderator))
        .route("/search", get(search::search_page))
        .route("/search/suggest", get(search::suggest))
        .route("/fediverse", get(search::old_fediverse_page))
        .route("/fediverse/follow", post(search::follow))
        .route("/fediverse/unfollow", post(search::unfollow))
        .route("/remote/{id}", get(search::remote_profile))
        .route("/media/{hash}", get(media::proxy))
        .route("/health/live", get(health_live))
        .route("/health/ready", get(health_ready))
        .route("/user/{username}", get(user::user_page))
        .route("/user/{username}/follow", post(user::follow_user))
        .route("/user/{username}/unfollow", post(user::unfollow_user))
        .route("/language", post(crate::handlers::system::set_language))
        .route("/terms-of-use", get(crate::handlers::system::site_page))
        .route("/privacy-policy", get(crate::handlers::system::site_page))
        .route("/cookies", get(crate::handlers::system::site_page))
        .route("/rules", get(crate::handlers::system::site_page))
        .route("/notifications", get(notification::notifications_page))
        .route("/notifications/live", get(notification::live))
        .route("/notifications/older", get(notification::older))
        .route("/notifications/read-all", post(notification::read_all))
        .route("/notifications/delete-all", post(notification::delete_all))
        .route("/notifications/{notification_id}/open", get(notification::open))
        .route("/notifications/{notification_id}/delete", post(notification::delete))
        .route("/push/key", get(push::key))
        .route("/push/subscribe", post(push::subscribe))
        .route("/push/unsubscribe", post(push::unsubscribe))
        .route("/push/status", post(push::status))
        .route("/push/test", post(push::test))
        .route("/settings", get(user::settings_page))
        .route("/login", get(auth::login_page).post(auth::login))
        .route("/login/two-factor", get(two_factor::login_page).post(two_factor::login_code))
        .route("/verify-email", get(verify_email::verify))
        .route("/settings/verify-email", post(verify_email::resend))
        .route("/settings/two-factor", get(two_factor::settings_page))
        .route("/settings/two-factor/start", post(two_factor::start))
        .route("/settings/two-factor/confirm", post(two_factor::confirm))
        .route("/settings/two-factor/recovery", post(two_factor::new_recovery))
        .route("/settings/two-factor/disable", post(two_factor::disable))
        .route("/register", get(auth::register_page).post(auth::register))
        .route("/logout", post(auth::logout))
        .route("/forgot-password", get(password::forgot_page).post(password::forgot))
        .route("/reset-password", get(password::reset_page).post(password::reset))
}

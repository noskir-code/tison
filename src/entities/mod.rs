pub mod author;
pub mod comment;
pub mod community;
pub mod event;
pub mod federation;
pub mod follow;
pub mod friend;
pub mod media;
pub mod message;
pub mod notification;
pub mod password_reset;
pub mod podcast;
pub mod post;
pub mod preview;
pub mod report;
pub mod schema;
pub mod session;
pub mod user;
pub mod video;

/// SQL that is true when a service of `configs/services.conf` is on, given
/// to the database connections by `Services::sql_settings`: `aggregator`
/// (the communities) or `dfrn` (friends and the messenger). On when unset.
macro_rules! service_on {
    (aggregator) => {
        "(COALESCE(current_setting('tison.content_aggregator', TRUE), 'on') <> 'off')"
    };
    (dfrn) => {
        "(COALESCE(current_setting('tison.dfrn', TRUE), 'on') <> 'off')"
    };
}
pub(crate) use service_on;

/// Tables kept up to date at startup. A table must come after the tables it references.
pub const TABLES: &[&schema::Table] = &[
    &user::TABLE,
    &federation::ACTORS_TABLE,
    &community::TABLE,
    &podcast::TABLE,
    &post::TABLE,
    &video::LIVE_TABLE,
    &video::FILES_TABLE,
    &event::SERIES_TABLE,
    &event::TABLE,
    &event::PARTICIPANTS_TABLE,
    &event::REMINDERS_TABLE,
    &post::LIKES_TABLE,
    &post::SHARES_TABLE,
    &comment::TABLE,
    &comment::LIKES_TABLE,
    &community::MEMBERS_TABLE,
    &community::MODERATORS_TABLE,
    &community::RULES_TABLE,
    &community::IGNORES_TABLE,
    &community::REQUESTS_TABLE,
    &media::MEDIA_TABLE,
    &media::ATTACHMENTS_TABLE,
    &follow::TABLE,
    &friend::TABLE,
    &session::TABLE,
    &password_reset::TABLE,
    &federation::FOLLOWERS_TABLE,
    &federation::COMMUNITY_FOLLOWERS_TABLE,
    &federation::FOLLOWING_TABLE,
    &federation::REACTIONS_TABLE,
    &federation::DELIVERIES_TABLE,
    &federation::INSTANCES_TABLE,
    &notification::TABLE,
    &message::CONVERSATIONS_TABLE,
    &message::MESSAGES_TABLE,
    &preview::TABLE,
    &report::TABLE,
];

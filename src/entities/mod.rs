pub mod article_picture;
pub mod author;
pub mod block;
pub mod comment;
pub mod creator_stats;
pub mod data_export;
pub mod email_domain;
pub mod community;
pub mod event;
pub mod federation;
pub mod hashtag_follow;
pub mod follow;
pub mod friend;
pub mod incident;
pub mod media;
pub mod mention;
pub mod message;
pub mod notice;
pub mod notification;
pub mod password_reset;
pub mod podcast;
pub mod post;
pub mod preview;
pub mod push;
pub mod relay;
pub mod report;
pub mod schema;
pub mod session;
pub mod spam;
pub mod site_page;
pub mod statistics;
pub mod two_factor;
pub mod user;
pub mod video;
pub mod views;

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
    &federation::INSTANCE_KEYS_TABLE,
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
    &mention::TABLE,
    &community::MEMBERS_TABLE,
    &community::MODERATORS_TABLE,
    &community::RULES_TABLE,
    &community::IGNORES_TABLE,
    &community::REQUESTS_TABLE,
    &media::MEDIA_TABLE,
    &follow::TABLE,
    &friend::TABLE,
    &block::TABLE,
    &hashtag_follow::TABLE,
    &session::TABLE,
    &two_factor::TABLE,
    &two_factor::RECOVERY_TABLE,
    &password_reset::TABLE,
    &federation::FOLLOWERS_TABLE,
    &federation::COMMUNITY_FOLLOWERS_TABLE,
    &federation::FOLLOWING_TABLE,
    &federation::REACTIONS_TABLE,
    &federation::DELIVERIES_TABLE,
    &federation::INSTANCES_TABLE,
    &relay::RELAYS_TABLE,
    &relay::SUBSCRIBERS_TABLE,
    &notification::TABLE,
    &views::DAILY_TABLE,
    &views::MARKS_TABLE,
    &article_picture::TABLE,
    &push::TABLE,
    &email_domain::TABLE,
    &data_export::TABLE,
    &incident::TABLE,
    &push::KEYS_TABLE,
    &message::CONVERSATIONS_TABLE,
    &message::MESSAGES_TABLE,
    // After the tables its rows belong to.
    &media::ATTACHMENTS_TABLE,
    &preview::TABLE,
    &report::TABLE,
    &spam::TABLE,
    &notice::TABLE,
];

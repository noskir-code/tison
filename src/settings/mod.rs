mod config;
pub mod logger;
pub mod shown;

pub use config::{Config, SmtpConfig, SmtpSecurity, install_bot_limit, install_removed_content_days, install_relay_server, relay_server, removed_content_days, install_instance_name, install_privacy, bot_posts_per_hour, install_site_domain, minimum_age, privacy_email, privacy_officer, instance_name, services, site_base_url, site_domain};

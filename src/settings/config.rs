use std::{collections::HashMap, env, fs, path::PathBuf};

pub(super) const DEFAULT_RUNTIME_FILE: &str = "configs/runtime.conf";
pub(super) const DEFAULT_SERVICES_FILE: &str = "configs/services.conf";
pub(super) const DEFAULT_SETTINGS_FILE: &str = "configs/settings.conf";

/// Path of a configuration file, overridable with the variable `name`.
pub(super) fn config_path(name: &str, default: &str) -> PathBuf {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default))
}

/// Parses `KEY=VALUE` lines. Empty lines and `#` comments are ignored,
/// surrounding quotes are stripped from values. Dashes in keys are read as
/// underscores: `CONTENT-AGGREGATOR` is `CONTENT_AGGREGATOR`.
pub(super) fn parse_runtime_file(content: &str) -> Result<HashMap<String, String>, String> {
    let mut values = HashMap::new();
    for (index, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((raw_key, raw_value)) = line.split_once('=') else {
            return Err(format!(
                "Invalid runtime config line {}: expected KEY=VALUE",
                index + 1
            ));
        };

        let key = raw_key.trim();
        if key.is_empty()
            || !key
                .bytes()
                .all(|byte| byte == b'_' || byte == b'-' || byte.is_ascii_alphanumeric())
        {
            return Err(format!(
                "Invalid runtime config key '{}' on line {}",
                key,
                index + 1
            ));
        }

        let mut value = raw_value.trim();
        if value.len() >= 2
            && ((value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\'')))
        {
            value = &value[1..value.len() - 1];
        }

        if !value.trim().is_empty() {
            values.insert(key.replace('-', "_"), value.trim().to_string());
        }
    }

    Ok(values)
}

/// Environment variables take priority over values from the runtime file.
struct Settings {
    file_values: HashMap<String, String>,
}

impl Settings {
    /// `file` is the path of the file, `variable` the environment variable
    /// that can replace it.
    fn load(variable: &str, file: &str) -> Result<Self, String> {
        let path = config_path(variable, file);
        let file_values = match fs::read_to_string(&path) {
            Ok(content) => parse_runtime_file(&content)
                .map_err(|err| format!("{} ({})", err, path.display()))?,
            Err(_) => HashMap::new(),
        };
        Ok(Self { file_values })
    }

    fn get(&self, name: &str) -> Option<String> {
        env::var(name)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| self.file_values.get(name).cloned())
    }

    fn parse<T: std::str::FromStr>(&self, name: &str, default: T) -> Result<T, String> {
        match self.get(name) {
            Some(raw) => raw
                .parse()
                .map_err(|_| format!("{name} has an invalid value: '{raw}'")),
            None => Ok(default),
        }
    }

    fn flag(&self, name: &str, default: bool) -> bool {
        self.get(name)
            .map(|raw| {
                matches!(
                    raw.to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(default)
    }
}

/// How the connection to the mail server is protected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmtpSecurity {
    /// Plain connection upgraded to TLS, usually port 587.
    StartTls,
    /// TLS from the start, usually port 465.
    Tls,
    /// No encryption, only for a relay on the same machine.
    None,
}

#[derive(Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub security: SmtpSecurity,
    pub username: Option<String>,
    pub password: Option<String>,
    /// Sender, like `Tison <noreply@example.com>`.
    pub from: String,
    /// Development mail catcher, see `SmtpConfig::mailpit`.
    pub is_mailpit: bool,
}

// Written by hand so the password never ends up in logs.
impl std::fmt::Debug for SmtpConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmtpConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("security", &self.security)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "***"))
            .field("from", &self.from)
            .field("is_mailpit", &self.is_mailpit)
            .finish()
    }
}

impl SmtpConfig {
    /// Mailpit (https://mailpit.axllent.org) with its default ports: it catches
    /// every email and shows them in a web inbox, nothing reaches real people.
    pub fn mailpit() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 1025,
            security: SmtpSecurity::None,
            username: None,
            password: None,
            from: "Tison <noreply@tison.localhost>".to_string(),
            is_mailpit: true,
        }
    }

    /// `None` when neither `SMTP_HOST` nor `SMTP_PROVIDER` is set: in debug builds
    /// emails then go to Mailpit if it runs, else in the log (see `mail.rs`);
    /// release builds can't send emails.
    fn from_settings(settings: &Settings) -> Result<Option<Self>, String> {
        let preset = match settings.get("SMTP_PROVIDER").map(|name| name.to_ascii_lowercase()) {
            None => None,
            Some(name) if name == "mailpit" => Some(Self::mailpit()),
            Some(other) => {
                return Err(format!("SMTP_PROVIDER has an invalid value: '{other}', known: mailpit"));
            }
        };
        let Some(host) = settings
            .get("SMTP_HOST")
            .or_else(|| preset.as_ref().map(|preset| preset.host.clone()))
        else {
            return Ok(None);
        };
        // Values set in the configuration win over the preset.
        let default_security = match &preset {
            Some(preset) => preset.security,
            None => SmtpSecurity::StartTls,
        };
        let security = match settings.get("SMTP_SECURITY").map(|value| value.to_ascii_lowercase()) {
            None => default_security,
            Some(value) => match value.as_str() {
                "starttls" => SmtpSecurity::StartTls,
                "tls" | "ssl" => SmtpSecurity::Tls,
                "none" => SmtpSecurity::None,
                other => return Err(format!("SMTP_SECURITY has an invalid value: '{other}'")),
            },
        };
        let default_port = match (&preset, security) {
            (Some(preset), _) => preset.port,
            (None, SmtpSecurity::StartTls) => 587,
            (None, SmtpSecurity::Tls) => 465,
            (None, SmtpSecurity::None) => 25,
        };
        let from = match (settings.get("SMTP_FROM"), &preset) {
            (Some(from), _) => from,
            (None, Some(preset)) => preset.from.clone(),
            (None, None) => return Err("SMTP_FROM is required when SMTP_HOST is set".to_string()),
        };
        Ok(Some(Self {
            host,
            port: settings.parse("SMTP_PORT", default_port)?,
            security,
            username: settings.get("SMTP_USERNAME"),
            password: settings.get("SMTP_PASSWORD"),
            from,
            is_mailpit: preset.is_some(),
        }))
    }
}

/// The services of `configs/services.conf` the site offers. Those not
/// listed here aren't written yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Services {
    /// `BLOGGING`: short text posts like on Mastodon, with mentions. Off,
    /// a post needs a photo (with `IMAGESHARING`), or there are none.
    pub blogging: bool,
    /// `BLOGGING-MAX-CHARS`: the longest post, counted like Mastodon, see
    /// `mention::mastodon_length`.
    pub blogging_max_chars: usize,
    /// `IMAGESHARING`: users can add photos to their posts.
    pub image_sharing: bool,
    /// `IMAGESYSTEM`: posts of photos shown like on Pixelfed (on) or in a
    /// gallery under the text like on Mastodon (off).
    pub image_system: bool,
    /// `PUBLISHING`: articles with a title, in Markdown, like WriteFreely.
    pub publishing: bool,
    /// `VIDEO`: videos, like PeerTube. Needs ffmpeg.
    pub video: bool,
    /// `LIVE-STREAMING`: going live from OBS, like Owncast and PeerTube.
    /// Needs ffmpeg.
    pub live_streaming: bool,
    /// `AUDIOSTREAMING`: podcasts, like Funkwhale and Castopod. Needs ffmpeg.
    pub podcasts: bool,
    /// `EVENT-PLANNING`: events and meetups, like Mobilizon.
    pub events: bool,
    /// `CONTENT-AGGREGATOR`: the communities, like Lemmy.
    pub content_aggregator: bool,
    /// `DFRN`: friends and the messenger, like Friendica.
    pub dfrn: bool,
}

impl Default for Services {
    fn default() -> Self {
        Self {
            blogging: true,
            blogging_max_chars: 500,
            image_sharing: true,
            image_system: true,
            publishing: true,
            video: true,
            live_streaming: true,
            podcasts: true,
            events: true,
            content_aggregator: true,
            dfrn: true,
        }
    }
}

static SERVICES: std::sync::OnceLock<Services> = std::sync::OnceLock::new();

static SITE_DOMAIN: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// The site's domain, like `tison.social` (from `PUBLIC_BASE_URL`), shown in
/// the addresses of its users: `@noskir@tison.social`.
pub fn site_domain() -> &'static str {
    SITE_DOMAIN.get().map(String::as_str).unwrap_or("localhost")
}

static SITE_BASE_URL: std::sync::OnceLock<String> = std::sync::OnceLock::new();

static INSTANCE_NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// See `Config::privacy_officer`, `privacy_email` and `minimum_age`.
struct PrivacySettings {
    officer: String,
    email: String,
    minimum_age: u32,
}

static PRIVACY: std::sync::OnceLock<PrivacySettings> = std::sync::OnceLock::new();

/// Set once at start, from the `Config`.
pub fn install_privacy(officer: &str, email: &str, minimum_age: u32) {
    let _ = PRIVACY.set(PrivacySettings { officer: officer.trim().to_string(), email: email.trim().to_string(), minimum_age });
}

/// The person in charge of personal data (a title, like "Administrator"),
/// empty when not set.
pub fn privacy_officer() -> &'static str {
    PRIVACY.get().map_or("", |privacy| privacy.officer.as_str())
}

pub fn privacy_email() -> &'static str {
    PRIVACY.get().map_or("", |privacy| privacy.email.as_str())
}

pub fn minimum_age() -> u32 {
    PRIVACY.get().map_or(16, |privacy| privacy.minimum_age)
}

static BOT_POSTS_PER_HOUR: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

/// Set once at start, from `Config::bot_posts_per_hour`.
pub fn install_bot_limit(posts_per_hour: u32) {
    BOT_POSTS_PER_HOUR.store(posts_per_hour, std::sync::atomic::Ordering::Relaxed);
}

/// The most posts a bot account can publish in an hour (Server rules, 7).
pub fn bot_posts_per_hour() -> u32 {
    BOT_POSTS_PER_HOUR.load(std::sync::atomic::Ordering::Relaxed)
}

static RELAY_SERVER: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Set once at start, from `Config::relay_server`.
pub fn install_relay_server(on: bool) {
    RELAY_SERVER.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Whether the site acts as a relay for other servers, see `API::relay`.
pub fn relay_server() -> bool {
    RELAY_SERVER.load(std::sync::atomic::Ordering::Relaxed)
}

static REMOVED_CONTENT_DAYS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(30);

/// Set once at start, from `Config::removed_content_days`.
pub fn install_removed_content_days(days: u32) {
    REMOVED_CONTENT_DAYS.store(days, std::sync::atomic::Ordering::Relaxed);
}

/// How many days posts and comments removed by the moderators are kept,
/// hidden, before being erased: the time to appeal and bring them back.
pub fn removed_content_days() -> u32 {
    REMOVED_CONTENT_DAYS.load(std::sync::atomic::Ordering::Relaxed)
}

/// The site's name (`INSTANCE_NAME` in runtime.conf), shown wherever the
/// site names itself. "Tison" stays the name of the software.
pub fn instance_name() -> &'static str {
    INSTANCE_NAME.get().map(String::as_str).unwrap_or("Tison")
}

/// Set once at start, from `Config::instance_name`.
pub fn install_instance_name(name: &str) {
    let _ = INSTANCE_NAME.set(name.trim().to_string());
}

/// `https://tison.social`: the site's address (`PUBLIC_BASE_URL`), for what
/// is read elsewhere, like the previews of its pages.
pub fn site_base_url() -> &'static str {
    SITE_BASE_URL.get().map(String::as_str).unwrap_or("")
}

/// Set once at start, from `Config::public_base_url`.
pub fn install_site_domain(public_base_url: &str) {
    let _ = SITE_DOMAIN.set(crate::entities::author::host_of(public_base_url).to_ascii_lowercase());
    let _ = SITE_BASE_URL.set(public_base_url.trim_end_matches('/').to_string());
}

/// The services read at start, all on before (in tests).
pub fn services() -> Services {
    SERVICES.get().copied().unwrap_or_default()
}

impl Services {
    /// Reads `configs/services.conf`, or the file in `TISON_SERVICES`. A missing
    /// file or line keeps the service on; environment variables (with `_`,
    /// like `CONTENT_AGGREGATOR=false`) win over the file.
    pub fn from_file() -> Result<Self, String> {
        let settings = Settings::load("TISON_SERVICES", DEFAULT_SERVICES_FILE)?;
        Ok(Self {
            blogging: settings.flag("BLOGGING", true),
            blogging_max_chars: match settings.parse("BLOGGING_MAX_CHARS", 500)? {
                chars @ 100..=10_000 => chars,
                chars => return Err(format!("BLOGGING-MAX-CHARS must be between 100 and 10000, not {chars}")),
            },
            image_sharing: settings.flag("IMAGESHARING", true),
            image_system: settings.flag("IMAGESYSTEM", true),
            publishing: settings.flag("PUBLISHING", true),
            video: settings.flag("VIDEO", true),
            live_streaming: settings.flag("LIVE_STREAMING", true),
            podcasts: settings.flag("AUDIOSTREAMING", true),
            events: settings.flag("EVENT_PLANNING", true),
            content_aggregator: settings.flag("CONTENT_AGGREGATOR", true),
            dfrn: settings.flag("DFRN", true),
        })
    }

    /// Without ffmpeg, videos, live streams and podcasts are turned off.
    pub fn without_media(self) -> Self {
        Self { video: false, live_streaming: false, podcasts: false, ..self }
    }

    /// Makes them the ones `services()` returns, once.
    pub fn install(self) {
        let _ = SERVICES.set(self);
    }

    /// Given to every database connection so SQL can read them, see
    /// `entities::service_on!`.
    pub fn sql_settings(self) -> [(&'static str, &'static str); 2] {
        let on = |enabled: bool| if enabled { "on" } else { "off" };
        [("tison.content_aggregator", on(self.content_aggregator)), ("tison.dfrn", on(self.dfrn))]
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub database_url: Option<String>,
    pub db_max_connections: u32,
    pub max_request_body_mb: usize,
    pub log_level: String,
    pub secure_cookies: bool,
    pub session_inactivity_days: i64,
    /// How long "stay signed in" lasts without visiting.
    pub remember_me_days: i32,
    /// Address of the site as users reach it, used in links sent by email.
    /// Never taken from the request, which an attacker controls.
    pub public_base_url: String,
    /// The site's name (`INSTANCE_NAME`), in the pages of `legal/`.
    pub instance_name: String,
    pub smtp: Option<SmtpConfig>,
    /// Fediverse (ActivityPub): other servers can find and follow local
    /// users. `FEDERATED` in settings.conf, or `FEDERATION` when set here.
    pub federation: bool,
    /// An account for the moderation team (`MULTI-MODERATOR` in
    /// settings.conf), see `user::sync_team_account`.
    pub multi_moderator: bool,
    /// Who answers questions about personal data, and how to reach them
    /// (`PRIVACY-OFFICER`, `PRIVACY-EMAIL` in settings.conf): Québec's Law 25
    /// asks to publish them. Shown under the menu and in the legal pages.
    pub privacy_officer: String,
    pub privacy_email: String,
    /// The youngest age to sign up (`MINIMUM-AGE` in settings.conf): 16 by
    /// default, the GDPR's age of consent without parents in most of the EU
    /// (Québec's is 14, the US's 13).
    pub minimum_age: u32,
    /// The most posts a bot can publish in an hour (`BOT-POSTS-PER-HOUR` in
    /// settings.conf, 1 by default), see `install_bot_limit`.
    pub bot_posts_per_hour: u32,
    /// How long posts and comments removed by the moderators are kept,
    /// hidden, so an accepted appeal can bring them back
    /// (`REMOVED-CONTENT-DAYS` in settings.conf, 30 by default).
    pub removed_content_days: u32,
    /// The site acts as a relay for other servers (`RELAY-SERVER` in
    /// settings.conf), each accepted by an admin, see `API::relay`.
    pub relay_server: bool,
    /// Lets federation reach `http://` and private addresses. Only to test
    /// two servers on one machine: on a real server it lets anyone make
    /// Tison send requests inside its network.
    pub federation_allow_insecure: bool,
    /// The reverse proxies believed about the visitor's address
    /// (`TRUSTED_PROXIES`), see `security::client_ip`. `loopback, private`
    /// by default.
    pub trusted_proxies: Vec<crate::security::Network>,
    /// The key encrypting the two-factor secrets (`TWO_FACTOR_KEY_FILE`,
    /// `keys/two_factor.key` by default), see `vault`.
    pub two_factor_key_file: String,
    /// From `configs/services.conf`.
    pub services: Services,
    /// The programs converting videos and sounds (`FFMPEG`, `FFPROBE`).
    pub ffmpeg: String,
    pub ffprobe: String,
    /// Where OBS sends live streams (`RTMP_HOST`, `RTMP_PORT`).
    pub rtmp_host: String,
    pub rtmp_port: u16,
    /// The largest video or episode uploaded (`MAX_VIDEO_MB`).
    pub max_video_mb: usize,
    /// x264's preset for uploaded videos (`VIDEO_PRESET`), `slow` by default.
    pub video_preset: String,
    /// The best quality made of videos, by its short side
    /// (`VIDEO_MAX_LINES`): 2160 (4K) by default.
    pub video_max_lines: u32,
    /// Conversions running at once (`VIDEO_CONVERSIONS`), each user's one
    /// at a time.
    pub video_conversions: usize,
    /// Where events' addresses are looked up for their map (`GEOCODER`):
    /// OpenStreetMap's Nominatim by default, `None` with `off`.
    pub geocoder: Option<String>,
}

impl Config {
    pub fn from_runtime() -> Result<Self, String> {
        let settings = Settings::load("TISON_CONFIG", DEFAULT_RUNTIME_FILE)?;
        let site = Settings::load("TISON_SETTINGS", DEFAULT_SETTINGS_FILE)?;

        let host = settings
            .get("APP_HOST")
            .unwrap_or_else(|| "127.0.0.1".to_string());
        let port = settings.parse("APP_PORT", 8080)?;
        let public_base_url = settings
            .get("PUBLIC_BASE_URL")
            .unwrap_or_else(|| format!("http://{host}:{port}"))
            .trim_end_matches('/')
            .to_string();

        Ok(Self {
            public_base_url,
            instance_name: settings.get("INSTANCE_NAME").filter(|name| !name.trim().is_empty()).unwrap_or_else(|| "Tison".to_string()),
            smtp: SmtpConfig::from_settings(&settings)?,
            host,
            port,
            database_url: settings.get("DATABASE_URL"),
            db_max_connections: settings.parse("DB_MAX_CONNECTIONS", 20)?,
            max_request_body_mb: settings.parse("MAX_REQUEST_BODY_MB", 20)?,
            log_level: settings
                .get("LOG_LEVEL")
                .unwrap_or_else(|| "info".to_string()),
            secure_cookies: settings.flag("COOKIE_SECURE", !cfg!(debug_assertions)),
            session_inactivity_days: settings.parse("SESSION_INACTIVITY_DAYS", 7)?,
            remember_me_days: settings.parse("REMEMBER_ME_DAYS", 30)?,
            federation: settings.flag("FEDERATION", site.flag("FEDERATED", true)),
            multi_moderator: site.flag("MULTI_MODERATOR", false),
            privacy_officer: site.get("PRIVACY_OFFICER").unwrap_or_default(),
            privacy_email: site.get("PRIVACY_EMAIL").unwrap_or_default(),
            minimum_age: match site.parse("MINIMUM_AGE", 16)? {
                age @ 13..=21 => age,
                age => return Err(format!("MINIMUM-AGE must be between 13 and 21, not {age}")),
            },
            bot_posts_per_hour: match site.parse("BOT_POSTS_PER_HOUR", 1)? {
                posts @ 1..=60 => posts,
                posts => return Err(format!("BOT-POSTS-PER-HOUR must be between 1 and 60, not {posts}")),
            },
            relay_server: site.flag("RELAY_SERVER", false),
            removed_content_days: match site.parse("REMOVED_CONTENT_DAYS", 30)? {
                days @ 1..=365 => days,
                days => return Err(format!("REMOVED-CONTENT-DAYS must be between 1 and 365, not {days}")),
            },
            federation_allow_insecure: settings.flag("FEDERATION_ALLOW_INSECURE", false),
            trusted_proxies: crate::security::parse_proxies(&settings.get("TRUSTED_PROXIES").unwrap_or_else(|| "loopback, private".to_string()))?,
            two_factor_key_file: settings.get("TWO_FACTOR_KEY_FILE").unwrap_or_else(|| "keys/two_factor.key".to_string()),
            services: Services::from_file()?,
            ffmpeg: settings.get("FFMPEG").unwrap_or_else(|| "ffmpeg".to_string()),
            ffprobe: settings.get("FFPROBE").unwrap_or_else(|| "ffprobe".to_string()),
            rtmp_host: settings.get("RTMP_HOST").unwrap_or_else(|| "0.0.0.0".to_string()),
            rtmp_port: settings.parse("RTMP_PORT", 1935)?,
            max_video_mb: settings.parse("MAX_VIDEO_MB", 4096)?,
            video_preset: {
                const PRESETS: [&str; 9] = ["ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow"];
                let preset = settings.get("VIDEO_PRESET").unwrap_or_else(|| "slow".to_string());
                if !PRESETS.contains(&preset.as_str()) {
                    return Err(format!("VIDEO_PRESET must be one of {}", PRESETS.join(", ")));
                }
                preset
            },
            geocoder: match settings.get("GEOCODER") {
                Some(value) if value.eq_ignore_ascii_case("off") => None,
                Some(url) if url.starts_with("https://") || url.starts_with("http://") => Some(url),
                Some(other) => return Err(format!("GEOCODER must be an address or off, not '{other}'")),
                None => Some(crate::geocode::DEFAULT_GEOCODER.to_string()),
            },
            video_conversions: match settings.parse("VIDEO_CONVERSIONS", 2)? {
                count @ 1..=32 => count,
                count => return Err(format!("VIDEO_CONVERSIONS must be between 1 and 32, not {count}")),
            },
            video_max_lines: match settings.parse("VIDEO_MAX_LINES", 2160)? {
                lines @ 144..=2160 => lines,
                lines => return Err(format!("VIDEO_MAX_LINES must be between 144 and 2160, not {lines}")),
            },
        })
    }

    pub fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashes_in_keys_are_underscores() {
        let values = parse_runtime_file("# Lemmy\nCONTENT-AGGREGATOR=false\nDFRN = 'true'\n").unwrap();
        assert_eq!(values.get("CONTENT_AGGREGATOR").map(String::as_str), Some("false"));
        assert_eq!(values.get("DFRN").map(String::as_str), Some("true"));
        assert!(parse_runtime_file("BAD KEY=1").is_err());
    }
}

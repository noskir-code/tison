use std::{collections::HashMap, env, fs, path::PathBuf};

const DEFAULT_RUNTIME_FILE: &str = "configs/runtime.conf";
const DEFAULT_SERVICES_FILE: &str = "configs/services.conf";

/// Path of a configuration file, overridable with the variable `name`.
fn config_path(name: &str, default: &str) -> PathBuf {
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
fn parse_runtime_file(content: &str) -> Result<HashMap<String, String>, String> {
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
    pub smtp: Option<SmtpConfig>,
    /// Fediverse (ActivityPub): other servers can find and follow local users.
    pub federation: bool,
    /// Lets federation reach `http://` and private addresses. Only to test
    /// two servers on one machine: on a real server it lets anyone make
    /// Tison send requests inside its network.
    pub federation_allow_insecure: bool,
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
            federation: settings.flag("FEDERATION", true),
            federation_allow_insecure: settings.flag("FEDERATION_ALLOW_INSECURE", false),
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

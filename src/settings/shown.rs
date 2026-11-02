//! The configuration files as the site's creator sees and edits them on the
//! Configuration tab (see `handlers::moderation`): each setting with the
//! comment above it, where its value comes from, and what is in effect.
//! Passwords, keys and the password of addresses are never shown.
//!
//! Saving rewrites only the lines that changed, keeping the comments and the
//! order, and keeps the previous file as `.bak`. Changes apply when Tison
//! restarts, like editing the file by hand.

use std::{collections::HashMap, env, fs};

use rsa::sha2::{Digest, Sha256};

use super::config::{DEFAULT_RUNTIME_FILE, DEFAULT_SERVICES_FILE, config_path, parse_runtime_file, services};

const DEFAULT_SETTINGS_FILE: &str = "configs/settings.conf";

/// Shown instead of a password in an address, and given back by the form.
const HIDDEN: &str = "••••••";

/// A configuration file and its settings.
pub struct ShownFile {
    /// In the form: `runtime`, `services` or `settings`.
    pub id: &'static str,
    pub name: &'static str,
    /// Where it was read, like `configs/runtime.conf`.
    pub path: String,
    /// What it is for.
    pub about: &'static str,
    /// `false`: nothing reads it in this version.
    pub read: bool,
    pub missing: bool,
    /// Of the content shown: a save is refused if the file changed since.
    pub version: String,
    pub lines: Vec<ShownLine>,
}

/// A `### Title ###` of the file, or one of its settings.
pub struct ShownLine {
    pub heading: Option<String>,
    /// Its line in the file, which names its field in the form (`line-12`).
    pub number: usize,
    pub key: String,
    /// What the field shows: the file's value, empty when commented out
    /// (the default applies) or secret, the password of an address hidden.
    pub field_value: String,
    /// The value written in the comment of a setting not set, as a hint.
    pub example: String,
    /// `true` or `false`: a drop-down.
    pub boolean: bool,
    /// A password or key: never shown, left empty to keep it.
    pub secret: bool,
    /// Set in the file, not commented out.
    pub set: bool,
    /// The value an environment variable gives, which wins over the file
    /// (hidden like the others).
    pub environment: Option<String>,
    /// The comment lines just above it.
    pub help: String,
    /// For services: whether it's on now, which differs from the file when
    /// ffmpeg is missing. `None` for the others.
    pub in_effect: Option<bool>,
    /// A service listed in the file that isn't written yet.
    pub unused: bool,
}

impl ShownLine {
    /// The drop-down's choice: `true`, `false`, or empty for the default.
    pub fn choice(&self) -> &str {
        if self.set { &self.field_value } else { "" }
    }

    /// The drop-down's first choice: not set, Tison's default applies. The
    /// commented value is only an example, it may not be the default.
    pub fn default_label(&self) -> &'static str {
        "Not set (default)"
    }
}

/// Never shown: passwords, secrets, tokens and keys.
fn is_secret(key: &str) -> bool {
    let key = key.to_ascii_uppercase();
    ["PASSWORD", "SECRET", "TOKEN", "PRIVATE"].iter().any(|word| key.contains(word)) || key.ends_with("KEY")
}

/// `scheme://user:password@rest` split around the password.
fn address_password(value: &str) -> Option<(&str, &str, &str)> {
    let (scheme, rest) = value.split_once("://")?;
    let (credentials, _) = rest.split_once('@')?;
    let (user, password) = credentials.split_once(':')?;
    let before = &value[..scheme.len() + 3 + user.len() + 1];
    let after = &value[before.len() + password.len()..];
    Some((before, password, after))
}

/// The value as shown: hidden for secrets, the password of `scheme://user:password@host` hidden.
fn shown_value(key: &str, value: &str) -> String {
    if is_secret(key) {
        return if value.is_empty() { String::new() } else { format!("{HIDDEN} (hidden)") };
    }
    match address_password(value) {
        Some((before, _, after)) => format!("{before}{HIDDEN}{after}"),
        None => value.to_string(),
    }
}

fn is_boolean(value: &str) -> bool {
    value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("false")
}

/// `KEY=VALUE`, written or commented out (`# KEY=VALUE`): the key and value.
fn setting(line: &str) -> Option<(&str, &str, bool)> {
    let (text, commented) = match line.strip_prefix('#') {
        Some(rest) => (rest.trim_start(), true),
        None => (line, false),
    };
    let (key, value) = text.split_once('=')?;
    let key = key.trim();
    let valid = !key.is_empty() && key.bytes().all(|byte| byte == b'_' || byte == b'-' || byte.is_ascii_uppercase() || byte.is_ascii_digit());
    valid.then(|| {
        let value = value.trim();
        let unquoted = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|value| value.strip_suffix('\'')))
            .unwrap_or(value);
        (key, unquoted, commented)
    })
}

/// A `### Title ###` line, without its decoration (`### ---- ###` gives none).
fn heading(line: &str) -> Option<String> {
    let inner = line.strip_prefix("###")?.trim_end_matches('#').trim();
    (!inner.is_empty() && inner.chars().any(char::is_alphanumeric)).then(|| inner.to_string())
}

/// Whether the service `key` of services.conf is on now: `None` for the
/// services not written yet.
fn service_in_effect(key: &str) -> Option<bool> {
    let services = services();
    Some(match key.replace('-', "_").as_str() {
        "IMAGESHARING" => services.image_sharing,
        "IMAGESYSTEM" => services.image_system,
        "PUBLISHING" => services.publishing,
        "VIDEO" => services.video,
        "LIVE_STREAMING" => services.live_streaming,
        "AUDIOSTREAMING" => services.podcasts,
        "EVENT_PLANNING" => services.events,
        "CONTENT_AGGREGATOR" => services.content_aggregator,
        "DFRN" => services.dfrn,
        _ => return None,
    })
}

pub(crate) fn version(content: &str) -> String {
    Sha256::digest(content.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The configuration files: (id, name, path, about, read by Tison, services file).
fn known() -> [(&'static str, &'static str, String, &'static str, bool, bool); 3] {
    [
        (
            "runtime",
            "runtime.conf",
            config_path("TISON_CONFIG", DEFAULT_RUNTIME_FILE).display().to_string(),
            "How the server runs: its address, the database, emails, the Fediverse, videos...",
            true,
            false,
        ),
        (
            "services",
            "services.conf",
            config_path("TISON_SERVICES", DEFAULT_SERVICES_FILE).display().to_string(),
            "What the site offers.",
            true,
            true,
        ),
        (
            "settings",
            "settings.conf",
            config_path("TISON_SETTINGS", DEFAULT_SETTINGS_FILE).display().to_string(),
            "Not read by this version of Tison yet.",
            false,
            false,
        ),
    ]
}

fn read(id: &'static str, name: &'static str, path: String, about: &'static str, read: bool, services_file: bool) -> ShownFile {
    let content = fs::read_to_string(&path);
    let missing = content.is_err();
    let content = content.unwrap_or_default();
    let mut lines = Vec::new();
    let mut help: Vec<String> = Vec::new();
    for (number, raw) in content.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            help.clear();
            continue;
        }
        if line.starts_with("###") {
            if let Some(title) = heading(line) {
                lines.push(ShownLine {
                    heading: Some(title),
                    number,
                    key: String::new(),
                    field_value: String::new(),
                    example: String::new(),
                    boolean: false,
                    secret: false,
                    set: false,
                    environment: None,
                    help: String::new(),
                    in_effect: None,
                    unused: false,
                });
            }
            help.clear();
            continue;
        }
        match setting(line) {
            Some((key, value, commented)) => {
                let secret = is_secret(key);
                // Variables use `_` where the file may write `-`.
                let environment = env::var(key.replace('-', "_"))
                    .ok()
                    .filter(|value| !value.trim().is_empty())
                    .map(|value| shown_value(key, value.trim()));
                let in_effect = if services_file { service_in_effect(key) } else { None };
                lines.push(ShownLine {
                    heading: None,
                    number,
                    key: key.to_string(),
                    field_value: if commented || secret { String::new() } else { shown_value(key, value) },
                    example: if commented && !secret { value.to_string() } else { String::new() },
                    boolean: is_boolean(value),
                    secret,
                    set: !commented,
                    environment,
                    help: help.join("\n"),
                    in_effect,
                    unused: services_file && in_effect.is_none(),
                });
                help.clear();
            }
            None => help.push(line.trim_start_matches('#').trim().to_string()),
        }
    }
    ShownFile { id, name, path, about, read, missing, version: version(&content), lines }
}

/// The configuration files, as read now.
pub fn files() -> Vec<ShownFile> {
    known().into_iter().map(|(id, name, path, about, is_read, services_file)| read(id, name, path, about, is_read, services_file)).collect()
}

#[derive(Debug, PartialEq, Eq)]
pub enum SaveError {
    UnknownFile,
    /// Changed on the server since the page was opened.
    Changed,
    /// A value that can't be written, with why.
    Invalid(String),
    Io(String),
}

impl SaveError {
    pub fn message(&self) -> String {
        match self {
            Self::UnknownFile => "This file can't be changed here.".to_string(),
            Self::Changed => "The file changed on the server since this page was opened: nothing was saved. Look again, then save.".to_string(),
            Self::Invalid(why) => why.clone(),
            Self::Io(why) => crate::i18n::tf("Failed to write the file: {}", why),
        }
    }
}

/// The new content of a file from the form's fields (`line-N`): only the
/// lines whose value changed are rewritten. Returns it with the keys changed.
fn edited(content: &str, fields: &HashMap<String, String>) -> Result<(String, Vec<String>), SaveError> {
    let mut changed = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    for (number, raw) in content.lines().enumerate() {
        let Some((key, old, commented)) = setting(raw.trim()) else {
            lines.push(raw.to_string());
            continue;
        };
        let Some(submitted) = fields.get(&format!("line-{number}")) else {
            lines.push(raw.to_string());
            continue;
        };
        let mut new = submitted.trim().to_string();
        if new.chars().any(char::is_control) {
            return Err(SaveError::Invalid(format!("{key}: a value fits on one line.")));
        }
        if is_secret(key) && new.is_empty() {
            // Left empty: the secret is kept.
            lines.push(raw.to_string());
            continue;
        }
        // The password of an address, shown hidden, given back as it was.
        if new.contains(HIDDEN) {
            let Some((_, password, _)) = address_password(old).filter(|_| !commented) else {
                return Err(SaveError::Invalid(format!("{key}: write the whole address, with its password.")));
            };
            new = new.replacen(HIDDEN, password, 1);
        }
        if is_boolean(old) && !new.is_empty() && !is_boolean(&new) {
            return Err(SaveError::Invalid(format!("{key} is true or false.")));
        }
        let number_expected = !old.is_empty() && old.bytes().all(|byte| byte.is_ascii_digit());
        if number_expected && !new.is_empty() && !new.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(SaveError::Invalid(format!("{key} is a number, like {old}.")));
        }
        let line = match (new.is_empty(), commented) {
            // Empty: back to the default, the old value kept in the comment.
            (true, true) => raw.to_string(),
            (true, false) => format!("# {key}={old}"),
            (false, _) if !commented && new == old => raw.to_string(),
            (false, _) => format!("{key}={new}"),
        };
        if line != raw {
            changed.push(key.to_string());
        }
        lines.push(line);
    }
    let mut new_content = lines.join("\n");
    if content.ends_with('\n') {
        new_content.push('\n');
    }
    // Read back like at start: a file Tison couldn't read is never written.
    parse_runtime_file(&new_content).map_err(SaveError::Invalid)?;
    Ok((new_content, changed))
}

/// Saves the file `id` from the form. `version` is the one the page showed.
/// Returns the keys changed, none when nothing changed.
pub fn save(id: &str, version_shown: &str, fields: &HashMap<String, String>) -> Result<Vec<String>, SaveError> {
    let (_, _, path, ..) = known().into_iter().find(|(known_id, ..)| *known_id == id).ok_or(SaveError::UnknownFile)?;
    let content = fs::read_to_string(&path).map_err(|err| SaveError::Io(err.to_string()))?;
    if version(&content) != version_shown {
        return Err(SaveError::Changed);
    }
    let (new_content, changed) = edited(&content, fields)?;
    if changed.is_empty() {
        return Ok(changed);
    }
    // The previous file kept, then the new one written whole and swapped in.
    fs::write(format!("{path}.bak"), &content).map_err(|err| SaveError::Io(err.to_string()))?;
    let temporary = format!("{path}.tmp");
    fs::write(&temporary, &new_content).map_err(|err| SaveError::Io(err.to_string()))?;
    fs::rename(&temporary, &path).map_err(|err| SaveError::Io(err.to_string()))?;
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_hidden() {
        assert_eq!(shown_value("SMTP_PASSWORD", "hunter2"), "•••••• (hidden)");
        assert_eq!(shown_value("API_KEY", "abc"), "•••••• (hidden)");
        assert_eq!(shown_value("DATABASE_URL", "postgres://user:pw@127.0.0.1:5432/db"), "postgres://user:••••••@127.0.0.1:5432/db");
        assert_eq!(shown_value("PUBLIC_BASE_URL", "https://example.com"), "https://example.com");
        assert_eq!(shown_value("APP_PORT", "8080"), "8080");
    }

    #[test]
    fn settings_and_comments_are_read() {
        assert_eq!(setting("APP_PORT=8080"), Some(("APP_PORT", "8080", false)));
        assert_eq!(setting("# FEDERATION=true"), Some(("FEDERATION", "true", true)));
        assert_eq!(setting("# SMTP_FROM=\"Tison <a@b.c>\""), Some(("SMTP_FROM", "Tison <a@b.c>", true)));
        assert_eq!(setting("# Use TISON_CONFIG=/path to load another file."), None);
        assert_eq!(setting("# Defaults to http://APP_HOST:APP_PORT."), None);
        assert_eq!(heading("###   ActivityPub  ###"), Some("ActivityPub".to_string()));
        assert_eq!(heading("###  ------------- ###"), None);
    }

    const FILE: &str = "# The port\nAPP_PORT=8080\nDATABASE_URL=postgres://u:secret@db/x\n# SMTP_PASSWORD=\nSMTP_PASSWORD=hunter2\n# FEDERATION=true\nVIDEO=true\n";

    fn fields(pairs: &[(usize, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(line, value)| (format!("line-{line}"), value.to_string())).collect()
    }

    #[test]
    fn only_changed_lines_are_rewritten() {
        let shown_url = "postgres://u:••••••@db/x";
        let (content, changed) =
            edited(FILE, &fields(&[(1, "8081"), (2, shown_url), (4, ""), (5, "false"), (6, "false")])).unwrap();
        assert_eq!(changed, ["APP_PORT", "FEDERATION", "VIDEO"]);
        assert_eq!(
            content,
            "# The port\nAPP_PORT=8081\nDATABASE_URL=postgres://u:secret@db/x\n# SMTP_PASSWORD=\nSMTP_PASSWORD=hunter2\nFEDERATION=false\nVIDEO=false\n"
        );
    }

    #[test]
    fn empty_goes_back_to_the_default() {
        let (content, changed) = edited(FILE, &fields(&[(1, ""), (6, "")])).unwrap();
        assert_eq!(changed, ["APP_PORT", "VIDEO"]);
        assert!(content.contains("\n# APP_PORT=8080\n") && content.contains("\n# VIDEO=true\n"));
    }

    #[test]
    fn secrets_and_passwords_are_kept_or_replaced() {
        let (content, _) = edited(FILE, &fields(&[(2, "postgres://v:new@db/y"), (4, "s3cret")])).unwrap();
        assert!(content.contains("DATABASE_URL=postgres://v:new@db/y\n") && content.contains("SMTP_PASSWORD=s3cret\n"));
    }

    #[test]
    fn wrong_values_are_refused() {
        assert!(matches!(edited(FILE, &fields(&[(1, "eighty")])), Err(SaveError::Invalid(_))));
        assert!(matches!(edited(FILE, &fields(&[(6, "maybe")])), Err(SaveError::Invalid(_))));
        assert!(matches!(edited(FILE, &fields(&[(1, "80\n81")])), Err(SaveError::Invalid(_))));
    }
}

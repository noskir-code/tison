//! The pages about the site linked under the menu: Terms of Use, Privacy
//! Policy, Cookies and Server rules. Each is a Markdown file of `legal/`
//! (see `markdown::document_to_html`), written by the creator of the site in
//! the Configuration tab of the moderation, or in any editor.
//!
//! On the site, `<!-- comments -->` are left out (notes for the creator), and
//! the `{{PLACEHOLDERS}}` this server knows are filled in, see `PLACEHOLDERS`.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use sqlx::types::chrono::{DateTime, Utc};

/// Where the pages are, next to `configs/`.
pub const DIR: &str = "legal";

/// Long enough for any legal text.
pub const BODY_MAX_CHARS: usize = 200_000;

/// Filled in on the site, the others shown as they are written.
pub const PLACEHOLDERS: [&str; 7] = ["INSTANCE_NAME", "DOMAIN", "SOFTWARE_NAME", "TERMS_URL", "PRIVACY_URL", "COOKIES_URL", "RULES_URL"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Terms,
    Privacy,
    Cookies,
    Rules,
}

impl Kind {
    pub const ALL: [Self; 4] = [Self::Terms, Self::Privacy, Self::Cookies, Self::Rules];

    /// Its address on the site, `/terms-of-use`.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Terms => "terms-of-use",
            Self::Privacy => "privacy-policy",
            Self::Cookies => "cookies",
            Self::Rules => "rules",
        }
    }

    pub fn parse(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.slug() == slug)
    }

    /// Its file in `DIR`.
    pub fn file_name(self) -> &'static str {
        match self {
            Self::Terms => "Terms.md",
            Self::Privacy => "Privacy.md",
            Self::Cookies => "Cookies.md",
            Self::Rules => "Rules.md",
        }
    }

    /// `legal/Terms.md`, shown to the creator.
    pub fn path(self) -> String {
        format!("{DIR}/{}", self.file_name())
    }

    /// In English, translated by the templates.
    pub fn title(self) -> &'static str {
        match self {
            Self::Terms => "Terms of Use",
            Self::Privacy => "Privacy Policy",
            Self::Cookies => "Cookies",
            Self::Rules => "Server rules",
        }
    }

    /// What the creator should write in it.
    pub fn about(self) -> &'static str {
        match self {
            Self::Terms => "The agreement between the site and the people who use it: who may sign up, what they agree to, what happens to their account.",
            Self::Privacy => "What personal data the site keeps (email, posts, IP addresses in the logs...), why, for how long, and how to have it deleted.",
            Self::Cookies => "The cookies of the site: the session cookie that keeps people signed in, and the one that remembers the chosen language.",
            Self::Rules => "What is allowed and what isn't on this server: the moderators act on reports with these rules.",
        }
    }
}

/// A page as its file is now. `body` is empty when there's no file.
pub struct SitePage {
    pub kind: Kind,
    pub body: String,
    pub updated_at: Option<DateTime<Utc>>,
    /// Of the file read, see `save`.
    pub version: String,
}

impl SitePage {
    /// What visitors see: something besides comments.
    pub fn is_written(&self) -> bool {
        !without_comments(&self.body).trim().is_empty()
    }

    /// Whether it starts with its own `# Title`, then shown instead of ours.
    pub fn has_own_title(&self) -> bool {
        without_comments(&self.body).trim_start().starts_with("# ")
    }

    /// For visitors, the addresses being of the site at `base_url`.
    pub fn html(&self, instance_name: &str, base_url: &str) -> String {
        let text = fill(&without_comments(&self.body), instance_name, base_url);
        crate::markdown::document_to_html(&text, &|address| match crate::entities::media::is_local(address) {
            true => address.to_string(),
            false => crate::entities::media::proxy_url(address),
        })
    }

    pub fn updated_label(&self) -> Option<String> {
        self.updated_at.map(|time| crate::i18n::date(time.date_naive()))
    }
}

pub fn read(kind: Kind) -> SitePage {
    read_in(Path::new(DIR), kind)
}

fn read_in(dir: &Path, kind: Kind) -> SitePage {
    let path = dir.join(kind.file_name());
    let body = match fs::read_to_string(&path) {
        Ok(body) => body,
        Err(err) => {
            if err.kind() != io::ErrorKind::NotFound {
                tracing::error!("failed to read {}: {}", path.display(), err);
            }
            String::new()
        }
    };
    let updated_at = fs::metadata(&path).and_then(|metadata| metadata.modified()).ok().map(DateTime::<Utc>::from);
    SitePage {
        kind,
        version: crate::settings::shown::version(&body),
        body,
        updated_at,
    }
}

#[derive(Debug)]
pub enum SaveError {
    /// The file changed since `version` was read: in an editor, or another tab.
    Changed,
    Io(String),
}

/// Writes the page, unless its file changed since it was read as
/// `version_shown`. The previous file is kept as `.bak`.
pub fn save(kind: Kind, version_shown: &str, body: &str) -> Result<(), SaveError> {
    save_in(Path::new(DIR), kind, version_shown, body)
}

fn save_in(dir: &Path, kind: Kind, version_shown: &str, body: &str) -> Result<(), SaveError> {
    let io = |err: io::Error| SaveError::Io(err.to_string());
    let path: PathBuf = dir.join(kind.file_name());
    let now = read_in(dir, kind);
    if now.version != version_shown {
        return Err(SaveError::Changed);
    }
    // Browsers send `\r\n` from a textarea; a file ends with a line break.
    let mut content = body.replace("\r\n", "\n").trim_end().to_string();
    if !content.is_empty() {
        content.push('\n');
    }
    if content == now.body {
        return Ok(());
    }
    fs::create_dir_all(dir).map_err(io)?;
    if path.exists() {
        fs::write(path.with_extension("md.bak"), &now.body).map_err(io)?;
    }
    let temporary = path.with_extension("md.tmp");
    fs::write(&temporary, &content).map_err(io)?;
    fs::rename(&temporary, &path).map_err(io)
}

/// The text without its `<!-- comments -->`, one left open going to the end.
fn without_comments(text: &str) -> String {
    let mut kept = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        kept.push_str(&rest[..start]);
        rest = rest[start..].find("-->").map_or("", |end| &rest[start + end + 3..]);
    }
    kept.push_str(rest);
    kept
}

/// `{{INSTANCE_NAME}}`, `{{TERMS_URL}}`... replaced, see `PLACEHOLDERS`.
fn fill(text: &str, instance_name: &str, base_url: &str) -> String {
    let domain = crate::entities::author::host_of(base_url);
    let url = |kind: Kind| format!("{base_url}/{}", kind.slug());
    [
        ("INSTANCE_NAME", instance_name.to_string()),
        ("DOMAIN", domain.to_string()),
        ("SOFTWARE_NAME", "Tison".to_string()),
        ("TERMS_URL", url(Kind::Terms)),
        ("PRIVACY_URL", url(Kind::Privacy)),
        ("COOKIES_URL", url(Kind::Cookies)),
        ("RULES_URL", url(Kind::Rules)),
    ]
    .iter()
    .fold(text.to_string(), |text, (name, value)| text.replace(&format!("{{{{{name}}}}}"), value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_and_files() {
        for kind in Kind::ALL {
            assert_eq!(Kind::parse(kind.slug()), Some(kind));
        }
        assert_eq!(Kind::parse("terms"), None);
        assert_eq!(Kind::Rules.path(), "legal/Rules.md");
    }

    #[test]
    fn comments_are_left_out_and_placeholders_filled() {
        assert_eq!(without_comments("<!--\nnotes\n-->\n# Rules\nA <!-- b --> c"), "\n# Rules\nA  c");
        assert_eq!(without_comments("A <!-- never closed"), "A ");
        let text = "{{INSTANCE_NAME}} at `{{DOMAIN}}`: [Terms]({{TERMS_URL}}), {{EFFECTIVE_DATE}}";
        assert_eq!(fill(text, "Lynx", "https://lynx.example"), "Lynx at `lynx.example`: [Terms](https://lynx.example/terms-of-use), {{EFFECTIVE_DATE}}");
        let page = SitePage { kind: Kind::Rules, body: "<!-- notes -->\n# Rules — {{INSTANCE_NAME}}\n\nSee\nthe [Terms]({{TERMS_URL}}).".to_string(), updated_at: None, version: String::new() };
        assert!(page.is_written() && page.has_own_title());
        assert_eq!(
            page.html("Lynx", "https://lynx.example"),
            "<h1>Rules — Lynx</h1>\n<p>See\nthe <a href=\"https://lynx.example/terms-of-use\" rel=\"nofollow noopener noreferrer ugc\" target=\"_blank\">Terms</a>.</p>\n"
        );
        let notes_only = SitePage { body: "<!-- to do -->\n".to_string(), ..page };
        assert!(!notes_only.is_written());
    }

    #[test]
    fn saving_keeps_edits_made_elsewhere() {
        let dir = std::env::temp_dir().join(format!("tison-legal-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let missing = read_in(&dir, Kind::Cookies);
        assert!(missing.body.is_empty() && missing.updated_at.is_none());
        save_in(&dir, Kind::Cookies, &missing.version, "# Cookies\r\nOne.\r\n\r\n").unwrap();
        let saved = read_in(&dir, Kind::Cookies);
        assert_eq!(saved.body, "# Cookies\nOne.\n");
        assert!(saved.updated_at.is_some());
        // Opened before that save: refused.
        assert!(matches!(save_in(&dir, Kind::Cookies, &missing.version, "Old"), Err(SaveError::Changed)));
        save_in(&dir, Kind::Cookies, &saved.version, "Two").unwrap();
        assert_eq!(fs::read_to_string(dir.join("Cookies.md.bak")).unwrap(), "# Cookies\nOne.\n");
        assert_eq!(read_in(&dir, Kind::Cookies).body, "Two\n");
        fs::remove_dir_all(&dir).unwrap();
    }
}

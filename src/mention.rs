//! Mentions, like on Mastodon: `@alice` names a user of this server,
//! `@bob@mastodon.social` an account of another server. In a text they are
//! linked (see `linkify`), the people mentioned are told (see
//! `entities::mention`), and other servers get them as `Mention` tags.
//!
//! Also the length of a post counted like Mastodon does (`BLOGGING`, see
//! `mastodon_length`).

/// What a link counts for in the length of a post, whatever its length:
/// Mastodon's (its links are shortened, like Twitter's were).
pub const LINK_LENGTH: usize = 23;

/// A mention found in a text: where it is, the username, and the domain of
/// its server when written (`@bob@mastodon.social`).
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Handle {
    pub start: usize,
    pub end: usize,
    pub user: String,
    pub domain: Option<String>,
}

impl Handle {
    /// Whether it names someone of this server (`domain` is this server's).
    pub fn is_local(&self, site_domain: &str) -> bool {
        self.domain.as_deref().is_none_or(|domain| domain.eq_ignore_ascii_case(site_domain))
    }

    /// `@bob@mastodon.social`, or `@alice` for this server.
    pub fn full(&self) -> String {
        match &self.domain {
            Some(domain) => format!("@{}@{}", self.user, domain),
            None => format!("@{}", self.user),
        }
    }
}

fn is_name_letter(letter: char) -> bool {
    letter.is_ascii_alphanumeric() || letter == '_'
}

/// The username starting at `start` (after the `@`): letters, digits and
/// `_`, with `.` and `-` inside (not at its end), like Mastodon's.
fn name_end(text: &str, start: usize) -> usize {
    let bytes = text.as_bytes();
    let mut end = start;
    let mut last_letter = start;
    while end < bytes.len() {
        let letter = bytes[end] as char;
        if is_name_letter(letter) {
            end += 1;
            last_letter = end;
        } else if matches!(letter, '.' | '-') && end > start {
            end += 1;
        } else {
            break;
        }
    }
    last_letter
}

/// The domain starting at `start` (after the second `@`): labels of
/// letters, digits and `-`, with at least one dot, not ending with one.
fn domain_end(text: &str, start: usize) -> Option<usize> {
    let rest = &text[start..];
    let length = rest.find(|letter: char| !(letter.is_ascii_alphanumeric() || matches!(letter, '-' | '.'))).unwrap_or(rest.len());
    let domain = rest[..length].trim_end_matches(['.', '-']);
    let valid = domain.contains('.') && domain.split('.').all(|label| !label.is_empty() && !label.starts_with('-') && !label.ends_with('-'));
    valid.then_some(start + domain.len())
}

/// Every mention of the text, in order. Not in an email address
/// (`me@example.com`) nor in a link (`https://x.org/@bob`).
pub fn spans(text: &str) -> Vec<Handle> {
    let mut found = Vec::new();
    let mut position = 0;
    while let Some(offset) = text[position..].find('@') {
        let at = position + offset;
        position = at + 1;
        let starts_word = text[..at]
            .chars()
            .next_back()
            .is_none_or(|before| !before.is_alphanumeric() && !matches!(before, '/' | '@' | '.' | '-' | '_' | ':'));
        if !starts_word {
            continue;
        }
        let user_end = name_end(text, at + 1);
        if user_end == at + 1 {
            continue;
        }
        let user = text[at + 1..user_end].to_string();
        let (end, domain) = match text[user_end..].starts_with('@') {
            true => match domain_end(text, user_end + 1) {
                Some(end) => (end, Some(text[user_end + 1..end].to_ascii_lowercase())),
                // `@bob@` or `@bob@nothing`: not a mention at all.
                None => continue,
            },
            false => (user_end, None),
        };
        found.push(Handle { start: at, end, user, domain });
        position = end;
    }
    found
}

/// The people mentioned, each once (usernames compared without case).
pub fn find(text: &str) -> Vec<Handle> {
    let mut people: Vec<Handle> = Vec::new();
    for handle in spans(text) {
        let known = people.iter().any(|known| known.user.eq_ignore_ascii_case(&handle.user) && known.domain == handle.domain);
        if !known {
            people.push(handle);
        }
    }
    people
}

/// The length of a post counted like Mastodon: every link counts for
/// `LINK_LENGTH` characters, and a mention of another server only for its
/// username (`@bob@mastodon.social` counts as `@bob`). The same rules as
/// the counter of the form, see `system.js`.
pub fn mastodon_length(text: &str) -> usize {
    let links = crate::linkify::address_spans(text);
    let mut length = 0;
    let mut written = 0;
    for (start, end) in &links {
        length += mentions_length(&text[written..*start]) + LINK_LENGTH;
        written = *end;
    }
    length + mentions_length(&text[written..])
}

/// The length of a text without links, its mentions counted without their
/// domain.
fn mentions_length(text: &str) -> usize {
    let mut length = text.chars().count();
    for handle in spans(text) {
        if let Some(domain) = &handle.domain {
            // The `@` and the domain.
            length -= domain.chars().count() + 1;
        }
    }
    length
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(text: &str) -> Vec<String> {
        spans(text).iter().map(Handle::full).collect()
    }

    #[test]
    fn mentions_are_found() {
        assert_eq!(names("Hi @alice and @Bob@Mastodon.Social!"), ["@alice", "@Bob@mastodon.social"]);
        assert_eq!(names("(@carol.d) @dan_e. @x-"), ["@carol.d", "@dan_e", "@x"]);
        // Emails, links, a lone @ and an unfinished domain aren't mentions.
        assert!(names("me@example.com https://x.org/@bob @ @bob@ @bob@nothing").is_empty());
        let handle = &spans("@alice@tison.social")[0];
        assert!(handle.is_local("tison.social") && !handle.is_local("other.example"));
        assert_eq!(find("@a @A @a@x.org").iter().map(Handle::full).collect::<Vec<_>>(), ["@a", "@a@x.org"]);
    }

    #[test]
    fn length_is_counted_like_mastodon() {
        assert_eq!(mastodon_length("Hello"), 5);
        let long = format!("See https://example.com/{} now", "a".repeat(200));
        assert_eq!(mastodon_length(&long), "See ".len() + LINK_LENGTH + " now".len());
        assert_eq!(mastodon_length("Hi @bob@mastodon.social"), "Hi @bob".len());
        assert_eq!(mastodon_length("café ☕"), 6);
    }
}

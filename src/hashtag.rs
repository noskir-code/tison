//! Hashtags: `#word` in the text of posts, articles, videos, episodes,
//! events and comments. They become links to `/tags/{tag}` (see
//! `linkify::to_html`), and are sent in the `tag` of the objects for other
//! servers as Mastodon, Pixelfed and PeerTube do (see `API::activities`).
//!
//! A hashtag starts a word, then has letters, digits or `_`, at least one
//! letter: `#rust`, `#été2026`, `#video_games`, not `#1` nor `a#b`.

/// Longer ones aren't hashtags, like on Mastodon.
pub const MAX_CHARS: usize = 100;

fn is_tag_char(letter: char) -> bool {
    letter.is_alphanumeric() || letter == '_'
}

/// A tag as compared and put in addresses: lowercase, `None` when it can't
/// be one (`#` and a leading `#` are taken off).
pub fn normalize(raw: &str) -> Option<String> {
    let tag = raw.trim().trim_start_matches('#');
    let valid = !tag.is_empty()
        && tag.chars().count() <= MAX_CHARS
        && tag.chars().all(is_tag_char)
        && tag.chars().any(char::is_alphabetic);
    valid.then(|| tag.to_lowercase())
}

/// The hashtags of `text`, where they are: (start of `#`, end), in order.
pub(crate) fn spans(text: &str) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut previous: Option<char> = None;
    let mut letters = text.char_indices().peekable();
    while let Some((start, letter)) = letters.next() {
        // Not inside a word, an address (`page#part`) or an HTML entity (`&#39;`).
        let starts_word = previous.is_none_or(|before| !is_tag_char(before) && !matches!(before, '&' | '/' | '#'));
        if letter == '#' && starts_word {
            let mut end = start + 1;
            while let Some(&(at, next)) = letters.peek() {
                if !is_tag_char(next) {
                    break;
                }
                end = at + next.len_utf8();
                letters.next();
            }
            if normalize(&text[start + 1..end]).is_some() {
                found.push((start, end));
            }
            previous = text[..end].chars().next_back();
            continue;
        }
        previous = Some(letter);
    }
    found
}

/// The hashtags of the texts, lowercase, each once, in order.
pub fn find(texts: &[&str]) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for text in texts {
        for (start, end) in spans(text) {
            if let Some(tag) = normalize(&text[start..end])
                && !tags.contains(&tag)
            {
                tags.push(tag);
            }
        }
    }
    tags
}

/// Where a hashtag's posts are listed, on this site.
pub fn path(tag: &str) -> String {
    format!("/tags/{}", crate::handlers::search::form_urlencode(tag))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashtags_are_found() {
        assert_eq!(find(&["Playing #VideoGames and #été2026, #video_games! #VideoGames again"]), ["videogames", "été2026", "video_games"]);
        assert_eq!(find(&["#rust", "more #Rust and #linux"]), ["rust", "linux"]);
    }

    #[test]
    fn not_hashtags() {
        assert!(find(&["#1 and #2024 and a#b and https://x.org/page#part and &#39; and # alone and ##"]).is_empty());
        assert_eq!(normalize("#Tison"), Some("tison".to_string()));
        assert_eq!(normalize("a-b"), None);
        assert_eq!(normalize(&"a".repeat(101)), None);
    }

    #[test]
    fn spans_cover_the_tag() {
        let text = "Hi #rust.";
        assert_eq!(spans(text).iter().map(|&(start, end)| &text[start..end]).collect::<Vec<_>>(), ["#rust"]);
    }
}

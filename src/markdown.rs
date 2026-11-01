//! Markdown of articles, like WriteFreely's: written here and shown as HTML
//! (`to_html`), and read back from the HTML of other servers
//! (`from_html`). The HTML made here is always safe: everything is escaped,
//! links and pictures only go to web addresses.
//!
//! What is understood: headings (`#`), paragraphs (a line break is kept),
//! `>` quotes, `-` / `*` / `1.` lists, fenced code (```), `---` lines,
//! `**bold**`, `*italic*`, `~~struck~~`, `` `code` ``, `[links](https://...)`,
//! `![pictures](https://...)`, `\` to keep a character as it is, and
//! addresses written as they are.

use crate::linkify::escape;

const LINK_ATTRIBUTES: &str = "rel=\"nofollow noopener noreferrer ugc\" target=\"_blank\"";

/// The article as HTML. `picture` gives the address shown for a picture
/// (through the media proxy on pages, absolute for other servers).
pub fn to_html(source: &str, picture: &dyn Fn(&str) -> String) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut html = String::with_capacity(source.len() + source.len() / 4);
    blocks(&lines, picture, &mut html);
    html
}

fn is_rule(line: &str) -> bool {
    let line: String = line.chars().filter(|letter| !letter.is_whitespace()).collect();
    line.len() >= 3 && ["-", "*", "_"].iter().any(|mark| line.chars().all(|letter| letter.to_string() == *mark))
}

/// `## Title`: its level and text.
fn heading(line: &str) -> Option<(usize, &str)> {
    let level = line.chars().take_while(|letter| *letter == '#').count();
    let rest = &line[level..];
    ((1..=6).contains(&level) && (rest.is_empty() || rest.starts_with(' '))).then(|| (level, rest.trim().trim_end_matches('#').trim_end()))
}

/// A list item: whether the list is numbered, and the item's text.
fn list_item(line: &str) -> Option<(bool, &str)> {
    let line = line.trim_start();
    for mark in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(mark) {
            return Some((false, rest));
        }
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if (1..=9).contains(&digits) {
        let rest = &line[digits..];
        if let Some(text) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return Some((true, text));
        }
    }
    None
}

fn is_fence(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with("```") || line.starts_with("~~~")
}

/// A line starting a block other than a paragraph.
fn starts_block(line: &str) -> bool {
    let trimmed = line.trim_start();
    heading(trimmed).is_some() || is_rule(trimmed) || trimmed.starts_with('>') || list_item(line).is_some() || is_fence(line)
}

fn blocks(lines: &[&str], picture: &dyn Fn(&str) -> String, html: &mut String) {
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            index += 1;
        } else if is_fence(line) {
            let fence = &trimmed[..3];
            let mut code = Vec::new();
            index += 1;
            while index < lines.len() && !lines[index].trim_start().starts_with(fence) {
                code.push(lines[index]);
                index += 1;
            }
            index += 1;
            html.push_str(&format!("<pre><code>{}</code></pre>\n", escape(&code.join("\n"))));
        } else if let Some((level, text)) = heading(trimmed) {
            html.push_str(&format!("<h{level}>{}</h{level}>\n", inline(text, picture)));
            index += 1;
        } else if is_rule(trimmed) {
            html.push_str("<hr>\n");
            index += 1;
        } else if trimmed.starts_with('>') {
            let mut quoted = Vec::new();
            while index < lines.len() && lines[index].trim_start().starts_with('>') {
                let rest = lines[index].trim_start()[1..].strip_prefix(' ').unwrap_or(&lines[index].trim_start()[1..]);
                quoted.push(rest);
                index += 1;
            }
            html.push_str("<blockquote>\n");
            blocks(&quoted, picture, html);
            html.push_str("</blockquote>\n");
        } else if let Some((numbered, _)) = list_item(line) {
            let tag = if numbered { "ol" } else { "ul" };
            html.push_str(&format!("<{tag}>\n"));
            while index < lines.len() {
                // A blank line between items keeps the list going.
                let next = (index..lines.len()).find(|&at| !lines[at].trim().is_empty()).unwrap_or(lines.len());
                let Some((_, text)) = lines.get(next).and_then(|line| list_item(line)).filter(|(kind, _)| *kind == numbered) else {
                    break;
                };
                index = next;
                // Indented lines after an item continue it.
                let mut item = vec![text.trim()];
                index += 1;
                while index < lines.len()
                    && lines[index].starts_with([' ', '\t'])
                    && !lines[index].trim().is_empty()
                    && list_item(lines[index]).is_none()
                {
                    item.push(lines[index].trim());
                    index += 1;
                }
                html.push_str(&format!("<li>{}</li>\n", inline(&item.join("\n"), picture)));
            }
            html.push_str(&format!("</{tag}>\n"));
        } else {
            let mut paragraph = vec![trimmed];
            index += 1;
            while index < lines.len() && !lines[index].trim().is_empty() && !starts_block(lines[index]) {
                paragraph.push(lines[index].trim());
                index += 1;
            }
            html.push_str(&format!("<p>{}</p>\n", inline(&paragraph.join("\n"), picture)));
        }
    }
}

/// `[text](address)` starting at `start` (on the `[`): the text, the
/// address and where it ends.
fn bracketed(text: &str, start: usize) -> Option<(&str, &str, usize)> {
    let mut depth = 0;
    let mut close = None;
    for (offset, letter) in text[start..].char_indices() {
        match letter {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(start + offset);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    let rest = text[close + 1..].strip_prefix('(')?;
    let end = rest.find(')')?;
    // An optional title: `(https://x "Title")`, not kept.
    let address = rest[..end].split_whitespace().next().unwrap_or("");
    Some((&text[start + 1..close], address, close + 2 + end + 1))
}

fn is_web_address(address: &str) -> bool {
    let lower = address.to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://")) && !address.contains(['"', '<', '>', ' '])
}

/// Where `marker` closes the span opened before `from`: not right after a
/// space, and with something inside.
fn closing(text: &str, from: usize, marker: &str) -> Option<usize> {
    let mut search = from;
    while let Some(found) = text[search..].find(marker) {
        let at = search + found;
        if at > from && !text[..at].ends_with(char::is_whitespace) {
            return Some(at);
        }
        search = at + marker.len();
    }
    None
}

fn inline(text: &str, picture: &dyn Fn(&str) -> String) -> String {
    let mut html = String::with_capacity(text.len() + 16);
    // Plain text waiting to be written, with its addresses made links.
    let mut plain = String::new();
    let flush = |plain: &mut String, html: &mut String| {
        html.push_str(&crate::linkify::to_html(plain).replace('\n', "<br>\n"));
        plain.clear();
    };
    let mut index = 0;
    while index < text.len() {
        let rest = &text[index..];
        let letter = rest.chars().next().unwrap_or_default();
        if letter == '\\'
            && let Some(next) = rest[1..].chars().next().filter(char::is_ascii_punctuation)
        {
            plain.push(next);
            index += 1 + next.len_utf8();
            continue;
        }
        if letter == '`'
            && let Some(end) = rest[1..].find('`')
        {
            flush(&mut plain, &mut html);
            html.push_str(&format!("<code>{}</code>", escape(&rest[1..1 + end])));
            index += end + 2;
            continue;
        }
        if rest.starts_with("![")
            && let Some((alt, address, end)) = bracketed(text, index + 1)
            && (is_web_address(address) || crate::entities::media::is_local(address))
        {
            flush(&mut plain, &mut html);
            html.push_str(&format!("<img src=\"{}\" alt=\"{}\" loading=\"lazy\">", escape(&picture(address)), escape(alt)));
            index = end;
            continue;
        }
        if letter == '['
            && let Some((words, address, end)) = bracketed(text, index)
            && is_web_address(address)
        {
            flush(&mut plain, &mut html);
            // No link inside a link: addresses in its words stay text.
            let inside = inline(words, picture);
            let inside = if inside.contains("<a ") { escape(words) } else { inside };
            html.push_str(&format!("<a href=\"{}\" {LINK_ATTRIBUTES}>{inside}</a>", escape(address)));
            index = end;
            continue;
        }
        let span = [("**", "strong"), ("__", "strong"), ("~~", "del"), ("*", "em"), ("_", "em")]
            .into_iter()
            .find(|(marker, _)| rest.starts_with(marker));
        if let Some((marker, tag)) = span {
            // `_` inside a word (snake_case) is not italic.
            let in_word = marker.starts_with('_') && plain.ends_with(char::is_alphanumeric);
            let opens = !in_word && rest[marker.len()..].chars().next().is_some_and(|next| !next.is_whitespace());
            if opens && let Some(close) = closing(text, index + marker.len(), marker) {
                flush(&mut plain, &mut html);
                html.push_str(&format!("<{tag}>{}</{tag}>", inline(&text[index + marker.len()..close], picture)));
                index = close + marker.len();
                continue;
            }
        }
        plain.push(letter);
        index += letter.len_utf8();
    }
    flush(&mut plain, &mut html);
    html
}

/// The words of the article on one line, without its marks or link
/// addresses: for its excerpt.
pub fn to_text(source: &str) -> String {
    let html = to_html(source, &|address| address.to_string());
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    for letter in html.chars() {
        match letter {
            '<' => in_tag = true,
            // Blocks are separated by a line break in the HTML made here.
            '>' => in_tag = false,
            _ if !in_tag => text.push(letter),
            _ => {}
        }
    }
    let text = crate::API::activities::decode_entities(&text);
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The address of the article's first picture (`![...](...)` outside code),
/// for its preview card.
pub fn first_picture(source: &str) -> Option<String> {
    let mut in_code = false;
    for line in source.lines() {
        if is_fence(line) {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        let mut search = 0;
        while let Some(found) = line[search..].find("![") {
            let start = search + found;
            if let Some((_, address, _)) = bracketed(line, start + 1)
                && (is_web_address(address) || crate::entities::media::is_local(address))
            {
                return Some(address.to_string());
            }
            search = start + 2;
        }
    }
    None
}

/// Minutes to read it, at 200 words a minute like WriteFreely.
pub fn reading_minutes(source: &str) -> usize {
    source.split_whitespace().count().div_ceil(200).max(1)
}

/// Markdown read from the HTML of an article of another server
/// (WriteFreely's, Friendica's...). Tags not understood are dropped, their
/// text kept; `<script>` and `<style>` are dropped whole.
pub fn from_html(html: &str) -> String {
    let mut markdown = String::with_capacity(html.len());
    let mut rest = html;
    // Where open links and quotes start, with the link's address.
    let mut links: Vec<(usize, Option<String>)> = Vec::new();
    let mut quotes: Vec<usize> = Vec::new();
    // Numbered lists and their count, `None` for the others.
    let mut lists: Vec<Option<usize>> = Vec::new();
    let mut in_pre = false;
    while let Some(start) = rest.find('<') {
        push_text(&mut markdown, &rest[..start], in_pre);
        let Some(end) = rest[start..].find('>') else {
            rest = "";
            break;
        };
        let original = rest[start + 1..start + end].trim();
        let tag = original.to_ascii_lowercase();
        let closing_tag = tag.starts_with('/');
        let name: String = tag.trim_start_matches('/').chars().take_while(char::is_ascii_alphanumeric).collect();
        rest = &rest[start + end + 1..];
        if !closing_tag && (name == "script" || name == "style") {
            rest = rest.find(&format!("</{name}")).map_or("", |close| &rest[close..]);
            continue;
        }
        match (name.as_str(), closing_tag) {
            ("br", _) => markdown.push_str("  \n"),
            ("p" | "div" | "section" | "figure", _) => markdown.push_str("\n\n"),
            ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", false) => {
                let level = name[1..].parse().unwrap_or(1);
                markdown.push_str(&format!("\n\n{} ", "#".repeat(level)));
            }
            ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", true) => markdown.push_str("\n\n"),
            ("strong" | "b", _) => markdown.push_str("**"),
            ("em" | "i", _) => markdown.push('*'),
            ("del" | "s" | "strike", _) => markdown.push_str("~~"),
            ("code", _) if !in_pre => markdown.push('`'),
            ("pre", false) => {
                markdown.push_str("\n\n```\n");
                in_pre = true;
            }
            ("pre", true) => {
                markdown.push_str("\n```\n\n");
                in_pre = false;
            }
            ("hr", _) => markdown.push_str("\n\n---\n\n"),
            ("ul", false) => lists.push(None),
            ("ol", false) => lists.push(Some(0)),
            ("ul" | "ol", true) => {
                lists.pop();
                markdown.push_str("\n\n");
            }
            ("li", false) => {
                let mark = match lists.last_mut() {
                    Some(Some(count)) => {
                        *count += 1;
                        format!("{count}. ")
                    }
                    _ => "- ".to_string(),
                };
                markdown.push_str(&format!("\n{}{mark}", "  ".repeat(lists.len().saturating_sub(1))));
            }
            ("blockquote", false) => {
                markdown.push_str("\n\n");
                quotes.push(markdown.len());
            }
            ("blockquote", true) => {
                if let Some(quote_start) = quotes.pop() {
                    let quoted = clean_lines(&markdown[quote_start..]);
                    markdown.truncate(quote_start);
                    for line in quoted.lines() {
                        markdown.push_str(if line.is_empty() { ">\n" } else { "> " });
                        if !line.is_empty() {
                            markdown.push_str(line);
                            markdown.push('\n');
                        }
                    }
                    markdown.push('\n');
                }
            }
            ("a", false) => links.push((markdown.len(), attribute(original, "href").filter(|href| is_web_address(href)))),
            ("a", true) => {
                if let Some((link_start, Some(href))) = links.pop() {
                    let words = markdown[link_start..].trim().to_string();
                    markdown.truncate(link_start);
                    // A written address stays as it is: it becomes a link anyway.
                    if words.is_empty() || words == href || href.ends_with(words.trim_end_matches('…')) {
                        markdown.push_str(&href);
                    } else {
                        markdown.push_str(&format!("[{words}]({href})"));
                    }
                }
            }
            ("img", _) => {
                if let Some(src) = attribute(original, "src").filter(|src| is_web_address(src)) {
                    let alt = attribute(original, "alt").unwrap_or_default().replace(['[', ']'], "");
                    markdown.push_str(&format!("![{alt}]({src})"));
                }
            }
            _ => {}
        }
    }
    push_text(&mut markdown, rest, in_pre);
    clean_lines(&markdown)
}

/// Text between tags: entities decoded, Markdown marks escaped, spaces of
/// the HTML source collapsed (except in code).
fn push_text(markdown: &mut String, text: &str, in_pre: bool) {
    let decoded = crate::API::activities::decode_entities(text);
    if in_pre {
        markdown.push_str(&decoded);
        return;
    }
    let mut last_space = markdown.ends_with([' ', '\n']) || markdown.is_empty();
    for letter in decoded.chars() {
        if letter.is_whitespace() {
            if !last_space {
                markdown.push(' ');
            }
            last_space = true;
            continue;
        }
        last_space = false;
        if matches!(letter, '\\' | '*' | '_' | '`' | '[' | ']' | '~') {
            markdown.push('\\');
        }
        markdown.push(letter);
    }
}

/// Lines without trailing spaces (except a kept line break), at most one
/// blank line in a row, no blank lines around.
fn clean_lines(text: &str) -> String {
    let mut cleaned = String::with_capacity(text.len());
    for line in text.split('\n') {
        let kept_break = line.ends_with("  ") && !line.trim().is_empty();
        let line = line.trim_end();
        if line.is_empty() && (cleaned.ends_with("\n\n") || cleaned.is_empty()) {
            continue;
        }
        cleaned.push_str(line);
        if kept_break {
            cleaned.push_str("  ");
        }
        cleaned.push('\n');
    }
    cleaned.trim_end().to_string()
}

/// The value of `name="..."` in a tag, from inside its `<` and `>`.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut search = 0;
    while let Some(found) = lower[search..].find(&format!("{name}=")) {
        let at = search + found;
        // ` src=`, not `data-src=`.
        if at > 0 && lower[..at].ends_with(char::is_whitespace) {
            let after = &tag[at + name.len() + 1..];
            let value = match after.chars().next()? {
                quote @ ('"' | '\'') => after[1..].split(quote).next()?,
                _ => after.split(char::is_whitespace).next()?,
            };
            return Some(crate::API::activities::decode_entities(value.trim()));
        }
        search = at + name.len() + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn html(source: &str) -> String {
        to_html(source, &|address| format!("/proxy?{address}"))
    }

    fn link(href: &str, text: &str) -> String {
        format!("<a href=\"{href}\" {LINK_ATTRIBUTES}>{text}</a>")
    }

    #[test]
    fn blocks_are_read() {
        assert_eq!(
            html("# Title\n\nFirst line\nsecond line\n\n> quoted\n> **still**\n\n- one\n- two\n  more\n\n1. a\n2. b\n\n---\n\n```\n<b>x</b>\n```"),
            "<h1>Title</h1>\n<p>First line<br>\nsecond line</p>\n<blockquote>\n<p>quoted<br>\n<strong>still</strong></p>\n</blockquote>\n\
             <ul>\n<li>one</li>\n<li>two<br>\nmore</li>\n</ul>\n<ol>\n<li>a</li>\n<li>b</li>\n</ol>\n<hr>\n<pre><code>&lt;b&gt;x&lt;/b&gt;</code></pre>\n"
        );
        assert_eq!(html("#hashtag"), "<p>#hashtag</p>\n");
    }

    #[test]
    fn inline_marks_are_read() {
        assert_eq!(html("**bold** *it* _it_ ~~no~~ `a*b`"), "<p><strong>bold</strong> <em>it</em> <em>it</em> <del>no</del> <code>a*b</code></p>\n");
        assert_eq!(html("snake_case_name and 2 * 3 * 4"), "<p>snake_case_name and 2 * 3 * 4</p>\n");
        assert_eq!(html("\\*kept\\*"), "<p>*kept*</p>\n");
    }

    #[test]
    fn links_and_pictures() {
        assert_eq!(html("[the *site*](https://a.org/x \"T\")"), format!("<p>{}</p>\n", link("https://a.org/x", "the <em>site</em>")));
        assert_eq!(html("see https://a.org"), format!("<p>see {}</p>\n", link("https://a.org", "a.org")));
        assert_eq!(html("![A cat](https://a.org/c.png)"), "<p><img src=\"/proxy?https://a.org/c.png\" alt=\"A cat\" loading=\"lazy\"></p>\n");
    }

    #[test]
    fn nothing_unsafe_gets_through() {
        assert_eq!(html("<script>alert(1)</script>"), "<p>&lt;script&gt;alert(1)&lt;/script&gt;</p>\n");
        assert_eq!(html("[x](javascript:alert(1))"), "<p>[x](javascript:alert(1))</p>\n");
        assert_eq!(html("![x](data:image/png;base64,AA)"), "<p>![x](data:image/png;base64,AA)</p>\n");
        assert!(!html("[x](https://a.org/\"onclick=\"y)").contains("\"onclick"));
    }

    #[test]
    fn writefreely_html_becomes_markdown() {
        let html = "<h2 id=\"x\">Day one</h2>\n<p>We <strong>left</strong> at <em>dawn</em>, see <a href=\"https://a.org/map\">the map</a>.<br>Then:</p>\n\
                    <ul><li>a tent</li><li>a stove</li></ul><blockquote><p>Go far</p></blockquote>\
                    <p><img src=\"https://a.org/p.jpg\" alt=\"Hills\"></p><pre><code>let x = *y;</code></pre><script>bad()</script><p>2*3 &amp; https://a.org</p>";
        assert_eq!(
            from_html(html),
            "## Day one\n\nWe **left** at *dawn*, see [the map](https://a.org/map).  \nThen:\n\n- a tent\n- a stove\n\n> Go far\n\n\
             ![Hills](https://a.org/p.jpg)\n\n```\nlet x = *y;\n```\n\n2\\*3 & https://a.org"
        );
    }

    #[test]
    fn excerpt_is_plain() {
        assert_eq!(to_text("## Hi\n\nSee [the *map*](https://a.org) & https://b.org\n\n- one"), "Hi See the map & b.org one");
    }

    #[test]
    fn first_picture_is_found() {
        assert_eq!(first_picture("```\n![x](https://a.org/no.png)\n```\nSee ![y](https://a.org/yes.png)"), Some("https://a.org/yes.png".to_string()));
        assert_eq!(first_picture("![x](javascript:1) and `![z](https://a.org/z.png)`"), Some("https://a.org/z.png".to_string()));
        assert_eq!(first_picture("No picture"), None);
    }

    #[test]
    fn reading_time() {
        assert_eq!(reading_minutes("word "), 1);
        assert_eq!(reading_minutes(&"word ".repeat(401)), 3);
    }
}

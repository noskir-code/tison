//! Translations of the interface: English, French and Spanish for now.
//!
//! The English text is the key, like gettext: templates write
//! `{{ "Home"|t }}`, Rust code `i18n::t("Home")`. The translations are in
//! `locales/<language>.po` (gettext's format, editable with Poedit), built
//! into the program. A text without a translation stays in English.
//!
//! The language of a request (see `middleware`): the `lang` cookie set by
//! the picker of the menu, else the browser's preferred languages, else
//! English. It's kept for the request's task, so templates and the code
//! they call read it without passing it around.

use std::{collections::HashMap, fmt::Display, sync::OnceLock};

use axum::{
    extract::Request,
    http::{HeaderMap, header},
    middleware::Next,
    response::Response,
};

/// The languages of the interface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lang {
    #[default]
    En,
    Fr,
    Es,
}

impl Lang {
    pub const ALL: [Self; 3] = [Self::En, Self::Fr, Self::Es];

    /// Like `fr`: in the cookie, `<html lang>` and the picker.
    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Fr => "fr",
            Self::Es => "es",
        }
    }

    /// The language's own name, as the picker shows it.
    pub fn name(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::Fr => "Français",
            Self::Es => "Español",
        }
    }

    /// `fr`, `fr-CA`, `FR`... `None` when not offered.
    pub fn parse(value: &str) -> Option<Self> {
        let primary = value.trim().split(['-', '_']).next()?.to_ascii_lowercase();
        Self::ALL.into_iter().find(|lang| lang.code() == primary)
    }

    fn catalog(self) -> Option<&'static HashMap<String, String>> {
        static FR: OnceLock<HashMap<String, String>> = OnceLock::new();
        static ES: OnceLock<HashMap<String, String>> = OnceLock::new();
        match self {
            Self::En => None,
            Self::Fr => Some(FR.get_or_init(|| parse_po(include_str!("../locales/fr.po")))),
            Self::Es => Some(ES.get_or_init(|| parse_po(include_str!("../locales/es.po")))),
        }
    }
}

tokio::task_local! {
    static LANG: Lang;
}

/// The language of the request being answered, English outside of one.
pub fn current() -> Lang {
    LANG.try_with(|lang| *lang).unwrap_or_default()
}

/// `tokio::spawn` keeping the language of the request: a spawned task starts
/// without it. For what's sent to the reader later, like the emails.
pub fn spawn<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(LANG.scope(current(), future))
}

/// Runs `future` as if answering a request in `lang`, for the tests of other modules.
#[cfg(test)]
pub async fn scoped<F: Future>(lang: Lang, future: F) -> F::Output {
    LANG.scope(lang, future).await
}

/// `text` in the language of the request, or as it is.
pub fn t(text: &str) -> String {
    current()
        .catalog()
        .and_then(|catalog| catalog.get(text))
        .cloned()
        .unwrap_or_else(|| text.to_string())
}

/// `t`, then `{}` replaced by `value`: `tf("{} views", 3)`.
pub fn tf(text: &str, value: impl Display) -> String {
    t(text).replacen("{}", &value.to_string(), 1)
}

/// `{}` replaced in order: `tf2("{} of {}", a, b)`.
pub fn tf2(text: &str, first: impl Display, second: impl Display) -> String {
    t(text).replacen("{}", &first.to_string(), 1).replacen("{}", &second.to_string(), 1)
}

// --- Dates, in each language's usual order ---

const MONTHS: [[&str; 12]; 3] = [
    ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"],
    ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"],
    ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre"],
];
const MONTHS_SHORT: [[&str; 12]; 3] = [
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"],
    ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."],
    ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sept", "oct", "nov", "dic"],
];
const WEEKDAYS_SHORT: [[&str; 7]; 3] = [
    ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
    ["lun.", "mar.", "mer.", "jeu.", "ven.", "sam.", "dim."],
    ["lun", "mar", "mié", "jue", "vie", "sáb", "dom"],
];

fn language_index() -> usize {
    match current() {
        Lang::En => 0,
        Lang::Fr => 1,
        Lang::Es => 2,
    }
}

/// Year, month (0-11), day and weekday (0 = Monday), read through
/// `format`: the chrono re-exported by sqlx doesn't bring `Datelike`.
fn parts(date: sqlx::types::chrono::NaiveDate) -> (i32, usize, u32, usize) {
    let text = date.format("%Y %m %d %u").to_string();
    let mut numbers = text.split(' ').map(|part| part.parse::<i32>().unwrap_or(1));
    let year = numbers.next().unwrap_or(1970);
    let month = numbers.next().unwrap_or(1).clamp(1, 12) as usize - 1;
    let day = numbers.next().unwrap_or(1) as u32;
    let weekday = numbers.next().unwrap_or(1).clamp(1, 7) as usize - 1;
    (year, month, day, weekday)
}

/// "Nov 1, 2026", "1 nov. 2026", "1 nov 2026".
pub fn date(date: sqlx::types::chrono::NaiveDate) -> String {
    let (year, month, day, _) = parts(date);
    let language = language_index();
    let name = MONTHS_SHORT[language][month];
    match current() {
        Lang::En => format!("{name} {day}, {year}"),
        _ => format!("{day} {name} {year}"),
    }
}

/// "November 1, 2026", "1 novembre 2026", "1 de noviembre de 2026".
pub fn long_date(date: sqlx::types::chrono::NaiveDate) -> String {
    let (year, month, day, _) = parts(date);
    let name = MONTHS[language_index()][month];
    match current() {
        Lang::En => format!("{name} {day}, {year}"),
        Lang::Fr => format!("{day} {name} {year}"),
        Lang::Es => format!("{day} de {name} de {year}"),
    }
}

/// "November 2026", "novembre 2026", "noviembre de 2026".
pub fn month_year(date: sqlx::types::chrono::NaiveDate) -> String {
    let (year, month, _, _) = parts(date);
    let name = MONTHS[language_index()][month];
    match current() {
        Lang::Es => format!("{name} de {year}"),
        _ => format!("{name} {year}"),
    }
}

/// "NOV", "NOV.", "NOV": the month on an event's card.
pub fn month_short_upper(date: sqlx::types::chrono::NaiveDate) -> String {
    MONTHS_SHORT[language_index()][parts(date).1].to_uppercase()
}

/// "Sun Nov 1, 2026 · 14:00 UTC", "dim. 1 nov. 2026 · 14:00 UTC".
pub fn weekday_date_time(time: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>) -> String {
    let (_, _, _, weekday) = parts(time.date_naive());
    format!("{} {} · {} UTC", WEEKDAYS_SHORT[language_index()][weekday], date(time.date_naive()), time.format("%H:%M"))
}

/// "Nov 1, 2026 · 14:00 UTC".
pub fn date_time(time: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>) -> String {
    format!("{} · {} UTC", date(time.date_naive()), time.format("%H:%M"))
}

/// The texts of `assets/system.js`, translated for the page: the page puts
/// them in a JSON block read by `t()` there. Only those translated.
pub fn js_texts() -> String {
    let Some(catalog) = current().catalog() else {
        return "{}".to_string();
    };
    let texts: serde_json::Map<String, serde_json::Value> = JS_TEXTS
        .iter()
        .filter_map(|text| catalog.get(*text).map(|translated| (text.to_string(), serde_json::Value::from(translated.as_str()))))
        .collect();
    // Closing tags can't end the <script> early.
    serde_json::Value::Object(texts).to_string().replace("</", "<\\/")
}

/// Every text `assets/system.js` shows, as written there in `t("...")`.
pub const JS_TEXTS: &[&str] = &include!("../locales/js_texts.in");

/// Reads gettext's `.po`: `msgid "..."` then `msgstr "..."`, each on one or
/// more quoted lines, with `\"`, `\\`, `\n` and `\t`. Comments (`#`) and
/// empty translations are skipped.
fn parse_po(content: &str) -> HashMap<String, String> {
    let mut catalog = HashMap::new();
    let (mut id, mut text) = (String::new(), String::new());
    // What the quoted lines being read belong to.
    let mut reading: Option<bool> = None;
    let mut finish = |id: &mut String, text: &mut String| {
        if !id.is_empty() && !text.is_empty() {
            catalog.insert(std::mem::take(id), std::mem::take(text));
        }
        id.clear();
        text.clear();
    };
    for line in content.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("msgid ") {
            finish(&mut id, &mut text);
            id = unquote(rest);
            reading = Some(true);
        } else if let Some(rest) = line.strip_prefix("msgstr ") {
            text = unquote(rest);
            reading = Some(false);
        } else if line.starts_with('"') {
            match reading {
                Some(true) => id.push_str(&unquote(line)),
                Some(false) => text.push_str(&unquote(line)),
                None => {}
            }
        } else if line.is_empty() || line.starts_with('#') {
            reading = None;
        }
    }
    finish(&mut id, &mut text);
    catalog
}

fn unquote(quoted: &str) -> String {
    let inner = quoted.trim().strip_prefix('"').and_then(|rest| rest.strip_suffix('"')).unwrap_or_default();
    let mut text = String::with_capacity(inner.len());
    let mut letters = inner.chars();
    while let Some(letter) = letters.next() {
        if letter != '\\' {
            text.push(letter);
            continue;
        }
        match letters.next() {
            Some('n') => text.push('\n'),
            Some('t') => text.push('\t'),
            Some(other) => text.push(other),
            None => {}
        }
    }
    text
}

/// The language of a request: the `lang` cookie, else `Accept-Language`
/// (the first offered, by preference), else English.
pub fn from_headers(headers: &HeaderMap) -> Lang {
    let cookie = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().strip_prefix("lang="))
        .find_map(Lang::parse);
    if let Some(lang) = cookie {
        return lang;
    }
    let accepted = headers.get(header::ACCEPT_LANGUAGE).and_then(|value| value.to_str().ok()).unwrap_or_default();
    let mut ranked: Vec<(f32, Lang)> = accepted
        .split(',')
        .filter_map(|part| {
            let mut pieces = part.split(';');
            let lang = Lang::parse(pieces.next()?)?;
            let quality = pieces
                .find_map(|piece| piece.trim().strip_prefix("q="))
                .and_then(|q| q.parse().ok())
                .unwrap_or(1.0);
            Some((quality, lang))
        })
        .collect();
    // Stable: equal preferences keep the browser's order.
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
    ranked.first().map(|(_, lang)| *lang).unwrap_or_default()
}

/// Answers the request in its language, see `from_headers`.
pub async fn middleware(request: Request, next: Next) -> Response {
    let lang = from_headers(request.headers());
    LANG.scope(lang, next.run(request)).await
}

/// Askama filters: `{{ "Home"|t }}`, `{{ "{} views"|tf(count) }}`.
pub mod filters {
    use std::fmt::Display;

    #[askama::filter_fn]
    pub fn t(value: impl Display, _: &dyn askama::Values) -> askama::Result<String> {
        Ok(super::t(&value.to_string()))
    }

    #[askama::filter_fn]
    pub fn tf<A: Display>(value: impl Display, _: &dyn askama::Values, argument: A) -> askama::Result<String> {
        Ok(super::tf(&value.to_string(), argument))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn po_files_are_read() {
        let catalog = parse_po(
            "# A comment\nmsgid \"\"\nmsgstr \"Header\"\n\nmsgid \"Home\"\nmsgstr \"Accueil\"\n\n\
             msgid \"Say \\\"hi\\\"\"\nmsgstr \"\"\n\"Dire \"\n\"\\\"salut\\\"\"\n\nmsgid \"Not yet\"\nmsgstr \"\"\n",
        );
        assert_eq!(catalog.get("Home").map(String::as_str), Some("Accueil"));
        assert_eq!(catalog.get("Say \"hi\"").map(String::as_str), Some("Dire \"salut\""));
        assert!(!catalog.contains_key("Not yet"));
        assert!(!catalog.contains_key(""));
    }

    #[test]
    fn the_language_comes_from_the_cookie_then_the_browser() {
        let headers = |pairs: &[(header::HeaderName, &str)]| {
            let mut map = HeaderMap::new();
            for (name, value) in pairs {
                map.append(name.clone(), value.parse().unwrap());
            }
            map
        };
        assert_eq!(from_headers(&headers(&[])), Lang::En);
        assert_eq!(from_headers(&headers(&[(header::ACCEPT_LANGUAGE, "fr-CA,fr;q=0.9,en;q=0.8")])), Lang::Fr);
        assert_eq!(from_headers(&headers(&[(header::ACCEPT_LANGUAGE, "de,es;q=0.5,en;q=0.7")])), Lang::En);
        assert_eq!(from_headers(&headers(&[(header::ACCEPT_LANGUAGE, "de,es;q=0.8")])), Lang::Es);
        assert_eq!(
            from_headers(&headers(&[(header::ACCEPT_LANGUAGE, "fr"), (header::COOKIE, "a=1; lang=es")])),
            Lang::Es
        );
        assert_eq!(from_headers(&headers(&[(header::COOKIE, "lang=xx")])), Lang::En);
    }

    #[tokio::test]
    async fn dates_follow_the_language() {
        let day = sqlx::types::chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
        assert_eq!(date(day), "Nov 1, 2026");
        assert_eq!(long_date(day), "November 1, 2026");
        LANG.scope(Lang::Fr, async move {
            assert_eq!(date(day), "1 nov. 2026");
            assert_eq!(month_year(day), "novembre 2026");
            assert_eq!(month_short_upper(day), "NOV.");
        })
        .await;
        LANG.scope(Lang::Es, async move {
            assert_eq!(long_date(day), "1 de noviembre de 2026");
            assert_eq!(month_year(day), "noviembre de 2026");
        })
        .await;
        let time = day.and_hms_opt(14, 5, 0).unwrap().and_utc();
        // November 1, 2026 is a Sunday.
        assert_eq!(weekday_date_time(time), "Sun Nov 1, 2026 · 14:05 UTC");
        LANG.scope(Lang::Fr, async move { assert_eq!(weekday_date_time(time), "dim. 1 nov. 2026 · 14:05 UTC") }).await;
    }

    #[tokio::test]
    async fn texts_follow_the_request() {
        assert_eq!(t("Home"), "Home");
        LANG.scope(Lang::Fr, async { assert_ne!(t("Home"), "Home") }).await;
        LANG.scope(Lang::Fr, async { assert_eq!(t("No such text, never"), "No such text, never") }).await;
    }

    #[tokio::test]
    async fn spawned_tasks_keep_the_language() {
        let lang = LANG.scope(Lang::Es, async { spawn(async { current() }).await }).await.unwrap();
        assert_eq!(lang, Lang::Es);
        assert_eq!(spawn(async { current() }).await.unwrap(), Lang::En);
    }
}

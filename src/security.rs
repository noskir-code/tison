//! Protections around every request, written by hand (no crate):
//!
//! - `limit`: how often an address or an account can try something
//!   (signing in, two-factor codes, signing up, password resets, reports,
//!   messages), against password guessing and spam;
//! - `middleware`: the client's address (`ClientIp`), forms sent from
//!   other sites refused (cross-site request forgery), and the security
//!   headers of every answer: Content-Security-Policy, framing refused,
//!   `nosniff`, a referrer kept on the site, HSTS on https, and uploaded
//!   files that can never run as a page.

use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use axum::{
    extract::{ConnectInfo, Request},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

// --- Rate limits ---

/// What is limited, each with its own counters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Limit {
    /// Sign-in tries from one address.
    LoginAddress,
    /// Wrong passwords for one account, from anywhere: guessing spread over
    /// many addresses stops too.
    LoginAccount,
    /// Two-factor codes from one address.
    TwoFactor,
    /// Accounts made from one address.
    Register,
    /// Password reset emails asked from one address.
    PasswordReset,
    /// Reports sent by one user.
    Report,
    /// Messages sent by one user.
    Message,
    /// Moderation notices' appeals and other rare forms from one address.
    Appeal,
    /// Posts (and videos, events...) published by one user.
    Post,
    /// Comments written by one user.
    Comment,
    /// Posts, comments and messages received from one account of another
    /// server: past it, dropped (a flood).
    RemoteContent,
}

impl Limit {
    /// How many in how long.
    fn rule(self) -> (u32, Duration) {
        const MINUTE: u64 = 60;
        match self {
            Self::LoginAddress => (20, Duration::from_secs(15 * MINUTE)),
            Self::LoginAccount => (10, Duration::from_secs(15 * MINUTE)),
            Self::TwoFactor => (15, Duration::from_secs(15 * MINUTE)),
            Self::Register => (5, Duration::from_secs(60 * MINUTE)),
            Self::PasswordReset => (5, Duration::from_secs(60 * MINUTE)),
            Self::Report => (20, Duration::from_secs(60 * MINUTE)),
            Self::Message => (60, Duration::from_secs(10 * MINUTE)),
            Self::Appeal => (10, Duration::from_secs(60 * MINUTE)),
            Self::Post => (30, Duration::from_secs(60 * MINUTE)),
            Self::Comment => (60, Duration::from_secs(60 * MINUTE)),
            Self::RemoteContent => (100, Duration::from_secs(10 * MINUTE)),
        }
    }

    /// Minutes before trying again.
    pub fn minutes(self) -> u64 {
        self.rule().1.as_secs().div_ceil(60)
    }

    /// What the person reads (translated by the page).
    pub fn message(self) -> &'static str {
        if self.minutes() >= 60 { "Too many tries: wait an hour and try again." } else { "Too many tries: wait 15 minutes and try again." }
    }
}

type Counters = HashMap<(Limit, String), (Instant, u32)>;

fn counters() -> &'static Mutex<Counters> {
    static COUNTERS: OnceLock<Mutex<Counters>> = OnceLock::new();
    COUNTERS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Above this many keys, the expired ones are dropped.
const MAX_KEYS: usize = 20_000;

/// Counts one try of `key` (an address, an account) and tells whether it's
/// allowed: `false` once the limit is reached, until its window ends.
pub fn hit(limit: Limit, key: &str) -> bool {
    let (max, window) = limit.rule();
    let now = Instant::now();
    let mut counters = counters().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if counters.len() > MAX_KEYS {
        counters.retain(|(limit, _), (start, _)| now.duration_since(*start) < limit.rule().1);
    }
    let entry = counters.entry((limit, key.to_lowercase())).or_insert((now, 0));
    if now.duration_since(entry.0) >= window {
        *entry = (now, 0);
    }
    if entry.1 >= max {
        return false;
    }
    entry.1 += 1;
    true
}

/// Whether `key` already reached the limit, without counting a try: for
/// limits counting only failures (`LoginAccount`).
pub fn blocked(limit: Limit, key: &str) -> bool {
    let (max, window) = limit.rule();
    let counters = counters().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    counters
        .get(&(limit, key.to_lowercase()))
        .is_some_and(|(start, count)| start.elapsed() < window && *count >= max)
}

/// Forgets the tries of `key`: a right password ends the count of wrong ones.
pub fn clear(limit: Limit, key: &str) {
    let mut counters = counters().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    counters.remove(&(limit, key.to_lowercase()));
}

// --- The client's address ---

/// The address of whoever sent the request, put in the request by
/// `middleware`: `Extension<ClientIp>` in handlers.
#[derive(Clone, Debug)]
pub struct ClientIp(pub String);

/// An address or a range of them (`10.0.0.0/8`, `2001:db8::/32`), for the
/// trusted proxies (`TRUSTED_PROXIES` in runtime.conf).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Network {
    address: IpAddr,
    prefix: u8,
}

impl Network {
    pub fn contains(&self, ip: IpAddr) -> bool {
        // `::ffff:10.0.0.1` is the IPv4 address 10.0.0.1.
        let ip = match ip {
            IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
            v4 => v4,
        };
        match (self.address, ip) {
            (IpAddr::V4(network), IpAddr::V4(ip)) => {
                let mask = u32::MAX.checked_shl(32 - u32::from(self.prefix)).unwrap_or(0);
                u32::from(network) & mask == u32::from(ip) & mask
            }
            (IpAddr::V6(network), IpAddr::V6(ip)) => {
                let mask = u128::MAX.checked_shl(128 - u32::from(self.prefix)).unwrap_or(0);
                u128::from(network) & mask == u128::from(ip) & mask
            }
            _ => false,
        }
    }
}

/// `TRUSTED_PROXIES`: addresses and ranges separated by commas, and the words
/// `loopback` (this machine), `private` (local networks: Docker, LAN) and
/// `none`. Proxies are believed when they say who the visitor is.
pub fn parse_proxies(raw: &str) -> Result<Vec<Network>, String> {
    let network = |address: &str, prefix: u8| Network { address: address.parse().expect("valid built-in range"), prefix };
    let mut networks = Vec::new();
    for entry in raw.split(',').map(str::trim).filter(|entry| !entry.is_empty()) {
        match entry.to_ascii_lowercase().as_str() {
            "none" => {}
            "loopback" => networks.extend([network("127.0.0.0", 8), network("::1", 128)]),
            "private" => networks.extend([network("10.0.0.0", 8), network("172.16.0.0", 12), network("192.168.0.0", 16), network("fc00::", 7)]),
            _ => {
                let (address, prefix) = match entry.split_once('/') {
                    Some((address, prefix)) => (address, Some(prefix)),
                    None => (entry, None),
                };
                let address: IpAddr = address.parse().map_err(|_| format!("TRUSTED-PROXIES: '{entry}' is not an address or a range"))?;
                let max = if address.is_ipv4() { 32 } else { 128 };
                let prefix = match prefix {
                    Some(prefix) => prefix.parse::<u8>().ok().filter(|prefix| *prefix <= max).ok_or(format!("TRUSTED-PROXIES: '{entry}' has an invalid range"))?,
                    None => max,
                };
                networks.push(Network { address, prefix });
            }
        }
    }
    Ok(networks)
}

/// An address of `X-Forwarded-For` or `Forwarded` (`for=`): with or without
/// quotes, brackets and port (`"[2001:db8::1]:4711"`, `203.0.113.5:80`).
fn parse_forwarded_address(raw: &str) -> Option<IpAddr> {
    let raw = raw.trim().trim_matches('"');
    if let Ok(ip) = raw.parse() {
        return Some(ip);
    }
    if let Some(rest) = raw.strip_prefix('[') {
        return rest.split_once(']').and_then(|(ip, _)| ip.parse().ok());
    }
    raw.rsplit_once(':').filter(|(ip, _)| !ip.contains(':')).and_then(|(ip, _)| ip.parse().ok())
}

/// The addresses the proxies added, oldest first: every `X-Forwarded-For`
/// line, else the `for=` of `Forwarded` (RFC 7239). `None` when one can't be
/// read: then nothing of it is believed.
fn forwarded_chain(headers: &HeaderMap) -> Option<Vec<IpAddr>> {
    let x_forwarded: Vec<&str> = headers.get_all("x-forwarded-for").iter().filter_map(|value| value.to_str().ok()).collect();
    let entries: Vec<String> = if !x_forwarded.is_empty() {
        x_forwarded.iter().flat_map(|line| line.split(',')).map(str::to_string).collect()
    } else {
        headers
            .get_all(header::FORWARDED)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|line| line.split(','))
            .filter_map(|element| {
                element.split(';').find_map(|pair| {
                    let (name, value) = pair.split_once('=')?;
                    name.trim().eq_ignore_ascii_case("for").then(|| value.to_string())
                })
            })
            .collect()
    };
    if entries.is_empty() {
        return None;
    }
    entries.iter().map(|entry| parse_forwarded_address(entry)).collect()
}

/// The visitor's address. A trusted proxy (`TRUSTED_PROXIES`) says it in
/// `X-Forwarded-For`; through several (a CDN, then nginx) the chain is read
/// from the end, skipping the trusted ones: the first other address is the
/// visitor. Anyone else can't be believed, and is the visitor.
fn client_ip(headers: &HeaderMap, peer: Option<IpAddr>, trusted: &[Network]) -> String {
    let is_trusted = |ip: IpAddr| trusted.iter().any(|network| network.contains(ip));
    let Some(peer) = peer else {
        return "unknown".to_string();
    };
    if !is_trusted(peer) {
        return peer.to_string();
    }
    let Some(chain) = forwarded_chain(headers) else {
        return peer.to_string();
    };
    chain
        .iter()
        .rev()
        .find(|ip| !is_trusted(**ip))
        .or(chain.first())
        .map_or_else(|| peer.to_string(), IpAddr::to_string)
}

// --- Headers and forged forms ---

struct Site {
    /// `https://tison.social`: forms must come from it.
    origin: String,
    /// The proxies believed about the visitor's address, see `client_ip`.
    trusted: Vec<Network>,
    https: bool,
    policy: HeaderValue,
}

static SITE: OnceLock<Site> = OnceLock::new();

/// Set once at start, from `PUBLIC_BASE_URL` and `TRUSTED_PROXIES`.
pub fn install(public_base_url: &str, trusted: Vec<Network>) {
    let origin = public_base_url.trim_end_matches('/').to_string();
    let https = origin.starts_with("https://");
    let host = origin.split_once("://").map(|(_, host)| host).unwrap_or_default();
    // Scripts and styles of the site only (style attributes are used), media
    // of other servers through the proxy, the event map's frame, and the
    // live WebSockets.
    let policy = format!(
        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; \
         media-src 'self' data: blob:; font-src 'self'; connect-src 'self' wss://{host} ws://{host}; \
         frame-src https://www.openstreetmap.org; worker-src 'self'; manifest-src 'self'; object-src 'none'; \
         base-uri 'self'; form-action 'self'; frame-ancestors 'none'"
    );
    let _ = SITE.set(Site {
        origin,
        trusted,
        https,
        policy: HeaderValue::from_str(&policy).unwrap_or_else(|_| HeaderValue::from_static("default-src 'self'")),
    });
}

/// A form or `fetch` sent from another site with the visitor's cookies:
/// refused. Browsers say where a request comes from (`Sec-Fetch-Site`,
/// `Origin`); other Fediverse servers send neither, so their inbox posts
/// pass, and are checked by their signature instead.
fn forged(method: &Method, headers: &HeaderMap) -> bool {
    // A WebSocket opens with a GET, with the cookies: checked too, or a page
    // of another site could read the live notifications and messages.
    let websocket = headers.get(header::UPGRADE).and_then(|value| value.to_str().ok()).is_some_and(|value| value.eq_ignore_ascii_case("websocket"));
    if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) && !websocket {
        return false;
    }
    if let Some(site) = headers.get("sec-fetch-site").and_then(|value| value.to_str().ok()) {
        return site == "cross-site";
    }
    let Some(origin) = headers.get(header::ORIGIN).and_then(|value| value.to_str().ok()) else {
        return false;
    };
    let host = headers.get(header::HOST).and_then(|value| value.to_str().ok()).unwrap_or_default();
    let same_host = origin.split_once("://").is_some_and(|(_, origin_host)| origin_host.eq_ignore_ascii_case(host));
    let same_site = SITE.get().is_some_and(|site| origin.eq_ignore_ascii_case(&site.origin));
    !(same_host || same_site)
}

/// Extensions of uploads shown in the page; any other file is downloaded.
const SHOWN_UPLOADS: [&str; 12] = ["jpg", "jpeg", "png", "gif", "webp", "avif", "mp4", "webm", "mp3", "m4a", "ogg", "m3u8"];

/// Around every request, see the module's comment.
pub async fn middleware(mut request: Request, next: Next) -> Response {
    let peer = request.extensions().get::<ConnectInfo<SocketAddr>>().map(|ConnectInfo(address)| address.ip());
    let trusted = SITE.get().map(|site| site.trusted.as_slice()).unwrap_or_default();
    let ip = client_ip(request.headers(), peer, trusted);
    if forged(request.method(), request.headers()) {
        tracing::warn!("refused a {} {} sent from another site ({})", request.method(), request.uri().path(), ip);
        return (StatusCode::FORBIDDEN, "Forms can only be sent from this site.").into_response();
    }
    request.extensions_mut().insert(ClientIp(ip));
    let path = request.uri().path().to_string();
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(header::REFERRER_POLICY, HeaderValue::from_static("strict-origin-when-cross-origin"));
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(self), microphone=(self), geolocation=(), payment=(), usb=()"),
    );
    headers.insert("cross-origin-opener-policy", HeaderValue::from_static("same-origin"));
    if let Some(site) = SITE.get() {
        if site.https {
            headers.insert(header::STRICT_TRANSPORT_SECURITY, HeaderValue::from_static("max-age=31536000"));
        }
        // An answer with its own policy (the media proxy's) keeps it.
        if !headers.contains_key(header::CONTENT_SECURITY_POLICY) {
            headers.insert(header::CONTENT_SECURITY_POLICY, site.policy.clone());
        }
    }
    // What people uploaded never runs as a page of the site, and what isn't
    // a picture, a video or a sound is downloaded rather than shown.
    if path.starts_with("/public/uploads/") {
        headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("default-src 'none'; style-src 'unsafe-inline'; sandbox"));
        let extension = path.rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase()).unwrap_or_default();
        if !SHOWN_UPLOADS.contains(&extension.as_str()) {
            headers.insert(header::CONTENT_DISPOSITION, HeaderValue::from_static("attachment"));
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_stop_then_clear() {
        let key = "test-limits-stop-then-clear";
        for _ in 0..10 {
            assert!(hit(Limit::LoginAccount, key));
        }
        assert!(!hit(Limit::LoginAccount, key));
        assert!(blocked(Limit::LoginAccount, "TEST-limits-stop-then-clear"));
        clear(Limit::LoginAccount, key);
        assert!(!blocked(Limit::LoginAccount, key));
    }

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(*name, HeaderValue::from_static(value));
        }
        map
    }

    #[test]
    fn proxies_are_parsed() {
        let trusted = parse_proxies("loopback, 203.0.113.0/24, 2001:db8::/32").unwrap();
        assert!(trusted.iter().any(|n| n.contains("127.0.0.1".parse().unwrap())));
        assert!(trusted.iter().any(|n| n.contains("203.0.113.77".parse().unwrap())));
        assert!(trusted.iter().any(|n| n.contains("2001:db8::5".parse().unwrap())));
        assert!(trusted.iter().any(|n| n.contains("::ffff:127.0.0.1".parse().unwrap())));
        assert!(!trusted.iter().any(|n| n.contains("10.0.0.1".parse().unwrap())));
        assert!(parse_proxies("none").unwrap().is_empty());
        assert!(parse_proxies("0.0.0.0/0").unwrap()[0].contains("8.8.8.8".parse().unwrap()));
        assert!(parse_proxies("10.0.0.0/40").is_err());
        assert!(parse_proxies("proxy.example").is_err());
    }

    #[test]
    fn the_visitor_is_found_through_trusted_proxies_only() {
        let local = parse_proxies("loopback").unwrap();
        let forwarded = headers(&[("x-forwarded-for", "6.6.6.6, 203.0.113.9")]);
        assert_eq!(client_ip(&forwarded, Some("127.0.0.1".parse().unwrap()), &local), "203.0.113.9");
        // A client writing the header itself isn't believed.
        assert_eq!(client_ip(&forwarded, Some("198.51.100.4".parse().unwrap()), &local), "198.51.100.4");
        assert_eq!(client_ip(&forwarded, Some("127.0.0.1".parse().unwrap()), &[]), "127.0.0.1", "none trusted");
        // A CDN then nginx: the CDN's range is trusted too.
        let chain = parse_proxies("loopback, 10.0.0.0/8").unwrap();
        let through_cdn = headers(&[("x-forwarded-for", "198.51.100.7, 10.0.0.2")]);
        assert_eq!(client_ip(&through_cdn, Some("127.0.0.1".parse().unwrap()), &chain), "198.51.100.7");
        let split = headers(&[("x-forwarded-for", "198.51.100.7"), ("x-forwarded-for", "10.0.0.2")]);
        assert_eq!(client_ip(&split, Some("127.0.0.1".parse().unwrap()), &chain), "198.51.100.7");
        // RFC 7239, with IPv6, quotes and a port.
        let standard = headers(&[("forwarded", "for=\"[2001:db8::1]:4711\";proto=https, for=10.0.0.2")]);
        assert_eq!(client_ip(&standard, Some("127.0.0.1".parse().unwrap()), &chain), "2001:db8::1");
        // Unreadable: nothing of it is believed.
        let broken = headers(&[("x-forwarded-for", "not-an-address")]);
        assert_eq!(client_ip(&broken, Some("127.0.0.1".parse().unwrap()), &chain), "127.0.0.1");
        assert_eq!(client_ip(&HeaderMap::new(), Some("127.0.0.1".parse().unwrap()), &chain), "127.0.0.1");
    }

    #[test]
    fn forms_from_other_sites_are_refused() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("tison.example"));
        assert!(!forged(&Method::POST, &headers), "servers send no Origin");
        headers.insert(header::ORIGIN, HeaderValue::from_static("https://tison.example"));
        assert!(!forged(&Method::POST, &headers));
        headers.insert(header::ORIGIN, HeaderValue::from_static("https://evil.example"));
        assert!(forged(&Method::POST, &headers));
        assert!(!forged(&Method::GET, &headers));
        headers.insert("sec-fetch-site", HeaderValue::from_static("same-origin"));
        assert!(!forged(&Method::POST, &headers));
        headers.insert("sec-fetch-site", HeaderValue::from_static("cross-site"));
        assert!(forged(&Method::POST, &headers));
        assert!(!forged(&Method::GET, &headers));
        headers.insert(header::UPGRADE, HeaderValue::from_static("websocket"));
        assert!(forged(&Method::GET, &headers), "a WebSocket opened by another site");
    }
}

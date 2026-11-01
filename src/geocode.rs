//! Where the address of an event is, for its map: asked to OpenStreetMap's
//! Nominatim (`GEOCODER` in `configs/runtime.conf`, `off` to never ask),
//! like Mobilizon does. At most one question a second, as its rules ask;
//! the site is named in the User-Agent (see `API::linker::client`).

use std::time::Duration;

use tokio::{sync::Mutex, time::Instant};

use crate::{API::Web, entities::event};

pub const DEFAULT_GEOCODER: &str = "https://nominatim.openstreetmap.org/search";

/// When the last question was asked.
static LAST_ASKED: Mutex<Option<Instant>> = Mutex::const_new(None);

/// Where `address` is, `None` when nothing was found.
pub async fn find(web: &Web, geocoder: &str, address: &str) -> Result<Option<(f64, f64)>, String> {
    if address.trim().is_empty() {
        return Ok(None);
    }
    coordinates(web, geocoder, address).await
}

async fn coordinates(web: &Web, geocoder: &str, address: &str) -> Result<Option<(f64, f64)>, String> {
    {
        let mut last = LAST_ASKED.lock().await;
        if let Some(last) = *last {
            let wait = Duration::from_millis(1100).saturating_sub(last.elapsed());
            tokio::time::sleep(wait).await;
        }
        *last = Some(Instant::now());
    }
    let separator = if geocoder.contains('?') { '&' } else { '?' };
    let url = format!("{geocoder}{separator}format=jsonv2&limit=1&q={}", event::url_encode(address));
    let response = web
        .pages
        .get(url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|err| format!("request failed: {err}"))?;
    if !response.status().is_success() {
        return Err(format!("answered {}", response.status()));
    }
    let body = response.text().await.map_err(|err| format!("failed to read: {err}"))?;
    Ok(parse(&body))
}

/// The first place of Nominatim's answer: `[{"lat": "48.85", "lon": "2.29", ...}]`.
fn parse(body: &str) -> Option<(f64, f64)> {
    let places: serde_json::Value = serde_json::from_str(body).ok()?;
    let place = places.get(0)?;
    let number = |name: &str| place[name].as_str().and_then(|value| value.parse::<f64>().ok()).or_else(|| place[name].as_f64());
    let (latitude, longitude) = (number("lat")?, number("lon")?);
    ((-90.0..=90.0).contains(&latitude) && (-180.0..=180.0).contains(&longitude)).then_some((latitude, longitude))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nominatim_answers_are_read() {
        assert_eq!(parse(r#"[{"lat": "48.8796", "lon": "2.3093", "display_name": "Parc Monceau"}]"#), Some((48.8796, 2.3093)));
        assert_eq!(parse("[]"), None);
        assert_eq!(parse(r#"[{"lat": "123", "lon": "2"}]"#), None);
        assert_eq!(parse("not json"), None);
    }
}

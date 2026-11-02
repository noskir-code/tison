//! The creator dashboard: how the user's videos, posts with pictures, other
//! posts and followers did, per day over a period, and the same for each of
//! their posts. The charts are drawn by `setupCharts` in assets/system.js
//! from the numbers in the page, which also lists them in a table.

use std::collections::HashMap;

use askama::Template;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use tower_sessions::Session;

use crate::{
    AppState,
    entities::{
        creator_stats::{self as stats, ContentRow, Metric, Scope, Series},
        views,
    },
    handlers::{
        system::{Layout, error_page, render_template},
        user::signed_in_user,
    },
};
/// `|t` and `|tf` in the templates, see `i18n`.
use crate::i18n::filters;

/// The periods offered, in days: the first row of the dashboard, then "All time".
const PERIODS: [i32; 4] = [7, 30, 90, 365];
const DEFAULT_DAYS: i32 = 30;

/// What the dashboard covers: the last days, or everything since the account
/// (a post's page: the post) was created.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Period {
    Days(i32),
    AllTime,
}

impl Period {
    /// `?days=30`, or `?days=all`. Anything else is the last 30 days.
    fn from_query(query: &HashMap<String, String>) -> Self {
        match query.get("days").map(String::as_str) {
            Some("all") => Self::AllTime,
            Some(days) => Self::Days(days.parse().ok().filter(|days| PERIODS.contains(days)).unwrap_or(DEFAULT_DAYS)),
            None => Self::Days(DEFAULT_DAYS),
        }
    }

    /// Every choice, in the order of the buttons.
    fn all() -> impl Iterator<Item = Self> {
        PERIODS.into_iter().map(Self::Days).chain([Self::AllTime])
    }

    fn slug(self) -> String {
        match self {
            Self::Days(days) => days.to_string(),
            Self::AllTime => "all".to_string(),
        }
    }

    fn label(self) -> String {
        match self {
            Self::Days(days) => crate::i18n::tf("{} days", days),
            Self::AllTime => crate::i18n::t("All time"),
        }
    }

    /// How many days, today included: for all time, since `start`.
    fn days(self, start: sqlx::types::chrono::DateTime<sqlx::types::chrono::Utc>) -> i32 {
        match self {
            Self::Days(days) => days,
            Self::AllTime => {
                let since = sqlx::types::chrono::Utc::now().date_naive() - start.date_naive();
                (since.num_days() + 1).clamp(1, i64::from(i32::MAX)) as i32
            }
        }
    }
}

/// The buttons of the periods, `url` given each one's `?days=` value.
fn period_links(current: Period, url: impl Fn(&str) -> String) -> Vec<Link> {
    Period::all()
        .map(|period| Link {
            label: period.label(),
            url: url(&period.slug()),
            active: period == current,
        })
        .collect()
}

/// One number of the period.
pub struct Tile {
    pub label: &'static str,
    pub value: i64,
}

impl Tile {
    pub fn value_label(&self) -> String {
        compact(self.value)
    }
}

/// `1,284`, `12.9K`, `4.2M`.
fn compact(value: i64) -> String {
    match value {
        ..10_000 => {
            let digits = value.to_string();
            let mut grouped = String::new();
            for (index, digit) in digits.chars().enumerate() {
                if index > 0 && (digits.len() - index) % 3 == 0 && digit.is_ascii_digit() {
                    grouped.push(',');
                }
                grouped.push(digit);
            }
            grouped
        }
        ..1_000_000 => format!("{:.1}K", value as f64 / 1_000.0).replace(".0K", "K"),
        _ => format!("{:.1}M", value as f64 / 1_000_000.0).replace(".0M", "M"),
    }
}

/// Past this many days, a chart has a point per week: a point per day of a
/// year is a fence of spikes. Past `MONTHLY_AFTER_DAYS` (all time of an old
/// account), a point per calendar month.
const WEEKLY_AFTER_DAYS: usize = 31;
const MONTHLY_AFTER_DAYS: usize = 730;

/// What a point of a chart stands for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Grouping {
    Day,
    Week,
    Month,
}

type Day = (sqlx::types::chrono::NaiveDate, i64);

/// A line chart: one measure per day, week or month.
pub struct Chart {
    pub id: String,
    pub title: &'static str,
    /// After the number in the tooltip: "views", "likes"...
    pub unit: &'static str,
    /// Each point: a day, or the first day of its week or month.
    pub points: Series,
    pub grouping: Grouping,
}

impl Chart {
    /// `daily` grouped by weeks or months when the period is long. Weeks end
    /// today, the oldest may be shorter; months are calendar months. `running`
    /// totals (followers) keep the last value of each, counts are added up.
    fn new(id: &str, title: &'static str, unit: &'static str, daily: &[Day], running: bool) -> Self {
        let grouping = match daily.len() {
            length if length > MONTHLY_AFTER_DAYS => Grouping::Month,
            length if length > WEEKLY_AFTER_DAYS => Grouping::Week,
            _ => Grouping::Day,
        };
        let groups: Vec<&[Day]> = match grouping {
            Grouping::Day => daily.chunks(1).collect(),
            Grouping::Week => {
                let first_week = daily.len() % 7;
                let mut weeks: Vec<&[Day]> = Vec::new();
                if first_week > 0 {
                    weeks.push(&daily[..first_week]);
                }
                weeks.extend(daily[first_week..].chunks(7));
                weeks
            }
            Grouping::Month => daily.chunk_by(|a, b| a.0.format("%Y-%m").to_string() == b.0.format("%Y-%m").to_string()).collect(),
        };
        let points = groups
            .into_iter()
            .map(|group| {
                let value = if running { group.last().map_or(0, |(_, count)| *count) } else { group.iter().map(|(_, count)| count).sum() };
                (group[0].0, value)
            })
            .collect();
        Self { id: id.to_string(), title, unit, points, grouping }
    }

    /// `day`, `week` or `month`.
    fn grouping_name(&self) -> &'static str {
        match self.grouping {
            Grouping::Day => "day",
            Grouping::Week => "week",
            Grouping::Month => "month",
        }
    }

    /// Read by `setupCharts`: `{"unit": ..., "per": "day", "points": [["2026-11-01", 3], ...]}`.
    pub fn data(&self) -> String {
        let points: Vec<_> = self.points.iter().map(|(day, count)| json!([day.to_string(), count])).collect();
        json!({ "unit": self.unit, "per": self.grouping_name(), "points": points }).to_string()
    }

    pub fn per(&self) -> String {
        format!("per {}", self.grouping_name())
    }

    /// For the table's first column.
    pub fn period_name(&self) -> &'static str {
        match self.grouping {
            Grouping::Day => "Day",
            Grouping::Week => "Week",
            Grouping::Month => "Month",
        }
    }

    pub fn day_label(&self, index: &usize) -> String {
        let Some((day, _)) = self.points.get(*index) else {
            return String::new();
        };
        match self.grouping {
            Grouping::Day => crate::i18n::date(*day),
            Grouping::Week => crate::i18n::tf("Week of {}", crate::i18n::date(*day)),
            Grouping::Month => crate::i18n::month_year(*day),
        }
    }
}

/// A tab of the dashboard.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Videos,
    ImagePosts,
    Posts,
    Followers,
}

impl Tab {
    const ALL: [Self; 4] = [Self::Videos, Self::ImagePosts, Self::Posts, Self::Followers];

    fn slug(self) -> &'static str {
        match self {
            Self::Videos => "videos",
            Self::ImagePosts => "pictures",
            Self::Posts => "posts",
            Self::Followers => "followers",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Videos => "Videos",
            Self::ImagePosts => "Picture posts",
            Self::Posts => "Posts",
            Self::Followers => "Followers",
        }
    }

    /// Videos with `VIDEO`, picture posts with `IMAGESHARING`.
    fn shown(self) -> bool {
        let services = crate::settings::services();
        match self {
            Self::Videos => services.video,
            Self::ImagePosts => services.image_sharing,
            _ => true,
        }
    }

    fn scope(self) -> Option<Scope> {
        match self {
            Self::Videos => Some(Scope::Videos),
            Self::ImagePosts => Some(Scope::ImagePosts),
            Self::Posts => Some(Scope::Posts),
            Self::Followers => None,
        }
    }

    /// The first shown, for a missing or unknown `?tab=`.
    fn from_query(value: Option<&String>) -> Self {
        let shown = Self::ALL.into_iter().filter(|tab| tab.shown());
        value
            .and_then(|slug| Self::ALL.into_iter().find(|tab| tab.slug() == slug && tab.shown()))
            .or_else(|| shown.clone().next())
            .unwrap_or(Self::Followers)
    }
}

pub struct Link {
    pub label: String,
    pub url: String,
    pub active: bool,
}

/// What is measured: views only for videos; shares, rare for videos, not on
/// the Videos tab (a video's own page has them).
fn metrics(scope: Scope, is_video: bool) -> Vec<(Metric, &'static str, &'static str, &'static str)> {
    let mut metrics = Vec::new();
    if is_video {
        metrics.push((Metric::Views, "Views", "views", "views"));
    }
    metrics.push((Metric::Likes, "Likes", "likes", "likes"));
    metrics.push((Metric::Comments, "Comments", "comments", "comments"));
    if scope != Scope::Videos {
        metrics.push((Metric::Shares, "Shares", "shares", "shares"));
    }
    metrics
}

/// The chart of the last `days`, and its total as a tile.
async fn measured(
    pool: &sqlx::PgPool,
    user_id: i32,
    scope: Scope,
    (metric, title, unit, id): (Metric, &'static str, &'static str, &'static str),
    days: i32,
) -> Result<(Tile, Chart), sqlx::Error> {
    let daily = stats::daily(pool, user_id, scope, metric, days).await?;
    let value = daily.iter().map(|(_, count)| count).sum();
    Ok((Tile { label: title, value }, Chart::new(id, title, unit, &daily, false)))
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    title: String,
    layout: Layout,
    tabs: Vec<Link>,
    periods: Vec<Link>,
    /// `?days=` of the links: `30` or `all`.
    period: String,
    tiles: Vec<Tile>,
    charts: Vec<Chart>,
    /// The posts of the tab, `None` on the followers tab.
    content: Option<Vec<ContentRow>>,
    /// Views are shown for videos.
    videos: bool,
    tab_slug: &'static str,
}

/// `/dashboard?tab=videos&days=30`, or `days=all`.
pub async fn dashboard(State(state): State<AppState>, session: Session, Query(query): Query<HashMap<String, String>>) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let period = Period::from_query(&query);
    let days = period.days(me.created_at);
    let tab = Tab::from_query(query.get("tab"));
    let loaded = async {
        let mut tiles = Vec::new();
        let mut charts = Vec::new();
        let content = match tab.scope() {
            Some(scope) => {
                let (published, _) = measured(&pool, me.id, scope, (Metric::Published, "Published", "published", "published"), days).await?;
                tiles.push(published);
                for metric in metrics(scope, tab == Tab::Videos) {
                    let (tile, chart) = measured(&pool, me.id, scope, metric, days).await?;
                    tiles.push(tile);
                    charts.push(chart);
                }
                Some(stats::content(&pool, me.id, scope).await?)
            }
            None => {
                // One day more: the followers the day before the period starts.
                let series = stats::followers(&pool, me.id, days.saturating_add(1)).await?;
                let (before, now) = series.split_at(1);
                let at_start = before[0].1;
                let today = now.last().map_or(at_start, |(_, count)| *count);
                let (here, fediverse) = stats::follower_split(&pool, me.id).await?;
                tiles.push(Tile { label: "Followers", value: today });
                tiles.push(Tile { label: "New", value: today - at_start });
                tiles.push(Tile { label: "Here", value: here });
                tiles.push(Tile { label: "From the Fediverse", value: fediverse });
                let mut new = Vec::with_capacity(now.len());
                let mut last = at_start;
                for (day, count) in now {
                    new.push((*day, count - last));
                    last = *count;
                }
                charts.push(Chart::new("followers", "Followers", "followers", now, true));
                charts.push(Chart::new("new-followers", "New followers", "new followers", &new, false));
                None
            }
        };
        Ok::<_, sqlx::Error>((tiles, charts, content))
    };
    let (tiles, charts, content) = match loaded.await {
        Ok(loaded) => loaded,
        Err(err) => {
            tracing::error!("failed to load the dashboard of @{}: {}", me.username, err);
            let layout = Layout::load(&state, &session).await;
            return error_page(layout, StatusCode::INTERNAL_SERVER_ERROR, "Failed to load your dashboard, try again later.");
        }
    };
    let tabs = Tab::ALL
        .into_iter()
        .filter(|candidate| candidate.shown())
        .map(|candidate| Link {
            label: candidate.label().to_string(),
            url: format!("/dashboard?tab={}&days={}", candidate.slug(), period.slug()),
            active: candidate == tab,
        })
        .collect();
    let periods = period_links(period, |slug| format!("/dashboard?tab={}&days={slug}", tab.slug()));
    render_template(&DashboardTemplate {
        title: "Dashboard".to_string(),
        layout: Layout::load(&state, &session).await,
        tabs,
        periods,
        period: period.slug(),
        tiles,
        charts,
        content,
        videos: tab == Tab::Videos,
        tab_slug: tab.slug(),
    })
}

#[derive(Template)]
#[template(path = "dashboard_post.html")]
struct PostDashboardTemplate {
    title: String,
    layout: Layout,
    post: ContentRow,
    periods: Vec<Link>,
    /// Totals since it was published.
    totals: Vec<Tile>,
    charts: Vec<Chart>,
    back: String,
}

/// `/dashboard/posts/{id}?days=30`, or `days=all`: one of the user's posts.
pub async fn post_dashboard(
    State(state): State<AppState>,
    session: Session,
    Path(post_id): Path<i64>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let (pool, me) = match signed_in_user(&state, &session).await {
        Ok(signed_in) => signed_in,
        Err(response) => return response,
    };
    let period = Period::from_query(&query);
    let loaded = async {
        let Some(scope) = stats::scope_of(&pool, me.id, post_id).await? else {
            return Ok(None);
        };
        let Some(post) = stats::content(&pool, me.id, Scope::Post(post_id)).await?.pop() else {
            return Ok(None);
        };
        // All time: since the post was published.
        let days = period.days(post.created_at);
        let is_video = scope == Scope::Videos;
        let mut charts = Vec::new();
        for metric in metrics(Scope::Post(post_id), is_video) {
            let (_, chart) = measured(&pool, me.id, Scope::Post(post_id), metric, days).await?;
            charts.push(chart);
        }
        let mut totals = Vec::new();
        if is_video {
            totals.push(Tile { label: "Views", value: post.views });
        }
        totals.push(Tile { label: "Likes", value: post.likes });
        totals.push(Tile { label: "Comments", value: post.comments });
        totals.push(Tile { label: "Shares", value: post.shares });
        let tab = match scope {
            Scope::Videos => Tab::Videos,
            Scope::ImagePosts => Tab::ImagePosts,
            _ => Tab::Posts,
        };
        Ok::<_, sqlx::Error>(Some((post, totals, charts, tab)))
    };
    let (post, totals, charts, tab) = match loaded.await {
        Ok(Some(loaded)) => loaded,
        Ok(None) => {
            let layout = Layout::load(&state, &session).await;
            return error_page(layout, StatusCode::NOT_FOUND, "This post does not exist or isn't yours.");
        }
        Err(err) => {
            tracing::error!("failed to load the dashboard of post {}: {}", post_id, err);
            let layout = Layout::load(&state, &session).await;
            return error_page(layout, StatusCode::INTERNAL_SERVER_ERROR, "Failed to load these statistics, try again later.");
        }
    };
    let periods = period_links(period, |slug| format!("/dashboard/posts/{post_id}?days={slug}"));
    render_template(&PostDashboardTemplate {
        title: crate::i18n::tf("Statistics: {}", &post.label),
        layout: Layout::load(&state, &session).await,
        post,
        periods,
        totals,
        charts,
        back: format!("/dashboard?tab={}&days={}", tab.slug(), period.slug()),
    })
}

/// `POST /posts/{id}/view`, `{"viewer": "..."}` from the player once a video
/// was watched a while. Visitors send a random id their browser keeps.
pub async fn view(
    State(state): State<AppState>,
    session: Session,
    Path(post_id): Path<i64>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Response {
    let Some(pool) = &state.pool else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let me = crate::handlers::session::signed_in(pool, &session).await.ok().flatten();
    let viewer = match &me {
        Some(me) => format!("user:{}", me.id),
        None => {
            let id = body["viewer"].as_str().unwrap_or_default();
            if id.len() < 16 || id.len() > 64 || !id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-') {
                return StatusCode::BAD_REQUEST.into_response();
            }
            format!("visitor:{id}")
        }
    };
    match views::record(pool, post_id, &viewer, me.as_ref().map(|me| me.id)).await {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => {
            tracing::warn!("failed to count a view of post {}: {}", post_id, err);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_compact() {
        assert_eq!(compact(0), "0");
        assert_eq!(compact(1284), "1,284");
        assert_eq!(compact(12_900), "12.9K");
        assert_eq!(compact(40_000), "40K");
        assert_eq!(compact(4_200_000), "4.2M");
    }

    #[test]
    fn long_periods_go_by_week() {
        let day = |offset: i64| sqlx::types::chrono::NaiveDate::from_yo_opt(2026, offset as u32 + 1).unwrap();
        let daily: Vec<_> = (0..90).map(|offset| (day(offset), 1)).collect();
        let counted = Chart::new("c", "C", "c", &daily, false);
        assert_eq!(counted.grouping, Grouping::Week);
        // 90 days: a first week of 6, then 12 of 7, the last ending on the last day.
        assert_eq!(counted.points.len(), 13);
        assert_eq!(counted.points[0], (day(0), 6));
        assert_eq!(counted.points[12], (day(83), 7));
        let running: Vec<_> = (0..90).map(|offset| (day(offset), offset)).collect();
        assert_eq!(Chart::new("r", "R", "r", &running, true).points[12], (day(83), 89));
        assert_eq!(Chart::new("d", "D", "d", &daily[..30], false).grouping, Grouping::Day);
    }

    #[test]
    fn years_go_by_month() {
        // From 2024-11-15 to 2026-11-01: 717 days... and a bit more.
        let start = sqlx::types::chrono::NaiveDate::from_ymd_opt(2024, 10, 15).unwrap();
        let daily: Vec<_> = start.iter_days().take(800).map(|day| (day, 1)).collect();
        let monthly = Chart::new("m", "M", "m", &daily, false);
        assert_eq!(monthly.grouping, Grouping::Month);
        // October 2024 from the 15th: 17 days, then whole months.
        assert_eq!(monthly.points[0], (start, 17));
        assert_eq!(monthly.points[1], (sqlx::types::chrono::NaiveDate::from_ymd_opt(2024, 11, 1).unwrap(), 30));
        assert_eq!(monthly.day_label(&1), "November 2024");
        assert_eq!(monthly.points.iter().map(|(_, count)| count).sum::<i64>(), 800);
    }
}

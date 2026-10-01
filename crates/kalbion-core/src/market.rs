//! Market prices from the Albion Data Project: a crowd-sourced, public, read-only API. Only
//! one fixed host per game server is contacted, quotes are cached in memory for a few
//! minutes, and an HTTP 429 pauses every request until the server's `Retry-After` passes.
use crate::domain::{parse_unique_name, validate_quality, CITIES, MAX_MONEY};
use crate::error::{invalid, Error, Result};
use chrono::{DateTime, NaiveDateTime, SecondsFormat, Utc};
use serde::Deserialize;
use std::collections::{BTreeSet, HashMap};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(15);
/// Quotes change slowly (the project aggregates player uploads), so repeated refreshes within
/// this window are answered from memory and do not count against the rate limit.
const FRESH_FOR: Duration = Duration::from_secs(5 * 60);
const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;
/// The service rejects URLs over ~4 KB; item lists are split well below that.
const MAX_ITEMS_IN_URL: usize = 1800;
const DEFAULT_BACKOFF: Duration = Duration::from_secs(60);
const MAX_BACKOFF: Duration = Duration::from_secs(10 * 60);
/// A pooled connection closed by the server fails almost at once; only such fast failures are
/// retried, so one request never waits for two full timeouts.
const FAST_FAILURE: Duration = Duration::from_secs(2);
/// Observation dates further ahead than this are bogus rather than clock skew.
const MAX_CLOCK_SKEW: chrono::TimeDelta = chrono::TimeDelta::minutes(10);

/// The lowest current sell order for one item and quality in one city.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quote {
    pub item_id: String,
    pub quality: u8,
    pub unit_silver: i64,
    /// When the market data was uploaded to the project, normalized to UTC.
    pub observed_at: String,
}

type CacheKey = (String, String, String, u8);

pub struct AlbionDataProject {
    /// `None` uses the official host for each server.
    base_url: Option<String>,
    agent: ureq::Agent,
    /// Negative answers are cached too, so items without market data are not asked again.
    cache: Mutex<HashMap<CacheKey, (Instant, Option<Quote>)>>,
    blocked_until: Mutex<Option<Instant>>,
}

#[derive(Deserialize)]
struct Row {
    item_id: String,
    city: String,
    quality: u8,
    sell_price_min: i64,
    sell_price_min_date: String,
}

impl Default for AlbionDataProject {
    fn default() -> Self {
        Self::build(None, true)
    }
}
impl AlbionDataProject {
    /// Test seam: sends every server to one plain-HTTP base URL.
    #[doc(hidden)]
    pub fn with_base_url(base_url: String) -> Self {
        Self::build(Some(base_url), false)
    }
    fn build(base_url: Option<String>, https_only: bool) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .http_status_as_error(false)
            .https_only(https_only)
            .max_redirects(0)
            .user_agent(concat!("Kalbion/", env!("CARGO_PKG_VERSION")))
            .build();
        Self {
            base_url,
            agent: ureq::Agent::new_with_config(config),
            cache: Mutex::new(HashMap::new()),
            blocked_until: Mutex::new(None),
        }
    }

    /// Quotes for the requested item/quality pairs in `city`. Pairs without market data are
    /// absent from the result: a missing price is never reported as zero. Fails as a whole
    /// when any request fails; pairs fetched before the failure stay cached for the retry.
    pub fn quotes(&self, server: &str, city: &str, wanted: &[(String, u8)]) -> Result<Vec<Quote>> {
        let base = self.base_url(server)?;
        if !CITIES.contains(&city) {
            return Err(invalid("Cidade inválida"));
        }
        let wanted: BTreeSet<_> = wanted.iter().cloned().collect();
        for (item_id, quality) in &wanted {
            parse_unique_name(item_id)?;
            if *quality == 0 {
                return Err(invalid("Quality desconhecida não tem cotação de mercado"));
            }
            validate_quality(Some(*quality))?;
        }
        let key = |(item_id, quality): &(String, u8)| {
            (
                server.to_string(),
                city.to_string(),
                item_id.clone(),
                *quality,
            )
        };
        let mut quotes = Vec::new();
        let mut missing = Vec::new();
        {
            let cache = self.lock_cache();
            for pair in &wanted {
                match cache.get(&key(pair)) {
                    Some((fetched, quote)) if fetched.elapsed() < FRESH_FOR => {
                        quotes.extend(quote.clone())
                    }
                    _ => missing.push(pair.clone()),
                }
            }
        }
        for chunk in chunks(&missing) {
            let rows = self.fetch(&base, city, chunk)?;
            let fetched = Instant::now();
            let mut cache = self.lock_cache();
            cache.retain(|_, (at, _)| at.elapsed() < FRESH_FOR);
            for pair in chunk {
                let quote = rows
                    .iter()
                    .filter(|quote| quote.item_id == pair.0 && quote.quality == pair.1)
                    .min_by_key(|quote| quote.unit_silver)
                    .cloned();
                cache.insert(key(pair), (fetched, quote.clone()));
                quotes.extend(quote);
            }
        }
        Ok(quotes)
    }

    fn base_url(&self, server: &str) -> Result<String> {
        let official = match server {
            "americas" => "https://west.albion-online-data.com",
            "europe" => "https://europe.albion-online-data.com",
            "asia" => "https://east.albion-online-data.com",
            _ => return Err(invalid("Servidor inválido")),
        };
        Ok(self.base_url.clone().unwrap_or_else(|| official.into()))
    }
    /// Valid quotes in `city` from one request. Rows that do not parse are skipped (and
    /// logged) instead of failing the whole answer.
    fn fetch(&self, base: &str, city: &str, chunk: &[(String, u8)]) -> Result<Vec<Quote>> {
        if let Some(until) = *self.lock_blocked() {
            if let Some(wait) = until.checked_duration_since(Instant::now()) {
                return Err(rate_limited(wait));
            }
        }
        let items: BTreeSet<_> = chunk.iter().map(|(item_id, _)| item_id.as_str()).collect();
        let qualities: BTreeSet<_> = chunk.iter().map(|(_, quality)| *quality).collect();
        let url = format!(
            "{base}/api/v2/stats/prices/{}.json?locations={}&qualities={}",
            items
                .into_iter()
                .collect::<Vec<_>>()
                .join(",")
                .replace('@', "%40"),
            city.replace(' ', "%20"),
            qualities
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(",")
        );
        // Same reasoning as the icon cache: a stale pooled connection fails at once and one
        // retry is safe for a GET; a timeout or a slow failure is not retried.
        let started = Instant::now();
        let mut response = match self.agent.get(&url).call() {
            Err(error)
                if !matches!(error, ureq::Error::Timeout(_))
                    && started.elapsed() < FAST_FAILURE =>
            {
                self.agent.get(&url).call()
            }
            result => result,
        }
        .map_err(|error| unavailable("market_request_failed", &error.to_string()))?;
        match response.status().as_u16() {
            200 => {}
            429 => {
                let wait = response
                    .headers()
                    .get("retry-after")
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.trim().parse::<u64>().ok())
                    .map_or(DEFAULT_BACKOFF, |seconds| {
                        Duration::from_secs(seconds).clamp(Duration::from_secs(1), MAX_BACKOFF)
                    });
                *self.lock_blocked() = Some(Instant::now() + wait);
                tracing::warn!(
                    operation = "market_rate_limited",
                    wait_seconds = wait.as_secs()
                );
                return Err(rate_limited(wait));
            }
            status => {
                return Err(unavailable(
                    "market_request_failed",
                    &format!("HTTP {status}"),
                ))
            }
        }
        let bytes = response
            .body_mut()
            .with_config()
            .limit(MAX_RESPONSE_BYTES)
            .read_to_vec()
            .map_err(|error| unavailable("market_response_unreadable", &error.to_string()))?;
        let rows: Vec<serde_json::Value> = serde_json::from_slice(&bytes)
            .map_err(|error| unavailable("market_response_invalid", &error.to_string()))?;
        let now = Utc::now();
        let mut rejected = 0;
        let mut quotes = Vec::new();
        for row in rows {
            match serde_json::from_value::<Row>(row)
                .map_err(|_| ())
                .and_then(|row| {
                    if row.city == city {
                        to_quote(&row, now)
                    } else {
                        Ok(None)
                    }
                }) {
                Ok(quote) => quotes.extend(quote),
                Err(()) => rejected += 1,
            }
        }
        if rejected > 0 {
            tracing::warn!(operation = "market_rows_rejected", rejected);
        }
        Ok(quotes)
    }
    fn lock_cache(&self) -> std::sync::MutexGuard<'_, HashMap<CacheKey, (Instant, Option<Quote>)>> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }
    fn lock_blocked(&self) -> std::sync::MutexGuard<'_, Option<Instant>> {
        self.blocked_until
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// Splits the pairs so that each URL's item list stays short; a pair never straddles two
/// requests, and every quality of an item goes in the same request.
fn chunks(pairs: &[(String, u8)]) -> Vec<&[(String, u8)]> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut length = 0;
    for (index, (item_id, _)) in pairs.iter().enumerate() {
        let new_item = index == 0 || pairs[index - 1].0 != *item_id;
        let cost = if new_item { item_id.len() + 3 } else { 0 };
        if new_item && index > start && length + cost > MAX_ITEMS_IN_URL {
            result.push(&pairs[start..index]);
            start = index;
            length = 0;
        }
        length += cost;
    }
    if start < pairs.len() {
        result.push(&pairs[start..]);
    }
    result
}
/// The project reports "no orders" as price 0 with date 0001-01-01; that is an absent price
/// (`Ok(None)`). A positive price with an unreadable or future date is rejected (`Err`), so a
/// format change shows up in the log instead of silently reading as "no offer".
fn to_quote(row: &Row, now: DateTime<Utc>) -> std::result::Result<Option<Quote>, ()> {
    if row.sell_price_min == 0 {
        return Ok(None);
    }
    if row.sell_price_min < 0 || row.sell_price_min > MAX_MONEY {
        return Err(());
    }
    let observed = NaiveDateTime::parse_from_str(&row.sell_price_min_date, "%Y-%m-%dT%H:%M:%S%.f")
        .map(|date| date.and_utc())
        .or_else(|_| {
            DateTime::parse_from_rfc3339(&row.sell_price_min_date).map(|date| date.to_utc())
        })
        .map_err(|_| ())?;
    if observed.timestamp() <= 0 || observed > now + MAX_CLOCK_SKEW {
        return Err(());
    }
    Ok(Some(Quote {
        item_id: row.item_id.clone(),
        quality: row.quality,
        unit_silver: row.sell_price_min,
        observed_at: observed.to_rfc3339_opts(SecondsFormat::Micros, true),
    }))
}
fn rate_limited(wait: Duration) -> Error {
    Error::Unavailable(format!(
        "Limite de consultas do Albion Data Project atingido; tente de novo em {} s",
        wait.as_secs().max(1)
    ))
}
fn unavailable(operation: &'static str, cause: &str) -> Error {
    tracing::warn!(operation, cause);
    Error::Unavailable("Albion Data Project indisponível; os preços atuais foram mantidos".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_keep_every_quality_of_an_item_together() {
        let pairs: Vec<(String, u8)> = (0..400)
            .flat_map(|index| {
                [
                    (format!("T4_ITEM_{index:04}"), 1),
                    (format!("T4_ITEM_{index:04}"), 2),
                ]
            })
            .collect();
        let parts = chunks(&pairs);
        assert!(parts.len() > 1);
        assert_eq!(parts.iter().map(|part| part.len()).sum::<usize>(), 800);
        for part in &parts {
            let items: BTreeSet<_> = part.iter().map(|(item_id, _)| item_id).collect();
            assert!(items.iter().map(|item| item.len() + 3).sum::<usize>() <= MAX_ITEMS_IN_URL);
        }
        for pair in parts.windows(2) {
            assert_ne!(pair[0].last().unwrap().0, pair[1][0].0);
        }
    }
}

use kalbion_core::market::{AlbionDataProject, Quote};
use kalbion_core::{domain::*, Error, Store};
use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

/// Status, body and optional `Retry-After` seconds for a request path.
type Answer = fn(&str) -> (u16, String, Option<&'static str>);

/// Minimal HTTP server that answers by path and records every request path.
fn serve(answer: Answer) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let log = requests.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            reader.read_line(&mut request_line).unwrap();
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                if header.trim().is_empty() {
                    break;
                }
            }
            let path = request_line.split_whitespace().nth(1).unwrap().to_string();
            let (status, body, retry_after) = answer(&path);
            log.lock().unwrap().push(path);
            let retry_after = retry_after
                .map(|value| format!("Retry-After: {value}\r\n"))
                .unwrap_or_default();
            write!(
                stream,
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\n{retry_after}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    (url, requests)
}
fn row(item_id: &str, city: &str, quality: u8, price: i64, date: &str) -> serde_json::Value {
    json!({
        "item_id": item_id, "city": city, "quality": quality,
        "sell_price_min": price, "sell_price_min_date": date,
        "sell_price_max": price + 10, "sell_price_max_date": date,
        "buy_price_min": 1, "buy_price_min_date": date,
        "buy_price_max": 2, "buy_price_max_date": date
    })
}
fn pairs(values: &[(&str, u8)]) -> Vec<(String, u8)> {
    values
        .iter()
        .map(|(item_id, quality)| (item_id.to_string(), *quality))
        .collect()
}
fn prices(_: &str) -> (u16, String, Option<&'static str>) {
    let body = json!([
        row("T5_BAG@1", "Fort Sterling", 2, 4321, "2026-10-01T12:30:00"),
        // Another city and another quality in the same answer must not leak in.
        row("T5_BAG@1", "Martlock", 2, 1, "2026-10-01T12:30:00"),
        row("T5_BAG@1", "Fort Sterling", 3, 9999, "2026-10-01T12:30:00"),
        // "No orders" is reported as zero with year 1: an absent price, not zero silver.
        row("T4_BAG", "Fort Sterling", 1, 0, "0001-01-01T00:00:00"),
    ]);
    (200, body.to_string(), None)
}

#[test]
fn quotes_use_the_lowest_sell_order_and_absent_prices_stay_absent() {
    let (url, requests) = serve(prices);
    let market = AlbionDataProject::with_base_url(url);
    let quotes = market
        .quotes(
            "europe",
            "Fort Sterling",
            &pairs(&[("T5_BAG@1", 2), ("T4_BAG", 1)]),
        )
        .unwrap();
    assert_eq!(
        quotes,
        [Quote {
            item_id: "T5_BAG@1".into(),
            quality: 2,
            unit_silver: 4321,
            observed_at: "2026-10-01T12:30:00.000000Z".into(),
        }]
    );
    assert_eq!(
        *requests.lock().unwrap(),
        ["/api/v2/stats/prices/T4_BAG,T5_BAG%401.json?locations=Fort%20Sterling&qualities=1,2"]
    );
    // Within the cache window, positive and negative answers are not requested again.
    assert_eq!(
        market
            .quotes(
                "europe",
                "Fort Sterling",
                &pairs(&[("T5_BAG@1", 2), ("T4_BAG", 1)])
            )
            .unwrap()
            .len(),
        1
    );
    assert_eq!(requests.lock().unwrap().len(), 1);
    // The cache is per city.
    market
        .quotes("europe", "Martlock", &pairs(&[("T5_BAG@1", 2)]))
        .unwrap();
    assert_eq!(requests.lock().unwrap().len(), 2);
}

#[test]
fn rate_limit_pauses_requests_until_retry_after() {
    let (url, requests) = serve(|_| (429, "Too Many Requests".into(), Some("1")));
    let market = AlbionDataProject::with_base_url(url);
    let wanted = pairs(&[("T4_BAG", 1)]);
    let error = market.quotes("americas", "Lymhurst", &wanted).unwrap_err();
    assert!(matches!(error, Error::Unavailable(_)));
    assert!(error.to_string().contains("tente de novo em 1 s"));
    assert!(market.quotes("americas", "Lymhurst", &wanted).is_err());
    assert_eq!(requests.lock().unwrap().len(), 1, "blocked during backoff");
    std::thread::sleep(std::time::Duration::from_millis(1100));
    assert!(market.quotes("americas", "Lymhurst", &wanted).is_err());
    assert_eq!(requests.lock().unwrap().len(), 2, "asks again afterwards");
}

#[test]
fn service_failures_are_unavailable_not_invalid_input() {
    let (url, _) = serve(|_| (500, "oops".into(), None));
    let wanted = pairs(&[("T4_BAG", 1)]);
    let error = AlbionDataProject::with_base_url(url)
        .quotes("asia", "Caerleon", &wanted)
        .unwrap_err();
    assert!(matches!(error, Error::Unavailable(_)));
    let (url, _) = serve(|_| (200, "<html>not json</html>".into(), None));
    assert!(matches!(
        AlbionDataProject::with_base_url(url).quotes("asia", "Caerleon", &wanted),
        Err(Error::Unavailable(_))
    ));
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", closed.local_addr().unwrap());
    drop(closed);
    assert!(matches!(
        AlbionDataProject::with_base_url(url).quotes("asia", "Caerleon", &wanted),
        Err(Error::Unavailable(_))
    ));
}

#[test]
fn invalid_requests_never_reach_the_network() {
    let (url, requests) = serve(prices);
    let market = AlbionDataProject::with_base_url(url);
    for (server, city, wanted) in [
        ("mars", "Lymhurst", pairs(&[("T4_BAG", 1)])),
        ("europe", "Lymhurst&x=1", pairs(&[("T4_BAG", 1)])),
        ("europe", "Lymhurst", pairs(&[("../T4_BAG", 1)])),
        ("europe", "Lymhurst", pairs(&[("T4_BAG,T5_BAG", 1)])),
        ("europe", "Lymhurst", pairs(&[("T4_BAG", 0)])),
        ("europe", "Lymhurst", pairs(&[("T4_BAG", 6)])),
    ] {
        assert!(matches!(
            market.quotes(server, city, &wanted),
            Err(Error::Validation(_))
        ));
    }
    assert!(market.quotes("europe", "Lymhurst", &[]).unwrap().is_empty());
    assert!(requests.lock().unwrap().is_empty());
}

#[test]
fn long_item_lists_are_split_into_several_requests() {
    let (url, requests) = serve(|_| (200, "[]".into(), None));
    let market = AlbionDataProject::with_base_url(url);
    let wanted: Vec<(String, u8)> = (0..300)
        .flat_map(|index| {
            let item_id = format!("T4_TEST_ITEM_{index:03}");
            [(item_id.clone(), 1), (item_id, 4)]
        })
        .collect();
    assert!(market
        .quotes("americas", "Thetford", &wanted)
        .unwrap()
        .is_empty());
    let requests = requests.lock().unwrap();
    assert!(requests.len() > 1);
    assert!(requests.iter().all(|path| path.len() < 2100));
    let requested: usize = requests
        .iter()
        .map(|path| {
            path.split(".json")
                .next()
                .unwrap()
                .matches("T4_TEST_ITEM")
                .count()
        })
        .sum();
    assert_eq!(requested, 300);
}

fn quote(item_id: &str, quality: u8, unit_silver: i64) -> Quote {
    Quote {
        item_id: item_id.into(),
        quality,
        unit_silver,
        observed_at: "2026-10-01T12:30:00.000000Z".into(),
    }
}

#[test]
fn manual_prices_prevail_over_market_prices() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.db");
    let mut store = Store::open(&path).unwrap();
    let session = store.create_session("Market").unwrap();
    for (player, item_id, quality, quantity) in [
        ("Alice", "T4_BAG", Some(1), 2),
        ("Bob", "T4_BAG", Some(2), 1),
        ("Bob", "T5_BAG@1", Some(1), 1),
        ("Alice", "T4_BAG", None, 3),
    ] {
        store
            .manual(&session.id, player, item_id, quality, quantity)
            .unwrap();
    }
    store
        .price(&session.id, "T4_BAG", Some(2), Some(50))
        .unwrap();
    let plan = store.market_plan(&session.id).unwrap();
    assert_eq!(
        (plan.server.as_str(), plan.city.as_str()),
        ("americas", "Bridgewatch")
    );
    assert_eq!(plan.wanted, pairs(&[("T4_BAG", 1), ("T5_BAG@1", 1)]));
    // A manual price typed while the request was in flight must survive the answer.
    store
        .price(&session.id, "T5_BAG@1", Some(1), Some(70))
        .unwrap();
    let result = store
        .apply_market_quotes(
            &session.id,
            &plan,
            &[
                quote("T4_BAG", 1, 1000),
                quote("T5_BAG@1", 1, 9000),
                quote("T4_BAG", 2, 9000),
            ],
        )
        .unwrap();
    assert_eq!(
        (
            result.updated,
            result.unavailable,
            result.manual_kept,
            result.unknown_quality
        ),
        (1, 0, 2, 1)
    );
    let view = store.view(&session.id, &Filter::default()).unwrap();
    let price_of = |item_id: &str, quality: Option<u8>| {
        view.rows
            .iter()
            .find(|row| row.event.item.id == item_id && row.event.quality == quality)
            .unwrap()
            .price
            .clone()
    };
    let market = price_of("T4_BAG", Some(1)).unwrap();
    assert_eq!(market.source, PriceSource::AlbionData);
    assert_eq!(market.unit_silver, 1000);
    assert_eq!(
        market.observed_at.as_deref(),
        Some("2026-10-01T12:30:00.000000Z")
    );
    assert_eq!(price_of("T4_BAG", Some(2)).unwrap().unit_silver, 50);
    assert_eq!(price_of("T5_BAG@1", Some(1)).unwrap().unit_silver, 70);
    assert!(
        price_of("T4_BAG", None).is_none(),
        "unknown quality is never guessed"
    );
    assert_eq!(view.totals.session.estimated_silver, 2000 + 50 + 70);
    assert_eq!(view.totals.session.unpriced_events, 1);
    assert_eq!(view.finance.income, 0, "an estimate never becomes cash");

    // Without a current quote, the older market price stays and is reported as unavailable.
    let plan = store.market_plan(&session.id).unwrap();
    assert_eq!(plan.wanted, pairs(&[("T4_BAG", 1)]));
    let result = store.apply_market_quotes(&session.id, &plan, &[]).unwrap();
    assert_eq!((result.updated, result.unavailable), (0, 1));
    // A manual price replaces a market price; removing it lets the market fill it again.
    store
        .price(&session.id, "T4_BAG", Some(1), Some(10))
        .unwrap();
    assert!(store.market_plan(&session.id).unwrap().wanted.is_empty());
    store.price(&session.id, "T4_BAG", Some(1), None).unwrap();
    assert_eq!(store.market_plan(&session.id).unwrap().wanted.len(), 1);

    let plan = store.market_plan(&session.id).unwrap();
    store
        .apply_market_quotes(&session.id, &plan, &[quote("T4_BAG", 1, 1200)])
        .unwrap();
    let csv = store.export(&session.id, "csv").unwrap();
    assert!(csv.lines().next().unwrap().ends_with(",price_observed_at"));
    assert!(csv.contains(",1200,albion_data,americas,Bridgewatch,"));
    assert!(csv.contains(",2026-10-01T12:30:00.000000Z"));
    drop(store);
    let store = Store::open(&path).unwrap();
    let view = store.view(&session.id, &Filter::default()).unwrap();
    assert_eq!(view.totals.session.estimated_silver, 2400 + 50 + 70);
}

#[test]
fn market_quotes_cannot_overflow_totals_or_change_another_market() {
    let mut store = Store::open(":memory:").unwrap();
    let session = store.create_session("Limits").unwrap();
    store
        .manual(&session.id, "Alice", "T4_BAG", Some(1), 1_000_000)
        .unwrap();
    let plan = store.market_plan(&session.id).unwrap();
    assert!(store
        .apply_market_quotes(&session.id, &plan, &[quote("T4_BAG", 1, 999_999_999_999)])
        .is_err());
    assert!(store.rows(&session.id, &Filter::default()).unwrap()[0]
        .price
        .is_none());
    let mut other = plan.clone();
    other.city = "Martlock".into();
    assert!(store
        .apply_market_quotes(&session.id, &other, &[quote("T4_BAG", 1, 10)])
        .is_err());
}

/// Contacts the real service over HTTPS: `cargo test -p kalbion-core --test market -- --ignored`.
#[test]
#[ignore]
fn live_service_answers_for_every_server() {
    let market = AlbionDataProject::default();
    for server in SERVERS {
        let quotes = market
            .quotes(server, "Bridgewatch", &pairs(&[("T4_BAG", 1)]))
            .unwrap();
        // The market may have no orders right now; whatever comes back must be sane.
        for quote in quotes {
            assert!(quote.unit_silver > 0);
            assert!(quote.observed_at.ends_with('Z'));
        }
    }
}

fn hostile(_: &str) -> (u16, String, Option<&'static str>) {
    let body = json!([
        row("T4_BAG", "Lymhurst", 1, 900, "2026-10-01T12:00:00"),
        // Duplicates: the lowest offer wins.
        row("T4_BAG", "Lymhurst", 1, 100, "2026-10-01T12:00:00"),
        // Malformed rows for anything must not sink the valid ones.
        { "item_id": "T4_BAG", "city": "Lymhurst", "quality": 300, "sell_price_min": 5, "sell_price_min_date": "2026-10-01T12:00:00" },
        { "item_id": "T4_BAG", "city": "Lymhurst", "quality": 2, "sell_price_min": null },
        row("T4_BAG", "Lymhurst", 2, -5, "2026-10-01T12:00:00"),
        // Other date spellings are still read; dates in the future are not.
        row("T5_BAG@1", "Lymhurst", 1, 700, "2026-10-01T12:00:00.250+03:00"),
        row("T6_BAG", "Lymhurst", 1, 600, "2099-01-01T00:00:00"),
        row("T7_BAG", "Lymhurst", 1, 500, "yesterday"),
    ]);
    (200, body.to_string(), None)
}

#[test]
fn malformed_rows_are_skipped_and_the_lowest_duplicate_wins() {
    let (url, _) = serve(hostile);
    let quotes = AlbionDataProject::with_base_url(url)
        .quotes(
            "europe",
            "Lymhurst",
            &pairs(&[
                ("T4_BAG", 1),
                ("T4_BAG", 2),
                ("T5_BAG@1", 1),
                ("T6_BAG", 1),
                ("T7_BAG", 1),
            ]),
        )
        .unwrap();
    let found: Vec<_> = quotes
        .iter()
        .map(|quote| {
            (
                quote.item_id.as_str(),
                quote.unit_silver,
                quote.observed_at.as_str(),
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            ("T4_BAG", 100, "2026-10-01T12:00:00.000000Z"),
            ("T5_BAG@1", 700, "2026-10-01T09:00:00.250000Z"),
        ]
    );
}

#[test]
fn voided_loot_is_not_quoted_and_counts_cover_only_what_needs_a_price() {
    let mut store = Store::open(":memory:").unwrap();
    let session = store.create_session("Counts").unwrap();
    for (item_id, quality) in [
        ("T4_BAG", Some(1)),
        ("T5_BAG@1", Some(1)),
        ("T4_WOOD", None),
        ("T6_ORE", None),
    ] {
        store
            .manual(&session.id, "Alice", item_id, quality, 1)
            .unwrap();
    }
    let rows = store.rows(&session.id, &Filter::default()).unwrap();
    let bag = rows
        .iter()
        .find(|row| row.event.item.id == "T5_BAG@1")
        .unwrap();
    store
        .set_voided(&session.id, &bag.event.source, &bag.event.id, true)
        .unwrap();
    store.price(&session.id, "T4_WOOD", None, Some(5)).unwrap();
    let plan = store.market_plan(&session.id).unwrap();
    assert_eq!(plan.wanted, pairs(&[("T4_BAG", 1)]));
    let result = store
        .apply_market_quotes(&session.id, &plan, &[quote("T4_BAG", 1, 10)])
        .unwrap();
    assert_eq!(
        (
            result.updated,
            result.unavailable,
            result.manual_kept,
            result.unknown_quality
        ),
        (1, 0, 0, 1)
    );
}

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
/// Besides connectivity, it requires real quotes for a raw and a refined resource (asked as
/// quality 1) and checks that they end up as the session's prices.
#[test]
#[ignore]
fn live_service_prices_raw_and_refined_resources() {
    let market = AlbionDataProject::default();
    for server in SERVERS {
        let quotes = market
            .quotes(server, "Bridgewatch", &pairs(&[("T4_BAG", 1)]))
            .unwrap();
        for quote in quotes {
            assert!(quote.unit_silver > 0);
            assert!(quote.observed_at.ends_with('Z'));
        }
    }
    let mut store = Store::open(":memory:").unwrap();
    let session = store.create_session("Live").unwrap();
    let file = json!({ "schema_version": 2, "events": [
        { "id": "wood", "source": "live.test", "origin": "manual", "session_id": session.id,
          "occurred_at": "2026-10-01T12:00:00Z", "player": "Ana",
          "item": { "id": "T4_WOOD", "name": "Wood" }, "quality": null, "quantity": 10 },
        { "id": "planks", "source": "live.test", "origin": "manual", "session_id": session.id,
          "occurred_at": "2026-10-01T12:00:00Z", "player": "Ana",
          "item": { "id": "T4_PLANKS", "name": "Planks" }, "quality": null, "quantity": 10 }
    ]});
    store.import(&session.id, &file.to_string()).unwrap();
    let plan = store.market_plan(&session.id).unwrap();
    assert_eq!(plan.wanted, pairs(&[("T4_PLANKS", 1), ("T4_WOOD", 1)]));
    let quotes = market
        .quotes(&plan.server, &plan.city, &plan.wanted)
        .unwrap();
    let result = store
        .apply_market_quotes(&session.id, &plan, &quotes)
        .unwrap();
    assert_eq!(
        result.updated, 2,
        "both resources should have offers: {quotes:?}"
    );
    for row in store.rows(&session.id, &Filter::default()).unwrap() {
        let price = row.price.unwrap();
        assert_eq!(price.source, PriceSource::AlbionData);
        assert!(price.unit_silver > 0);
        assert_eq!(row.event.quality, None);
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
        ("T4_BAG", None),
        ("T5_BAG@1", None),
        ("T4_WOOD", None),
    ] {
        store
            .manual(&session.id, "Alice", item_id, quality, 1)
            .unwrap();
    }
    let rows = store.rows(&session.id, &Filter::default()).unwrap();
    let bag = rows
        .iter()
        .find(|row| row.event.item.id == "T5_BAG@1" && row.event.quality.is_some())
        .unwrap();
    store
        .set_voided(&session.id, &bag.event.source, &bag.event.id, true)
        .unwrap();
    store.price(&session.id, "T5_BAG@1", None, Some(5)).unwrap();
    let plan = store.market_plan(&session.id).unwrap();
    assert_eq!(plan.wanted, pairs(&[("T4_BAG", 1), ("T4_WOOD", 1)]));
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
        (1, 1, 0, 1)
    );
}

#[test]
fn resources_have_no_quality_and_are_quoted_as_quality_one() {
    for (item_id, expected) in [
        ("T4_WOOD", false),
        ("T6_ORE", false),
        ("T5_HIDE_LEVEL1@1", false),
        ("T8_FIBER_LEVEL3@3", false),
        ("T4_ROCK", false),
        ("T4_PLANKS", false),
        ("T5_PLANKS_LEVEL1@1", false),
        ("T6_METALBAR_LEVEL2@2", false),
        ("T5_LEATHER_LEVEL1@1", false),
        ("T4_CLOTH_LEVEL3@3", false),
        ("T5_STONEBLOCK", false),
        ("T4_BAG", true),
        ("T6_MAIN_SWORD@2", true),
        // Equipment and other items named after a resource keep their quality.
        ("T4_ARMOR_LEATHER_SET1", true),
        ("T5_HEAD_CLOTH_SET2@1", true),
        ("T6_2H_ROCKSTAFF_KEEPER", true),
        ("T4_BACKPACK_GATHERER_ORE", true),
        ("T4_JOURNAL_WOOD_FULL", true),
        ("T5_PLANKS_LEVELX", true),
        ("UNIQUE_HIDEOUT", true),
        ("TREASURE_ORE", true),
    ] {
        assert_eq!(has_quality(item_id), expected, "{item_id}");
    }
    let wood = Item::new("T5_WOOD_LEVEL2@2", "Madeira").unwrap();
    assert_eq!(
        (wood.tier, wood.enchantment, wood.has_quality),
        (Some(5), 2, false)
    );

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.db");
    let mut store = Store::open(&path).unwrap();
    let session = store.create_session("Gathering").unwrap();
    // Whatever quality a source reports for a resource is dropped, never rejected.
    store
        .manual(&session.id, "Alice", "T4_WOOD", Some(1), 30)
        .unwrap();
    let file = |quality: u8| {
        json!({ "schema_version": 2, "events": [{
            "id": format!("ore-{quality}"), "source": "test.v1", "origin": "manual",
            "session_id": session.id, "occurred_at": "2026-10-01T12:00:00Z", "player": "Bob",
            "item": { "id": "T6_ORE", "name": "Minério", "tier": 6, "enchantment": 0 },
            "quality": quality, "quantity": 10
        }]})
        .to_string()
    };
    assert_eq!(store.import(&session.id, &file(1)).unwrap().inserted, 1);
    assert_eq!(store.import(&session.id, &file(3)).unwrap().inserted, 1);
    assert!(store.import(&session.id, &file(6)).is_err());
    let rows = store.rows(&session.id, &Filter::default()).unwrap();
    assert!(rows
        .iter()
        .all(|row| row.event.quality.is_none() && !row.event.item.has_quality));
    let exported = store.export(&session.id, "json").unwrap();
    let exported: serde_json::Value = serde_json::from_str(&exported).unwrap();
    let replay = json!({ "schema_version": 2, "events": exported["events"] }).to_string();
    assert_eq!(store.import(&session.id, &replay).unwrap().duplicates, 3);
    // Loot rows as the app serializes them (with has_quality) also replay; a contradicting
    // has_quality is rejected like a contradicting tier.
    let events: Vec<_> = rows
        .iter()
        .map(|row| serde_json::to_value(&row.event).unwrap())
        .collect();
    let replay = json!({ "schema_version": 2, "events": events }).to_string();
    assert_eq!(store.import(&session.id, &replay).unwrap().duplicates, 3);
    let mut lying = events[0].clone();
    lying["id"] = json!("lying");
    lying["item"]["has_quality"] = json!(true);
    let lying = json!({ "schema_version": 2, "events": [lying] }).to_string();
    assert!(store.import(&session.id, &lying).is_err());

    let plan = store.market_plan(&session.id).unwrap();
    assert_eq!(plan.wanted, pairs(&[("T4_WOOD", 1), ("T6_ORE", 1)]));
    let result = store
        .apply_market_quotes(
            &session.id,
            &plan,
            &[quote("T4_WOOD", 1, 7), quote("T6_ORE", 1, 40)],
        )
        .unwrap();
    assert_eq!((result.updated, result.unknown_quality), (2, 0));
    drop(store);
    let store = Store::open(&path).unwrap();
    let view = store.view(&session.id, &Filter::default()).unwrap();
    assert_eq!(view.totals.session.estimated_silver, 30 * 7 + 20 * 40);
    assert_eq!(view.totals.session.unpriced_events, 0);
    // A manual price for a resource is also stored without quality and prevails.
    store.price(&session.id, "T6_ORE", None, Some(50)).unwrap();
    assert_eq!(
        store.market_plan(&session.id).unwrap().wanted,
        pairs(&[("T4_WOOD", 1)])
    );
}

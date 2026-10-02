//! A decoded capture flows into Kalbion's existing loot pipeline. Synthetic data only.
mod common;
use common::*;
use kalbion_capture::{convert, decode, Capture, Limits};
use kalbion_core::{catalog, domain::*, market::Quote, Store};
use serde_json::json;

fn capture() -> Capture {
    let frames = vec![
        (
            T0 + 1,
            0,
            from_server(&packet(&[reliable(1, &loot("Mob", "Kazz", 950, 30))])),
        ),
        (
            T0 + 2,
            0,
            from_server(&packet(&[reliable(2, &loot("Mob", "Luna", 2888, 1))])),
        ),
        // An enemy looting nearby: not on the roster, so never stored.
        (
            T0 + 3,
            0,
            from_server(&packet(&[reliable(3, &loot("Kazz", "Inimigo", 4100, 1))])),
        ),
        // An item index the catalog does not know.
        (
            T0 + 4,
            0,
            from_server(&packet(&[reliable(4, &loot("Mob", "kazz", 99999, 2))])),
        ),
        (
            T0 + 5,
            0,
            from_server(&packet(&[reliable(5, &loot("Baú", "Luna", 1088, 12))])),
        ),
    ];
    decode(&pcap(&frames)[..], Limits::default()).unwrap()
}
fn store_with_catalog() -> (Store, Session) {
    let mut store = Store::open(":memory:").unwrap();
    let parsed = catalog::parse_ao_bin_dumps(catalog_json().as_bytes()).unwrap();
    store.replace_catalog(&parsed, "items.json").unwrap();
    let session = store.create_session("Captura").unwrap();
    (store, session)
}
fn import(
    store: &mut Store,
    session: &Session,
    capture: &Capture,
    roster: &[&str],
) -> (kalbion_core::store::InsertResult, convert::Report) {
    let roster = convert::roster(
        &roster
            .iter()
            .map(|name| name.to_string())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let (events, report) = convert::to_events(capture, &session.id, &roster, |index| {
        store.catalog_item_by_game_index(index)
    })
    .unwrap();
    (store.ingest(&events, true).unwrap(), report)
}

#[test]
fn reimporting_the_same_capture_adds_nothing() {
    let (mut store, session) = store_with_catalog();
    let first = capture();
    let (result, report) = import(&mut store, &session, &first, &["Kazz", "LUNA"]);
    assert_eq!((result.inserted, result.duplicates), (3, 0));
    assert_eq!(report.loot_observed, 5);
    assert_eq!(report.outside_roster, 1);
    assert_eq!(report.unknown_items.get(&99999), Some(&1));
    let (again, _) = import(&mut store, &session, &capture(), &["Kazz", "Luna"]);
    assert_eq!((again.inserted, again.duplicates), (0, 3));

    let rows = store.rows(&session.id, &Filter::default()).unwrap();
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|row| row.imported
        && row.event.origin == Origin::Observed
        && row.event.source == convert::SOURCE
        && row.event.quality.is_none()
        && row.event.id.starts_with(&first.fingerprint)));
    assert!(!rows.iter().any(|row| row.event.player == "Inimigo"));
    let planks = rows
        .iter()
        .find(|row| row.event.item.id == "T5_PLANKS_LEVEL1@1")
        .unwrap();
    assert_eq!(
        (planks.event.item.tier, planks.event.item.enchantment),
        (Some(5), 1)
    );
    assert!(!planks.event.item.has_quality);
    assert_eq!(planks.event.occurred_at, "2026-09-10T12:00:05.000000Z");

    store
        .record_capture_import(&kalbion_core::store::CaptureImport {
            session_id: session.id.clone(),
            file_label: "loot.pcap".into(),
            file_sha256: first.file_sha256.clone(),
            fingerprint: first.fingerprint.clone(),
            decoder_version: kalbion_capture::DECODER_VERSION.into(),
            inserted: 3,
            duplicates: 0,
            diagnostics: json!(first.diagnostics).to_string(),
        })
        .unwrap();
    let audit = store.capture_imports(&session.id).unwrap();
    // The same capture belongs to one session: another session is told where it went, and
    // the store itself refuses the events if a caller skips that check.
    let other = store.create_session("Outra").unwrap();
    assert_eq!(
        store
            .capture_session_elsewhere(&first.fingerprint, &other.id)
            .unwrap()
            .as_deref(),
        Some("Captura")
    );
    assert_eq!(
        store
            .capture_session_elsewhere(&first.fingerprint, &session.id)
            .unwrap(),
        None
    );
    let roster = convert::roster(&["Kazz".to_string()]).unwrap();
    let (elsewhere, _) = convert::to_events(&first, &other.id, &roster, |index| {
        store.catalog_item_by_game_index(index)
    })
    .unwrap();
    assert!(store.ingest(&elsewhere, true).is_err());
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].decoder_version, kalbion_capture::DECODER_VERSION);
    assert!(
        !audit[0].diagnostics.contains("Inimigo"),
        "diagnostics hold counts, not names"
    );
}

#[test]
fn captured_loot_works_with_manual_loot_json_import_and_prices() {
    let (mut store, session) = store_with_catalog();
    store
        .manual(&session.id, "Thorin", "T4_BAG", Some(3), 1)
        .unwrap();
    import(
        &mut store,
        &session,
        &capture(),
        &["Kazz", "Luna", "Thorin"],
    );
    // Manual price for captured loot of unknown quality, separate from known qualities.
    store.price(&session.id, "T4_BAG", None, Some(500)).unwrap();
    // Resources are quoted by the market as quality 1; the bag's quality is unknown.
    let plan = store.market_plan(&session.id).unwrap();
    assert_eq!(
        plan.wanted,
        [
            ("T4_BAG".to_string(), 3),
            ("T4_WOOD".to_string(), 1),
            ("T5_PLANKS_LEVEL1@1".to_string(), 1)
        ]
    );
    let quote = |item_id: &str, quality, unit_silver| Quote {
        item_id: item_id.into(),
        quality,
        unit_silver,
        observed_at: "2026-09-10T11:00:00.000000Z".into(),
    };
    let result = store
        .apply_market_quotes(
            &session.id,
            &plan,
            &[
                quote("T4_WOOD", 1, 100),
                quote("T5_PLANKS_LEVEL1@1", 1, 900),
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
        // The bag of unknown quality is manually priced and never quoted, so it is in no
        // count; the bag of quality 3 has no offer.
        (2, 1, 0, 0)
    );
    let view = store.view(&session.id, &Filter::default()).unwrap();
    assert_eq!(
        view.totals.session.estimated_silver,
        30 * 100 + 500 + 12 * 900
    );
    assert_eq!(
        view.totals.session.unpriced_events, 1,
        "the manual T4_BAG of quality 3"
    );
    // The JSON export of a session with captured loot replays into it as duplicates.
    let exported: serde_json::Value =
        serde_json::from_str(&store.export(&session.id, "json").unwrap()).unwrap();
    let replay = json!({ "schema_version": 2, "events": exported["events"] }).to_string();
    let result = store.import(&session.id, &replay).unwrap();
    assert_eq!((result.inserted, result.duplicates), (0, 4));
    // Voiding captured loot works like any other loot and survives a reimport.
    let row = store
        .rows(&session.id, &Filter::default())
        .unwrap()
        .into_iter()
        .find(|row| row.event.player == "Kazz")
        .unwrap();
    store
        .set_voided(&session.id, &row.event.source, &row.event.id, true)
        .unwrap();
    let (again, _) = import(
        &mut store,
        &session,
        &capture(),
        &["Kazz", "Luna", "Thorin"],
    );
    assert_eq!(again.inserted, 0);
    assert!(store
        .rows(&session.id, &Filter::default())
        .unwrap()
        .iter()
        .any(|row| row.voided_at.is_some()));
}

#[test]
fn captures_need_an_imported_catalog_a_roster_and_an_open_session() {
    let store = Store::open(":memory:").unwrap();
    let session = store.create_session("Sem catálogo").unwrap();
    let roster = convert::roster(&["Kazz".to_string()]).unwrap();
    let error = convert::to_events(&capture(), &session.id, &roster, |index| {
        store.catalog_item_by_game_index(index)
    })
    .unwrap_err();
    assert!(error.to_string().contains("catálogo"));
    assert!(convert::roster(&[]).is_err());
    assert!(convert::roster(&["  ".to_string()]).is_err());
    assert!(convert::roster(&["a\u{7}b".to_string()]).is_err());
    let (mut store, session) = store_with_catalog();
    store.set_closed(&session.id, true).unwrap();
    let roster = convert::roster(&["Kazz".to_string()]).unwrap();
    let (events, _) = convert::to_events(&capture(), &session.id, &roster, |index| {
        store.catalog_item_by_game_index(index)
    })
    .unwrap();
    assert!(store.ingest(&events, true).is_err());
}

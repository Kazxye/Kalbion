use kalbion_core::{adapters, catalog, domain::*, Store};
use serde_json::json;

fn import_file(version: u32, events: serde_json::Value) -> String {
    json!({ "schema_version": version, "events": events }).to_string()
}
fn v2_event(session_id: &str, id: &str, item_id: &str, quality: Option<u8>) -> serde_json::Value {
    json!({
        "id": id,
        "origin": "observed",
        "source": "test.import",
        "session_id": session_id,
        "occurred_at": "2026-10-01T12:00:00Z",
        "player": "Alice",
        "item": { "id": item_id, "name": "Item" },
        "quality": quality,
        "quantity": 1
    })
}

#[test]
fn replay_is_idempotent_but_identical_legitimate_loot_survives_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.db");
    let mut store = Store::open(&path).unwrap();
    let session = store.create_session("Dungeon").unwrap();
    let event = adapters::simulated(&session.id).remove(0);
    assert_eq!(
        store
            .ingest(std::slice::from_ref(&event), false)
            .unwrap()
            .inserted,
        1
    );
    assert_eq!(
        store
            .ingest(std::slice::from_ref(&event), true)
            .unwrap()
            .duplicates,
        1
    );
    let mut second = event.clone();
    second.id = "another-real-event".into();
    store.ingest(&[second.clone()], false).unwrap();
    let mut collision = event.clone();
    collision.quantity = 10;
    let mut third = second;
    third.id = "rollback-me".into();
    assert!(store.ingest(&[third, collision], false).is_err());
    drop(store);
    let mut store = Store::open(path).unwrap();
    assert_eq!(
        store.rows(&session.id, &Filter::default()).unwrap().len(),
        2
    );
    assert_eq!(store.ingest(&[event], false).unwrap().duplicates, 1);
}

#[test]
fn prices_are_scoped_by_quality_and_estimates_never_become_cash() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.db");
    let mut store = Store::open(&path).unwrap();
    let session = store.create_session("Party").unwrap();
    store
        .manual(&session.id, "Alice", "T4_BAG", Some(1), 2)
        .unwrap();
    store
        .manual(&session.id, "Bob", "T4_BAG", Some(1), 3)
        .unwrap();
    store
        .manual(&session.id, "Alice", "T4_BAG", Some(5), 1)
        .unwrap();
    store
        .manual(&session.id, "Alice", "T4_BAG", None, 4)
        .unwrap();
    store
        .price(&session.id, "T4_BAG", Some(1), Some(100))
        .unwrap();
    store.price(&session.id, "T4_BAG", None, Some(10)).unwrap();
    let view = store.view(&session.id, &Filter::default()).unwrap();
    assert_eq!(view.totals.session.estimated_silver, 540);
    assert_eq!(view.totals.session.unpriced_events, 1);
    assert_eq!(view.totals.players["Alice"].estimated_silver, 240);
    assert_eq!(view.finance.income, 0);
    let bob = Filter {
        player: "bob".into(),
        ..Filter::default()
    };
    let view = store.view(&session.id, &bob).unwrap();
    assert_eq!(view.totals.session.estimated_silver, 300);
    assert_eq!(view.full_totals.session.estimated_silver, 540);
    store
        .add_ledger(&session.id, LedgerKind::Income, "Alice", "Venda", 1001)
        .unwrap();
    store
        .add_ledger(&session.id, LedgerKind::Regear, "Bob", "Regear", 100)
        .unwrap();
    let shares = store
        .record_split(&session.id, 901, &["Bob".into(), "Alice".into()])
        .unwrap();
    assert_eq!(
        (shares[0].player.as_str(), shares[0].silver),
        ("Alice", 451)
    );
    assert_eq!((shares[1].player.as_str(), shares[1].silver), ("Bob", 450));
    assert!(store
        .record_split(&session.id, 1, &["Alice".into()])
        .is_err());
    drop(store);
    let store = Store::open(path).unwrap();
    let view = store.view(&session.id, &Filter::default()).unwrap();
    assert_eq!(view.finance.available, 0);
    assert_eq!(
        view.rows.iter().filter(|row| row.price.is_some()).count(),
        3
    );
}

#[test]
fn invalid_batches_closed_sessions_and_csv_injection_are_handled() {
    let mut store = Store::open(":memory:").unwrap();
    let session = store.create_session("Test").unwrap();
    let mut events = adapters::simulated(&session.id);
    events[1].quantity = 0;
    assert!(store.ingest(&events, true).is_err());
    assert!(store
        .rows(&session.id, &Filter::default())
        .unwrap()
        .is_empty());
    store
        .manual(&session.id, "=HYPERLINK(123)", "T4_BAG", Some(1), 1)
        .unwrap();
    let csv = store.export(&session.id, "csv").unwrap();
    assert!(csv.contains("'=HYPERLINK(123)"));
    store.set_closed(&session.id, true).unwrap();
    assert!(store.simulate(&session.id).is_err());
    assert!(store.export(&session.id, "json").is_ok());
    store.set_closed(&session.id, false).unwrap();
    assert!(store.simulate(&session.id).is_ok());
}

#[test]
fn zero_price_differs_from_missing_price() {
    let mut store = Store::open(":memory:").unwrap();
    let session = store.create_session("Prices").unwrap();
    store
        .manual(&session.id, "Alice", "T4_BAG", Some(2), 1)
        .unwrap();
    let unpriced = |store: &Store| {
        store
            .view(&session.id, &Filter::default())
            .unwrap()
            .totals
            .session
            .unpriced_events
    };
    assert_eq!(unpriced(&store), 1);
    store
        .price(&session.id, "T4_BAG", Some(2), Some(0))
        .unwrap();
    assert_eq!(unpriced(&store), 0);
    store.price(&session.id, "T4_BAG", Some(2), None).unwrap();
    assert_eq!(unpriced(&store), 1);
    assert!(store
        .price(&session.id, "T4_BAG", Some(3), Some(5))
        .is_err());
}

#[test]
fn excessive_estimates_are_rejected_without_poisoning_persisted_data() {
    let mut store = Store::open(":memory:").unwrap();
    let session = store.create_session("Limits").unwrap();
    store
        .manual(&session.id, "Alice", "T4_BAG", Some(1), 1_000_000)
        .unwrap();
    assert!(store
        .price(&session.id, "T4_BAG", Some(1), Some(1_000_000_000_000))
        .is_err());
    assert!(store.rows(&session.id, &Filter::default()).unwrap()[0]
        .price
        .is_none());
    store
        .price(&session.id, "T4_BAG", Some(1), Some(9_000_000_000))
        .unwrap();
    assert!(store
        .manual(&session.id, "Alice", "T4_BAG", Some(1), 1)
        .is_err());
    assert_eq!(
        store.rows(&session.id, &Filter::default()).unwrap().len(),
        1
    );
    assert!(store.export(&session.id, "json").is_ok());
}

#[test]
fn json_export_replays_into_the_same_session_and_settings_survive_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.db");
    let mut store = Store::open(&path).unwrap();
    store
        .save_settings(&Settings {
            server: "europe".into(),
            city: "Martlock".into(),
        })
        .unwrap();
    let session = store.create_session("Exports").unwrap();
    store.simulate(&session.id).unwrap();
    store
        .add_ledger(
            &session.id,
            LedgerKind::Expense,
            "Alice",
            "=formula, with comma",
            42,
        )
        .unwrap();
    let exported: serde_json::Value =
        serde_json::from_str(&store.export(&session.id, "json").unwrap()).unwrap();
    assert_eq!(exported["events"].as_array().unwrap().len(), 7);
    assert_eq!(exported["ledger"][0]["amount"], 42);
    assert_eq!(exported["session"]["server"], "europe");
    let replay = import_file(2, exported["events"].clone());
    let result = store.import(&session.id, &replay).unwrap();
    assert_eq!((result.inserted, result.duplicates), (0, 7));
    let csv = store.export(&session.id, "csv").unwrap();
    let records = csv::Reader::from_reader(csv.as_bytes())
        .records()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(records.len(), 8);
    assert_eq!(&records[7][23], "'=formula, with comma");
    drop(store);
    let store = Store::open(path).unwrap();
    assert_eq!(store.settings().unwrap().city, "Martlock");
}

#[test]
fn equal_instants_with_different_offsets_are_the_same_event() {
    let mut store = Store::open(":memory:").unwrap();
    let session = store.create_session("Offsets").unwrap();
    let mut event = v2_event(&session.id, "e1", "T4_BAG", Some(1));
    event["occurred_at"] = json!("2026-10-01T14:00:00+02:00");
    assert_eq!(
        store
            .import(&session.id, &import_file(2, json!([event.clone()])))
            .unwrap()
            .inserted,
        1
    );
    event["occurred_at"] = json!("2026-10-01T12:00:00.000Z");
    assert_eq!(
        store
            .import(&session.id, &import_file(2, json!([event])))
            .unwrap()
            .duplicates,
        1
    );
}

#[test]
fn items_without_tier_and_unknown_quality_are_kept_distinct() {
    let mut store = Store::open(":memory:").unwrap();
    let session = store.create_session("Treasure").unwrap();
    let file = import_file(
        2,
        json!([
            v2_event(&session.id, "a", "UNIQUE_HIDEOUT", None),
            v2_event(&session.id, "b", "T5_BAG@1", Some(3)),
        ]),
    );
    assert_eq!(store.import(&session.id, &file).unwrap().inserted, 2);
    let rows = store.rows(&session.id, &Filter::default()).unwrap();
    let hideout = rows.iter().find(|row| row.event.id == "a").unwrap();
    assert_eq!(
        (hideout.event.item.tier, hideout.event.quality),
        (None, None)
    );
    let unknown = Filter {
        quality: Some(0),
        ..Filter::default()
    };
    assert_eq!(store.rows(&session.id, &unknown).unwrap().len(), 1);
    let tier5 = Filter {
        tier: Some(5),
        enchantment: Some(1),
        ..Filter::default()
    };
    assert_eq!(store.rows(&session.id, &tier5).unwrap()[0].event.id, "b");

    let mut mismatch = v2_event(&session.id, "c", "T5_BAG@1", Some(1));
    mismatch["item"]["tier"] = json!(6);
    assert!(store
        .import(&session.id, &import_file(2, json!([mismatch])))
        .is_err());
    let mut bad_quality = v2_event(&session.id, "d", "T5_BAG", Some(1));
    bad_quality["quality"] = json!(6);
    assert!(store
        .import(&session.id, &import_file(2, json!([bad_quality])))
        .is_err());
    assert_eq!(
        store.rows(&session.id, &Filter::default()).unwrap().len(),
        2
    );
}

#[test]
fn voided_loot_leaves_totals_but_still_deduplicates() {
    let mut store = Store::open(":memory:").unwrap();
    let session = store.create_session("Void").unwrap();
    let event = adapters::simulated(&session.id).remove(0);
    store.ingest(std::slice::from_ref(&event), false).unwrap();
    store
        .manual(&session.id, "Bob", "T4_BAG", Some(1), 1)
        .unwrap();
    store
        .set_voided(&session.id, &event.source, &event.id, true)
        .unwrap();
    assert!(store
        .set_voided(&session.id, &event.source, &event.id, true)
        .is_err());
    let view = store.view(&session.id, &Filter::default()).unwrap();
    assert_eq!(view.rows.len(), 2);
    assert_eq!(view.full_totals.session.events, 1);
    assert!(!view.full_totals.players.contains_key(&event.player));
    assert_eq!(
        store
            .ingest(std::slice::from_ref(&event), true)
            .unwrap()
            .duplicates,
        1
    );
    store
        .set_voided(&session.id, &event.source, &event.id, false)
        .unwrap();
    let view = store.view(&session.id, &Filter::default()).unwrap();
    assert_eq!(view.full_totals.session.events, 2);
}

#[test]
fn ledger_mistakes_are_corrected_by_reversal_not_edits() {
    let mut store = Store::open(":memory:").unwrap();
    let session = store.create_session("Ledger").unwrap();
    let wrong = store
        .add_ledger(&session.id, LedgerKind::Income, "Alice", "Typo", 10_000)
        .unwrap();
    store
        .add_ledger(&session.id, LedgerKind::Income, "Alice", "Venda", 1_000)
        .unwrap();
    let reversal = store.reverse_ledger(&session.id, &wrong.id).unwrap();
    assert!(store.reverse_ledger(&session.id, &wrong.id).is_err());
    assert!(store.reverse_ledger(&session.id, &reversal.id).is_err());
    let view = store.view(&session.id, &Filter::default()).unwrap();
    assert_eq!(view.ledger.len(), 3);
    assert_eq!(view.finance.income, 1_000);
    assert_eq!(view.finance.available, 1_000);
    assert!(store
        .record_split(&session.id, 1_001, &["Alice".into()])
        .is_err());
    store
        .record_split(&session.id, 1_000, &["Alice".into()])
        .unwrap();
}

#[test]
fn split_rejects_names_that_differ_only_by_case_or_spaces() {
    assert!(split(10, &["Alice".into(), "alice".into()]).is_err());
    assert!(split(10, &["Alice".into(), " Alice ".into()]).is_err());
    let shares = split(10, &[" Bob ".into(), "Alice".into(), "Carl".into()]).unwrap();
    let names: Vec<_> = shares.iter().map(|share| share.player.as_str()).collect();
    assert_eq!(names, ["Alice", "Bob", "Carl"]);
    assert_eq!(shares.iter().map(|share| share.silver).sum::<i64>(), 10);
}

#[test]
fn imported_catalog_replaces_demo_items_for_manual_loot() {
    let dump = json!([
        { "Index": "1", "UniqueName": "UNIQUE_HIDEOUT", "LocalizedNames": { "EN-US": "Hideout Kit", "PT-BR": "Kit de Esconderijo" } },
        { "Index": "2", "UniqueName": "T4_BAG", "LocalizedNames": { "EN-US": "Adept's Bag" } },
        { "Index": "3", "UniqueName": "T4_BAG@1", "LocalizedNames": null },
        { "Index": "4", "UniqueName": "bad name", "LocalizedNames": null }
    ])
    .to_string();
    let parsed = catalog::parse_ao_bin_dumps(dump.as_bytes()).unwrap();
    assert_eq!((parsed.entries.len(), parsed.skipped), (3, 1));
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.db");
    let mut store = Store::open(&path).unwrap();
    let session = store.create_session("Catalog").unwrap();
    assert_eq!(store.catalog_info().unwrap().kind, "builtin");
    store
        .manual(&session.id, "Alice", "T6_ORE", None, 1)
        .unwrap();
    let info = store.replace_catalog(&parsed, "items.json").unwrap();
    assert_eq!((info.item_count, info.skipped_count), (3, 1));
    assert!(store
        .manual(&session.id, "Alice", "T6_ORE", None, 1)
        .is_err());
    store
        .manual(&session.id, "Alice", "UNIQUE_HIDEOUT", None, 1)
        .unwrap();
    let found = store.search_catalog("esconderijo", 10).unwrap();
    assert_eq!(found[0].id, "UNIQUE_HIDEOUT");
    assert_eq!(
        store.search_catalog("t4_bag@1", 10).unwrap()[0].name,
        "T4_BAG@1"
    );
    drop(store);
    let store = Store::open(path).unwrap();
    assert_eq!(store.catalog_info().unwrap().label, "items.json");
    let names: Vec<_> = store
        .rows(&session.id, &Filter::default())
        .unwrap()
        .into_iter()
        .map(|row| row.event.item.name)
        .collect();
    assert!(names.contains(&"Minério de Runita".to_string()));

    let duplicate = json!([
        { "Index": "1", "UniqueName": "T4_BAG", "LocalizedNames": null },
        { "Index": "2", "UniqueName": "T4_BAG", "LocalizedNames": null }
    ])
    .to_string();
    assert!(catalog::parse_ao_bin_dumps(duplicate.as_bytes()).is_err());
}

#[test]
fn version_one_database_is_migrated_with_backup_and_old_exports_still_replay() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("kalbion.db");
    let old_event = json!({
        "id": "old-1", "origin": "simulated", "source": "kalbion.simulator.v1",
        "session_id": "s1", "occurred_at": "2026-10-01T04:22:32.137513+00:00",
        "player": "Kazz",
        "item": { "id": "T5_BAG@1", "name": "Bolsa do Especialista", "tier": 5, "enchantment": 1, "quality": 2 },
        "quantity": 1
    });
    let wood_events: Vec<_> = [("wood-1", 1), ("wood-2", 2)]
        .into_iter()
        .map(|(id, quality)| {
            json!({
                "id": id, "origin": "manual", "source": "kalbion.manual.v1",
                "session_id": "s1", "occurred_at": "2026-10-01T04:30:00+00:00", "player": "Luna",
                "item": { "id": "T4_WOOD", "name": "Toras de Pinho", "tier": 4, "enchantment": 0, "quality": quality },
                "quantity": 10
            })
        })
        .collect();
    {
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch(include_str!("../migrations/001_initial.sql"))
            .unwrap();
        connection
            .execute(
                "INSERT INTO sessions VALUES ('s1', 'Antiga', '2026-10-01T04:00:00+00:00', NULL, 'europe', 'Lymhurst')",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO events VALUES ('kalbion.simulator.v1', 'old-1', 's1', ?1, 0)",
                [old_event.to_string()],
            )
            .unwrap();
        connection.execute(
            "INSERT INTO prices VALUES ('s1', 'T5_BAG@1', 2, ?1)",
            [json!({ "unit_silver": 500, "source": "manual", "server": "europe", "city": "Lymhurst", "queried_at": "2026-10-01T05:00:00+00:00" }).to_string()],
        ).unwrap();
        connection.execute(
            "INSERT INTO ledger VALUES ('l1', 's1', ?1)",
            [json!({ "id": "l1", "session_id": "s1", "kind": "income", "player": "Kazz", "description": "Venda", "amount": 700, "occurred_at": "2026-10-01T06:00:00+00:00" }).to_string()],
        ).unwrap();
        // The old manual form defaulted resources to Normal; prices followed that quality.
        for event in &wood_events {
            let id = event["id"].as_str().unwrap();
            connection
                .execute(
                    "INSERT INTO events VALUES ('kalbion.manual.v1', ?1, 's1', ?2, 0)",
                    [id.to_string(), event.to_string()],
                )
                .unwrap();
        }
        for (quality, amount, at) in [
            (1, 3, "2026-10-01T05:00:00+00:00"),
            (2, 9, "2026-10-01T05:30:00+00:00"),
        ] {
            connection.execute(
                "INSERT INTO prices VALUES ('s1', 'T4_WOOD', ?1, ?2)",
                rusqlite::params![quality, json!({ "unit_silver": amount, "source": "manual", "server": "europe", "city": "Lymhurst", "queried_at": at }).to_string()],
            ).unwrap();
        }
        connection
            .execute(
                "INSERT INTO settings VALUES (1, ?1)",
                [json!({ "server": "europe", "city": "Lymhurst" }).to_string()],
            )
            .unwrap();
    }
    let mut store = Store::open(&path).unwrap();
    let backups = std::fs::read_dir(directory.path())
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("kalbion-v1-backup-")
        })
        .count();
    assert_eq!(backups, 1);
    assert_eq!(store.settings().unwrap().city, "Lymhurst");
    let view = store.view("s1", &Filter::default()).unwrap();
    let bag = view
        .rows
        .iter()
        .find(|row| row.event.id == "old-1")
        .unwrap();
    assert_eq!(bag.event.occurred_at, "2026-10-01T04:22:32.137513Z");
    assert_eq!(bag.event.quality, Some(2));
    // Resource loot loses its meaningless quality; the most recent of its prices wins.
    let wood: Vec<_> = view
        .rows
        .iter()
        .filter(|row| row.event.item.id == "T4_WOOD")
        .collect();
    assert_eq!(wood.len(), 2);
    assert!(wood.iter().all(|row| row.event.quality.is_none()
        && row
            .price
            .as_ref()
            .is_some_and(|price| price.unit_silver == 9)));
    assert_eq!(view.totals.session.estimated_silver, 500 + 20 * 9);
    assert_eq!(view.finance.income, 700);
    let result = store
        .import("s1", &import_file(1, json!([old_event])))
        .unwrap();
    assert_eq!(result.duplicates, 1);
    // Old exports that still say quality 1 or 2 for the wood replay as duplicates.
    let result = store
        .import("s1", &import_file(1, json!(wood_events)))
        .unwrap();
    assert_eq!((result.inserted, result.duplicates), (0, 2));
    drop(store);
    assert!(Store::open(&path).is_ok());
}

/// A development database already at version 3 (before resources lost their quality) must
/// still be normalized: version 4 runs on it, with a backup, and old exports keep replaying.
#[test]
fn version_three_database_moves_resources_to_no_quality() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("kalbion.db");
    {
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch(include_str!("../migrations/001_initial.sql"))
            .unwrap();
        connection
            .execute_batch(include_str!("../migrations/002_structured.sql"))
            .unwrap();
        connection
            .execute_batch(
                "DROP TABLE events; DROP TABLE prices; DROP TABLE ledger; DROP TABLE settings;",
            )
            .unwrap();
        connection
            .execute_batch(include_str!("../migrations/003_market_prices.sql"))
            .unwrap();
        connection.pragma_update(None, "user_version", 3).unwrap();
        connection
            .execute_batch(
                "INSERT INTO sessions VALUES ('s3', 'Dev', '2026-10-01T10:00:00.000000Z', NULL, 'americas', 'Martlock');
                 INSERT INTO loot_events VALUES
                   ('kalbion.manual.v1', 'planks', 's3', 'manual', 0, '2026-10-01T10:01:00.000000Z', 'Ana', 'T5_PLANKS_LEVEL1@1', 'Tábuas', 5, 1, 1, 20, NULL),
                   ('kalbion.manual.v1', 'bar', 's3', 'manual', 0, '2026-10-01T10:02:00.000000Z', 'Ana', 'T6_METALBAR', 'Barras', 6, 0, 3, 5, NULL),
                   ('kalbion.manual.v1', 'bag', 's3', 'manual', 0, '2026-10-01T10:03:00.000000Z', 'Ana', 'T4_BAG', 'Bolsa', 4, 0, 3, 1, NULL),
                   ('kalbion.manual.v1', 'armor', 's3', 'manual', 0, '2026-10-01T10:04:00.000000Z', 'Ana', 'T4_ARMOR_LEATHER_SET1', 'Jaqueta', 4, 0, 2, 1, NULL);
                 INSERT INTO item_prices VALUES
                   ('s3', 'T5_PLANKS_LEVEL1@1', 1, 40, 'manual', 'americas', 'Martlock', '2026-10-01T10:05:00.000000Z', NULL),
                   ('s3', 'T6_METALBAR', 3, 70, 'manual', 'americas', 'Martlock', '2026-10-01T10:05:00.000000Z', NULL),
                   ('s3', 'T6_METALBAR', 0, 60, 'manual', 'americas', 'Martlock', '2026-10-01T10:06:00.000000Z', NULL),
                   ('s3', 'T4_BAG', 3, 900, 'albion_data', 'americas', 'Martlock', '2026-10-01T10:07:00.000000Z', '2026-10-01T09:00:00.000000Z'),
                   ('s3', 'T4_ARMOR_LEATHER_SET1', 2, 300, 'manual', 'americas', 'Martlock', '2026-10-01T10:07:00.000000Z', NULL);",
            )
            .unwrap();
    }
    let mut store = Store::open(&path).unwrap();
    let names: Vec<String> = std::fs::read_dir(directory.path())
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names
            .iter()
            .filter(|name| name.starts_with("kalbion-v3-backup-"))
            .count(),
        1
    );
    let view = store.view("s3", &Filter::default()).unwrap();
    let row = |id: &str| view.rows.iter().find(|row| row.event.id == id).unwrap();
    assert_eq!(row("planks").event.quality, None);
    assert_eq!(row("planks").price.as_ref().unwrap().unit_silver, 40);
    assert_eq!(row("bar").event.quality, None);
    // Two prices for the same resource: the most recent one wins.
    assert_eq!(row("bar").price.as_ref().unwrap().unit_silver, 60);
    // Equipment keeps its quality and its prices, market prices included.
    assert_eq!(row("bag").event.quality, Some(3));
    assert_eq!(
        row("bag").price.as_ref().unwrap().source,
        PriceSource::AlbionData
    );
    assert_eq!(row("armor").event.quality, Some(2));
    assert_eq!(row("armor").price.as_ref().unwrap().unit_silver, 300);
    assert_eq!(
        view.totals.session.estimated_silver,
        20 * 40 + 5 * 60 + 900 + 300
    );
    // An export made by the version 3 build said quality 1 and 3 for these resources.
    let old_export = json!([
        { "id": "planks", "source": "kalbion.manual.v1", "origin": "manual", "session_id": "s3",
          "occurred_at": "2026-10-01T10:01:00Z", "player": "Ana",
          "item": { "id": "T5_PLANKS_LEVEL1@1", "name": "Tábuas", "tier": 5, "enchantment": 1 },
          "quality": 1, "quantity": 20 },
        { "id": "bar", "source": "kalbion.manual.v1", "origin": "manual", "session_id": "s3",
          "occurred_at": "2026-10-01T10:02:00Z", "player": "Ana",
          "item": { "id": "T6_METALBAR", "name": "Barras", "tier": 6, "enchantment": 0 },
          "quality": 3, "quantity": 5 }
    ]);
    let result = store.import("s3", &import_file(2, old_export)).unwrap();
    assert_eq!((result.inserted, result.duplicates), (0, 2));
    // Only the market-priced bag is refreshed; manual prices (resources included) prevail.
    let plan = store.market_plan("s3").unwrap();
    assert_eq!(plan.wanted, [("T4_BAG".to_string(), 3)]);
    drop(store);
    let connection = rusqlite::Connection::open(&path).unwrap();
    let version: u32 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 5);
    connection
        .execute(
            "INSERT INTO item_prices VALUES ('s3', 'T4_WOOD', 0, 5, 'albion_data', 'americas',
             'Martlock', '2026-10-01T11:00:00.000000Z', '2026-10-01T10:00:00.000000Z')",
            [],
        )
        .unwrap();
    assert!(connection
        .execute(
            "INSERT INTO item_prices VALUES ('s3', 'T4_ORE', 0, 5, 'albion_data', 'americas',
             'Martlock', '2026-10-01T11:00:00.000000Z', NULL)",
            [],
        )
        .is_err());
}

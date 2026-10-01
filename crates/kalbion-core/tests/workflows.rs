use kalbion_core::{adapters, domain::*, Store};

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
    assert_eq!(store.rows(&session.id).unwrap().len(), 2);
    assert_eq!(store.ingest(&[event], false).unwrap().duplicates, 1);
}

#[test]
fn prices_are_scoped_and_estimates_never_become_cash() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.db");
    let mut store = Store::open(&path).unwrap();
    let session = store.create_session("Party").unwrap();
    let item = adapters::catalog().remove(0);
    store.manual(&session.id, "Alice", item.clone(), 2).unwrap();
    store.manual(&session.id, "Bob", item.clone(), 3).unwrap();
    let mut quality = item.clone();
    quality.quality = 5;
    store.manual(&session.id, "Alice", quality, 1).unwrap();
    store.price(&session.id, &item.id, 1, Some(100)).unwrap();
    let view = store.view(&session.id, &Filter::default()).unwrap();
    assert_eq!(view.totals.session.estimated_silver, 500);
    assert_eq!(view.totals.session.unpriced_events, 1);
    assert_eq!(view.totals.players["Alice"].estimated_silver, 200);
    assert_eq!(view.finance.income, 0);
    let filter = Filter {
        player: "bob".into(),
        ..Filter::default()
    };
    let view = store.view(&session.id, &filter).unwrap();
    assert_eq!(view.totals.session.estimated_silver, 300);
    assert_eq!(view.full_totals.session.estimated_silver, 500);
    store
        .add_ledger(&session.id, LedgerKind::Income, "Alice", "Venda", 1001)
        .unwrap();
    store
        .add_ledger(&session.id, LedgerKind::Regear, "Bob", "Regear", 100)
        .unwrap();
    let shares = store
        .record_split(&session.id, 901, &["Bob".into(), "Alice".into()])
        .unwrap();
    assert_eq!(shares[0].silver, 451);
    assert_eq!(shares[1].silver, 450);
    assert!(store
        .record_split(&session.id, 1, &["Alice".into()])
        .is_err());
    drop(store);
    let store = Store::open(path).unwrap();
    assert_eq!(
        store
            .view(&session.id, &Filter::default())
            .unwrap()
            .finance
            .available,
        0
    );
    assert_eq!(
        store
            .rows(&session.id)
            .unwrap()
            .iter()
            .filter(|row| row.price.is_some())
            .count(),
        2
    );
}

#[test]
fn invalid_batches_closed_sessions_and_csv_injection_are_handled() {
    let mut store = Store::open(":memory:").unwrap();
    let session = store.create_session("Test").unwrap();
    let mut events = adapters::simulated(&session.id);
    events[1].quantity = 0;
    assert!(store.ingest(&events, true).is_err());
    assert!(store.rows(&session.id).unwrap().is_empty());
    store
        .manual(
            &session.id,
            "=HYPERLINK(123)",
            adapters::catalog().remove(0),
            1,
        )
        .unwrap();
    let csv = store.export(&session.id, "csv").unwrap();
    assert!(csv.contains("'=HYPERLINK(123)"));
    assert!(csv.contains(",1,,"));
    store.set_closed(&session.id, true).unwrap();
    assert!(store.simulate(&session.id).is_err());
    assert!(store.export(&session.id, "json").is_ok());
    store.set_closed(&session.id, false).unwrap();
    assert!(store.simulate(&session.id).is_ok());
}

#[test]
fn zero_price_differs_from_missing_and_import_replays() {
    let mut store = Store::open(":memory:").unwrap();
    let session = store.create_session("Import").unwrap();
    let event = adapters::simulated(&session.id).remove(0);
    let json = serde_json::to_string(&adapters::ImportBatch {
        schema_version: 1,
        events: vec![event.clone()],
    })
    .unwrap();
    assert_eq!(store.import(&session.id, &json).unwrap().inserted, 1);
    assert_eq!(store.import(&session.id, &json).unwrap().duplicates, 1);
    store
        .price(&session.id, &event.item.id, event.item.quality, Some(0))
        .unwrap();
    assert_eq!(
        store
            .view(&session.id, &Filter::default())
            .unwrap()
            .totals
            .session
            .unpriced_events,
        0
    );
    store
        .price(&session.id, &event.item.id, event.item.quality, None)
        .unwrap();
    assert_eq!(
        store
            .view(&session.id, &Filter::default())
            .unwrap()
            .totals
            .session
            .unpriced_events,
        1
    );
}

#[test]
fn excessive_estimates_are_rejected_without_poisoning_persisted_data() {
    let mut store = Store::open(":memory:").unwrap();
    let session = store.create_session("Limits").unwrap();
    let item = adapters::catalog().remove(0);
    store
        .manual(&session.id, "Alice", item.clone(), 1_000_000)
        .unwrap();
    assert!(store
        .price(&session.id, &item.id, 1, Some(1_000_000_000_000))
        .is_err());
    assert!(store.rows(&session.id).unwrap()[0].price.is_none());
    store
        .price(&session.id, &item.id, 1, Some(9_000_000_000))
        .unwrap();
    assert!(store.manual(&session.id, "Alice", item, 1).is_err());
    assert_eq!(store.rows(&session.id).unwrap().len(), 1);
    assert!(store.export(&session.id, "json").is_ok());
}

#[test]
fn exports_preserve_financial_context_and_settings_survive_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.db");
    let mut store = Store::open(&path).unwrap();
    store
        .save_settings(Settings {
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
    let csv = store.export(&session.id, "csv").unwrap();
    let records = csv::Reader::from_reader(csv.as_bytes())
        .records()
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(records.len(), 8);
    assert_eq!(&records[7][21], "'=formula, with comma");
    drop(store);
    let store = Store::open(path).unwrap();
    assert_eq!(store.settings().unwrap().city, "Martlock");
}

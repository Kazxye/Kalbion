//! The live capture path: frame-by-frame decoding must match the file import, and the
//! diagnostic trace must never carry player or container names. Synthetic input only.
mod common;
use common::*;
use kalbion_capture::{decode, pcap::Reader, trace::Tracer, Limits, Pipeline};
use serde_json::Value;

fn capture() -> Vec<u8> {
    pcap(&[
        (
            T0 + 1,
            0,
            from_server(&packet(&[reliable(1, &loot("Baú", "Kazz", 950, 30))])),
        ),
        (
            T0 + 2,
            0,
            from_server(&packet(&[reliable(2, &silver("Luna"))])),
        ),
        // A watched item event (98, NewLoot) with a game identifier and a name.
        (
            T0 + 3,
            0,
            from_server(&packet(&[reliable(
                3,
                &event(&[
                    (1, P::Str("T4_BAG@1")),
                    (2, P::Str("Thorin")),
                    (3, P::Int(77)),
                    (252, P::Short(98)),
                ]),
            )])),
        ),
        (
            T0 + 4,
            0,
            from_server(&packet(&[reliable(4, &other_event(3))])),
        ),
        // 12 s later: the code histogram of the first window is written before this one.
        (
            T0 + 16,
            0,
            from_server(&packet(&[reliable(5, &other_event(3))])),
        ),
    ])
}

fn live(bytes: &[u8]) -> (Vec<kalbion_capture::Observation>, Vec<Value>) {
    let mut reader = Reader::new(bytes, Limits::default().max_packet_bytes, u64::MAX).unwrap();
    let mut pipeline = Pipeline::new(None);
    let mut tracer = Tracer::new();
    let mut observations = Vec::new();
    let mut records = Vec::new();
    while let Some(frame) = reader.next_frame().unwrap() {
        pipeline
            .frame(&frame, &mut observations, &mut |seen| {
                tracer.seen(&seen, &mut records)
            })
            .unwrap();
    }
    tracer.flush(&mut records);
    (observations, records)
}

#[test]
fn live_decoding_matches_the_file_import() {
    let bytes = capture();
    let (observations, _) = live(&bytes);
    let file = decode(&bytes[..], Limits::default()).unwrap();
    assert_eq!(observations, file.observations);
    assert_eq!(observations.len(), 1);
}

#[test]
fn the_trace_shows_item_events_and_counts_the_rest_without_names() {
    let (_, records) = live(&capture());
    let text = records
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    for name in ["Kazz", "Luna", "Thorin", "Baú"] {
        assert!(
            !text.contains(name),
            "{name} leaked into the trace:\n{text}"
        );
    }

    let events: Vec<&Value> = records.iter().filter(|r| r["kind"] == "event").collect();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0]["code"], 279);
    assert_eq!(events[0]["classified"], "loot");
    assert_eq!(events[0]["parameters"]["4"], 950);
    assert_eq!(events[0]["parameters"]["2"], "<texto: 4 caracteres>");
    assert_eq!(events[1]["classified"], "silver");
    assert_eq!(events[2]["name"], "NewLoot");
    assert_eq!(events[2]["parameters"]["1"], "T4_BAG@1");
    assert_eq!(events[2]["parameters"]["3"], 77);

    let windows: Vec<&Value> = records.iter().filter(|r| r["kind"] == "codes").collect();
    assert_eq!(windows.len(), 2);
    assert_eq!(windows[0]["counts"]["3"], 1);
    assert_eq!(windows[1]["counts"]["3"], 1);
}

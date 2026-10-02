//! Photon fragment reassembly against hostile or inconsistent fragment headers.
//! Synthetic input only; drives the decoder directly to observe its pending state.
mod common;
use common::*;
use kalbion_capture::net::Endpoint;
use kalbion_capture::photon::{Datagram, Decoder, EventMessage};
use kalbion_capture::Diagnostics;
use std::net::{IpAddr, Ipv4Addr};

/// Mirrors `MAX_PENDING` in `photon.rs`.
const MAX_PENDING: usize = 64;

/// Fragment command with every header field chosen by the test.
fn fragment(
    sequence: u32,
    start: u32,
    count: u32,
    number: u32,
    total: usize,
    offset: usize,
    data: &[u8],
) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend(start.to_be_bytes());
    payload.extend(count.to_be_bytes());
    payload.extend(number.to_be_bytes());
    payload.extend((total as u32).to_be_bytes());
    payload.extend((offset as u32).to_be_bytes());
    payload.extend(data);
    command(8, 0, sequence, &payload)
}

struct Harness {
    decoder: Decoder,
    diagnostics: Diagnostics,
    out: Vec<EventMessage>,
    packets: u64,
}

impl Harness {
    fn new() -> Self {
        Self {
            decoder: Decoder::new(),
            diagnostics: Diagnostics::default(),
            out: Vec::new(),
            packets: 0,
        }
    }

    fn push(&mut self, commands: &[Vec<u8>]) {
        self.packets += 1;
        let photon = packet(commands);
        let datagram = Datagram {
            packet: self.packets,
            timestamp_ns: 0,
            source: Endpoint {
                address: IpAddr::V4(Ipv4Addr::from(SERVER)),
                port: 5056,
            },
            destination: Endpoint {
                address: IpAddr::V4(Ipv4Addr::from(CLIENT)),
                port: CLIENT_PORT,
            },
            payload: &photon,
        };
        self.decoder
            .push(&datagram, &mut self.diagnostics, &mut self.out);
    }
}

#[test]
fn overlapping_fragments_that_add_up_to_the_total_are_rejected() {
    let whole = message(4, &loot("Baú", "Thorin", 1088, 12));
    let total = whole.len();
    // [0, 6) and [4, total - 2): sizes add up to `total`, but [total - 2, total) is never written.
    let mut harness = Harness::new();
    harness.push(&[fragment(1, 1, 2, 0, total, 0, &whole[..6])]);
    harness.push(&[fragment(2, 1, 2, 1, total, 4, &whole[4..total - 2])]);

    assert!(
        harness.out.is_empty(),
        "a message with a gap must not be emitted"
    );
    assert_eq!(harness.diagnostics.malformed_photon, 1);
    assert_eq!(harness.decoder.incomplete(), 0);
}

#[test]
fn contiguous_fragments_out_of_order_still_reassemble() {
    let whole = message(4, &loot("Baú", "Thorin", 1088, 12));
    let total = whole.len();
    let mut harness = Harness::new();
    harness.push(&[fragment(2, 1, 2, 1, total, 10, &whole[10..])]);
    harness.push(&[fragment(1, 1, 2, 0, total, 0, &whole[..10])]);

    assert_eq!(harness.out.len(), 1);
    assert_eq!(harness.out[0].data, whole[2..]);
    assert_eq!(harness.diagnostics.malformed_photon, 0);
}

#[test]
fn a_conflicting_message_never_lets_pending_exceed_the_cap() {
    let mut harness = Harness::new();
    let mut sequence = 0;
    let mut next = || {
        sequence += 1;
        sequence
    };

    // Same start sequence, different total: both are dropped.
    harness.push(&[fragment(next(), 1, 2, 0, 100, 0, &[0; 10])]);
    harness.push(&[fragment(next(), 1, 2, 1, 200, 10, &[0; 10])]);
    assert_eq!(harness.decoder.incomplete(), 0);
    assert_eq!(harness.diagnostics.malformed_photon, 1);

    // Fill the cap, then go past it: every extra message must evict one, never grow.
    for start in 0..(MAX_PENDING as u32 + 8) {
        harness.push(&[fragment(next(), 1000 + start, 2, 0, 100, 0, &[0; 10])]);
        assert!(
            harness.decoder.incomplete() <= MAX_PENDING,
            "pending grew to {}",
            harness.decoder.incomplete()
        );
    }
    assert_eq!(harness.decoder.incomplete(), MAX_PENDING);
    assert_eq!(harness.diagnostics.fragments_dropped, 8);
}

#[test]
fn a_restarted_message_is_not_evicted_by_its_dropped_predecessor() {
    let whole = message(4, &loot("Baú", "Thorin", 1088, 12));
    let total = whole.len();
    let mut harness = Harness::new();
    let mut sequence = 0;
    let mut next = || {
        sequence += 1;
        sequence
    };

    // Start sequence 1 conflicts and is dropped.
    harness.push(&[fragment(next(), 1, 2, 0, 100, 0, &[0; 10])]);
    harness.push(&[fragment(next(), 1, 2, 1, 200, 10, &[0; 10])]);
    // Oldest live message, then a valid message that reuses start sequence 1.
    harness.push(&[fragment(next(), 999, 2, 0, 100, 0, &[0; 10])]);
    harness.push(&[fragment(next(), 1, 2, 0, total, 0, &whole[..10])]);

    // Reach the cap, then one more: the eviction must take start 999 (the oldest live one).
    for start in 0..(MAX_PENDING as u32 - 1) {
        harness.push(&[fragment(next(), 1000 + start, 2, 0, 100, 0, &[0; 10])]);
    }
    assert_eq!(harness.diagnostics.fragments_dropped, 1);

    harness.push(&[fragment(next(), 1, 2, 1, total, 10, &whole[10..])]);
    assert_eq!(
        harness.out.len(),
        1,
        "the live message must survive and complete"
    );
    assert_eq!(harness.out[0].data, whole[2..]);
}

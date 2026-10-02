//! Decoder tests on synthetic captures. Nothing here comes from real game traffic.
mod common;
use common::*;
use kalbion_capture::{albion::Loot, decode, photon::Position, CaptureError, Limits, Observation};

fn run(bytes: &[u8]) -> Result<kalbion_capture::Capture, CaptureError> {
    decode(bytes, Limits::default())
}
fn observation(
    packet: u64,
    command: u32,
    micros: u32,
    by: &str,
    item: u32,
    quantity: u32,
) -> Observation {
    Observation {
        position: Position { packet, command },
        occurred_at: format!("2026-09-10T12:00:{:02}.{micros:06}Z", packet),
        loot: Loot {
            looted_by: by.into(),
            item_index: item,
            quantity,
        },
    }
}

/// A capture with every situation the decoder must handle, and its expected loot.
fn scenario() -> (Frames, Vec<Observation>) {
    let parts = fragments(40, &loot("Baú", "Thorin", 1088, 12), 12);
    let count = parts.len();
    assert!(count >= 3);
    // Last fragment first; then the first one again (a retransmission) and the rest.
    let opening = vec![parts[count - 1].clone(), parts[0].clone()];
    let closing: Vec<_> = parts[..count - 1].to_vec();
    let frames = vec![
        // 1: the client's own requests are ignored.
        (
            T0 + 1,
            0,
            from_client(&packet(&[reliable(1, &other_event(22))])),
        ),
        // 2: a reliable loot event and an unrelated event in the same packet.
        (
            T0 + 2,
            0,
            from_server(&packet(&[
                reliable(10, &other_event(123)),
                reliable(11, &loot("Mob", "Kazz", 950, 30)),
            ])),
        ),
        // 3: the same reliable command again (retransmission) and a silver pick-up.
        (
            T0 + 3,
            0,
            from_server(&packet(&[
                reliable(11, &loot("Mob", "Kazz", 950, 30)),
                reliable(12, &silver("Kazz")),
            ])),
        ),
        // 4: an unreliable loot event.
        (
            T0 + 4,
            0,
            from_server(&packet(&[unreliable(12, &loot("Chest", "Luna", 2888, 1))])),
        ),
        // 5: a fragmented loot event, last fragment first, plus a duplicated fragment.
        (T0 + 5, 0, from_server(&packet(&opening))),
        (T0 + 6, 0, from_server(&packet(&closing))),
        // 7: CRC-protected packet with loot; 8: the same with a wrong CRC.
        (
            T0 + 7,
            0,
            from_server(&packet_with_crc(
                &[reliable(50, &loot("Mob", "Kazz", 4100, 1))],
                false,
            )),
        ),
        (
            T0 + 8,
            0,
            from_server(&packet_with_crc(
                &[reliable(51, &loot("Mob", "Kazz", 4100, 9))],
                true,
            )),
        ),
        // 9: an encrypted packet.
        (
            T0 + 9,
            0,
            from_server(&[vec![0, 1, 1, 1], vec![0xAB; 20]].concat()),
        ),
        // 10: two Photon packets coalesced in one datagram.
        (
            T0 + 10,
            250_000,
            from_server(
                &[
                    packet(&[reliable(60, &loot("Mob", "Luna", 950, 5))]),
                    packet(&[reliable(61, &other_event(3))]),
                ]
                .concat(),
            ),
        ),
    ];
    let expected = vec![
        observation(2, 1, 0, "Kazz", 950, 30),
        observation(4, 0, 0, "Luna", 2888, 1),
        observation(6, count as u32 - 2, 0, "Thorin", 1088, 12),
        observation(7, 0, 0, "Kazz", 4100, 1),
        observation(10, 0, 250_000, "Luna", 950, 5),
    ];
    (frames, expected)
}

#[test]
fn golden_bytes_match_the_builder() {
    // Written by hand from the format description, independent of the builder.
    let event: &[u8] = &[
        0x01, // dispatch byte (generic)
        0x06, // six parameters
        0x01, 0x07, 0x03, b'M', b'o', b'b', // 1: string "Mob"
        0x02, 0x07, 0x04, b'K', b'a', b'z', b'z', // 2: string "Kazz"
        0x03, 0x1B, // 3: false
        0x04, 0x09, 0xEC, 0x0E, // 4: compressed int 950 (zigzag 1900)
        0x05, 0x09, 0x3C, // 5: compressed int 30 (zigzag 60)
        0xFC, 0x04, 0x17, 0x01, // 252: short 279, little endian
    ];
    assert_eq!(loot("Mob", "Kazz", 950, 30), event);
    let reliable_command: Vec<u8> = [
        &[0x06, 0x00, 0x00, 0x00][..], // reliable, channel 0
        &((12 + 2 + event.len()) as u32).to_be_bytes()[..], // length incl. header
        &[0x00, 0x00, 0x00, 0x0B],     // sequence 11
        &[0xF3, 0x04],                 // signal, event message
        event,
    ]
    .concat();
    assert_eq!(reliable(11, event), reliable_command);
    let capture = run(&pcap(&[(
        T0 + 2,
        0,
        from_server(&packet(&[reliable_command])),
    )]))
    .unwrap();
    assert_eq!(
        capture.observations,
        [observation(2, 0, 0, "Kazz", 950, 30)].map(|mut o| {
            o.position.packet = 1;
            o.occurred_at = "2026-09-10T12:00:02.000000Z".into();
            o
        })
    );
}

#[test]
fn known_events_decode_to_the_expected_loot() {
    let (frames, expected) = scenario();
    let capture = run(&pcap(&frames)).unwrap();
    assert_eq!(capture.observations, expected);
    let diagnostics = &capture.diagnostics;
    assert_eq!(diagnostics.packets, 10);
    assert_eq!(diagnostics.udp_from_game_server, 9);
    assert_eq!(
        diagnostics.retransmissions, 2,
        "one reliable command, one fragment"
    );
    assert_eq!(diagnostics.crc_failures, 1);
    assert_eq!(diagnostics.encrypted_packets, 1);
    assert_eq!(diagnostics.loot_events, 6);
    assert_eq!(diagnostics.loot_silver, 1);
    assert_eq!(diagnostics.loot_malformed, 0);
    assert_eq!(diagnostics.events_undecodable, 0);
    assert_eq!(diagnostics.fragments_incomplete, 0);
    assert_eq!(diagnostics.unrecognized_event_codes.get(&123), Some(&1));
    assert_eq!(diagnostics.unrecognized_event_codes.get(&3), Some(&1));
    assert_eq!(
        diagnostics.first_packet_at.as_deref(),
        Some("2026-09-10T12:00:01.000000Z")
    );
    assert_eq!(capture.fingerprint.len(), 64);
    assert_eq!(capture.file_sha256.len(), 64);
}

#[test]
fn pcap_variants_give_the_same_loot() {
    let (frames, expected) = scenario();
    let raw_ip: Vec<_> = frames
        .iter()
        .map(|(seconds, micros, frame)| (*seconds, micros * 1000, frame[14..].to_vec()))
        .collect();
    let big = run(&pcap_big_endian_nanos(&raw_ip)).unwrap();
    let ng = run(&pcapng_sll(&raw_ip)).unwrap();
    assert_eq!(big.observations, expected);
    assert_eq!(ng.observations, expected);
    assert_eq!(
        ng.diagnostics.file_format,
        Some(kalbion_capture::pcap::Format::Pcapng)
    );
}

#[test]
fn decoding_is_deterministic() {
    let (frames, _) = scenario();
    let bytes = pcap(&frames);
    let first = run(&bytes).unwrap();
    let second = run(&bytes).unwrap();
    assert_eq!(first.observations, second.observations);
    assert_eq!(first.fingerprint, second.fingerprint);
    assert_eq!(first.file_sha256, second.file_sha256);
}

#[test]
fn a_capture_cut_anywhere_never_panics_and_keeps_its_identity() {
    let (frames, expected) = scenario();
    let bytes = pcap(&frames);
    let full = run(&bytes).unwrap();
    let first_record_end = 24 + 16 + frames[0].2.len();
    for length in 0..bytes.len() {
        match run(&bytes[..length]) {
            Ok(capture) => {
                assert!(
                    expected.starts_with(&capture.observations),
                    "cut at {length}"
                );
                if length >= first_record_end {
                    assert_eq!(capture.fingerprint, full.fingerprint, "cut at {length}");
                }
            }
            Err(CaptureError::Unsupported(_) | CaptureError::Malformed(_)) => {}
            Err(error) => panic!("cut at {length}: {error}"),
        }
    }
    // A record cut by the end of the file is reported.
    let cut = run(&bytes[..bytes.len() - 3]).unwrap();
    assert!(cut.diagnostics.file_truncated);
}

#[test]
fn random_corruption_never_panics() {
    let (frames, _) = scenario();
    let pristine = pcap(&frames);
    let pristine_ng = pcapng_sll(
        &frames
            .iter()
            .map(|(seconds, micros, frame)| (*seconds, *micros, frame[14..].to_vec()))
            .collect::<Vec<_>>(),
    );
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for bytes in [&pristine, &pristine_ng] {
        for _ in 0..4000 {
            let mut corrupted = bytes.to_vec();
            for _ in 0..(1 + next() % 6) {
                let index = (next() % corrupted.len() as u64) as usize;
                corrupted[index] = next() as u8;
            }
            let _ = run(&corrupted);
        }
    }
}

#[test]
fn limits_stop_oversized_input() {
    let (frames, _) = scenario();
    let bytes = pcap(&frames);
    let small = Limits {
        max_file_bytes: 100,
        max_packet_bytes: 262_144,
    };
    assert!(matches!(
        decode(&bytes[..], small),
        Err(CaptureError::Limit(_))
    ));
    let tiny_packets = Limits {
        max_file_bytes: 1 << 20,
        max_packet_bytes: 64,
    };
    assert!(matches!(
        decode(&bytes[..], tiny_packets),
        Err(CaptureError::Malformed(_))
    ));

    // A fragment claiming a 2 MB message is refused without allocating it.
    let mut huge = fragments(70, &loot("Mob", "Kazz", 950, 1), 16);
    let body = &mut huge[0];
    body[12 + 12..12 + 16].copy_from_slice(&(2u32 << 20).to_be_bytes());
    // Deeply nested values and absurd counts end as undecodable events, not crashes.
    let mut nested = vec![1, 2, 0, 23];
    for _ in 0..20 {
        nested.extend([1, 23]);
    }
    nested.extend([0]);
    let absurd = [vec![1, 2, 0, 0x43], varint(1 << 30), vec![252, 4, 3, 0]].concat();
    let capture = run(&pcap(&[
        (T0, 0, from_server(&packet(&[huge[0].clone()]))),
        (T0 + 1, 0, from_server(&packet(&[reliable(80, &nested)]))),
        (T0 + 2, 0, from_server(&packet(&[reliable(81, &absurd)]))),
        (
            T0 + 3,
            0,
            from_server(&packet(&[reliable(82, &loot("Mob", "Kazz", 950, 1))])),
        ),
        (
            T0 + 4,
            0,
            from_server(&packet(&[reliable(83, &other_event(3))])),
        ),
        (
            T0 + 5,
            0,
            from_server(&packet(&[reliable(84, &other_event(3))])),
        ),
        (
            T0 + 6,
            0,
            from_server(&packet(&[reliable(85, &other_event(3))])),
        ),
    ]))
    .unwrap();
    assert_eq!(capture.observations.len(), 1);
    assert_eq!(capture.diagnostics.malformed_photon, 1);
    assert_eq!(capture.diagnostics.events_undecodable, 2);
    assert_eq!(
        capture.diagnostics.undecodable_reasons.get("TooDeep"),
        Some(&1)
    );
    assert_eq!(
        capture.diagnostics.undecodable_reasons.get("TooLarge"),
        Some(&1)
    );
}

#[test]
fn unsupported_captures_fail_explicitly() {
    let message = |result: Result<kalbion_capture::Capture, CaptureError>| match result {
        Err(CaptureError::Unsupported(message)) => message,
        other => panic!("expected an explicit refusal, got {other:?}"),
    };
    assert!(message(run(b"not a capture at all")).contains("não reconhecido"));
    assert!(message(run(&pcap(&[]))).contains("não contém pacotes"));
    let web = ethernet(&ipv4(SERVER, CLIENT, 17, &udp(443, CLIENT_PORT, &[0; 40])));
    assert!(message(run(&pcap(&[(T0, 0, web)]))).contains("UDP 5056"));
    let encrypted = from_server(&[vec![0, 1, 1, 1], vec![0xAB; 20]].concat());
    assert!(message(run(&pcap(&[(T0, 0, encrypted)]))).contains("criptografado"));
    // 2026-06-20: before the patch that gave the loot event its current code.
    let old = from_server(&packet(&[reliable(1, &loot("Mob", "Kazz", 950, 1))]));
    assert!(message(run(&pcap(&[(1_781_956_800, 0, old)]))).contains("anterior ao patch"));
    // Code 279 with another layout: the codebook does not match this game version.
    let changed = event(&[(1, P::Int(5)), (2, P::Int(6)), (252, P::Short(279))]);
    let frames: Vec<_> = (0..3)
        .map(|index| {
            (
                T0 + index,
                0,
                from_server(&packet(&[reliable(index as u32 + 1, &changed)])),
            )
        })
        .collect();
    assert!(message(run(&pcap(&frames))).contains("formato diferente"));
    // Mostly undecodable events: not the protocol this decoder knows (Protocol16, say).
    let disputed = [vec![1, 2, 0, 21], vec![0, 0, 0], vec![252, 4, 3, 0]].concat();
    let frames: Vec<_> = (0..6)
        .map(|index| {
            (
                T0 + index,
                0,
                from_server(&packet(&[reliable(index as u32 + 1, &disputed)])),
            )
        })
        .collect();
    assert!(message(run(&pcap(&frames))).contains("não puderam ser decodificados"));
}

#[test]
fn disputed_types_are_reported_not_guessed() {
    let hashtable = [vec![1, 2, 0, 21, 0], vec![252, 4, 3, 0]].concat();
    let frames = vec![
        (T0, 0, from_server(&packet(&[reliable(1, &hashtable)]))),
        (
            T0 + 1,
            0,
            from_server(&packet(&[reliable(2, &loot("Mob", "Kazz", 950, 2))])),
        ),
        (
            T0 + 2,
            0,
            from_server(&packet(&[reliable(3, &other_event(3))])),
        ),
    ];
    let capture = run(&pcap(&frames)).unwrap();
    assert_eq!(capture.observations.len(), 1);
    assert_eq!(
        capture
            .diagnostics
            .undecodable_reasons
            .get("UnsupportedType(21)"),
        Some(&1)
    );
}

#[test]
fn protocol18_integers_decode_every_encoding() {
    use kalbion_capture::protocol18::{decode_event, Value};
    let body = [
        vec![1, 9],
        vec![0, 9],
        varint(zigzag(-5)),
        vec![1, 11, 200],
        vec![2, 12, 200],
        vec![3, 13, 0x10, 0x27],
        vec![4, 14, 0x10, 0x27],
        vec![5, 30],
        vec![6, 4, 0xFF, 0xFF],
        vec![7, 3, 7],
        vec![8, 10],
        varint(1 << 40),
    ]
    .concat();
    let event = decode_event(&body).unwrap();
    let integer = |key| event.parameter(key).and_then(Value::as_integer);
    assert_eq!(integer(0), Some(-5));
    assert_eq!(integer(1), Some(200));
    assert_eq!(integer(2), Some(-200));
    assert_eq!(integer(3), Some(10_000));
    assert_eq!(integer(4), Some(-10_000));
    assert_eq!(integer(5), Some(0));
    assert_eq!(integer(6), Some(-1));
    assert_eq!(integer(7), Some(7));
    assert_eq!(integer(8), Some(1 << 39));
    // Trailing bytes and repeated keys are malformed, not ignored.
    assert!(decode_event(&[1, 1, 0, 3, 7, 0xFF]).is_err());
    assert!(decode_event(&[1, 2, 0, 3, 7, 0, 3, 8]).is_err());
}

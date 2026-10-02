//! Synthetic capture builder for tests. It encodes what the documented formats say (see
//! docs/captura-offline.md); `golden_bytes_match_the_builder` pins it to hand-written bytes
//! so the builder and the decoder cannot silently share the same mistake.
#![allow(dead_code)]

pub const SERVER: [u8; 4] = [5, 188, 125, 30];
pub const CLIENT: [u8; 4] = [192, 168, 0, 10];
pub const CLIENT_PORT: u16 = 51000;
/// 2026-09-10T12:00:00Z
pub const T0: u64 = 1_789_041_600;
/// `(seconds, sub-second fraction, frame bytes)` for the PCAP writers.
pub type Frames = Vec<(u64, u32, Vec<u8>)>;

pub fn varint(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return out;
        }
        out.push(byte | 0x80);
    }
}
pub fn zigzag(value: i32) -> u64 {
    u64::from(((value << 1) ^ (value >> 31)) as u32)
}

/// Protocol18 typed values: type code followed by the value.
pub enum P {
    Str(&'static str),
    Owned(String),
    Bool(bool),
    Short(i16),
    Int(i32),
    Byte(u8),
    Raw(u8, Vec<u8>),
}
pub fn value(value: &P) -> Vec<u8> {
    match value {
        P::Str(text) => [vec![7], varint(text.len() as u64), text.as_bytes().to_vec()].concat(),
        P::Owned(text) => [vec![7], varint(text.len() as u64), text.as_bytes().to_vec()].concat(),
        P::Bool(true) => vec![28],
        P::Bool(false) => vec![27],
        P::Short(number) => [vec![4], number.to_le_bytes().to_vec()].concat(),
        P::Int(number) => [vec![9], varint(zigzag(*number))].concat(),
        P::Byte(number) => vec![3, *number],
        P::Raw(code, bytes) => [vec![*code], bytes.clone()].concat(),
    }
}
/// Event message body: dispatch byte 1, parameter table (count, then key + typed value).
pub fn event(parameters: &[(u8, P)]) -> Vec<u8> {
    let mut out = vec![1];
    out.extend(varint(parameters.len() as u64));
    for (key, parameter) in parameters {
        out.push(*key);
        out.extend(value(parameter));
    }
    out
}
pub fn loot(from: &'static str, by: &'static str, item: i32, quantity: i32) -> Vec<u8> {
    event(&[
        (1, P::Str(from)),
        (2, P::Str(by)),
        (3, P::Bool(false)),
        (4, P::Int(item)),
        (5, P::Int(quantity)),
        (252, P::Short(279)),
    ])
}
pub fn silver(by: &'static str) -> Vec<u8> {
    event(&[
        (2, P::Str(by)),
        (3, P::Bool(true)),
        (5, P::Int(40)),
        (252, P::Short(279)),
    ])
}
pub fn other_event(code: i16) -> Vec<u8> {
    event(&[(0, P::Int(7)), (252, P::Short(code))])
}

/// Photon command: type, channel, flags, reserved, length, reliable sequence, body.
pub fn command(kind: u8, channel: u8, sequence: u32, body: &[u8]) -> Vec<u8> {
    let mut out = vec![kind, channel, 0, 0];
    out.extend(((12 + body.len()) as u32).to_be_bytes());
    out.extend(sequence.to_be_bytes());
    out.extend(body);
    out
}
/// Signal byte and message type in front of a message body.
pub fn message(kind: u8, body: &[u8]) -> Vec<u8> {
    [vec![0xF3, kind], body.to_vec()].concat()
}
pub fn reliable(sequence: u32, body: &[u8]) -> Vec<u8> {
    command(6, 0, sequence, &message(4, body))
}
pub fn unreliable(sequence: u32, body: &[u8]) -> Vec<u8> {
    let mut payload = 7u32.to_be_bytes().to_vec();
    payload.extend(message(4, body));
    command(7, 0, sequence, &payload)
}
/// Splits a message into fragment commands of at most `size` bytes.
pub fn fragments(first_sequence: u32, body: &[u8], size: usize) -> Vec<Vec<u8>> {
    let whole = message(4, body);
    let parts: Vec<&[u8]> = whole.chunks(size).collect();
    parts
        .iter()
        .enumerate()
        .map(|(number, part)| {
            let mut payload = Vec::new();
            payload.extend(first_sequence.to_be_bytes());
            payload.extend((parts.len() as u32).to_be_bytes());
            payload.extend((number as u32).to_be_bytes());
            payload.extend((whole.len() as u32).to_be_bytes());
            payload.extend(((number * size) as u32).to_be_bytes());
            payload.extend(*part);
            command(8, 0, first_sequence + number as u32, &payload)
        })
        .collect()
}
/// Photon packet header (peer id, flags, command count, timestamp, challenge) + commands.
pub fn packet(commands: &[Vec<u8>]) -> Vec<u8> {
    let mut out = vec![0, 1, 0, commands.len() as u8];
    out.extend(1000u32.to_be_bytes());
    out.extend(0x1234_5678u32.to_be_bytes());
    for command in commands {
        out.extend(command);
    }
    out
}
pub fn packet_with_crc(commands: &[Vec<u8>], corrupt: bool) -> Vec<u8> {
    let mut out = vec![0, 1, 0xCC, commands.len() as u8];
    out.extend(1000u32.to_be_bytes());
    out.extend(0x1234_5678u32.to_be_bytes());
    out.extend([0, 0, 0, 0]);
    for command in commands {
        out.extend(command);
    }
    let crc = kalbion_capture::photon::crc(&out) ^ u32::from(corrupt);
    out[12..16].copy_from_slice(&crc.to_be_bytes());
    out
}

pub fn udp(source_port: u16, destination_port: u16, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(source_port.to_be_bytes());
    out.extend(destination_port.to_be_bytes());
    out.extend(((8 + payload.len()) as u16).to_be_bytes());
    out.extend([0, 0]);
    out.extend(payload);
    out
}
pub fn ipv4(source: [u8; 4], destination: [u8; 4], protocol: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![0x45, 0];
    out.extend(((20 + payload.len()) as u16).to_be_bytes());
    out.extend([0, 0, 0x40, 0, 64, protocol, 0, 0]);
    out.extend(source);
    out.extend(destination);
    out.extend(payload);
    out
}
pub fn ethernet(ip: &[u8]) -> Vec<u8> {
    let mut out = vec![0x11; 6];
    out.extend([0x22; 6]);
    out.extend([0x08, 0x00]);
    out.extend(ip);
    out
}
/// Ethernet frame carrying a datagram from the game server to the client.
pub fn from_server(photon: &[u8]) -> Vec<u8> {
    ethernet(&ipv4(SERVER, CLIENT, 17, &udp(5056, CLIENT_PORT, photon)))
}
pub fn from_client(photon: &[u8]) -> Vec<u8> {
    ethernet(&ipv4(CLIENT, SERVER, 17, &udp(CLIENT_PORT, 5056, photon)))
}

/// Classic PCAP, little endian, microseconds, Ethernet. Frames are `(seconds, micros, bytes)`.
pub fn pcap(frames: &[(u64, u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(0xA1B2_C3D4u32.to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend(4u16.to_le_bytes());
    out.extend([0u8; 8]);
    out.extend(262_144u32.to_le_bytes());
    out.extend(1u32.to_le_bytes());
    for (seconds, micros, data) in frames {
        out.extend((*seconds as u32).to_le_bytes());
        out.extend(micros.to_le_bytes());
        out.extend((data.len() as u32).to_le_bytes());
        out.extend((data.len() as u32).to_le_bytes());
        out.extend(data);
    }
    out
}
/// Classic PCAP, big endian, nanoseconds, raw IPv4 frames (link type 101).
pub fn pcap_big_endian_nanos(frames: &[(u64, u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(0xA1B2_3C4Du32.to_be_bytes());
    out.extend(2u16.to_be_bytes());
    out.extend(4u16.to_be_bytes());
    out.extend([0u8; 8]);
    out.extend(262_144u32.to_be_bytes());
    out.extend(101u32.to_be_bytes());
    for (seconds, nanos, data) in frames {
        out.extend((*seconds as u32).to_be_bytes());
        out.extend(nanos.to_be_bytes());
        out.extend((data.len() as u32).to_be_bytes());
        out.extend((data.len() as u32).to_be_bytes());
        out.extend(data);
    }
    out
}
fn block(kind: u32, body: &[u8]) -> Vec<u8> {
    let padded = body.len().div_ceil(4) * 4;
    let total = (12 + padded) as u32;
    let mut out = Vec::new();
    out.extend(kind.to_le_bytes());
    out.extend(total.to_le_bytes());
    out.extend(body);
    out.extend(vec![0u8; padded - body.len()]);
    out.extend(total.to_le_bytes());
    out
}
/// PCAPNG, little endian, one Linux "cooked" (SLL) interface with nanosecond timestamps.
pub fn pcapng_sll(frames: &[(u64, u32, Vec<u8>)]) -> Vec<u8> {
    let mut section = Vec::new();
    section.extend(0x1A2B_3C4Du32.to_le_bytes());
    section.extend(1u16.to_le_bytes());
    section.extend(0u16.to_le_bytes());
    section.extend(u64::MAX.to_le_bytes());
    let mut out = block(0x0A0D_0D0A, &section);
    let mut interface = Vec::new();
    interface.extend(113u16.to_le_bytes());
    interface.extend(0u16.to_le_bytes());
    interface.extend(262_144u32.to_le_bytes());
    interface.extend(9u16.to_le_bytes()); // if_tsresol
    interface.extend(1u16.to_le_bytes());
    interface.extend([9, 0, 0, 0]); // 10^-9, padded
    interface.extend([0, 0, 0, 0]); // opt_endofopt
    out.extend(block(1, &interface));
    // A statistics block that must be skipped.
    out.extend(block(5, &[0u8; 20]));
    for (seconds, nanos, ip) in frames {
        let mut sll = vec![0, 0, 0, 1, 0, 6];
        sll.extend([0u8; 8]);
        sll.extend([0x08, 0x00]);
        sll.extend(ip);
        let ticks = seconds * 1_000_000_000 + u64::from(*nanos);
        let mut body = Vec::new();
        body.extend(0u32.to_le_bytes());
        body.extend(((ticks >> 32) as u32).to_le_bytes());
        body.extend((ticks as u32).to_le_bytes());
        body.extend((sll.len() as u32).to_le_bytes());
        body.extend((sll.len() as u32).to_le_bytes());
        body.extend(&sll);
        out.extend(block(6, &body));
    }
    out
}

/// Minimal ao-bin-dumps catalog: the items the fixtures loot, with their game indexes.
pub fn catalog_json() -> String {
    serde_json::json!([
        { "Index": "950", "UniqueName": "T4_WOOD", "LocalizedNames": { "PT-BR": "Toras de Pinho" } },
        { "Index": "1088", "UniqueName": "T5_PLANKS_LEVEL1@1", "LocalizedNames": { "PT-BR": "Tábuas de Cedro Incomum" } },
        { "Index": "2888", "UniqueName": "T4_BAG", "LocalizedNames": { "PT-BR": "Bolsa do Adepto" } },
        { "Index": "4100", "UniqueName": "T6_MAIN_SWORD@2", "LocalizedNames": { "PT-BR": "Espada Larga do Mestre" } }
    ])
    .to_string()
}

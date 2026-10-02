//! Photon transport over UDP, as observed in Albion Online traffic: a 12-byte packet header
//! (optionally followed by a 4-byte CRC), then commands with a 12-byte header each.
//! Reliable commands are deduplicated by sequence number (retransmissions), and fragmented
//! messages are reassembled with bounded memory. Only event messages are handed on.
use crate::net::Endpoint;
use crate::Diagnostics;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

const PACKET_HEADER: usize = 12;
const COMMAND_HEADER: usize = 12;
const FRAGMENT_HEADER: usize = 20;
const FLAG_ENCRYPTED: u8 = 1;
const FLAG_CRC: u8 = 0xCC;
const COMMAND_RELIABLE: u8 = 6;
const COMMAND_UNRELIABLE: u8 = 7;
const COMMAND_FRAGMENT: u8 = 8;
const MESSAGE_EVENT: u8 = 4;
const MESSAGE_ENCRYPTED: u8 = 131;
/// Reassembled messages larger than this are dropped.
pub const MAX_MESSAGE: usize = 1024 * 1024;
const MAX_FRAGMENTS: u32 = 4096;
/// Incomplete fragmented messages kept at once; the oldest is dropped beyond this.
const MAX_PENDING: usize = 64;
/// A sequence number this far below the highest one seen means a new connection.
const SEQUENCE_RESET_GAP: u32 = 1 << 16;

/// Where a message was completed in the file: packet record and command within it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    pub packet: u64,
    pub command: u32,
}

#[derive(Debug)]
pub struct EventMessage {
    pub position: Position,
    pub timestamp_ns: i128,
    /// Photon payload after the signal and message-type bytes.
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Channel {
    source: Endpoint,
    destination: Endpoint,
    channel: u8,
}

#[derive(Default)]
struct Window {
    seen: HashSet<u32>,
    highest: u32,
}
impl Window {
    /// True when this sequence number was already delivered on this channel.
    fn is_repeat(&mut self, sequence: u32) -> bool {
        if sequence.saturating_add(SEQUENCE_RESET_GAP) < self.highest {
            self.seen.clear();
            self.highest = 0;
        }
        if !self.seen.insert(sequence) {
            return true;
        }
        self.highest = self.highest.max(sequence);
        if self.seen.len() > 2 * SEQUENCE_RESET_GAP as usize {
            let floor = self.highest.saturating_sub(SEQUENCE_RESET_GAP);
            self.seen.retain(|value| *value >= floor);
        }
        false
    }
}

struct Pending {
    total: usize,
    count: u32,
    received: HashSet<u32>,
    /// Byte ranges already written, `start -> end`. Kept disjoint, so `written == total`
    /// at completion means every byte was covered exactly once.
    ranges: BTreeMap<usize, usize>,
    written: usize,
    payload: Vec<u8>,
}

impl Pending {
    fn overlaps(&self, start: usize, end: usize) -> bool {
        let before = self.ranges.range(..=start).next_back();
        let after = self.ranges.range(start..).next();
        before.is_some_and(|(_, &previous_end)| previous_end > start)
            || after.is_some_and(|(&next_start, _)| next_start < end)
    }
}

pub struct Decoder {
    windows: HashMap<Channel, Window>,
    pending: HashMap<(Channel, u32), Pending>,
    pending_order: VecDeque<(Channel, u32)>,
}

pub struct Datagram<'a> {
    pub packet: u64,
    pub timestamp_ns: i128,
    pub source: Endpoint,
    pub destination: Endpoint,
    pub payload: &'a [u8],
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}
impl Decoder {
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            pending: HashMap::new(),
            pending_order: VecDeque::new(),
        }
    }

    /// Fragmented messages still incomplete; call at the end of the file.
    pub fn incomplete(&self) -> usize {
        self.pending.len()
    }

    /// One UDP payload, which may hold several Photon packets back to back.
    pub fn push(
        &mut self,
        datagram: &Datagram,
        diagnostics: &mut Diagnostics,
        out: &mut Vec<EventMessage>,
    ) {
        let mut offset = 0;
        let mut command_index = 0u32;
        while offset < datagram.payload.len() {
            let packet = &datagram.payload[offset..];
            match self.packet(datagram, packet, &mut command_index, diagnostics, out) {
                Ok(length) => {
                    diagnostics.photon_packets += 1;
                    offset += length;
                }
                Err(Stop::Encrypted) => {
                    diagnostics.photon_packets += 1;
                    diagnostics.encrypted_packets += 1;
                    return;
                }
                Err(Stop::Crc) => {
                    diagnostics.crc_failures += 1;
                    return;
                }
                Err(Stop::Malformed) => {
                    diagnostics.malformed_photon += 1;
                    return;
                }
            }
        }
    }

    fn packet(
        &mut self,
        datagram: &Datagram,
        packet: &[u8],
        command_index: &mut u32,
        diagnostics: &mut Diagnostics,
        out: &mut Vec<EventMessage>,
    ) -> Result<usize, Stop> {
        let header = packet.get(..PACKET_HEADER).ok_or(Stop::Malformed)?;
        let flags = header[2];
        let commands = header[3];
        let mut offset = PACKET_HEADER;
        match flags {
            0 => {}
            FLAG_ENCRYPTED => return Err(Stop::Encrypted),
            FLAG_CRC => offset += 4,
            _ => return Err(Stop::Malformed),
        }
        if commands == 0 || packet.len() < offset {
            return Err(Stop::Malformed);
        }
        // Find where this packet ends before using any command.
        let mut end = offset;
        for _ in 0..commands {
            let command = packet
                .get(end..end + COMMAND_HEADER)
                .ok_or(Stop::Malformed)?;
            let length =
                u32::from_be_bytes([command[4], command[5], command[6], command[7]]) as usize;
            if length < COMMAND_HEADER || packet.len() - end < length {
                return Err(Stop::Malformed);
            }
            end += length;
        }
        if flags == FLAG_CRC {
            let expected = u32::from_be_bytes([packet[12], packet[13], packet[14], packet[15]]);
            let mut copy = packet[..end].to_vec();
            copy[12..16].fill(0);
            if crc(&copy) != expected {
                return Err(Stop::Crc);
            }
        }
        let mut cursor = offset;
        for _ in 0..commands {
            let command = &packet[cursor..];
            let length =
                u32::from_be_bytes([command[4], command[5], command[6], command[7]]) as usize;
            let body = &command[COMMAND_HEADER..length];
            let channel = Channel {
                source: datagram.source,
                destination: datagram.destination,
                channel: command[1],
            };
            let sequence = u32::from_be_bytes([command[8], command[9], command[10], command[11]]);
            let position = Position {
                packet: datagram.packet,
                command: *command_index,
            };
            *command_index += 1;
            cursor += length;
            match command[0] {
                COMMAND_RELIABLE => {
                    if self.window(channel).is_repeat(sequence) {
                        diagnostics.retransmissions += 1;
                        continue;
                    }
                    message(body, position, datagram.timestamp_ns, diagnostics, out);
                }
                COMMAND_UNRELIABLE => match body.get(4..) {
                    Some(body) => message(body, position, datagram.timestamp_ns, diagnostics, out),
                    None => diagnostics.malformed_photon += 1,
                },
                COMMAND_FRAGMENT => {
                    if self.window(channel).is_repeat(sequence) {
                        diagnostics.retransmissions += 1;
                        continue;
                    }
                    if let Some(payload) = self.fragment(channel, body, diagnostics) {
                        message(&payload, position, datagram.timestamp_ns, diagnostics, out);
                    }
                }
                // Acknowledgements, connection management, pings.
                _ => {}
            }
        }
        Ok(end)
    }

    /// Removes a pending message from both the map and the eviction order, keeping them in sync.
    fn discard(&mut self, key: &(Channel, u32)) -> Option<Pending> {
        self.pending_order.retain(|entry| entry != key);
        self.pending.remove(key)
    }

    fn window(&mut self, channel: Channel) -> &mut Window {
        self.windows.entry(channel).or_default()
    }

    fn fragment(
        &mut self,
        channel: Channel,
        body: &[u8],
        diagnostics: &mut Diagnostics,
    ) -> Option<Vec<u8>> {
        let Some(header) = body.get(..FRAGMENT_HEADER) else {
            diagnostics.malformed_photon += 1;
            return None;
        };
        let field = |index: usize| {
            u32::from_be_bytes([
                header[index],
                header[index + 1],
                header[index + 2],
                header[index + 3],
            ])
        };
        let start = field(0);
        let count = field(4);
        let number = field(8);
        let total = field(12) as usize;
        let offset = field(16) as usize;
        let data = &body[FRAGMENT_HEADER..];
        if total == 0
            || total > MAX_MESSAGE
            || count == 0
            || count > MAX_FRAGMENTS
            || number >= count
            || data.is_empty()
        {
            diagnostics.malformed_photon += 1;
            return None;
        }
        if offset > total || data.len() > total - offset {
            diagnostics.malformed_photon += 1;
            return None;
        }
        let key = (channel, start);
        if let Some(existing) = self.pending.get(&key) {
            if existing.total != total || existing.count != count {
                // Two messages claim the same start sequence: drop both rather than mix them.
                self.discard(&key);
                diagnostics.malformed_photon += 1;
                return None;
            }
        } else {
            while self.pending.len() >= MAX_PENDING {
                let Some(oldest) = self.pending_order.pop_front() else {
                    break;
                };
                if self.pending.remove(&oldest).is_some() {
                    diagnostics.fragments_dropped += 1;
                }
            }
            self.pending.insert(
                key,
                Pending {
                    total,
                    count,
                    received: HashSet::new(),
                    ranges: BTreeMap::new(),
                    written: 0,
                    payload: vec![0; total],
                },
            );
            self.pending_order.push_back(key);
        }
        let pending = self.pending.get_mut(&key)?;
        if pending.received.contains(&number) {
            diagnostics.retransmissions += 1;
            return None;
        }
        let end = offset + data.len();
        if pending.overlaps(offset, end) {
            // A new fragment number over bytes already written: the headers are inconsistent,
            // and keeping either version could emit a message assembled from two sources.
            self.discard(&key);
            diagnostics.malformed_photon += 1;
            return None;
        }
        pending.received.insert(number);
        pending.ranges.insert(offset, end);
        pending.payload[offset..end].copy_from_slice(data);
        pending.written += data.len();
        if pending.received.len() as u32 == pending.count {
            let done = self.discard(&key)?;
            if done.written != done.total {
                diagnostics.malformed_photon += 1;
                return None;
            }
            return Some(done.payload);
        }
        None
    }
}

enum Stop {
    Encrypted,
    Crc,
    Malformed,
}

fn message(
    body: &[u8],
    position: Position,
    timestamp_ns: i128,
    diagnostics: &mut Diagnostics,
    out: &mut Vec<EventMessage>,
) {
    // Signal byte, then message type.
    let Some(&kind) = body.get(1) else {
        diagnostics.malformed_photon += 1;
        return;
    };
    match kind {
        MESSAGE_EVENT => out.push(EventMessage {
            position,
            timestamp_ns,
            data: body[2..].to_vec(),
        }),
        MESSAGE_ENCRYPTED => diagnostics.encrypted_messages += 1,
        // Requests and responses carry no loot.
        2 | 3 | 7 => {}
        _ => diagnostics.unknown_message_types += 1,
    }
}

/// Photon's CRC-32 variant: IEEE polynomial (reflected), initial value all ones, no final XOR.
pub fn crc(data: &[u8]) -> u32 {
    let mut value = u32::MAX;
    for byte in data {
        value ^= u32::from(*byte);
        for _ in 0..8 {
            value = if value & 1 != 0 {
                (value >> 1) ^ 0xEDB8_8320
            } else {
                value >> 1
            };
        }
    }
    value
}

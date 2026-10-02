//! Offline loot capture for Kalbion: reads a PCAP/PCAPNG file the user recorded, decodes
//! Albion Online's Photon (Protocol18) events and keeps the loot pick-ups. It never opens a
//! network interface, never sends anything, and needs no privileges.
//!
//! The stages are separate: `pcap` (file container) → `net` (link/IP/UDP) → `photon`
//! (transport, retransmissions, fragments) → `protocol18` (values) → `albion` (event codes)
//! → `convert` (Kalbion loot events, through the item catalog and a player roster).
pub mod albion;
pub mod convert;
pub mod net;
pub mod pcap;
pub mod photon;
pub mod protocol18;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::Read;

pub const DECODER_VERSION: &str = concat!("kalbion-capture ", env!("CARGO_PKG_VERSION"));
/// Albion's game server port (Photon over UDP).
pub const GAME_PORT: u16 = 5056;

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    /// The file or its traffic is not something this decoder supports; nothing was imported.
    #[error("{0}")]
    Unsupported(String),
    #[error("{0}")]
    Malformed(String),
    #[error("{0}")]
    Limit(String),
    #[error("Falha ao ler o arquivo: {0}")]
    Io(std::io::Error),
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_file_bytes: u64,
    pub max_packet_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_file_bytes: 512 * 1024 * 1024,
            max_packet_bytes: 256 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    pub position: photon::Position,
    /// Packet time in canonical UTC (when the capturing machine received it).
    pub occurred_at: String,
    pub loot: albion::Loot,
}

/// Counts that explain what the decoder saw and skipped. Player names never appear here.
#[derive(Debug, Default, Clone, Serialize)]
pub struct Diagnostics {
    pub decoder_version: &'static str,
    pub codebook_from: &'static str,
    pub codebook_observed_until: &'static str,
    pub file_format: Option<pcap::Format>,
    pub file_truncated: bool,
    pub packets: u64,
    pub packets_cut_short: u64,
    pub first_packet_at: Option<String>,
    pub last_packet_at: Option<String>,
    pub link_types_skipped: u64,
    pub not_udp: u64,
    pub ip_fragments: u64,
    pub truncated_packets: u64,
    pub malformed_packets: u64,
    pub udp_from_game_server: u64,
    pub photon_packets: u64,
    pub crc_failures: u64,
    pub encrypted_packets: u64,
    pub encrypted_messages: u64,
    pub malformed_photon: u64,
    pub unknown_message_types: u64,
    pub retransmissions: u64,
    pub fragments_dropped: u64,
    pub fragments_incomplete: u64,
    pub events: u64,
    pub events_undecodable: u64,
    /// Why events could not be decoded (e.g. a disputed Protocol18 type code).
    pub undecodable_reasons: BTreeMap<String, u64>,
    /// Real event codes seen, except the loot event: not handled by this decoder.
    pub unrecognized_event_codes: BTreeMap<u16, u64>,
    pub events_without_code: u64,
    pub loot_events: u64,
    pub loot_silver: u64,
    pub loot_malformed: u64,
    pub loot_malformed_reasons: BTreeMap<String, u64>,
}

#[derive(Debug)]
pub struct Capture {
    /// SHA-256 of the file start through the first packet: the capture's identity.
    pub fingerprint: String,
    pub file_sha256: String,
    pub observations: Vec<Observation>,
    pub diagnostics: Diagnostics,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn timestamp(nanoseconds: i128) -> Option<String> {
    let seconds = i64::try_from(nanoseconds.div_euclid(1_000_000_000)).ok()?;
    let nanos = nanoseconds.rem_euclid(1_000_000_000) as u32;
    DateTime::<Utc>::from_timestamp(seconds, nanos)
        .map(|time| time.to_rfc3339_opts(SecondsFormat::Micros, true))
}

/// Decodes a whole capture file. Fails, importing nothing, when the file is not a capture,
/// holds no Albion game traffic, predates the codebook, or its loot events do not have the
/// layout the codebook expects.
pub fn decode(source: impl Read, limits: Limits) -> Result<Capture, CaptureError> {
    let mut reader = pcap::Reader::new(source, limits.max_packet_bytes, limits.max_file_bytes)?;
    let mut diagnostics = Diagnostics {
        decoder_version: DECODER_VERSION,
        codebook_from: albion::CODEBOOK_FROM,
        codebook_observed_until: albion::CODEBOOK_OBSERVED_UNTIL,
        file_format: Some(reader.format()),
        ..Diagnostics::default()
    };
    let mut decoder = photon::Decoder::new();
    let mut observations = Vec::new();
    let mut messages = Vec::new();
    let mut first_ns = None;
    let mut last_ns = None;
    while let Some(frame) = reader.next_frame()? {
        diagnostics.packets += 1;
        if frame.cut_short {
            diagnostics.packets_cut_short += 1;
        }
        first_ns = first_ns.or(Some(frame.timestamp_ns));
        last_ns = Some(frame.timestamp_ns);
        let udp = match net::udp(frame.link_type, &frame.data) {
            Ok(udp) => udp,
            Err(net::Skip::LinkType) => {
                diagnostics.link_types_skipped += 1;
                continue;
            }
            Err(net::Skip::NotUdp) => {
                diagnostics.not_udp += 1;
                continue;
            }
            Err(net::Skip::IpFragment) => {
                diagnostics.ip_fragments += 1;
                continue;
            }
            Err(net::Skip::Truncated) => {
                diagnostics.truncated_packets += 1;
                continue;
            }
            Err(net::Skip::Malformed) => {
                diagnostics.malformed_packets += 1;
                continue;
            }
        };
        // Loot events travel from the game server to the client.
        if udp.source.port != GAME_PORT {
            continue;
        }
        diagnostics.udp_from_game_server += 1;
        messages.clear();
        decoder.push(
            &photon::Datagram {
                packet: frame.index,
                timestamp_ns: frame.timestamp_ns,
                source: udp.source,
                destination: udp.destination,
                payload: udp.payload,
            },
            &mut diagnostics,
            &mut messages,
        );
        for message in messages.drain(..) {
            diagnostics.events += 1;
            let event = match protocol18::decode_event(&message.data) {
                Ok(event) => event,
                Err((error, partial)) => {
                    diagnostics.events_undecodable += 1;
                    *diagnostics
                        .undecodable_reasons
                        .entry(format!("{error:?}"))
                        .or_default() += 1;
                    // A loot event that failed after its code was read still counts as one.
                    if albion::event_code(&partial) == Some(albion::LOOT_EVENT) {
                        diagnostics.loot_events += 1;
                        diagnostics.loot_malformed += 1;
                        *diagnostics
                            .loot_malformed_reasons
                            .entry(format!("decodificação: {error:?}"))
                            .or_default() += 1;
                    }
                    continue;
                }
            };
            match albion::classify(&event) {
                albion::Classified::Loot(loot) => {
                    diagnostics.loot_events += 1;
                    let occurred_at = timestamp(message.timestamp_ns).ok_or_else(|| {
                        CaptureError::Malformed(format!(
                            "Pacote {} com horário inválido",
                            message.position.packet
                        ))
                    })?;
                    observations.push(Observation {
                        position: message.position,
                        occurred_at,
                        loot,
                    });
                }
                albion::Classified::Silver => {
                    diagnostics.loot_events += 1;
                    diagnostics.loot_silver += 1;
                }
                albion::Classified::LootMalformed(reason) => {
                    diagnostics.loot_events += 1;
                    diagnostics.loot_malformed += 1;
                    *diagnostics
                        .loot_malformed_reasons
                        .entry(reason.to_string())
                        .or_default() += 1;
                }
                albion::Classified::Other(code) => {
                    *diagnostics
                        .unrecognized_event_codes
                        .entry(code)
                        .or_default() += 1;
                }
                albion::Classified::NoCode => diagnostics.events_without_code += 1,
            }
        }
    }
    diagnostics.file_truncated = reader.truncated;
    diagnostics.fragments_incomplete = decoder.incomplete() as u64;
    diagnostics.first_packet_at = first_ns.and_then(timestamp);
    diagnostics.last_packet_at = last_ns.and_then(timestamp);
    let (fingerprint, file_sha256) = reader.finish()?;
    check_support(&diagnostics)?;
    Ok(Capture {
        fingerprint: hex(&fingerprint),
        file_sha256: hex(&file_sha256),
        observations,
        diagnostics,
    })
}

/// Explicit refusals: the decoder only claims what its codebook covers.
fn check_support(diagnostics: &Diagnostics) -> Result<(), CaptureError> {
    if diagnostics.packets == 0 {
        return Err(CaptureError::Unsupported(
            "A captura não contém pacotes".into(),
        ));
    }
    if diagnostics.udp_from_game_server == 0 {
        return Err(CaptureError::Unsupported(format!(
            "Nenhum tráfego do servidor do Albion (UDP {GAME_PORT}) nesta captura"
        )));
    }
    if let Some(first) = &diagnostics.first_packet_at {
        if first.as_str() < albion::CODEBOOK_FROM {
            return Err(CaptureError::Unsupported(format!(
                "Captura de {}, anterior ao patch de {}: os códigos de evento eram outros e este decoder não os suporta",
                &first[..10],
                albion::CODEBOOK_FROM
            )));
        }
    }
    if diagnostics.events == 0 {
        return Err(CaptureError::Unsupported(
            if diagnostics.encrypted_packets + diagnostics.encrypted_messages > 0 {
                "Só há tráfego criptografado do Albion nesta captura; nada pode ser lido".into()
            } else {
                "Tráfego do Albion sem eventos legíveis; o formato não é o Photon/Protocol18 suportado".into()
            },
        ));
    }
    // Most events failing to decode means the protocol is not the one this decoder knows.
    if diagnostics.events_undecodable >= 5
        && diagnostics.events_undecodable * 2 >= diagnostics.events
    {
        return Err(CaptureError::Unsupported(format!(
            "{} de {} eventos não puderam ser decodificados; o protocolo desta captura não é suportado",
            diagnostics.events_undecodable, diagnostics.events
        )));
    }
    // Loot events with another layout mean the codebook does not match this game version.
    if diagnostics.loot_malformed > 0 && diagnostics.loot_malformed * 2 >= diagnostics.loot_events {
        return Err(CaptureError::Unsupported(format!(
            "{} de {} eventos de loot (código {}) têm formato diferente do esperado; esta versão do jogo não é suportada",
            diagnostics.loot_malformed,
            diagnostics.loot_events,
            albion::LOOT_EVENT
        )));
    }
    Ok(())
}

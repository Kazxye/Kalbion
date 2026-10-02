//! Diagnostic trace for the live capture: JSON records that let a user check, after playing,
//! which events the game sent around the moment they picked something up.
//!
//! - Events whose code is in `WATCHED` (item and loot related, by the current codebook of
//!   other projects) are written with their parameters.
//! - Every other event code is only counted, in one histogram record per `WINDOW_NS`, so a
//!   code that moved in a patch still shows up next to the time of the pick-up.
//! - Text parameters are redacted: player, mob and container names never reach the file.
//!   Only game identifiers (uppercase with an underscore, such as `T4_BAG`) are kept, since
//!   Albion character names cannot contain underscores.
use crate::albion::{self, Classified};
use crate::protocol18::Value;
use crate::{timestamp, Seen};
use serde_json::{json, Map, Value as Json};
use std::collections::BTreeMap;

/// Item and loot related events, as numbered by third-party codebooks since the 2026-06-29
/// patch. Used only to choose what the trace shows in full; nothing is stored from them.
pub const WATCHED: &[(u16, &str)] = &[
    (26, "InventoryPutItem"),
    (30, "NewEquipmentItem"),
    (31, "NewSiegeBannerItem"),
    (32, "NewSimpleItem"),
    (98, "NewLoot"),
    (99, "AttachItemContainer"),
    (100, "DetachItemContainer"),
    (albion::LOOT_EVENT, "OtherGrabbedLoot"),
    (393, "NewLootChest"),
];
const WINDOW_NS: i128 = 10_000_000_000;
const MAX_ITEMS: usize = 32;
const MAX_BYTES_SHOWN: usize = 32;
const MAX_DEPTH: usize = 4;

#[derive(Default)]
pub struct Tracer {
    window_start_ns: Option<i128>,
    window_end_ns: i128,
    counts: BTreeMap<u16, u64>,
    without_code: u64,
}

impl Tracer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends the records for one thing the pipeline saw.
    pub fn seen(&mut self, seen: &Seen, out: &mut Vec<Json>) {
        let at_ns = match seen {
            Seen::Event { timestamp_ns, .. } | Seen::Undecodable { timestamp_ns, .. } => {
                *timestamp_ns
            }
        };
        self.roll(at_ns, out);
        match seen {
            Seen::Event {
                timestamp_ns,
                position,
                event,
                classified,
            } => {
                let code = albion::event_code(event);
                let watched = code.and_then(|code| {
                    WATCHED
                        .iter()
                        .find(|(watched, _)| *watched == code)
                        .map(|(_, name)| *name)
                });
                match (code, watched) {
                    (Some(code), Some(name)) => {
                        let mut parameters = Map::new();
                        for (key, value) in &event.parameters {
                            if *key != albion::EVENT_CODE_PARAMETER {
                                parameters.insert(key.to_string(), redact(value, 0));
                            }
                        }
                        out.push(json!({
                            "kind": "event",
                            "at": timestamp(*timestamp_ns),
                            "packet": position.packet,
                            "command": position.command,
                            "code": code,
                            "name": name,
                            "classified": describe(classified),
                            "parameters": parameters,
                        }));
                    }
                    (Some(code), None) => *self.counts.entry(code).or_default() += 1,
                    (None, _) => self.without_code += 1,
                }
            }
            Seen::Undecodable {
                timestamp_ns,
                position,
                code,
                error,
            } => out.push(json!({
                "kind": "undecodable",
                "at": timestamp(*timestamp_ns),
                "packet": position.packet,
                "command": position.command,
                "code": code,
                "error": format!("{error:?}"),
            })),
        }
    }

    /// Writes the pending histogram (call when the capture stops).
    pub fn flush(&mut self, out: &mut Vec<Json>) {
        if let Some(start) = self.window_start_ns.take() {
            if !self.counts.is_empty() || self.without_code > 0 {
                out.push(json!({
                    "kind": "codes",
                    "from": timestamp(start),
                    "to": timestamp(self.window_end_ns),
                    "counts": std::mem::take(&mut self.counts),
                    "without_code": std::mem::take(&mut self.without_code),
                }));
            }
        }
    }

    fn roll(&mut self, at_ns: i128, out: &mut Vec<Json>) {
        match self.window_start_ns {
            Some(start) if at_ns - start < WINDOW_NS && at_ns >= start => {}
            Some(_) => {
                self.flush(out);
                self.window_start_ns = Some(at_ns);
            }
            None => self.window_start_ns = Some(at_ns),
        }
        self.window_end_ns = at_ns;
    }
}

fn describe(classified: &Classified) -> Json {
    match classified {
        Classified::Loot(_) => json!("loot"),
        Classified::Silver => json!("silver"),
        Classified::LootMalformed(reason) => json!(format!("loot ilegível: {reason}")),
        Classified::Other(_) | Classified::NoCode => Json::Null,
    }
}

/// Game identifiers keep their text; anything else (names) only shows its length.
fn redact_text(text: &str) -> Json {
    let identifier = text.len() <= 96
        && text.contains('_')
        && text
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_' || c == '@');
    if identifier {
        json!(text)
    } else {
        json!(format!("<texto: {} caracteres>", text.chars().count()))
    }
}

fn redact(value: &Value, depth: usize) -> Json {
    if depth >= MAX_DEPTH {
        return json!("<…>");
    }
    match value {
        Value::Null => Json::Null,
        Value::Bool(value) => json!(value),
        Value::Byte(value) => json!(value),
        Value::Short(value) => json!(value),
        Value::Int(value) => json!(value),
        Value::Long(value) => json!(value),
        Value::Float(value) => json!(f64::from(*value)),
        Value::Double(value) => json!(value),
        Value::String(text) => redact_text(text),
        Value::Bytes(bytes) => json!({
            "bytes": bytes.len(),
            "hex": bytes.iter().take(MAX_BYTES_SHOWN).map(|b| format!("{b:02x}")).collect::<String>(),
        }),
        Value::Array(values) => {
            let mut items: Vec<Json> = values
                .iter()
                .take(MAX_ITEMS)
                .map(|value| redact(value, depth + 1))
                .collect();
            if values.len() > MAX_ITEMS {
                items.push(json!(format!("<+{} itens>", values.len() - MAX_ITEMS)));
            }
            Json::Array(items)
        }
        Value::Dictionary(entries) => {
            let mut items: Vec<Json> = entries
                .iter()
                .take(MAX_ITEMS)
                .map(|(key, value)| json!([redact(key, depth + 1), redact(value, depth + 1)]))
                .collect();
            if entries.len() > MAX_ITEMS {
                items.push(json!(format!("<+{} entradas>", entries.len() - MAX_ITEMS)));
            }
            json!({ "dictionary": items })
        }
        Value::Custom { code, data } => json!({ "custom": code, "bytes": data.len() }),
    }
}

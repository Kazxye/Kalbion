//! External file contract for loot events. These DTOs are what users export and import;
//! they are converted to and from domain events here, so the domain can evolve without
//! breaking files that already exist.
use crate::domain::{
    normalize_player, normalize_timestamp, Item, LootReceived, Origin, MAX_QUANTITY,
};
use crate::error::{invalid, Result};
use serde::{Deserialize, Serialize};

pub const MAX_IMPORT_BYTES: usize = 5_000_000;
pub const MAX_IMPORT_EVENTS: usize = 10_000;
pub const CURRENT_VERSION: u32 = 2;

#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum OriginV1 {
    Simulated,
    Manual,
    Observed,
}
impl From<OriginV1> for Origin {
    fn from(origin: OriginV1) -> Self {
        match origin {
            OriginV1::Simulated => Origin::Simulated,
            OriginV1::Manual => Origin::Manual,
            OriginV1::Observed => Origin::Observed,
        }
    }
}
impl From<Origin> for OriginV1 {
    fn from(origin: Origin) -> Self {
        match origin {
            Origin::Simulated => OriginV1::Simulated,
            Origin::Manual => OriginV1::Manual,
            Origin::Observed => OriginV1::Observed,
        }
    }
}

#[derive(Deserialize)]
struct Envelope {
    schema_version: u32,
}

/// Version 1: files exported by the first Kalbion build. Quality was mandatory.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileV1 {
    #[allow(dead_code)]
    schema_version: u32,
    events: Vec<EventV1>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EventV1 {
    id: String,
    origin: OriginV1,
    source: String,
    session_id: String,
    occurred_at: String,
    player: String,
    item: ItemV1,
    quantity: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemV1 {
    id: String,
    name: String,
    tier: u8,
    enchantment: u8,
    quality: u8,
}

/// Version 2: quality may be unknown and items may have no tier.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileV2 {
    pub schema_version: u32,
    pub events: Vec<EventV2>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventV2 {
    id: String,
    origin: OriginV1,
    source: String,
    session_id: String,
    occurred_at: String,
    player: String,
    item: ItemV2,
    quality: Option<u8>,
    quantity: u32,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemV2 {
    id: String,
    name: String,
    #[serde(default)]
    tier: Option<u8>,
    #[serde(default)]
    enchantment: Option<u8>,
}
impl From<&LootReceived> for EventV2 {
    fn from(event: &LootReceived) -> Self {
        Self {
            id: event.id.clone(),
            origin: event.origin.into(),
            source: event.source.clone(),
            session_id: event.session_id.clone(),
            occurred_at: event.occurred_at.clone(),
            player: event.player.clone(),
            item: ItemV2 {
                id: event.item.id.clone(),
                name: event.item.name.clone(),
                tier: event.item.tier,
                enchantment: Some(event.item.enchantment),
            },
            quality: event.quality,
            quantity: event.quantity,
        }
    }
}

struct Fields {
    id: String,
    origin: Origin,
    source: String,
    session_id: String,
    occurred_at: String,
    player: String,
    item_id: String,
    item_name: String,
    tier: Option<u8>,
    enchantment: Option<u8>,
    quality: Option<u8>,
    quantity: u32,
}
fn normalize(fields: Fields) -> Result<LootReceived> {
    let item = Item::new(&fields.item_id, &fields.item_name)?;
    // Tier and enchantment are derived from the ID; when a file states them, they must agree.
    if fields.tier.is_some_and(|tier| Some(tier) != item.tier)
        || fields
            .enchantment
            .is_some_and(|enchantment| enchantment != item.enchantment)
    {
        return Err(invalid("Tier/enchantment não corresponde ao ID do item"));
    }
    if fields.quantity == 0 || fields.quantity > MAX_QUANTITY {
        return Err(invalid("Quantidade deve estar entre 1 e 1.000.000"));
    }
    let event = LootReceived {
        id: fields.id.trim().into(),
        origin: fields.origin,
        source: fields.source.trim().into(),
        session_id: fields.session_id,
        occurred_at: normalize_timestamp(&fields.occurred_at)?,
        player: normalize_player(&fields.player)?,
        item,
        quality: fields.quality,
        quantity: fields.quantity,
    };
    event.validate()?;
    Ok(event)
}

/// Parses an import file of any supported version into validated domain events.
pub fn parse(json: &str) -> Result<Vec<LootReceived>> {
    if json.len() > MAX_IMPORT_BYTES {
        return Err(invalid("Importação limitada a 5 MB"));
    }
    let version = serde_json::from_str::<Envelope>(json)?.schema_version;
    let fields: Vec<Fields> = match version {
        1 => serde_json::from_str::<FileV1>(json)?
            .events
            .into_iter()
            .map(|event| Fields {
                id: event.id,
                origin: event.origin.into(),
                source: event.source,
                session_id: event.session_id,
                occurred_at: event.occurred_at,
                player: event.player,
                item_id: event.item.id,
                item_name: event.item.name,
                tier: Some(event.item.tier),
                enchantment: Some(event.item.enchantment),
                quality: Some(event.item.quality),
                quantity: event.quantity,
            })
            .collect(),
        2 => serde_json::from_str::<FileV2>(json)?
            .events
            .into_iter()
            .map(|event| Fields {
                id: event.id,
                origin: event.origin.into(),
                source: event.source,
                session_id: event.session_id,
                occurred_at: event.occurred_at,
                player: event.player,
                item_id: event.item.id,
                item_name: event.item.name,
                tier: event.item.tier,
                enchantment: event.item.enchantment,
                quality: event.quality,
                quantity: event.quantity,
            })
            .collect(),
        _ => return Err(invalid("Versão incompatível; use schema_version 1 ou 2")),
    };
    if fields.len() > MAX_IMPORT_EVENTS {
        return Err(invalid("Importação limitada a 10.000 eventos"));
    }
    fields
        .into_iter()
        .enumerate()
        .map(|(index, fields)| {
            normalize(fields).map_err(|error| invalid(format!("Evento {}: {error}", index + 1)))
        })
        .collect()
}

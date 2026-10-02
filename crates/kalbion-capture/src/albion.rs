//! Albion Online event codes and the loot event layout ("codebook"). Codes change with game
//! patches, so the codebook states the patch it starts at and the latest capture date its
//! sources were checked against; see docs/captura-offline.md for the sources.
use crate::protocol18::{Event, Value};

/// Albion carries the real event code here; the Photon dispatch byte is generic.
pub const EVENT_CODE_PARAMETER: u8 = 252;
/// `EvOtherGrabbedLoot`: a player picked up loot. 277 before the 2026-06-29 patch.
pub const LOOT_EVENT: u16 = 279;
/// The game patch from which `LOOT_EVENT` has this value.
pub const CODEBOOK_FROM: &str = "2026-06-29";
/// Latest capture date at which third-party sources observed this layout in real traffic.
/// Kalbion itself has not confirmed it against a real capture.
pub const CODEBOOK_OBSERVED_UNTIL: &str = "2026-09-28";

const LOOTED_FROM: u8 = 1;
const LOOTED_BY: u8 = 2;
const IS_SILVER: u8 = 3;
const ITEM: u8 = 4;
const QUANTITY: u8 = 5;
pub const MAX_QUANTITY: i64 = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loot {
    pub looted_by: String,
    /// The game's numeric item index (`Index` in ao-bin-dumps), not a guess.
    pub item_index: u32,
    pub quantity: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Classified {
    Loot(Loot),
    Silver,
    /// A loot event whose parameters do not have the expected shape.
    LootMalformed(&'static str),
    Other(u16),
    NoCode,
}

pub fn event_code(event: &Event) -> Option<u16> {
    event
        .parameter(EVENT_CODE_PARAMETER)
        .and_then(Value::as_integer)
        .and_then(|code| u16::try_from(code).ok())
}

pub fn classify(event: &Event) -> Classified {
    let Some(code) = event_code(event) else {
        return Classified::NoCode;
    };
    if code != LOOT_EVENT {
        return Classified::Other(code);
    }
    match loot(event) {
        Ok(Some(loot)) => Classified::Loot(loot),
        Ok(None) => Classified::Silver,
        Err(reason) => Classified::LootMalformed(reason),
    }
}

/// `Ok(None)` for silver pick-ups, which are not item loot.
fn loot(event: &Event) -> Result<Option<Loot>, &'static str> {
    let silver = match event.parameter(IS_SILVER) {
        None => false,
        Some(Value::Bool(value)) => *value,
        Some(_) => return Err("parâmetro 3 (silver) não é booleano"),
    };
    let looted_by = match event.parameter(LOOTED_BY) {
        Some(Value::String(name)) if !name.trim().is_empty() => name.trim().to_string(),
        Some(Value::String(_)) => return Err("parâmetro 2 (jogador) vazio"),
        Some(_) => return Err("parâmetro 2 (jogador) não é texto"),
        None => return Err("parâmetro 2 (jogador) ausente"),
    };
    if silver {
        return Ok(None);
    }
    match event.parameter(LOOTED_FROM) {
        Some(Value::String(_)) => {}
        Some(_) => return Err("parâmetro 1 (origem) não é texto"),
        None => return Err("parâmetro 1 (origem) ausente"),
    }
    let item = match event.parameter(ITEM).map(Value::as_integer) {
        Some(Some(index)) => u32::try_from(index)
            .ok()
            .filter(|index| *index > 0)
            .ok_or("parâmetro 4 (item) fora da faixa")?,
        Some(None) => return Err("parâmetro 4 (item) não é inteiro"),
        None => return Err("parâmetro 4 (item) ausente"),
    };
    let quantity = match event.parameter(QUANTITY).map(Value::as_integer) {
        Some(Some(quantity)) if (1..=MAX_QUANTITY).contains(&quantity) => quantity as u32,
        Some(Some(_)) => return Err("parâmetro 5 (quantidade) fora da faixa"),
        Some(None) => return Err("parâmetro 5 (quantidade) não é inteiro"),
        None => return Err("parâmetro 5 (quantidade) ausente"),
    };
    Ok(Some(Loot {
        looted_by,
        item_index: item,
        quantity,
    }))
}

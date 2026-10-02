//! Turns decoded loot into Kalbion's normalized loot events.
//!
//! - Identity: `source` names the decoder family and `id` is the capture fingerprint plus
//!   the packet/command position, so importing the same file again yields duplicates, never
//!   new rows. Content is never used to decide identity.
//! - Players: only names on the roster the user gives (their party or guild) are kept;
//!   everyone else is counted, not stored. The victim/container name is never stored.
//! - Items: resolved by the game's numeric index through the imported catalog; an index the
//!   catalog does not know is reported, not guessed.
//! - Quality: the loot event does not carry it, so it stays unknown.
use crate::{Capture, Observation};
use kalbion_core::domain::{normalize_player, Item, LootReceived, Origin};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub const SOURCE: &str = "kalbion.capture.pcap";
/// Live capture: `id` is the capture run plus the packet/command position in that run.
pub const LIVE_SOURCE: &str = "kalbion.capture.live";
pub const MAX_ROSTER: usize = 300;

#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub loot_observed: usize,
    pub converted: usize,
    /// Loot picked up by players not on the roster; their names are not kept.
    pub outside_roster: usize,
    /// Numeric item indexes the catalog does not contain, with how often they appeared.
    pub unknown_items: BTreeMap<u32, u64>,
    pub invalid_players: usize,
}

/// Parses and checks the roster: 1 to `MAX_ROSTER` names, compared case-insensitively.
pub fn roster(names: &[String]) -> Result<BTreeSet<String>, String> {
    let mut roster = BTreeSet::new();
    for name in names {
        if name.trim().is_empty() {
            continue;
        }
        let name = normalize_player(name).map_err(|error| error.to_string())?;
        roster.insert(name.to_lowercase());
    }
    if roster.is_empty() {
        return Err("Informe os jogadores da party ou guilda cujo loot deve ser importado".into());
    }
    if roster.len() > MAX_ROSTER {
        return Err(format!("Lista limitada a {MAX_ROSTER} jogadores"));
    }
    Ok(roster)
}

/// What became of one observed loot pick-up.
#[derive(Debug)]
pub enum Outcome {
    Event(LootReceived),
    /// Picked up by someone outside the roster; the name is not kept.
    OutsideRoster,
    /// The catalog has no item with this game index.
    UnknownItem(u32),
    InvalidPlayer,
}

/// Converts one observation. `run` prefixes the event id (capture fingerprint or live run).
pub fn observation<E>(
    observation: &Observation,
    run: &str,
    source: &str,
    session_id: &str,
    roster: &BTreeSet<String>,
    resolve: &mut impl FnMut(u32) -> Result<Option<Item>, E>,
) -> Result<Outcome, E> {
    let Ok(player) = normalize_player(&observation.loot.looted_by) else {
        return Ok(Outcome::InvalidPlayer);
    };
    if !roster.contains(&player.to_lowercase()) {
        return Ok(Outcome::OutsideRoster);
    }
    let Some(item) = resolve(observation.loot.item_index)? else {
        return Ok(Outcome::UnknownItem(observation.loot.item_index));
    };
    Ok(Outcome::Event(LootReceived {
        id: format!(
            "{run}:{}:{}",
            observation.position.packet, observation.position.command
        ),
        origin: Origin::Observed,
        source: source.into(),
        session_id: session_id.into(),
        occurred_at: observation.occurred_at.clone(),
        player,
        item,
        quality: None,
        quantity: observation.loot.quantity,
    }))
}

pub fn to_events<E>(
    capture: &Capture,
    session_id: &str,
    roster: &BTreeSet<String>,
    mut resolve: impl FnMut(u32) -> Result<Option<Item>, E>,
) -> Result<(Vec<LootReceived>, Report), E> {
    let mut report = Report {
        loot_observed: capture.observations.len(),
        ..Report::default()
    };
    let mut events = Vec::new();
    for item in &capture.observations {
        match observation(
            item,
            &capture.fingerprint,
            SOURCE,
            session_id,
            roster,
            &mut resolve,
        )? {
            Outcome::Event(event) => events.push(event),
            Outcome::OutsideRoster => report.outside_roster += 1,
            Outcome::UnknownItem(index) => *report.unknown_items.entry(index).or_default() += 1,
            Outcome::InvalidPlayer => report.invalid_players += 1,
        }
    }
    report.converted = events.len();
    Ok((events, report))
}

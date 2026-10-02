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
use crate::Capture;
use kalbion_core::domain::{normalize_player, Item, LootReceived, Origin};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub const SOURCE: &str = "kalbion.capture.pcap";
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
    for observation in &capture.observations {
        let Ok(player) = normalize_player(&observation.loot.looted_by) else {
            report.invalid_players += 1;
            continue;
        };
        if !roster.contains(&player.to_lowercase()) {
            report.outside_roster += 1;
            continue;
        }
        let Some(item) = resolve(observation.loot.item_index)? else {
            *report
                .unknown_items
                .entry(observation.loot.item_index)
                .or_default() += 1;
            continue;
        };
        events.push(LootReceived {
            id: format!(
                "{}:{}:{}",
                capture.fingerprint, observation.position.packet, observation.position.command
            ),
            origin: Origin::Observed,
            source: SOURCE.into(),
            session_id: session_id.into(),
            occurred_at: observation.occurred_at.clone(),
            player,
            item,
            quality: None,
            quantity: observation.loot.quantity,
        });
    }
    report.converted = events.len();
    Ok((events, report))
}

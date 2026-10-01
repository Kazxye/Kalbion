//! Simulated loot for trying the app without real data. Market prices live in `market`.
use crate::catalog;
use crate::domain::{now, LootReceived, Origin};

pub const SIMULATOR_SOURCE: &str = "kalbion.simulator.v1";

pub fn simulated(session_id: &str) -> Vec<LootReceived> {
    let occurred_at = now();
    catalog::builtin()
        .into_iter()
        .enumerate()
        .map(|(index, entry)| LootReceived {
            id: uuid::Uuid::new_v4().to_string(),
            origin: Origin::Simulated,
            source: SIMULATOR_SOURCE.into(),
            session_id: session_id.into(),
            occurred_at: occurred_at.clone(),
            player: ["Kazz", "Luna", "Thorin"][index % 3].into(),
            // Equipment shows each quality once; resources show an unreported quality.
            quality: (index < 5).then_some(index as u8 + 1),
            quantity: if index > 4 { 30 } else { 1 },
            item: entry.item,
        })
        .collect()
}

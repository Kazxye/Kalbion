//! Data sources that are not the user typing: the simulator and the market price contract.
use crate::catalog;
use crate::domain::{now, LootReceived, Origin, Price};
use crate::error::{invalid, Result};
use serde::Serialize;

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

#[derive(Debug, Serialize)]
pub struct PriceRequest {
    pub item_id: String,
    pub quality: Option<u8>,
    pub server: String,
    pub city: String,
}
pub trait MarketPrices {
    fn quote(&self, request: &PriceRequest) -> Result<Option<Price>>;
}
pub struct AlbionDataProject;
impl MarketPrices for AlbionDataProject {
    fn quote(&self, _request: &PriceRequest) -> Result<Option<Price>> {
        Err(invalid(
            "Consulta Albion Data Project ainda não implementada; use preço manual",
        ))
    }
}

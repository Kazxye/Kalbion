use crate::domain::*;
use serde::{Deserialize, Serialize};

pub fn catalog() -> Vec<Item> {
    [
        ("T4_BAG", "Bolsa do Adepto", 4, 0, 1),
        ("T5_BAG@1", "Bolsa do Especialista", 5, 1, 2),
        ("T6_MAIN_SWORD@2", "Espada larga do Mestre", 6, 2, 3),
        ("T7_2H_BOW@3", "Arco do Grão-Mestre", 7, 3, 4),
        (
            "T8_ARMOR_PLATE_SET1@4",
            "Armadura de Soldado do Ancião",
            8,
            4,
            5,
        ),
        ("T4_WOOD", "Troncos de pinho", 4, 0, 1),
        ("T6_ORE", "Minério de runita", 6, 0, 1),
    ]
    .into_iter()
    .map(|(id, name, tier, enchantment, quality)| Item {
        id: id.into(),
        name: name.into(),
        tier,
        enchantment,
        quality,
    })
    .collect()
}
pub fn simulated(session_id: &str) -> Vec<LootReceived> {
    let now = chrono::Utc::now().to_rfc3339();
    catalog()
        .into_iter()
        .enumerate()
        .map(|(index, item)| LootReceived {
            id: uuid::Uuid::new_v4().to_string(),
            origin: Origin::Simulated,
            source: "kalbion.simulator.v1".into(),
            session_id: session_id.into(),
            occurred_at: now.clone(),
            player: ["Kazz", "Luna", "Thorin"][index % 3].into(),
            quantity: if index > 4 { 30 } else { 1 },
            item,
        })
        .collect()
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportBatch {
    pub schema_version: u32,
    pub events: Vec<LootReceived>,
}
pub fn parse_import(json: &str) -> Result<ImportBatch> {
    if json.len() > 5_000_000 {
        return Err(invalid("Importação limitada a 5 MB"));
    }
    let batch: ImportBatch = serde_json::from_str(json)?;
    if batch.schema_version != 1 || batch.events.len() > 10_000 {
        return Err(invalid("Versão incompatível ou mais de 10.000 eventos"));
    }
    for event in &batch.events {
        event.validate()?;
    }
    Ok(batch)
}
#[derive(Debug, Serialize)]
pub struct PriceRequest {
    pub item_id: String,
    pub quality: u8,
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

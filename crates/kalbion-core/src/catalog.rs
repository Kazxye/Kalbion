//! Item catalog sources. Kalbion does not ship game data: until the user imports a dump
//! they obtained themselves, only the small built-in demo catalog is available.
use crate::domain::Item;
use crate::error::{invalid, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_CATALOG_BYTES: u64 = 64 * 1024 * 1024;
const MAX_CATALOG_ITEMS: usize = 100_000;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CatalogEntry {
    pub item: Item,
    /// Numeric index used by the game client; needed to resolve items in future captured events.
    pub game_index: Option<u32>,
}
#[derive(Debug, Clone, Serialize)]
pub struct CatalogInfo {
    /// `builtin` or `ao_bin_dumps`.
    pub kind: String,
    pub label: String,
    pub imported_at: Option<String>,
    pub item_count: u64,
    pub skipped_count: u64,
}
pub struct ParsedCatalog {
    pub entries: Vec<CatalogEntry>,
    /// Entries whose UniqueName or name failed validation; reported to the user, never hidden.
    pub skipped: u64,
}

pub fn builtin() -> Vec<CatalogEntry> {
    [
        ("T4_BAG", "Bolsa do Adepto"),
        ("T5_BAG@1", "Bolsa do Especialista"),
        ("T6_MAIN_SWORD@2", "Espada Larga do Mestre"),
        ("T7_2H_BOW@3", "Arco do Grão-Mestre"),
        ("T8_ARMOR_PLATE_SET1@4", "Armadura de Soldado do Ancião"),
        ("T4_WOOD", "Toras de Pinho"),
        ("T6_ORE", "Minério de Runita"),
    ]
    .into_iter()
    .map(|(id, name)| CatalogEntry {
        item: Item::new(id, name).expect("built-in catalog is valid"),
        game_index: None,
    })
    .collect()
}
pub fn builtin_info() -> CatalogInfo {
    CatalogInfo {
        kind: "builtin".into(),
        label: "Catálogo demonstrativo embutido".into(),
        imported_at: None,
        item_count: builtin().len() as u64,
        skipped_count: 0,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DumpItem {
    index: String,
    unique_name: String,
    localized_names: Option<BTreeMap<String, String>>,
}
/// Parses `formatted/items.json` from the ao-data/ao-bin-dumps repository.
/// Names prefer PT-BR, then EN-US, then the UniqueName itself.
pub fn parse_ao_bin_dumps(reader: impl std::io::Read) -> Result<ParsedCatalog> {
    let items: Vec<DumpItem> = serde_json::from_reader(std::io::BufReader::new(reader))?;
    if items.is_empty() || items.len() > MAX_CATALOG_ITEMS {
        return Err(invalid("Catálogo vazio ou com itens demais"));
    }
    let mut seen = BTreeSet::new();
    let mut indexes = BTreeSet::new();
    let mut entries = Vec::with_capacity(items.len());
    let mut skipped = 0;
    for raw in items {
        let names = raw.localized_names.unwrap_or_default();
        let name = ["PT-BR", "EN-US"]
            .iter()
            .filter_map(|locale| names.get(*locale))
            .map(|name| name.trim())
            .find(|name| !name.is_empty())
            .unwrap_or(&raw.unique_name);
        let Ok(item) = Item::new(&raw.unique_name, name) else {
            skipped += 1;
            continue;
        };
        let game_index = raw
            .index
            .parse::<u32>()
            .map_err(|_| invalid(format!("Index inválido em {}", item.id)))?;
        if !seen.insert(item.id.clone()) || !indexes.insert(game_index) {
            return Err(invalid(format!(
                "Catálogo com UniqueName ou Index repetido: {}",
                item.id
            )));
        }
        entries.push(CatalogEntry {
            item,
            game_index: Some(game_index),
        });
    }
    if entries.is_empty() {
        return Err(invalid("Nenhum item válido no catálogo"));
    }
    Ok(ParsedCatalog { entries, skipped })
}

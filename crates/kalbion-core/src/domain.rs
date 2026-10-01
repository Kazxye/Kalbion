use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MAX_MONEY: i64 = 1_000_000_000_000;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Validation(String),
    #[error("Falha no banco de dados local")]
    Database(#[from] rusqlite::Error),
    #[error("JSON inválido: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Falha ao gerar CSV")]
    Csv(#[from] csv::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
pub fn invalid(message: &str) -> Error {
    Error::Validation(message.into())
}
pub fn text(value: &str, max: usize) -> Result<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(invalid(
            "Texto vazio, longo demais ou com caracteres de controle",
        ));
    }
    Ok(())
}
pub fn timestamp(value: &str) -> Result<()> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map_err(|_| invalid("Horário deve usar RFC 3339"))?;
    Ok(())
}
pub fn money(value: i64) -> Result<()> {
    if !(0..=MAX_MONEY).contains(&value) {
        return Err(invalid("Valor fora do limite permitido"));
    }
    Ok(())
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Simulated,
    Manual,
    Observed,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub tier: u8,
    pub enchantment: u8,
    pub quality: u8,
}
impl Item {
    pub fn validate(&self) -> Result<()> {
        text(&self.id, 100)?;
        text(&self.name, 150)?;
        if !(1..=8).contains(&self.tier) || self.enchantment > 4 || !(1..=5).contains(&self.quality)
        {
            return Err(invalid("Tier, enchantment ou quality inválido"));
        }
        let prefix = format!("T{}_", self.tier);
        let suffix = format!("@{}", self.enchantment);
        let base = if self.enchantment > 0 {
            self.id
                .strip_suffix(&suffix)
                .ok_or_else(|| invalid("Enchantment não corresponde ao ID"))?
        } else {
            &self.id
        };
        if !base.starts_with(&prefix)
            || !base
                .chars()
                .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
        {
            return Err(invalid(
                "ID do catálogo não corresponde ao tier/enchantment",
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LootReceived {
    pub id: String,
    pub origin: Origin,
    pub source: String,
    pub session_id: String,
    pub occurred_at: String,
    pub player: String,
    pub item: Item,
    pub quantity: u32,
}
impl LootReceived {
    pub fn validate(&self) -> Result<()> {
        text(&self.id, 128)?;
        text(&self.source, 128)?;
        text(&self.session_id, 128)?;
        text(&self.player, 64)?;
        timestamp(&self.occurred_at)?;
        self.item.validate()?;
        if self.quantity == 0 || self.quantity > 1_000_000 {
            return Err(invalid("Quantidade deve estar entre 1 e 1.000.000"));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub closed_at: Option<String>,
    pub server: String,
    pub city: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub server: String,
    pub city: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            server: "americas".into(),
            city: "Bridgewatch".into(),
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<()> {
        if !["americas", "europe", "asia"].contains(&self.server.as_str())
            || ![
                "Bridgewatch",
                "Martlock",
                "Lymhurst",
                "Fort Sterling",
                "Thetford",
                "Caerleon",
                "Brecilien",
            ]
            .contains(&self.city.as_str())
        {
            return Err(invalid("Servidor ou cidade inválido"));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Price {
    pub unit_silver: i64,
    pub source: String,
    pub server: String,
    pub city: String,
    pub queried_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LootRow {
    pub event: LootReceived,
    pub imported: bool,
    pub price: Option<Price>,
}
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Filter {
    pub player: String,
    pub item: String,
    pub tier: Option<u8>,
    pub enchantment: Option<u8>,
    pub quality: Option<u8>,
}
impl Filter {
    pub fn matches(&self, row: &LootRow) -> bool {
        let event = &row.event;
        event
            .player
            .to_lowercase()
            .contains(&self.player.to_lowercase())
            && format!("{} {}", event.item.name, event.item.id)
                .to_lowercase()
                .contains(&self.item.to_lowercase())
            && self.tier.is_none_or(|value| value == event.item.tier)
            && self
                .enchantment
                .is_none_or(|value| value == event.item.enchantment)
            && self.quality.is_none_or(|value| value == event.item.quality)
    }
}
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Total {
    pub events: u64,
    pub quantity: u64,
    pub estimated_silver: i64,
    pub unpriced_events: u64,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Totals {
    pub session: Total,
    pub players: BTreeMap<String, Total>,
}
pub fn totals(rows: &[LootRow]) -> Result<Totals> {
    let mut result = Totals {
        session: Total::default(),
        players: BTreeMap::new(),
    };
    for row in rows {
        let value = row
            .price
            .as_ref()
            .map(|price| {
                price
                    .unit_silver
                    .checked_mul(i64::from(row.event.quantity))
                    .ok_or_else(|| invalid("Total excedeu o limite"))
            })
            .transpose()?;
        for total in [
            &mut result.session,
            result.players.entry(row.event.player.clone()).or_default(),
        ] {
            total.events += 1;
            total.quantity += u64::from(row.event.quantity);
            if let Some(value) = value {
                total.estimated_silver = total
                    .estimated_silver
                    .checked_add(value)
                    .filter(|sum| *sum <= 9_000_000_000_000_000)
                    .ok_or_else(|| invalid("Total excedeu o limite"))?;
            } else {
                total.unpriced_events += 1;
            }
        }
    }
    Ok(result)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerKind {
    Income,
    Expense,
    Regear,
    Settlement,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub id: String,
    pub session_id: String,
    pub kind: LedgerKind,
    pub player: String,
    pub description: String,
    pub amount: i64,
    pub occurred_at: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Finance {
    pub income: i64,
    pub expenses: i64,
    pub settlements: i64,
    pub available: i64,
}
pub fn finance(entries: &[LedgerEntry]) -> Result<Finance> {
    let mut result = Finance {
        income: 0,
        expenses: 0,
        settlements: 0,
        available: 0,
    };
    for entry in entries {
        let target = match entry.kind {
            LedgerKind::Income => &mut result.income,
            LedgerKind::Expense | LedgerKind::Regear => &mut result.expenses,
            LedgerKind::Settlement => &mut result.settlements,
        };
        *target = target
            .checked_add(entry.amount)
            .filter(|value| *value <= 4_000_000_000_000_000)
            .ok_or_else(|| invalid("Saldo excedeu o limite"))?;
    }
    result.available = result.income - result.expenses - result.settlements;
    Ok(result)
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Split {
    pub player: String,
    pub silver: i64,
}
pub fn split(amount: i64, players: &[String]) -> Result<Vec<Split>> {
    money(amount)?;
    let unique: std::collections::BTreeSet<_> = players.iter().collect();
    if unique.is_empty() || unique.len() != players.len() || players.len() > 100 {
        return Err(invalid("Informe 1 a 100 participantes sem duplicatas"));
    }
    for player in players {
        text(player, 64)?;
    }
    let count = players.len() as i64;
    Ok(unique
        .into_iter()
        .enumerate()
        .map(|(index, player)| Split {
            player: player.clone(),
            silver: amount / count + i64::from((index as i64) < amount % count),
        })
        .collect())
}

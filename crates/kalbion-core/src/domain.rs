use crate::error::{invalid, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_MONEY: i64 = 1_000_000_000_000;
/// Totals are shown in JavaScript, so they must stay below 2^53.
pub const MAX_TOTAL: i64 = 9_000_000_000_000_000;
pub const MAX_QUANTITY: u32 = 1_000_000;

pub fn text(value: &str, max: usize) -> Result<()> {
    if value.trim().is_empty() || value.chars().count() > max || value.chars().any(char::is_control)
    {
        return Err(invalid(
            "Texto vazio, longo demais ou com caracteres de controle",
        ));
    }
    Ok(())
}
pub fn money(value: i64) -> Result<()> {
    if !(0..=MAX_MONEY).contains(&value) {
        return Err(invalid("Valor fora do limite permitido"));
    }
    Ok(())
}
/// Canonical UTC form with fixed width, so equal instants compare equal and sort lexically.
pub fn normalize_timestamp(value: &str) -> Result<String> {
    let parsed =
        DateTime::parse_from_rfc3339(value).map_err(|_| invalid("Horário deve usar RFC 3339"))?;
    Ok(parsed
        .with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Micros, true))
}
pub fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true)
}
pub fn normalize_player(value: &str) -> Result<String> {
    text(value, 64)?;
    Ok(value.trim().into())
}
pub fn validate_quality(quality: Option<u8>) -> Result<()> {
    if quality.is_some_and(|value| !(1..=5).contains(&value)) {
        return Err(invalid("Quality deve estar entre 1 e 5"));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Simulated,
    Manual,
    Observed,
}
impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Simulated => "simulated",
            Self::Manual => "manual",
            Self::Observed => "observed",
        }
    }
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "simulated" => Ok(Self::Simulated),
            "manual" => Ok(Self::Manual),
            "observed" => Ok(Self::Observed),
            _ => Err(invalid("Origem desconhecida")),
        }
    }
}

/// An item type identified by its Albion UniqueName (e.g. `T5_BAG@1`).
/// Tier and enchantment are derived from the name, so they cannot disagree with it.
/// Quality belongs to a looted instance, not to the item type.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub tier: Option<u8>,
    pub enchantment: u8,
}
impl Item {
    pub fn new(id: &str, name: &str) -> Result<Self> {
        let (tier, enchantment) = parse_unique_name(id)?;
        text(name, 150)?;
        Ok(Self {
            id: id.into(),
            name: name.trim().into(),
            tier,
            enchantment,
        })
    }
}
/// Items such as `UNIQUE_HIDEOUT` or `TREASURE_*` have no tier; that is valid, not an error.
pub fn parse_unique_name(id: &str) -> Result<(Option<u8>, u8)> {
    let (base, enchantment) = match id.split_once('@') {
        Some((base, level)) => (
            base,
            match level {
                "1" => 1,
                "2" => 2,
                "3" => 3,
                "4" => 4,
                _ => return Err(invalid("Enchantment do ID deve estar entre @1 e @4")),
            },
        ),
        None => (id, 0),
    };
    if base.is_empty()
        || id.len() > 100
        || !base
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(invalid("ID de item inválido"));
    }
    let tier = base
        .strip_prefix('T')
        .and_then(|rest| rest.split_once('_'))
        .and_then(|(tier, _)| tier.parse::<u8>().ok())
        .filter(|tier| (1..=8).contains(tier));
    Ok((tier, enchantment))
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LootReceived {
    pub id: String,
    pub origin: Origin,
    pub source: String,
    pub session_id: String,
    /// Always in the canonical form produced by `normalize_timestamp`.
    pub occurred_at: String,
    pub player: String,
    pub item: Item,
    /// `None` when the source did not report quality; never guessed.
    pub quality: Option<u8>,
    pub quantity: u32,
}
impl LootReceived {
    pub fn validate(&self) -> Result<()> {
        text(&self.id, 128)?;
        text(&self.source, 128)?;
        text(&self.session_id, 128)?;
        text(&self.player, 64)?;
        if normalize_timestamp(&self.occurred_at)? != self.occurred_at {
            return Err(invalid("Horário não normalizado"));
        }
        if Item::new(&self.item.id, &self.item.name)? != self.item {
            return Err(invalid("Tier/enchantment não corresponde ao ID do item"));
        }
        validate_quality(self.quality)?;
        if self.quantity == 0 || self.quantity > MAX_QUANTITY {
            return Err(invalid("Quantidade deve estar entre 1 e 1.000.000"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub closed_at: Option<String>,
    pub server: String,
    pub city: String,
}
pub const SERVERS: [&str; 3] = ["americas", "europe", "asia"];
/// Market locations, spelled as the game and the Albion Data Project spell them.
pub const CITIES: [&str; 7] = [
    "Bridgewatch",
    "Martlock",
    "Lymhurst",
    "Fort Sterling",
    "Thetford",
    "Caerleon",
    "Brecilien",
];
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
        if !SERVERS.contains(&self.server.as_str()) || !CITIES.contains(&self.city.as_str()) {
            return Err(invalid("Servidor ou cidade inválido"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PriceSource {
    /// Typed by the user; always prevails over market data for the same item and quality.
    Manual,
    /// Lowest sell order reported by the Albion Data Project.
    AlbionData,
}
impl PriceSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::AlbionData => "albion_data",
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Price {
    pub unit_silver: i64,
    pub source: PriceSource,
    pub server: String,
    pub city: String,
    /// When Kalbion stored the price.
    pub recorded_at: String,
    /// When the market observed it; only external sources know this, and they always do.
    pub observed_at: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct LootRow {
    pub event: LootReceived,
    pub imported: bool,
    /// Voided rows stay stored (so replays remain duplicates) but are excluded from totals.
    pub voided_at: Option<String>,
    pub price: Option<Price>,
}

/// `quality: Some(0)` selects events whose quality is unknown.
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
    pub fn validate(&self) -> Result<()> {
        if self.player.chars().count() > 64
            || self.item.chars().count() > 150
            || self.tier.is_some_and(|tier| !(1..=8).contains(&tier))
            || self.enchantment.is_some_and(|value| value > 4)
            || self.quality.is_some_and(|value| value > 5)
        {
            return Err(invalid("Filtro inválido"));
        }
        Ok(())
    }
    pub fn is_empty(&self) -> bool {
        self.player.trim().is_empty()
            && self.item.trim().is_empty()
            && self.tier.is_none()
            && self.enchantment.is_none()
            && self.quality.is_none()
    }
}

#[derive(Debug, Default, Serialize)]
pub struct Total {
    pub events: u64,
    pub quantity: u64,
    pub estimated_silver: i64,
    pub unpriced_events: u64,
}
#[derive(Debug, Serialize)]
pub struct Totals {
    pub session: Total,
    pub players: BTreeMap<String, Total>,
}
pub fn totals(rows: &[LootRow]) -> Result<Totals> {
    let mut result = Totals {
        session: Total::default(),
        players: BTreeMap::new(),
    };
    for row in rows.iter().filter(|row| row.voided_at.is_none()) {
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
                    .filter(|sum| *sum <= MAX_TOTAL)
                    .ok_or_else(|| invalid("Total excedeu o limite"))?;
            } else {
                total.unpriced_events += 1;
            }
        }
    }
    Ok(result)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LedgerKind {
    Income,
    Expense,
    Regear,
    Settlement,
}
impl LedgerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Income => "income",
            Self::Expense => "expense",
            Self::Regear => "regear",
            Self::Settlement => "settlement",
        }
    }
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "income" => Ok(Self::Income),
            "expense" => Ok(Self::Expense),
            "regear" => Ok(Self::Regear),
            "settlement" => Ok(Self::Settlement),
            _ => Err(invalid("Tipo de lançamento desconhecido")),
        }
    }
}
/// The ledger is append-only: mistakes are corrected by a reversal entry, never by editing.
#[derive(Debug, Clone, Serialize)]
pub struct LedgerEntry {
    pub id: String,
    pub session_id: String,
    pub kind: LedgerKind,
    pub player: String,
    pub description: String,
    pub amount: i64,
    pub occurred_at: String,
    /// Set on a reversal entry: the entry it cancels.
    pub reverses: Option<String>,
    /// Set on an entry that has been cancelled by a reversal.
    pub reversed_by: Option<String>,
}
#[derive(Debug, Serialize)]
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
        let amount = if entry.reverses.is_some() {
            -entry.amount
        } else {
            entry.amount
        };
        *target = target
            .checked_add(amount)
            .filter(|value| value.abs() <= MAX_TOTAL / 2)
            .ok_or_else(|| invalid("Saldo excedeu o limite"))?;
    }
    result.available = result.income - result.expenses - result.settlements;
    Ok(result)
}
#[derive(Debug, Serialize)]
pub struct Split {
    pub player: String,
    pub silver: i64,
}
/// Equal split in whole silver; the remainder goes one silver each, in alphabetical order.
pub fn split(amount: i64, players: &[String]) -> Result<Vec<Split>> {
    money(amount)?;
    let players = players
        .iter()
        .map(|player| normalize_player(player))
        .collect::<Result<Vec<_>>>()?;
    let unique: BTreeSet<_> = players.iter().map(|player| player.to_lowercase()).collect();
    if players.is_empty() || unique.len() != players.len() || players.len() > 100 {
        return Err(invalid("Informe 1 a 100 participantes sem duplicatas"));
    }
    let mut players = players;
    players.sort();
    let count = players.len() as i64;
    Ok(players
        .into_iter()
        .enumerate()
        .map(|(index, player)| Split {
            player,
            silver: amount / count + i64::from((index as i64) < amount % count),
        })
        .collect())
}

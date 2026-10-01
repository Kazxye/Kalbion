use crate::catalog::{self, CatalogInfo, ParsedCatalog};
use crate::domain::*;
use crate::error::{invalid, Error, Result};
use crate::market::Quote;
use crate::{adapters, import, migrations};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSqlOutput, ValueRef};
use rusqlite::{params, Connection, OptionalExtension, Row, ToSql};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

pub struct Store {
    connection: Connection,
}
#[derive(Serialize)]
pub struct View {
    pub session: Session,
    pub rows: Vec<LootRow>,
    pub totals: Totals,
    pub full_totals: Totals,
    pub ledger: Vec<LedgerEntry>,
    pub finance: Finance,
}
/// What a market refresh will ask for, computed under the store lock so the network request
/// itself can run without holding it.
#[derive(Debug, Clone)]
pub struct MarketPlan {
    pub server: String,
    pub city: String,
    /// Item/quality pairs without a manual price.
    pub wanted: Vec<(String, u8)>,
}
#[derive(Debug, Default, Serialize)]
pub struct MarketRefresh {
    pub updated: usize,
    /// Pairs the market has no current sell order for; an older market price, if any, is kept.
    pub unavailable: usize,
    pub manual_kept: usize,
    /// Pairs whose quality was not reported; market data is per quality, so only a manual
    /// price can cover them.
    pub unknown_quality: usize,
}
#[derive(Debug, Serialize)]
pub struct InsertResult {
    pub inserted: usize,
    pub duplicates: usize,
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn from_sql_error(error: Error) -> FromSqlError {
    FromSqlError::Other(Box::new(error))
}
impl FromSql for Origin {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        Origin::parse(value.as_str()?).map_err(from_sql_error)
    }
}
impl ToSql for Origin {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(self.as_str().into())
    }
}
impl FromSql for LedgerKind {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        LedgerKind::parse(value.as_str()?).map_err(from_sql_error)
    }
}
impl ToSql for LedgerKind {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(self.as_str().into())
    }
}
impl FromSql for PriceSource {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "manual" => Ok(PriceSource::Manual),
            "albion_data" => Ok(PriceSource::AlbionData),
            _ => Err(from_sql_error(invalid("Origem de preço desconhecida"))),
        }
    }
}
impl ToSql for PriceSource {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(self.as_str().into())
    }
}

const SESSION_SELECT: &str = "SELECT id, name, created_at, closed_at, server, city FROM sessions";
fn read_session(row: &Row) -> rusqlite::Result<Session> {
    Ok(Session {
        id: row.get(0)?,
        name: row.get(1)?,
        created_at: row.get(2)?,
        closed_at: row.get(3)?,
        server: row.get(4)?,
        city: row.get(5)?,
    })
}

const ROW_SELECT: &str = "SELECT e.source, e.event_id, e.session_id, e.origin, e.imported,
    e.occurred_at, e.player, e.item_id, e.item_name, e.tier, e.enchantment, e.quality,
    e.quantity, e.voided_at, p.unit_silver, p.source, p.server, p.city, p.recorded_at,
    p.observed_at
    FROM loot_events e
    LEFT JOIN item_prices p ON p.session_id = e.session_id AND p.item_id = e.item_id
        AND p.quality = IFNULL(e.quality, 0)";
fn read_loot_row(row: &Row) -> rusqlite::Result<LootRow> {
    let price = match row.get::<_, Option<i64>>(14)? {
        Some(unit_silver) => Some(Price {
            unit_silver,
            source: row.get(15)?,
            server: row.get(16)?,
            city: row.get(17)?,
            recorded_at: row.get(18)?,
            observed_at: row.get(19)?,
        }),
        None => None,
    };
    Ok(LootRow {
        event: LootReceived {
            source: row.get(0)?,
            id: row.get(1)?,
            session_id: row.get(2)?,
            origin: row.get(3)?,
            occurred_at: row.get(5)?,
            player: row.get(6)?,
            item: Item {
                id: row.get(7)?,
                name: row.get(8)?,
                tier: row.get(9)?,
                enchantment: row.get(10)?,
            },
            quality: row.get(11)?,
            quantity: row.get(12)?,
        },
        imported: row.get(4)?,
        voided_at: row.get(13)?,
        price,
    })
}

fn load_ledger(connection: &Connection, session_id: &str) -> Result<Vec<LedgerEntry>> {
    let mut statement = connection.prepare(
        "SELECT l.id, l.session_id, l.kind, l.player, l.description, l.amount, l.occurred_at,
         l.reverses, r.id
         FROM ledger_entries l LEFT JOIN ledger_entries r ON r.reverses = l.id
         WHERE l.session_id = ?1 ORDER BY l.rowid DESC",
    )?;
    let rows = statement.query_map([session_id], |row| {
        Ok(LedgerEntry {
            id: row.get(0)?,
            session_id: row.get(1)?,
            kind: row.get(2)?,
            player: row.get(3)?,
            description: row.get(4)?,
            amount: row.get(5)?,
            occurred_at: row.get(6)?,
            reverses: row.get(7)?,
            reversed_by: row.get(8)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}
fn insert_ledger(connection: &Connection, entry: &LedgerEntry) -> Result<()> {
    connection.execute(
        "INSERT INTO ledger_entries (id, session_id, kind, player, description, amount,
         occurred_at, reverses) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            entry.id,
            entry.session_id,
            entry.kind,
            entry.player,
            entry.description,
            entry.amount,
            entry.occurred_at,
            entry.reverses
        ],
    )?;
    Ok(())
}
/// Item/quality pairs that still count in the session (at least one row not voided), mapped
/// to whether they have a manual price.
fn market_pairs(rows: &[LootRow]) -> BTreeMap<(String, Option<u8>), bool> {
    let mut pairs = BTreeMap::new();
    for row in rows.iter().filter(|row| row.voided_at.is_none()) {
        let manual = row
            .price
            .as_ref()
            .is_some_and(|price| price.source == PriceSource::Manual);
        pairs.insert((row.event.item.id.clone(), row.event.quality), manual);
    }
    pairs
}
/// Case-insensitive (ASCII) substring pattern for LIKE; empty input disables the condition.
fn like_pattern(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        return String::new();
    }
    let mut pattern = String::from("%");
    for character in value.chars() {
        if matches!(character, '%' | '_' | '\\') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern.push('%');
    pattern
}
fn same_priced_item(row: &LootRow, item_id: &str, quality: Option<u8>) -> bool {
    row.event.item.id == item_id && row.event.quality == quality
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )?;
        migrations::run(&mut connection, path)?;
        Ok(Self { connection })
    }

    pub fn settings(&self) -> Result<Settings> {
        Ok(self
            .connection
            .query_row(
                "SELECT server, city FROM app_settings WHERE id = 1",
                [],
                |row| {
                    Ok(Settings {
                        server: row.get(0)?,
                        city: row.get(1)?,
                    })
                },
            )
            .optional()?
            .unwrap_or_default())
    }
    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        settings.validate()?;
        self.connection.execute(
            "INSERT INTO app_settings (id, server, city) VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET server = excluded.server, city = excluded.city",
            params![settings.server, settings.city],
        )?;
        tracing::info!(operation = "settings_saved");
        Ok(())
    }

    pub fn sessions(&self) -> Result<Vec<Session>> {
        let mut statement = self
            .connection
            .prepare(&format!("{SESSION_SELECT} ORDER BY created_at DESC, id"))?;
        let rows = statement.query_map([], read_session)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn session(&self, session_id: &str) -> Result<Session> {
        self.connection
            .query_row(
                &format!("{SESSION_SELECT} WHERE id = ?1"),
                [session_id],
                read_session,
            )
            .optional()?
            .ok_or_else(|| invalid("Sessão não encontrada"))
    }
    fn require_open(&self, session_id: &str) -> Result<Session> {
        let session = self.session(session_id)?;
        if session.closed_at.is_some() {
            return Err(invalid("Sessão encerrada; reabra para alterar o loot"));
        }
        Ok(session)
    }
    pub fn create_session(&self, name: &str) -> Result<Session> {
        text(name, 120)?;
        let settings = self.settings()?;
        let session = Session {
            id: new_id(),
            name: name.trim().into(),
            created_at: now(),
            closed_at: None,
            server: settings.server,
            city: settings.city,
        };
        self.connection.execute(
            "INSERT INTO sessions (id, name, created_at, closed_at, server, city)
             VALUES (?1, ?2, ?3, NULL, ?4, ?5)",
            params![
                session.id,
                session.name,
                session.created_at,
                session.server,
                session.city
            ],
        )?;
        tracing::info!(operation = "session_created", session_id = %session.id);
        Ok(session)
    }
    pub fn set_closed(&self, session_id: &str, closed: bool) -> Result<()> {
        self.session(session_id)?;
        self.connection.execute(
            "UPDATE sessions SET closed_at = ?1 WHERE id = ?2",
            params![closed.then(now), session_id],
        )?;
        tracing::info!(operation = "session_status_changed", %session_id, closed);
        Ok(())
    }

    fn has_imported_catalog(&self) -> Result<bool> {
        Ok(self
            .connection
            .query_row("SELECT 1 FROM catalog_source WHERE id = 1", [], |_| Ok(()))
            .optional()?
            .is_some())
    }
    pub fn catalog_info(&self) -> Result<CatalogInfo> {
        let imported = self
            .connection
            .query_row(
                "SELECT label, imported_at, item_count, skipped_count FROM catalog_source WHERE id = 1",
                [],
                |row| {
                    Ok(CatalogInfo {
                        kind: "ao_bin_dumps".into(),
                        label: row.get(0)?,
                        imported_at: Some(row.get(1)?),
                        item_count: row.get::<_, i64>(2)?.unsigned_abs(),
                        skipped_count: row.get::<_, i64>(3)?.unsigned_abs(),
                    })
                },
            )
            .optional()?;
        Ok(imported.unwrap_or_else(catalog::builtin_info))
    }
    pub fn search_catalog(&self, query: &str, limit: u32) -> Result<Vec<Item>> {
        if query.chars().count() > 150 {
            return Err(invalid("Busca longa demais"));
        }
        let limit = limit.clamp(1, 100);
        if !self.has_imported_catalog()? {
            let query = query.trim().to_lowercase();
            return Ok(catalog::builtin()
                .into_iter()
                .map(|entry| entry.item)
                .filter(|item| {
                    item.name.to_lowercase().contains(&query)
                        || item.id.to_lowercase().contains(&query)
                })
                .take(limit as usize)
                .collect());
        }
        let mut statement = self.connection.prepare(
            "SELECT unique_name, name, tier, enchantment FROM catalog_items
             WHERE ?1 = '' OR name LIKE ?1 ESCAPE '\\' OR unique_name LIKE ?1 ESCAPE '\\'
             ORDER BY unique_name = ?2 DESC, tier IS NULL, tier, unique_name LIMIT ?3",
        )?;
        let rows = statement.query_map(
            params![like_pattern(query), query.trim().to_uppercase(), limit],
            |row| {
                Ok(Item {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    tier: row.get(2)?,
                    enchantment: row.get(3)?,
                })
            },
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
    fn catalog_item(&self, item_id: &str) -> Result<Item> {
        let item = if self.has_imported_catalog()? {
            self.connection
                .query_row(
                    "SELECT unique_name, name, tier, enchantment FROM catalog_items
                     WHERE unique_name = ?1",
                    [item_id],
                    |row| {
                        Ok(Item {
                            id: row.get(0)?,
                            name: row.get(1)?,
                            tier: row.get(2)?,
                            enchantment: row.get(3)?,
                        })
                    },
                )
                .optional()?
        } else {
            catalog::builtin()
                .into_iter()
                .map(|entry| entry.item)
                .find(|item| item.id == item_id)
        };
        item.ok_or_else(|| invalid("Item não está no catálogo"))
    }
    /// Replaces the whole catalog atomically. Recorded loot keeps the names it was saved with.
    pub fn replace_catalog(&mut self, parsed: &ParsedCatalog, label: &str) -> Result<CatalogInfo> {
        text(label, 200)?;
        let transaction = self.connection.transaction()?;
        transaction.execute_batch("DELETE FROM catalog_items; DELETE FROM catalog_source;")?;
        {
            let mut insert = transaction.prepare(
                "INSERT INTO catalog_items (unique_name, game_index, name, tier, enchantment)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for entry in &parsed.entries {
                insert.execute(params![
                    entry.item.id,
                    entry.game_index,
                    entry.item.name,
                    entry.item.tier,
                    entry.item.enchantment
                ])?;
            }
        }
        transaction.execute(
            "INSERT INTO catalog_source (id, kind, label, imported_at, item_count, skipped_count)
             VALUES (1, 'ao_bin_dumps', ?1, ?2, ?3, ?4)",
            params![
                label.trim(),
                now(),
                parsed.entries.len() as i64,
                i64::try_from(parsed.skipped).unwrap_or(i64::MAX)
            ],
        )?;
        transaction.commit()?;
        tracing::info!(
            operation = "catalog_imported",
            items = parsed.entries.len(),
            skipped = parsed.skipped
        );
        self.catalog_info()
    }

    /// Stores events idempotently by `(source, id)`: an identical replay is counted as a
    /// duplicate, the same identity with different content rejects the whole batch, and
    /// equal loot with distinct IDs is kept. Content is never used to guess identity.
    pub fn ingest(&mut self, events: &[LootReceived], imported: bool) -> Result<InsertResult> {
        if events.len() > import::MAX_IMPORT_EVENTS {
            return Err(invalid("Lote grande demais"));
        }
        for event in events {
            event.validate()?;
        }
        // Check that totals stay representable first, so a batch cannot poison a session view.
        let mut prospective: BTreeMap<&str, Vec<LootRow>> = BTreeMap::new();
        for event in events {
            if !prospective.contains_key(event.session_id.as_str()) {
                self.require_open(&event.session_id)?;
                let rows = self.rows(&event.session_id, &Filter::default())?;
                prospective.insert(&event.session_id, rows);
            }
            let rows = prospective
                .get_mut(event.session_id.as_str())
                .expect("session rows were loaded above");
            if rows
                .iter()
                .any(|row| row.event.source == event.source && row.event.id == event.id)
            {
                continue;
            }
            let price = rows
                .iter()
                .find(|row| same_priced_item(row, &event.item.id, event.quality))
                .and_then(|row| row.price.clone());
            rows.push(LootRow {
                event: event.clone(),
                imported,
                voided_at: None,
                price,
            });
        }
        for rows in prospective.values() {
            totals(rows)?;
        }
        let transaction = self.connection.transaction()?;
        let mut result = InsertResult {
            inserted: 0,
            duplicates: 0,
        };
        for event in events {
            let existing = transaction
                .query_row(
                    &format!("{ROW_SELECT} WHERE e.source = ?1 AND e.event_id = ?2"),
                    params![event.source, event.id],
                    read_loot_row,
                )
                .optional()?;
            match existing {
                Some(row) if row.event == *event => result.duplicates += 1,
                Some(_) => {
                    return Err(invalid(
                        "ID de evento reutilizado com conteúdo diferente; lote rejeitado",
                    ))
                }
                None => {
                    transaction.execute(
                        "INSERT INTO loot_events (source, event_id, session_id, origin, imported,
                         occurred_at, player, item_id, item_name, tier, enchantment, quality,
                         quantity) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                        params![
                            event.source,
                            event.id,
                            event.session_id,
                            event.origin,
                            imported,
                            event.occurred_at,
                            event.player,
                            event.item.id,
                            event.item.name,
                            event.item.tier,
                            event.item.enchantment,
                            event.quality,
                            event.quantity
                        ],
                    )?;
                    result.inserted += 1;
                }
            }
        }
        transaction.commit()?;
        tracing::info!(
            operation = "loot_ingested",
            inserted = result.inserted,
            duplicates = result.duplicates,
            imported
        );
        Ok(result)
    }
    pub fn simulate(&mut self, session_id: &str) -> Result<InsertResult> {
        self.ingest(&adapters::simulated(session_id), false)
    }
    pub fn manual(
        &mut self,
        session_id: &str,
        player: &str,
        item_id: &str,
        quality: Option<u8>,
        quantity: u32,
    ) -> Result<InsertResult> {
        validate_quality(quality)?;
        let event = LootReceived {
            id: new_id(),
            origin: Origin::Manual,
            source: "kalbion.manual.v1".into(),
            session_id: session_id.into(),
            occurred_at: now(),
            player: normalize_player(player)?,
            item: self.catalog_item(item_id)?,
            quality,
            quantity,
        };
        self.ingest(&[event], false)
    }
    pub fn import(&mut self, session_id: &str, json: &str) -> Result<InsertResult> {
        let events = import::parse(json)?;
        if events.iter().any(|event| event.session_id != session_id) {
            return Err(invalid(
                "O session_id importado deve corresponder à sessão selecionada; IDs não são remapeados",
            ));
        }
        self.ingest(&events, true)
    }
    /// Voiding keeps the event stored, so replaying its source still counts as a duplicate.
    pub fn set_voided(
        &self,
        session_id: &str,
        source: &str,
        event_id: &str,
        voided: bool,
    ) -> Result<()> {
        self.require_open(session_id)?;
        let mut rows = self.rows(session_id, &Filter::default())?;
        let row = rows
            .iter_mut()
            .find(|row| row.event.source == source && row.event.id == event_id)
            .ok_or_else(|| invalid("Evento não encontrado na sessão"))?;
        if row.voided_at.is_some() == voided {
            return Err(invalid(if voided {
                "Evento já está anulado"
            } else {
                "Evento não está anulado"
            }));
        }
        row.voided_at = voided.then(now);
        let voided_at = row.voided_at.clone();
        totals(&rows)?;
        self.connection.execute(
            "UPDATE loot_events SET voided_at = ?1 WHERE source = ?2 AND event_id = ?3",
            params![voided_at, source, event_id],
        )?;
        tracing::info!(operation = "loot_void_changed", %session_id, voided);
        Ok(())
    }
    pub fn price(
        &self,
        session_id: &str,
        item_id: &str,
        quality: Option<u8>,
        amount: Option<i64>,
    ) -> Result<()> {
        let session = self.session(session_id)?;
        validate_quality(quality)?;
        let mut rows = self.rows(session_id, &Filter::default())?;
        if !rows
            .iter()
            .any(|row| same_priced_item(row, item_id, quality))
        {
            return Err(invalid("Item/quality não encontrado na sessão"));
        }
        let key_quality = quality.unwrap_or(0);
        if let Some(unit_silver) = amount {
            money(unit_silver)?;
            let price = Price {
                unit_silver,
                source: PriceSource::Manual,
                server: session.server,
                city: session.city,
                recorded_at: now(),
                observed_at: None,
            };
            for row in rows
                .iter_mut()
                .filter(|row| same_priced_item(row, item_id, quality))
            {
                row.price = Some(price.clone());
            }
            totals(&rows)?;
            self.connection.execute(
                "INSERT INTO item_prices (session_id, item_id, quality, unit_silver, source,
                 server, city, recorded_at, observed_at)
                 VALUES (?1, ?2, ?3, ?4, 'manual', ?5, ?6, ?7, NULL)
                 ON CONFLICT(session_id, item_id, quality) DO UPDATE SET
                 unit_silver = excluded.unit_silver, source = excluded.source,
                 server = excluded.server, city = excluded.city,
                 recorded_at = excluded.recorded_at, observed_at = excluded.observed_at",
                params![
                    session_id,
                    item_id,
                    key_quality,
                    unit_silver,
                    price.server,
                    price.city,
                    price.recorded_at
                ],
            )?;
        } else {
            self.connection.execute(
                "DELETE FROM item_prices WHERE session_id = ?1 AND item_id = ?2 AND quality = ?3",
                params![session_id, item_id, key_quality],
            )?;
        }
        tracing::info!(operation = "price_updated", %session_id);
        Ok(())
    }
    pub fn market_plan(&self, session_id: &str) -> Result<MarketPlan> {
        let session = self.session(session_id)?;
        let wanted = market_pairs(&self.rows(session_id, &Filter::default())?)
            .into_iter()
            .filter(|(_, manual)| !manual)
            .filter_map(|((item_id, quality), _)| Some((item_id, quality?)))
            // Loot migrated from old versions was never re-validated; such IDs cannot be quoted.
            .filter(|(item_id, _)| parse_unique_name(item_id).is_ok())
            .collect();
        Ok(MarketPlan {
            server: session.server,
            city: session.city,
            wanted,
        })
    }
    /// Stores market quotes as the session's prices. A manual price is never replaced, even
    /// one typed while the quotes were being fetched. Like manual prices, quotes are a
    /// snapshot: a closed session does not drift with the market until refreshed again.
    pub fn apply_market_quotes(
        &mut self,
        session_id: &str,
        plan: &MarketPlan,
        quotes: &[Quote],
    ) -> Result<MarketRefresh> {
        let session = self.session(session_id)?;
        if session.server != plan.server || session.city != plan.city {
            return Err(invalid(
                "Contexto de mercado da sessão mudou; atualize de novo",
            ));
        }
        let mut rows = self.rows(session_id, &Filter::default())?;
        let pairs = market_pairs(&rows);
        let mut result = MarketRefresh {
            unknown_quality: pairs
                .iter()
                .filter(|((_, quality), manual)| quality.is_none() && !*manual)
                .count(),
            manual_kept: pairs
                .iter()
                .filter(|((_, quality), manual)| quality.is_some() && **manual)
                .count(),
            ..MarketRefresh::default()
        };
        let quotes: HashMap<(&str, u8), &Quote> = quotes
            .iter()
            .map(|quote| ((quote.item_id.as_str(), quote.quality), quote))
            .collect();
        let recorded_at = now();
        let mut prices: HashMap<(String, u8), Price> = HashMap::new();
        for (item_id, quality) in &plan.wanted {
            if pairs.get(&(item_id.clone(), Some(*quality))) != Some(&false) {
                // Now manual, or no longer an active pair of this session.
                continue;
            }
            let Some(quote) = quotes.get(&(item_id.as_str(), *quality)) else {
                result.unavailable += 1;
                continue;
            };
            money(quote.unit_silver)?;
            normalize_timestamp(&quote.observed_at)?;
            prices.insert(
                (item_id.clone(), *quality),
                Price {
                    unit_silver: quote.unit_silver,
                    source: PriceSource::AlbionData,
                    server: session.server.clone(),
                    city: session.city.clone(),
                    recorded_at: recorded_at.clone(),
                    observed_at: Some(quote.observed_at.clone()),
                },
            );
        }
        for row in &mut rows {
            if let Some(quality) = row.event.quality {
                if let Some(price) = prices.get(&(row.event.item.id.clone(), quality)) {
                    row.price = Some(price.clone());
                }
            }
        }
        totals(&rows)?;
        let transaction = self.connection.transaction()?;
        for ((item_id, quality), price) in &prices {
            transaction.execute(
                "INSERT INTO item_prices (session_id, item_id, quality, unit_silver, source,
                 server, city, recorded_at, observed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(session_id, item_id, quality) DO UPDATE SET
                 unit_silver = excluded.unit_silver, source = excluded.source,
                 server = excluded.server, city = excluded.city,
                 recorded_at = excluded.recorded_at, observed_at = excluded.observed_at
                 WHERE item_prices.source <> 'manual'",
                params![
                    session_id,
                    item_id,
                    quality,
                    price.unit_silver,
                    price.source,
                    price.server,
                    price.city,
                    price.recorded_at,
                    price.observed_at
                ],
            )?;
        }
        transaction.commit()?;
        result.updated = prices.len();
        tracing::info!(
            operation = "market_prices_applied",
            %session_id,
            updated = result.updated,
            unavailable = result.unavailable,
            manual_kept = result.manual_kept,
            unknown_quality = result.unknown_quality
        );
        Ok(result)
    }
    pub fn rows(&self, session_id: &str, filter: &Filter) -> Result<Vec<LootRow>> {
        filter.validate()?;
        let mut statement = self.connection.prepare(&format!(
            "{ROW_SELECT} WHERE e.session_id = ?1
             AND (?2 = '' OR e.player LIKE ?2 ESCAPE '\\')
             AND (?3 = '' OR e.item_name LIKE ?3 ESCAPE '\\' OR e.item_id LIKE ?3 ESCAPE '\\')
             AND (?4 IS NULL OR e.tier = ?4)
             AND (?5 IS NULL OR e.enchantment = ?5)
             AND (?6 IS NULL OR (?6 = 0 AND e.quality IS NULL) OR e.quality = ?6)
             ORDER BY e.occurred_at DESC, e.event_id"
        ))?;
        let rows = statement.query_map(
            params![
                session_id,
                like_pattern(&filter.player),
                like_pattern(&filter.item),
                filter.tier,
                filter.enchantment,
                filter.quality
            ],
            read_loot_row,
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn ledger(&self, session_id: &str) -> Result<Vec<LedgerEntry>> {
        load_ledger(&self.connection, session_id)
    }
    pub fn add_ledger(
        &self,
        session_id: &str,
        kind: LedgerKind,
        player: &str,
        description: &str,
        amount: i64,
    ) -> Result<LedgerEntry> {
        self.session(session_id)?;
        text(description, 200)?;
        money(amount)?;
        if amount == 0 {
            return Err(invalid("Lançamento deve ser maior que zero"));
        }
        let entry = LedgerEntry {
            id: new_id(),
            session_id: session_id.into(),
            kind,
            player: normalize_player(player)?,
            description: description.trim().into(),
            amount,
            occurred_at: now(),
            reverses: None,
            reversed_by: None,
        };
        let mut entries = self.ledger(session_id)?;
        entries.push(entry.clone());
        finance(&entries)?;
        insert_ledger(&self.connection, &entry)?;
        tracing::info!(operation = "ledger_added", %session_id, entry_id = %entry.id);
        Ok(entry)
    }
    /// Cancels an entry with an opposite entry; both stay visible in the history.
    pub fn reverse_ledger(&self, session_id: &str, entry_id: &str) -> Result<LedgerEntry> {
        self.session(session_id)?;
        let mut entries = self.ledger(session_id)?;
        let original = entries
            .iter()
            .find(|entry| entry.id == entry_id)
            .ok_or_else(|| invalid("Lançamento não encontrado na sessão"))?;
        if original.reverses.is_some() {
            return Err(invalid("Um estorno não pode ser estornado"));
        }
        if original.reversed_by.is_some() {
            return Err(invalid("Lançamento já estornado"));
        }
        let reversal = LedgerEntry {
            id: new_id(),
            session_id: session_id.into(),
            kind: original.kind,
            player: original.player.clone(),
            description: format!("Estorno: {}", original.description)
                .chars()
                .take(200)
                .collect(),
            amount: original.amount,
            occurred_at: now(),
            reverses: Some(original.id.clone()),
            reversed_by: None,
        };
        entries.push(reversal.clone());
        finance(&entries)?;
        insert_ledger(&self.connection, &reversal)?;
        tracing::info!(operation = "ledger_reversed", %session_id, entry_id = %entry_id);
        Ok(reversal)
    }
    pub fn record_split(
        &mut self,
        session_id: &str,
        amount: i64,
        players: &[String],
    ) -> Result<Vec<Split>> {
        self.session(session_id)?;
        let shares = split(amount, players)?;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let available = finance(&load_ledger(&transaction, session_id)?)?.available;
        if amount <= 0 || amount > available {
            return Err(invalid(
                "Divisão deve ser positiva e não exceder o saldo recebido disponível",
            ));
        }
        for share in shares.iter().filter(|share| share.silver > 0) {
            insert_ledger(
                &transaction,
                &LedgerEntry {
                    id: new_id(),
                    session_id: session_id.into(),
                    kind: LedgerKind::Settlement,
                    player: share.player.clone(),
                    description: "Divisão igualitária confirmada".into(),
                    amount: share.silver,
                    occurred_at: now(),
                    reverses: None,
                    reversed_by: None,
                },
            )?;
        }
        transaction.commit()?;
        tracing::info!(operation = "split_recorded", %session_id, participants = players.len());
        Ok(shares)
    }

    pub fn view(&self, session_id: &str, filter: &Filter) -> Result<View> {
        filter.validate()?;
        let session = self.session(session_id)?;
        let all = self.rows(session_id, &Filter::default())?;
        let full_totals = totals(&all)?;
        let rows = if filter.is_empty() {
            all
        } else {
            self.rows(session_id, filter)?
        };
        let ledger = self.ledger(session_id)?;
        Ok(View {
            session,
            totals: totals(&rows)?,
            full_totals,
            finance: finance(&ledger)?,
            rows,
            ledger,
        })
    }
    /// Exports the whole session regardless of filters. JSON `events` use the import
    /// contract, so an export can be replayed into the same session.
    pub fn export(&self, session_id: &str, format: &str) -> Result<String> {
        if format != "json" && format != "csv" {
            return Err(invalid("Formato deve ser json ou csv"));
        }
        let view = self.view(session_id, &Filter::default())?;
        if format == "json" {
            let events: Vec<import::EventV2> = view
                .rows
                .iter()
                .map(|row| import::EventV2::from(&row.event))
                .collect();
            return Ok(serde_json::to_string_pretty(&serde_json::json!({
                "schema_version": import::CURRENT_VERSION,
                "exported_at": now(),
                "session": view.session,
                "events": events,
                "loot": view.rows,
                "totals": view.full_totals,
                "ledger": view.ledger,
                "finance": view.finance,
            }))?);
        }
        let mut writer = csv::Writer::from_writer(Vec::new());
        writer.write_record(CSV_HEADER)?;
        for row in &view.rows {
            let event = &row.event;
            let price = row.price.as_ref();
            let mut record = vec![String::new(); CSV_HEADER.len()];
            record[0] = "loot".into();
            record[1] = csv_safe(&event.id);
            record[2] = csv_safe(session_id);
            record[3] = csv_safe(&event.source);
            record[4] = event.origin.as_str().into();
            record[5] = row.imported.to_string();
            record[6] = row.voided_at.clone().unwrap_or_default();
            record[7] = event.occurred_at.clone();
            record[8] = csv_safe(&event.player);
            record[9] = event.item.id.clone();
            record[10] = csv_safe(&event.item.name);
            record[11] = event
                .item
                .tier
                .map(|tier| tier.to_string())
                .unwrap_or_default();
            record[12] = event.item.enchantment.to_string();
            record[13] = event
                .quality
                .map(|quality| quality.to_string())
                .unwrap_or_default();
            record[14] = event.quantity.to_string();
            if let Some(price) = price {
                record[15] = price.unit_silver.to_string();
                record[16] = price.source.as_str().into();
                record[17] = price.server.clone();
                record[18] = price.city.clone();
                record[19] = price.recorded_at.clone();
                record[24] = price.observed_at.clone().unwrap_or_default();
            }
            writer.write_record(record)?;
        }
        for entry in &view.ledger {
            let mut record = vec![String::new(); CSV_HEADER.len()];
            record[0] = "ledger".into();
            record[1] = entry.id.clone();
            record[2] = csv_safe(session_id);
            record[4] = "manual".into();
            record[7] = entry.occurred_at.clone();
            record[8] = csv_safe(&entry.player);
            record[20] = entry.kind.as_str().into();
            record[21] = entry.amount.to_string();
            record[22] = entry.reverses.clone().unwrap_or_default();
            record[23] = csv_safe(&entry.description);
            writer.write_record(record)?;
        }
        let bytes = writer
            .into_inner()
            .map_err(|_| invalid("Falha ao concluir CSV"))?;
        String::from_utf8(bytes).map_err(|_| invalid("Falha de codificação CSV"))
    }
}
const CSV_HEADER: [&str; 25] = [
    "record_type",
    "id",
    "session_id",
    "source",
    "origin",
    "imported",
    "voided_at",
    "occurred_at",
    "player",
    "item_id",
    "item_name",
    "tier",
    "enchantment",
    "quality",
    "quantity",
    "unit_silver",
    "price_source",
    "price_server",
    "price_city",
    "price_recorded_at",
    "ledger_kind",
    "amount",
    "reverses",
    "description",
    "price_observed_at",
];
/// Neutralizes spreadsheet formulas in untrusted text.
fn csv_safe(value: &str) -> String {
    if value
        .trim_start()
        .starts_with(['=', '+', '-', '@', '\t', '\r', '\n'])
    {
        format!("'{value}")
    } else {
        value.into()
    }
}

use crate::{adapters, domain::*};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
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
#[derive(Serialize)]
pub struct InsertResult {
    pub inserted: usize,
    pub duplicates: usize,
}
fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )?;
        let version: u32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > 1 {
            return Err(invalid(
                "Banco criado por uma versão mais recente do Kalbion",
            ));
        }
        if version == 0 {
            let transaction = connection.transaction()?;
            transaction.execute_batch(include_str!("../migrations/001_initial.sql"))?;
            transaction.commit()?;
        }
        Ok(Self { connection })
    }
    pub fn settings(&self) -> Result<Settings> {
        let payload: Option<String> = self
            .connection
            .query_row("SELECT payload FROM settings WHERE id=1", [], |row| {
                row.get(0)
            })
            .optional()?;
        payload
            .map(|value| serde_json::from_str(&value).map_err(Error::from))
            .unwrap_or_else(|| Ok(Settings::default()))
    }
    pub fn save_settings(&self, settings: Settings) -> Result<()> {
        settings.validate()?;
        self.connection.execute("INSERT INTO settings VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload", [serde_json::to_string(&settings)?])?;
        tracing::info!(operation = "settings_saved");
        Ok(())
    }
    pub fn sessions(&self) -> Result<Vec<Session>> {
        let mut statement = self.connection.prepare("SELECT id,name,created_at,closed_at,server,city FROM sessions ORDER BY created_at DESC,id")?;
        let rows = statement.query_map([], |row| {
            Ok(Session {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
                closed_at: row.get(3)?,
                server: row.get(4)?,
                city: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
    pub fn session(&self, session_id: &str) -> Result<Session> {
        self.sessions()?
            .into_iter()
            .find(|session| session.id == session_id)
            .ok_or_else(|| invalid("Sessão não encontrada"))
    }
    fn require_open(&self, session_id: &str) -> Result<Session> {
        let session = self.session(session_id)?;
        if session.closed_at.is_some() {
            return Err(invalid("Sessão encerrada; reabra para adicionar loot"));
        }
        Ok(session)
    }
    pub fn create_session(&self, name: &str) -> Result<Session> {
        text(name, 120)?;
        let settings = self.settings()?;
        let session = Session {
            id: id(),
            name: name.trim().into(),
            created_at: now(),
            closed_at: None,
            server: settings.server,
            city: settings.city,
        };
        self.connection.execute(
            "INSERT INTO sessions VALUES(?1,?2,?3,NULL,?4,?5)",
            params![
                session.id,
                session.name,
                session.created_at,
                session.server,
                session.city
            ],
        )?;
        tracing::info!(operation="session_created", session_id=%session.id);
        Ok(session)
    }
    pub fn set_closed(&self, session_id: &str, closed: bool) -> Result<()> {
        self.session(session_id)?;
        self.connection.execute(
            "UPDATE sessions SET closed_at=?1 WHERE id=?2",
            params![if closed { Some(now()) } else { None }, session_id],
        )?;
        tracing::info!(operation="session_status_changed", %session_id, closed);
        Ok(())
    }
    pub fn ingest(&mut self, events: &[LootReceived], imported: bool) -> Result<InsertResult> {
        if events.len() > 10_000 {
            return Err(invalid("Lote grande demais"));
        }
        let mut prospective = std::collections::BTreeMap::new();
        for event in events {
            event.validate()?;
            if !prospective.contains_key(&event.session_id) {
                self.require_open(&event.session_id)?;
                prospective.insert(event.session_id.clone(), self.rows(&event.session_id)?);
            }
            let rows = prospective
                .get_mut(&event.session_id)
                .ok_or_else(|| invalid("Sessão ausente"))?;
            if !rows
                .iter()
                .any(|row| row.event.source == event.source && row.event.id == event.id)
            {
                let price = rows
                    .iter()
                    .find(|row| {
                        row.event.item.id == event.item.id
                            && row.event.item.quality == event.item.quality
                    })
                    .and_then(|row| row.price.clone());
                rows.push(LootRow {
                    event: event.clone(),
                    imported,
                    price,
                });
            }
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
            let existing: Option<String> = transaction
                .query_row(
                    "SELECT payload FROM events WHERE source=?1 AND id=?2",
                    params![event.source, event.id],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(payload) = existing {
                let previous: LootReceived = serde_json::from_str(&payload)?;
                if previous != *event {
                    return Err(invalid(
                        "ID de evento reutilizado com conteúdo diferente; lote rejeitado",
                    ));
                }
                result.duplicates += 1;
            } else {
                transaction.execute(
                    "INSERT INTO events VALUES(?1,?2,?3,?4,?5)",
                    params![
                        event.source,
                        event.id,
                        event.session_id,
                        serde_json::to_string(event)?,
                        imported
                    ],
                )?;
                result.inserted += 1;
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
        item: Item,
        quantity: u32,
    ) -> Result<InsertResult> {
        self.ingest(
            &[LootReceived {
                id: id(),
                origin: Origin::Manual,
                source: "kalbion.manual.v1".into(),
                session_id: session_id.into(),
                occurred_at: now(),
                player: player.trim().into(),
                item,
                quantity,
            }],
            false,
        )
    }
    pub fn import(&mut self, session_id: &str, json: &str) -> Result<InsertResult> {
        let batch = adapters::parse_import(json)?;
        if batch
            .events
            .iter()
            .any(|event| event.session_id != session_id)
        {
            return Err(invalid("O session_id importado deve corresponder à sessão selecionada; IDs não são remapeados"));
        }
        self.ingest(&batch.events, true)
    }
    pub fn price(
        &self,
        session_id: &str,
        item_id: &str,
        quality: u8,
        amount: Option<i64>,
    ) -> Result<()> {
        let session = self.session(session_id)?;
        if !self
            .rows(session_id)?
            .iter()
            .any(|row| row.event.item.id == item_id && row.event.item.quality == quality)
        {
            return Err(invalid("Item/quality não encontrado na sessão"));
        }
        if let Some(unit_silver) = amount {
            money(unit_silver)?;
            let price = Price {
                unit_silver,
                source: "manual".into(),
                server: session.server,
                city: session.city,
                queried_at: now(),
            };
            let mut prospective = self.rows(session_id)?;
            for row in &mut prospective {
                if row.event.item.id == item_id && row.event.item.quality == quality {
                    row.price = Some(price.clone());
                }
            }
            totals(&prospective)?;
            self.connection.execute("INSERT INTO prices VALUES(?1,?2,?3,?4) ON CONFLICT(session_id,item_id,quality) DO UPDATE SET payload=excluded.payload", params![session_id,item_id,quality,serde_json::to_string(&price)?])?;
        } else {
            self.connection.execute(
                "DELETE FROM prices WHERE session_id=?1 AND item_id=?2 AND quality=?3",
                params![session_id, item_id, quality],
            )?;
        }
        tracing::info!(operation="price_updated", %session_id);
        Ok(())
    }
    pub fn rows(&self, session_id: &str) -> Result<Vec<LootRow>> {
        let mut statement = self.connection.prepare(
            "SELECT payload,imported FROM events WHERE session_id=?1 ORDER BY rowid DESC",
        )?;
        let entries = statement.query_map([session_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?))
        })?;
        let mut result = Vec::new();
        for entry in entries {
            let (payload, imported) = entry?;
            let event: LootReceived = serde_json::from_str(&payload)?;
            let payload: Option<String> = self
                .connection
                .query_row(
                    "SELECT payload FROM prices WHERE session_id=?1 AND item_id=?2 AND quality=?3",
                    params![session_id, event.item.id, event.item.quality],
                    |row| row.get(0),
                )
                .optional()?;
            let price = payload
                .map(|value| serde_json::from_str(&value))
                .transpose()?;
            result.push(LootRow {
                event,
                imported,
                price,
            });
        }
        result.sort_by(|first, second| {
            let first_time = chrono::DateTime::parse_from_rfc3339(&first.event.occurred_at).ok();
            let second_time = chrono::DateTime::parse_from_rfc3339(&second.event.occurred_at).ok();
            second_time
                .cmp(&first_time)
                .then_with(|| first.event.id.cmp(&second.event.id))
        });
        Ok(result)
    }
    pub fn ledger(&self, session_id: &str) -> Result<Vec<LedgerEntry>> {
        let mut statement = self
            .connection
            .prepare("SELECT payload FROM ledger WHERE session_id=?1 ORDER BY rowid DESC")?;
        let rows = statement.query_map([session_id], |row| row.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }
    pub fn add_ledger(
        &self,
        session_id: &str,
        kind: LedgerKind,
        player: &str,
        description: &str,
        amount: i64,
    ) -> Result<()> {
        self.session(session_id)?;
        text(player, 64)?;
        text(description, 200)?;
        money(amount)?;
        if amount == 0 {
            return Err(invalid("Lançamento deve ser maior que zero"));
        }
        let entry = LedgerEntry {
            id: id(),
            session_id: session_id.into(),
            kind,
            player: player.trim().into(),
            description: description.trim().into(),
            amount,
            occurred_at: now(),
        };
        let mut entries = self.ledger(session_id)?;
        entries.push(entry.clone());
        finance(&entries)?;
        self.connection.execute(
            "INSERT INTO ledger VALUES(?1,?2,?3)",
            params![entry.id, session_id, serde_json::to_string(&entry)?],
        )?;
        tracing::info!(operation="ledger_added", %session_id, entry_id=%entry.id);
        Ok(())
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
        let entries = {
            let mut statement =
                transaction.prepare("SELECT payload FROM ledger WHERE session_id=?1")?;
            let payloads = statement.query_map([session_id], |row| row.get::<_, String>(0))?;
            payloads
                .map(|payload| Ok(serde_json::from_str::<LedgerEntry>(&payload?)?))
                .collect::<Result<Vec<_>>>()?
        };
        let available = finance(&entries)?.available;
        if amount <= 0 || amount > available {
            return Err(invalid(
                "Divisão deve ser positiva e não exceder o saldo recebido disponível",
            ));
        }
        for share in &shares {
            if share.silver == 0 {
                continue;
            }
            let entry = LedgerEntry {
                id: id(),
                session_id: session_id.into(),
                kind: LedgerKind::Settlement,
                player: share.player.clone(),
                description: "Divisão igualitária confirmada".into(),
                amount: share.silver,
                occurred_at: now(),
            };
            transaction.execute(
                "INSERT INTO ledger VALUES(?1,?2,?3)",
                params![entry.id, session_id, serde_json::to_string(&entry)?],
            )?;
        }
        transaction.commit()?;
        tracing::info!(operation="split_recorded", %session_id, participants=players.len());
        Ok(shares)
    }
    pub fn view(&self, session_id: &str, filter: &Filter) -> Result<View> {
        let session = self.session(session_id)?;
        let all = self.rows(session_id)?;
        let full_totals = totals(&all)?;
        let rows: Vec<_> = all.into_iter().filter(|row| filter.matches(row)).collect();
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
    pub fn export(&self, session_id: &str, format: &str) -> Result<String> {
        let view = self.view(session_id, &Filter::default())?;
        if format == "json" {
            return Ok(serde_json::to_string_pretty(
                &serde_json::json!({"schema_version":1,"exported_at":now(),"session":view.session,"events":view.rows.iter().map(|row| &row.event).collect::<Vec<_>>(),"loot":view.rows,"totals":view.totals,"ledger":view.ledger,"finance":view.finance}),
            )?);
        }
        if format != "csv" {
            return Err(invalid("Formato deve ser json ou csv"));
        }
        let mut writer = csv::Writer::from_writer(Vec::new());
        writer.write_record([
            "record_type",
            "id",
            "session_id",
            "source",
            "origin",
            "imported",
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
            "server",
            "city",
            "price_timestamp",
            "ledger_kind",
            "amount",
            "description",
        ])?;
        for row in view.rows {
            let event = row.event;
            let price = row.price;
            let origin = serde_json::to_value(&event.origin)?
                .as_str()
                .unwrap_or("")
                .to_owned();
            writer.write_record([
                "loot".into(),
                csv_safe(&event.id),
                csv_safe(session_id),
                csv_safe(&event.source),
                origin,
                row.imported.to_string(),
                event.occurred_at,
                csv_safe(&event.player),
                event.item.id,
                csv_safe(&event.item.name),
                event.item.tier.to_string(),
                event.item.enchantment.to_string(),
                event.item.quality.to_string(),
                event.quantity.to_string(),
                price
                    .as_ref()
                    .map(|value| value.unit_silver.to_string())
                    .unwrap_or_default(),
                price
                    .as_ref()
                    .map(|value| value.source.clone())
                    .unwrap_or_default(),
                view.session.server.clone(),
                view.session.city.clone(),
                price.map(|value| value.queried_at).unwrap_or_default(),
                String::new(),
                String::new(),
                String::new(),
            ])?;
        }
        for entry in view.ledger {
            let mut record = vec![String::new(); 22];
            record[0] = "ledger".into();
            record[1] = entry.id;
            record[2] = session_id.into();
            record[4] = "manual".into();
            record[6] = entry.occurred_at;
            record[7] = csv_safe(&entry.player);
            record[19] = serde_json::to_value(entry.kind)?
                .as_str()
                .unwrap_or("")
                .into();
            record[20] = entry.amount.to_string();
            record[21] = csv_safe(&entry.description);
            writer.write_record(record)?;
        }
        let bytes = writer
            .into_inner()
            .map_err(|_| invalid("Falha ao concluir CSV"))?;
        String::from_utf8(bytes).map_err(|_| invalid("Falha de codificação CSV"))
    }
}
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

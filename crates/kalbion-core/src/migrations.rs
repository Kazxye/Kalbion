use crate::domain::{has_quality, normalize_timestamp, LedgerKind, Origin};
use crate::error::{invalid, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::Deserialize;
use std::path::Path;

pub const LATEST: u32 = 5;

/// Applies pending migrations, one transaction per version. Before upgrading an existing
/// database file, a consistent copy is written next to it so a failed or unwanted upgrade
/// never costs the user their history.
pub fn run(connection: &mut Connection, path: &Path) -> Result<()> {
    let version: u32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > LATEST {
        return Err(invalid(
            "Banco criado por uma versão mais recente do Kalbion",
        ));
    }
    if version > 0 && version < LATEST {
        backup(connection, path, version)?;
    }
    for target in version + 1..=LATEST {
        let transaction = connection.transaction()?;
        match target {
            1 => transaction.execute_batch(include_str!("../migrations/001_initial.sql"))?,
            2 => to_v2(&transaction)?,
            3 => transaction.execute_batch(include_str!("../migrations/003_market_prices.sql"))?,
            4 => to_v4(&transaction)?,
            5 => {
                transaction.execute_batch(include_str!("../migrations/005_capture_imports.sql"))?
            }
            _ => unreachable!("every version up to LATEST has a migration"),
        }
        transaction.pragma_update(None, "user_version", target)?;
        transaction.commit()?;
        tracing::info!(operation = "migration_applied", version = target);
    }
    Ok(())
}

fn backup(connection: &Connection, path: &Path, version: u32) -> Result<()> {
    if path.as_os_str().is_empty() || path == Path::new(":memory:") {
        return Ok(());
    }
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("kalbion");
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let target = path.with_file_name(format!("{stem}-v{version}-backup-{stamp}.db"));
    let target = target
        .to_str()
        .ok_or_else(|| invalid("Caminho do banco não é UTF-8; backup impossível"))?;
    connection.execute("VACUUM INTO ?1", [target])?;
    tracing::info!(
        operation = "database_backup_created",
        from_version = version
    );
    Ok(())
}

#[derive(Deserialize)]
struct V1Settings {
    server: String,
    city: String,
}
#[derive(Deserialize)]
struct V1Event {
    id: String,
    origin: String,
    source: String,
    session_id: String,
    occurred_at: String,
    player: String,
    item: V1Item,
    quantity: u32,
}
#[derive(Deserialize)]
struct V1Item {
    id: String,
    name: String,
    tier: u8,
    enchantment: u8,
    quality: u8,
}
#[derive(Deserialize)]
struct V1Price {
    unit_silver: i64,
    server: String,
    city: String,
    queried_at: String,
}
#[derive(Deserialize)]
struct V1Ledger {
    id: String,
    session_id: String,
    kind: String,
    player: String,
    description: String,
    amount: i64,
    occurred_at: String,
}

/// Resources have no quality. Older versions stored whatever was entered (the manual form
/// defaulted to Normal), so their loot moves to "no quality" and so do their prices; when a
/// session priced the same resource under several qualities, the most recent price wins.
fn to_v4(transaction: &Transaction) -> Result<()> {
    transaction.execute_batch(include_str!(
        "../migrations/004_resources_without_quality.sql"
    ))?;
    let resources: Vec<String> = {
        let mut select = transaction.prepare(
            "SELECT DISTINCT item_id FROM loot_events WHERE quality IS NOT NULL
             UNION SELECT DISTINCT item_id FROM item_prices WHERE quality > 0",
        )?;
        let rows = select.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .filter(|item_id| !has_quality(item_id))
            .collect()
    };
    for item_id in &resources {
        transaction.execute(
            "UPDATE loot_events SET quality = NULL WHERE item_id = ?1",
            [item_id],
        )?;
        transaction.execute(
            "DELETE FROM item_prices WHERE item_id = ?1 AND rowid NOT IN (
               SELECT rowid FROM item_prices p WHERE p.item_id = ?1
               AND p.recorded_at = (SELECT MAX(recorded_at) FROM item_prices q
                 WHERE q.item_id = ?1 AND q.session_id = p.session_id)
               GROUP BY p.session_id)",
            [item_id],
        )?;
        transaction.execute(
            "UPDATE item_prices SET quality = 0 WHERE item_id = ?1",
            [item_id],
        )?;
    }
    Ok(())
}

fn to_v2(transaction: &Transaction) -> Result<()> {
    transaction.execute_batch(include_str!("../migrations/002_structured.sql"))?;

    let sessions = {
        let mut select = transaction.prepare("SELECT id, created_at, closed_at FROM sessions")?;
        let rows = select.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (id, created_at, closed_at) in sessions {
        transaction.execute(
            "UPDATE sessions SET created_at = ?1, closed_at = ?2 WHERE id = ?3",
            params![
                normalize_timestamp(&created_at)?,
                closed_at.as_deref().map(normalize_timestamp).transpose()?,
                id
            ],
        )?;
    }

    if let Some(payload) = transaction
        .query_row("SELECT payload FROM settings WHERE id = 1", [], |row| {
            row.get::<_, String>(0)
        })
        .optional()?
    {
        let settings: V1Settings = serde_json::from_str(&payload)?;
        transaction.execute(
            "INSERT INTO app_settings (id, server, city) VALUES (1, ?1, ?2)",
            params![settings.server, settings.city],
        )?;
    }

    {
        let mut select =
            transaction.prepare("SELECT payload, imported FROM events ORDER BY rowid")?;
        let mut insert = transaction.prepare(
            "INSERT INTO loot_events (source, event_id, session_id, origin, imported, occurred_at,
             player, item_id, item_name, tier, enchantment, quality, quantity)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        )?;
        let rows = select.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?))
        })?;
        for row in rows {
            let (payload, imported) = row?;
            let event: V1Event = serde_json::from_str(&payload)?;
            insert.execute(params![
                event.source,
                event.id,
                event.session_id,
                Origin::parse(&event.origin)?.as_str(),
                imported,
                normalize_timestamp(&event.occurred_at)?,
                event.player,
                event.item.id,
                event.item.name,
                event.item.tier,
                event.item.enchantment,
                event.item.quality,
                event.quantity
            ])?;
        }
    }

    {
        let mut select =
            transaction.prepare("SELECT session_id, item_id, quality, payload FROM prices")?;
        let mut insert = transaction.prepare(
            "INSERT INTO item_prices (session_id, item_id, quality, unit_silver, source, server,
             city, recorded_at, observed_at)
             VALUES (?1, ?2, ?3, ?4, 'manual', ?5, ?6, ?7, NULL)",
        )?;
        let rows = select.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u8>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        for row in rows {
            let (session_id, item_id, quality, payload) = row?;
            let price: V1Price = serde_json::from_str(&payload)?;
            insert.execute(params![
                session_id,
                item_id,
                quality,
                price.unit_silver,
                price.server,
                price.city,
                normalize_timestamp(&price.queried_at)?
            ])?;
        }
    }

    {
        let mut select = transaction.prepare("SELECT payload FROM ledger ORDER BY rowid")?;
        let mut insert = transaction.prepare(
            "INSERT INTO ledger_entries (id, session_id, kind, player, description, amount, occurred_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        let rows = select.query_map([], |row| row.get::<_, String>(0))?;
        for row in rows {
            let entry: V1Ledger = serde_json::from_str(&row?)?;
            insert.execute(params![
                entry.id,
                entry.session_id,
                LedgerKind::parse(&entry.kind)?.as_str(),
                entry.player,
                entry.description,
                entry.amount,
                normalize_timestamp(&entry.occurred_at)?
            ])?;
        }
    }

    transaction.execute_batch(
        "DROP TABLE events; DROP TABLE prices; DROP TABLE ledger; DROP TABLE settings;",
    )?;
    Ok(())
}

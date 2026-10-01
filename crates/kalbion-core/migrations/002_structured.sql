-- Version 2: real columns instead of JSON payloads, catalog tables, voidable loot and
-- reversible ledger entries. Data from version 1 is copied by migrations.rs before the
-- old tables are dropped.
CREATE TABLE app_settings (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  server TEXT NOT NULL,
  city TEXT NOT NULL
);

-- At most one imported catalog; without a row the built-in demo catalog is used.
CREATE TABLE catalog_source (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  kind TEXT NOT NULL CHECK (kind IN ('ao_bin_dumps')),
  label TEXT NOT NULL,
  imported_at TEXT NOT NULL,
  item_count INTEGER NOT NULL,
  skipped_count INTEGER NOT NULL
);
CREATE TABLE catalog_items (
  unique_name TEXT PRIMARY KEY,
  game_index INTEGER UNIQUE,
  name TEXT NOT NULL,
  tier INTEGER CHECK (tier BETWEEN 1 AND 8),
  enchantment INTEGER NOT NULL CHECK (enchantment BETWEEN 0 AND 4)
);
CREATE INDEX catalog_items_name ON catalog_items(name);

CREATE TABLE loot_events (
  source TEXT NOT NULL,
  event_id TEXT NOT NULL,
  session_id TEXT NOT NULL REFERENCES sessions(id),
  origin TEXT NOT NULL CHECK (origin IN ('simulated', 'manual', 'observed')),
  imported INTEGER NOT NULL CHECK (imported IN (0, 1)),
  occurred_at TEXT NOT NULL,
  player TEXT NOT NULL,
  item_id TEXT NOT NULL,
  item_name TEXT NOT NULL,
  tier INTEGER CHECK (tier BETWEEN 1 AND 8),
  enchantment INTEGER NOT NULL CHECK (enchantment BETWEEN 0 AND 4),
  quality INTEGER CHECK (quality BETWEEN 1 AND 5),
  quantity INTEGER NOT NULL CHECK (quantity BETWEEN 1 AND 1000000),
  voided_at TEXT,
  PRIMARY KEY (source, event_id)
);
CREATE INDEX loot_events_session_time ON loot_events(session_id, occurred_at);

-- quality 0 means "unknown" here, because NULL cannot take part in the primary key.
CREATE TABLE item_prices (
  session_id TEXT NOT NULL REFERENCES sessions(id),
  item_id TEXT NOT NULL,
  quality INTEGER NOT NULL CHECK (quality BETWEEN 0 AND 5),
  unit_silver INTEGER NOT NULL CHECK (unit_silver BETWEEN 0 AND 1000000000000),
  source TEXT NOT NULL CHECK (source IN ('manual')),
  server TEXT NOT NULL,
  city TEXT NOT NULL,
  recorded_at TEXT NOT NULL,
  observed_at TEXT,
  PRIMARY KEY (session_id, item_id, quality)
);

CREATE TABLE ledger_entries (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions(id),
  kind TEXT NOT NULL CHECK (kind IN ('income', 'expense', 'regear', 'settlement')),
  player TEXT NOT NULL,
  description TEXT NOT NULL,
  amount INTEGER NOT NULL CHECK (amount BETWEEN 1 AND 1000000000000),
  occurred_at TEXT NOT NULL,
  reverses TEXT UNIQUE REFERENCES ledger_entries(id)
);
CREATE INDEX ledger_entries_session ON ledger_entries(session_id);

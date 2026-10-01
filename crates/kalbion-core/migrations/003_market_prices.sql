-- Version 3: prices may also come from the Albion Data Project. SQLite cannot change a CHECK
-- constraint in place, so the table is rebuilt. Market prices always carry the time the
-- market observed them.
CREATE TABLE item_prices_v3 (
  session_id TEXT NOT NULL REFERENCES sessions(id),
  item_id TEXT NOT NULL,
  quality INTEGER NOT NULL CHECK (quality BETWEEN 0 AND 5),
  unit_silver INTEGER NOT NULL CHECK (unit_silver BETWEEN 0 AND 1000000000000),
  source TEXT NOT NULL CHECK (source IN ('manual', 'albion_data')),
  server TEXT NOT NULL,
  city TEXT NOT NULL,
  recorded_at TEXT NOT NULL,
  observed_at TEXT,
  PRIMARY KEY (session_id, item_id, quality),
  CHECK (source = 'manual' OR (observed_at IS NOT NULL AND quality > 0))
);
INSERT INTO item_prices_v3 (session_id, item_id, quality, unit_silver, source, server, city,
  recorded_at, observed_at)
  SELECT session_id, item_id, quality, unit_silver, source, server, city, recorded_at, observed_at
  FROM item_prices;
DROP TABLE item_prices;
ALTER TABLE item_prices_v3 RENAME TO item_prices;

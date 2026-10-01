-- Version 4: resources have no quality, and their market price is stored like their loot,
-- under quality 0. Version 3 required quality > 0 for market prices, so the table is rebuilt
-- once more. migrations.rs then moves existing resource loot and prices to "no quality".
CREATE TABLE item_prices_v4 (
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
  CHECK (source = 'manual' OR observed_at IS NOT NULL)
);
INSERT INTO item_prices_v4 (session_id, item_id, quality, unit_silver, source, server, city,
  recorded_at, observed_at)
  SELECT session_id, item_id, quality, unit_silver, source, server, city, recorded_at, observed_at
  FROM item_prices;
DROP TABLE item_prices;
ALTER TABLE item_prices_v4 RENAME TO item_prices;

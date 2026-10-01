CREATE TABLE sessions (id TEXT PRIMARY KEY, name TEXT NOT NULL, created_at TEXT NOT NULL, closed_at TEXT, server TEXT NOT NULL, city TEXT NOT NULL);
CREATE TABLE events (source TEXT NOT NULL, id TEXT NOT NULL, session_id TEXT NOT NULL REFERENCES sessions(id), payload TEXT NOT NULL, imported INTEGER NOT NULL, PRIMARY KEY(source,id));
CREATE INDEX events_session ON events(session_id);
CREATE TABLE prices (session_id TEXT NOT NULL REFERENCES sessions(id), item_id TEXT NOT NULL, quality INTEGER NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(session_id,item_id,quality));
CREATE TABLE ledger (id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(id), payload TEXT NOT NULL);
CREATE INDEX ledger_session ON ledger(session_id);
CREATE TABLE settings (id INTEGER PRIMARY KEY CHECK(id=1), payload TEXT NOT NULL);
PRAGMA user_version = 1;

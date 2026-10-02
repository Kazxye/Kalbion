-- Version 5: an audit record of every capture file imported: which file, which decoder
-- version, and what it saw. Loot rows themselves live in loot_events like any other source.
CREATE TABLE capture_imports (
  id INTEGER PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions(id),
  imported_at TEXT NOT NULL,
  file_label TEXT NOT NULL,
  file_sha256 TEXT NOT NULL,
  fingerprint TEXT NOT NULL,
  decoder_version TEXT NOT NULL,
  inserted INTEGER NOT NULL CHECK (inserted >= 0),
  duplicates INTEGER NOT NULL CHECK (duplicates >= 0),
  diagnostics TEXT NOT NULL
);
CREATE INDEX capture_imports_session ON capture_imports(session_id);

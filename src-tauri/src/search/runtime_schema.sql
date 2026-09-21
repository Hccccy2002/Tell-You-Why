CREATE TABLE search_options (id INTEGER PRIMARY KEY CHECK(id=1), record TEXT NOT NULL);
CREATE TABLE search_runs (
    id TEXT PRIMARY KEY,
    card_id TEXT REFERENCES cards(id) ON DELETE CASCADE,
    state TEXT NOT NULL,
    created_at TEXT NOT NULL,
    options_json TEXT NOT NULL,
    error_code TEXT,
    evidence_json TEXT
);
CREATE TABLE search_attempts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,
    local_day TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX search_attempts_day ON search_attempts(local_day);
CREATE INDEX search_attempts_run ON search_attempts(run_id);
CREATE TABLE search_cache (id TEXT PRIMARY KEY, expires_at INTEGER NOT NULL, evidence_json TEXT NOT NULL);
INSERT INTO schema_migrations VALUES (15, strftime('%Y-%m-%dT%H:%M:%fZ','now'));

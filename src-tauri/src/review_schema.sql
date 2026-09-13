CREATE TABLE review_runs (
    id TEXT PRIMARY KEY,
    kb TEXT NOT NULL,
    state TEXT NOT NULL,
    record TEXT NOT NULL,
    cancel_requested INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX review_runs_book ON review_runs(kb, created_at DESC);
INSERT INTO schema_migrations VALUES (9, strftime('%Y-%m-%dT%H:%M:%fZ','now'));

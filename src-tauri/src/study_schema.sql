CREATE TABLE study_sessions (
    id TEXT PRIMARY KEY,
    state TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 0,
    record TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX study_sessions_recent ON study_sessions(updated_at DESC);
CREATE TABLE study_memory (
    concept_key TEXT PRIMARY KEY,
    topic TEXT NOT NULL,
    title TEXT NOT NULL,
    attempts INTEGER NOT NULL,
    correct_count INTEGER NOT NULL,
    streak INTEGER NOT NULL,
    due_at INTEGER NOT NULL,
    last_correct INTEGER NOT NULL,
    updated_at TEXT NOT NULL
);
INSERT INTO schema_migrations VALUES (11, strftime('%Y-%m-%dT%H:%M:%fZ','now'));

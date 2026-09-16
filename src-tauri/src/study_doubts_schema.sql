CREATE TABLE study_doubts (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES study_sessions(id) ON DELETE CASCADE,
    question_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('unresolved','understood')),
    updated_at TEXT NOT NULL
);
CREATE INDEX study_doubts_status ON study_doubts(status, updated_at DESC);
INSERT INTO schema_migrations VALUES (13, strftime('%Y-%m-%dT%H:%M:%fZ','now'));

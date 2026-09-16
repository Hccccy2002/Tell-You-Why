CREATE TABLE study_highlights (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES study_sessions(id) ON DELETE CASCADE,
    source_kind TEXT NOT NULL CHECK(source_kind IN ('step','question')),
    source_id TEXT NOT NULL,
    card_id TEXT,
    title TEXT NOT NULL,
    text TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(session_id, source_kind, source_id)
);
CREATE INDEX study_highlights_card ON study_highlights(card_id, created_at DESC);
INSERT INTO schema_migrations VALUES (12, strftime('%Y-%m-%dT%H:%M:%fZ','now'));

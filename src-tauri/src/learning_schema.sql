CREATE TABLE rag_learning_units (
    id TEXT PRIMARY KEY, kb TEXT NOT NULL, version TEXT NOT NULL,
    source_key TEXT NOT NULL, chapter_key TEXT NOT NULL, record TEXT NOT NULL,
    UNIQUE(kb, version, source_key)
);
CREATE TABLE rag_card_units (
    card_id TEXT NOT NULL REFERENCES rag_cards(id) ON DELETE CASCADE,
    unit_id TEXT NOT NULL REFERENCES rag_learning_units(id) ON DELETE CASCADE,
    PRIMARY KEY(card_id, unit_id)
);
CREATE TABLE rag_learning_state (
    card_id TEXT PRIMARY KEY REFERENCES rag_cards(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'new' CHECK(status IN ('new','learning','review','mastered')),
    shown_count INTEGER NOT NULL DEFAULT 0,
    revealed_count INTEGER NOT NULL DEFAULT 0,
    first_shown_at TEXT, last_shown_at TEXT, last_revealed_at TEXT,
    updated_at TEXT, revision INTEGER NOT NULL DEFAULT 0
);
INSERT INTO rag_learning_state(card_id) SELECT id FROM rag_cards;
CREATE TRIGGER rag_card_learning_state AFTER INSERT ON rag_cards
BEGIN INSERT INTO rag_learning_state(card_id) VALUES (NEW.id); END;
CREATE TABLE rag_learning_sessions (
    id TEXT PRIMARY KEY, kb TEXT NOT NULL, active INTEGER NOT NULL DEFAULT 1,
    record TEXT NOT NULL, updated_at TEXT NOT NULL
);
CREATE UNIQUE INDEX rag_learning_active_session ON rag_learning_sessions(kb) WHERE active=1;
CREATE TABLE rag_learning_events (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES rag_learning_sessions(id) ON DELETE CASCADE,
    card_id TEXT NOT NULL REFERENCES rag_cards(id) ON DELETE CASCADE,
    presentation_id TEXT NOT NULL, kind TEXT NOT NULL,
    occurred_at TEXT NOT NULL, before_status TEXT NOT NULL, after_status TEXT NOT NULL,
    state_revision INTEGER NOT NULL, request TEXT NOT NULL
);
CREATE UNIQUE INDEX rag_learning_once ON rag_learning_events(presentation_id,kind)
    WHERE kind IN ('shown','revealed','skipped');
CREATE INDEX rag_learning_recent ON rag_learning_events(kind, occurred_at DESC);
CREATE INDEX rag_learning_card_events ON rag_learning_events(card_id, occurred_at DESC);
INSERT INTO schema_migrations VALUES (8, strftime('%Y-%m-%dT%H:%M:%fZ','now'));

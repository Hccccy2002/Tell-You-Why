CREATE TABLE review_memory (
    id TEXT PRIMARY KEY,
    kb TEXT NOT NULL,
    version TEXT NOT NULL,
    chapter_path TEXT NOT NULL,
    topic TEXT NOT NULL,
    question TEXT NOT NULL,
    attempts INTEGER NOT NULL,
    correct_count INTEGER NOT NULL,
    lapses INTEGER NOT NULL,
    streak INTEGER NOT NULL,
    last_correct INTEGER NOT NULL,
    last_reviewed_at INTEGER NOT NULL,
    due_at INTEGER NOT NULL
);
CREATE INDEX idx_review_memory_due ON review_memory(kb,version,due_at);
CREATE TABLE review_memory_events (
    run_id TEXT NOT NULL,
    question_id TEXT NOT NULL,
    memory_id TEXT NOT NULL REFERENCES review_memory(id),
    answered_at INTEGER NOT NULL,
    correct INTEGER NOT NULL,
    PRIMARY KEY(run_id,question_id)
);
INSERT INTO schema_migrations VALUES (10,strftime('%Y-%m-%dT%H:%M:%fZ','now'));

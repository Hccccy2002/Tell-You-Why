CREATE TABLE follow_up_search (
    exchange_id INTEGER PRIMARY KEY REFERENCES follow_up_exchanges(id) ON DELETE CASCADE,
    record TEXT NOT NULL
);
INSERT INTO schema_migrations VALUES(16, strftime('%Y-%m-%dT%H:%M:%fZ','now'));

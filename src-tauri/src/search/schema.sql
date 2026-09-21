CREATE TABLE search_profiles (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    credential_ref TEXT NOT NULL,
    key_last4 TEXT NOT NULL,
    connection_verified INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL
);
INSERT INTO schema_migrations VALUES (14, strftime('%Y-%m-%dT%H:%M:%fZ','now'));

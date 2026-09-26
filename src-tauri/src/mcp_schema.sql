CREATE TABLE IF NOT EXISTS mcp_servers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    transport TEXT NOT NULL,
    config_json TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    credential_ref TEXT,
    key_last4 TEXT,
    status TEXT NOT NULL DEFAULT 'disconnected',
    protocol_version TEXT,
    capabilities_json TEXT NOT NULL DEFAULT '{}',
    instructions TEXT,
    last_error TEXT,
    last_connected_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS mcp_items (
    server_id TEXT NOT NULL REFERENCES mcp_servers(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    schema_hash TEXT NOT NULL,
    record_json TEXT NOT NULL,
    discovered_at TEXT NOT NULL,
    PRIMARY KEY(server_id, kind, name)
);
CREATE INDEX IF NOT EXISTS mcp_items_server_kind ON mcp_items(server_id, kind, name);
INSERT OR IGNORE INTO schema_migrations VALUES (17, strftime('%Y-%m-%dT%H:%M:%fZ','now'));

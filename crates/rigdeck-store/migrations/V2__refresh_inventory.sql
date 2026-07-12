CREATE TABLE inventory_entries (
    id TEXT PRIMARY KEY NOT NULL,
    agent_instance_id TEXT NOT NULL,
    path TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN (
        'managed_clean', 'managed_modified', 'external_new', 'external_removed',
        'source_update_available', 'conflict', 'unsupported', 'manual_required'
    )),
    json TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    UNIQUE(agent_instance_id, path),
    FOREIGN KEY(agent_instance_id) REFERENCES agent_instances(id) ON DELETE CASCADE
);

CREATE TABLE refresh_runs (
    id TEXT PRIMARY KEY NOT NULL,
    agent_instance_id TEXT NOT NULL,
    json TEXT NOT NULL,
    started_at_ms INTEGER NOT NULL,
    finished_at_ms INTEGER NOT NULL,
    FOREIGN KEY(agent_instance_id) REFERENCES agent_instances(id) ON DELETE CASCADE
);

CREATE INDEX idx_inventory_agent_state ON inventory_entries(agent_instance_id, state);
CREATE INDEX idx_refresh_agent_finished ON refresh_runs(agent_instance_id, finished_at_ms DESC);

UPDATE rigdeck_meta SET value='2' WHERE key='schema_version';

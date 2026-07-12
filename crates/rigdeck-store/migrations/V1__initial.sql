CREATE TABLE rigdeck_meta (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);

INSERT INTO rigdeck_meta(key, value) VALUES ('schema_version', '1');

CREATE TABLE assets (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('skill', 'prompt', 'mcp_server')),
    json TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE TABLE revisions (
    id TEXT PRIMARY KEY NOT NULL,
    raw_hash TEXT NOT NULL,
    normalized_hash TEXT NOT NULL,
    content_object TEXT NOT NULL,
    json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
);

CREATE TABLE agent_instances (
    id TEXT PRIMARY KEY NOT NULL,
    adapter_id TEXT NOT NULL,
    json TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE TABLE assignments (
    id TEXT PRIMARY KEY NOT NULL,
    asset_id TEXT NOT NULL,
    revision_id TEXT NOT NULL,
    agent_instance_id TEXT NOT NULL,
    scope TEXT NOT NULL,
    json TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    FOREIGN KEY(asset_id) REFERENCES assets(id),
    FOREIGN KEY(revision_id) REFERENCES revisions(id),
    FOREIGN KEY(agent_instance_id) REFERENCES agent_instances(id)
);

CREATE TABLE plans (
    id TEXT PRIMARY KEY NOT NULL,
    schema_version INTEGER NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('pending', 'applied', 'invalid', 'abandoned')),
    json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
);

CREATE TABLE deployment_snapshots (
    id TEXT PRIMARY KEY NOT NULL,
    plan_id TEXT NOT NULL,
    json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY(plan_id) REFERENCES plans(id)
);

CREATE TABLE conflicts (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL,
    resolved INTEGER NOT NULL CHECK(resolved IN (0, 1)),
    json TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE TABLE audit_events (
    id TEXT PRIMARY KEY NOT NULL,
    event_type TEXT NOT NULL,
    plan_id TEXT,
    json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    FOREIGN KEY(plan_id) REFERENCES plans(id)
);

CREATE INDEX idx_assets_kind ON assets(kind);
CREATE INDEX idx_agents_adapter ON agent_instances(adapter_id);
CREATE INDEX idx_assignments_asset ON assignments(asset_id);
CREATE INDEX idx_conflicts_resolved ON conflicts(resolved);
CREATE INDEX idx_audit_created ON audit_events(created_at_ms);


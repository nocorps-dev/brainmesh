-- BrainMesh SQLite Local-First Schema (v1)

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS agents (
    id TEXT PRIMARY KEY,
    organization_id TEXT,
    workspace_id TEXT,
    name TEXT NOT NULL,
    framework TEXT NOT NULL,
    model TEXT,
    capabilities_json TEXT NOT NULL DEFAULT '[]',
    api_key_hash TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    revoked_at TEXT
);

CREATE TABLE IF NOT EXISTS knowledge (
    id TEXT PRIMARY KEY,
    organization_id TEXT,
    workspace_id TEXT,
    category TEXT NOT NULL,
    record_type TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    domain TEXT NOT NULL,
    tags_json TEXT NOT NULL DEFAULT '[]',
    content_json TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    verification_status TEXT NOT NULL DEFAULT 'unverified',
    verification_method TEXT,
    verification_evidence_json TEXT NOT NULL DEFAULT '[]',
    verified_at TEXT,
    agent_id TEXT NOT NULL,
    framework TEXT NOT NULL,
    contributor_id TEXT NOT NULL,
    visibility TEXT NOT NULL DEFAULT 'private',
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    expires_at TEXT,
    deleted_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_sqlite_knowledge_domain_vis ON knowledge(domain, visibility);
CREATE INDEX IF NOT EXISTS idx_sqlite_knowledge_hash ON knowledge(content_hash);

CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_fts USING fts5(
    knowledge_id UNINDEXED,
    title,
    description,
    domain,
    tags,
    content_text
);

CREATE TABLE IF NOT EXISTS knowledge_versions (
    id TEXT PRIMARY KEY,
    knowledge_id TEXT NOT NULL REFERENCES knowledge(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    content_json TEXT NOT NULL,
    changed_by TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS knowledge_relationships (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL REFERENCES knowledge(id) ON DELETE CASCADE,
    target_id TEXT NOT NULL REFERENCES knowledge(id) ON DELETE CASCADE,
    relationship_type TEXT NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_by TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS experiences (
    id TEXT PRIMARY KEY,
    organization_id TEXT,
    workspace_id TEXT,
    agent_id TEXT NOT NULL,
    framework TEXT NOT NULL,
    title TEXT NOT NULL,
    outcome TEXT NOT NULL,
    summary TEXT NOT NULL,
    raw_events_json TEXT NOT NULL DEFAULT '[]',
    redacted INTEGER NOT NULL DEFAULT 1,
    approved_for_publication INTEGER NOT NULL DEFAULT 0,
    extracted_knowledge_id TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS verification_records (
    id TEXT PRIMARY KEY,
    knowledge_id TEXT NOT NULL REFERENCES knowledge(id) ON DELETE CASCADE,
    verifier_agent_id TEXT NOT NULL,
    status TEXT NOT NULL,
    method TEXT NOT NULL,
    evidence_json TEXT NOT NULL DEFAULT '[]',
    conditions TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS challenges (
    id TEXT PRIMARY KEY,
    knowledge_id TEXT NOT NULL REFERENCES knowledge(id) ON DELETE CASCADE,
    challenger_agent_id TEXT NOT NULL,
    reason TEXT NOT NULL,
    evidence_json TEXT NOT NULL DEFAULT '[]',
    status TEXT NOT NULL DEFAULT 'open',
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS federation_nodes (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    url TEXT NOT NULL,
    trust_tier TEXT NOT NULL DEFAULT 'trusted_network',
    enabled INTEGER NOT NULL DEFAULT 1,
    last_sync_at TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS audit_logs (
    id TEXT PRIMARY KEY,
    principal TEXT NOT NULL,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    allowed INTEGER NOT NULL,
    reason TEXT,
    created_at TEXT NOT NULL
);

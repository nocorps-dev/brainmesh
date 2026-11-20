-- BrainMesh PostgreSQL + pgvector Initial Schema (v1)

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "vector";

CREATE TABLE IF NOT EXISTS organizations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS workspaces (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (organization_id, name)
);

CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    email TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS contributors (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    external_id TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    reputation_score DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS agents (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID REFERENCES organizations(id) ON DELETE SET NULL,
    workspace_id UUID REFERENCES workspaces(id) ON DELETE SET NULL,
    name TEXT NOT NULL,
    framework TEXT NOT NULL,
    model TEXT,
    capabilities JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS agent_credentials (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    key_hash TEXT NOT NULL UNIQUE,
    scopes JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS api_keys (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID REFERENCES organizations(id) ON DELETE CASCADE,
    workspace_id UUID REFERENCES workspaces(id) ON DELETE CASCADE,
    agent_id UUID REFERENCES agents(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL UNIQUE,
    scopes JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS permissions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    principal_id UUID NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id UUID,
    action TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS knowledge (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID REFERENCES organizations(id) ON DELETE SET NULL,
    workspace_id UUID REFERENCES workspaces(id) ON DELETE SET NULL,
    category TEXT NOT NULL,
    record_type TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    domain TEXT NOT NULL,
    content JSONB NOT NULL,
    content_hash TEXT NOT NULL,
    verification_status TEXT NOT NULL DEFAULT 'unverified',
    verification_method TEXT,
    verification_evidence JSONB NOT NULL DEFAULT '[]'::jsonb,
    verified_at TIMESTAMPTZ,
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    framework TEXT NOT NULL,
    contributor_id TEXT NOT NULL,
    visibility TEXT NOT NULL DEFAULT 'private',
    version INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_knowledge_domain_vis ON knowledge(domain, visibility) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_knowledge_fts ON knowledge USING GIN (to_tsvector('english', title || ' ' || description));
CREATE INDEX IF NOT EXISTS idx_knowledge_hash ON knowledge(content_hash);

CREATE TABLE IF NOT EXISTS knowledge_versions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    knowledge_id UUID NOT NULL REFERENCES knowledge(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    content JSONB NOT NULL,
    changed_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (knowledge_id, version)
);

CREATE TABLE IF NOT EXISTS knowledge_tags (
    knowledge_id UUID NOT NULL REFERENCES knowledge(id) ON DELETE CASCADE,
    tag TEXT NOT NULL,
    PRIMARY KEY (knowledge_id, tag)
);
CREATE INDEX IF NOT EXISTS idx_knowledge_tags_tag ON knowledge_tags(tag);

CREATE TABLE IF NOT EXISTS knowledge_relationships (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    source_id UUID NOT NULL REFERENCES knowledge(id) ON DELETE CASCADE,
    target_id UUID NOT NULL REFERENCES knowledge(id) ON DELETE CASCADE,
    relationship_type TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_rel_source ON knowledge_relationships(source_id);
CREATE INDEX IF NOT EXISTS idx_rel_target ON knowledge_relationships(target_id);

CREATE TABLE IF NOT EXISTS knowledge_embeddings (
    knowledge_id UUID PRIMARY KEY REFERENCES knowledge(id) ON DELETE CASCADE,
    model_name TEXT NOT NULL,
    embedding vector(1536),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS experiences (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID REFERENCES organizations(id) ON DELETE SET NULL,
    workspace_id UUID REFERENCES workspaces(id) ON DELETE SET NULL,
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    framework TEXT NOT NULL,
    title TEXT NOT NULL,
    outcome TEXT NOT NULL,
    summary TEXT NOT NULL,
    raw_events JSONB NOT NULL DEFAULT '[]'::jsonb,
    redacted BOOLEAN NOT NULL DEFAULT TRUE,
    approved_for_publication BOOLEAN NOT NULL DEFAULT FALSE,
    extracted_knowledge_id UUID REFERENCES knowledge(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS contributions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    knowledge_id UUID NOT NULL REFERENCES knowledge(id) ON DELETE CASCADE,
    contributor_id TEXT NOT NULL,
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    source_experience_id UUID REFERENCES experiences(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS verification_records (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    knowledge_id UUID NOT NULL REFERENCES knowledge(id) ON DELETE CASCADE,
    verifier_agent_id TEXT NOT NULL,
    status TEXT NOT NULL,
    method TEXT NOT NULL,
    evidence JSONB NOT NULL DEFAULT '[]'::jsonb,
    conditions TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_verif_knowledge ON verification_records(knowledge_id);

CREATE TABLE IF NOT EXISTS challenges (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    knowledge_id UUID NOT NULL REFERENCES knowledge(id) ON DELETE CASCADE,
    challenger_agent_id TEXT NOT NULL,
    reason TEXT NOT NULL,
    evidence JSONB NOT NULL DEFAULT '[]'::jsonb,
    status TEXT NOT NULL DEFAULT 'open',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_challenges_knowledge ON challenges(knowledge_id);

CREATE TABLE IF NOT EXISTS reputation_events (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    contributor_id UUID NOT NULL REFERENCES contributors(id) ON DELETE CASCADE,
    delta DOUBLE PRECISION NOT NULL,
    reason TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS audit_logs (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    principal TEXT NOT NULL,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    allowed BOOLEAN NOT NULL,
    reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_audit_created ON audit_logs(created_at DESC);

CREATE TABLE IF NOT EXISTS federation_nodes (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name TEXT NOT NULL UNIQUE,
    url TEXT NOT NULL,
    trust_tier TEXT NOT NULL DEFAULT 'trusted_network',
    public_key TEXT,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    last_sync_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS sync_records (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    node_id UUID NOT NULL REFERENCES federation_nodes(id) ON DELETE CASCADE,
    direction TEXT NOT NULL,
    items_synced INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS webhooks (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    organization_id UUID REFERENCES organizations(id) ON DELETE CASCADE,
    url TEXT NOT NULL,
    events JSONB NOT NULL DEFAULT '[]'::jsonb,
    secret_hash TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Row-Level Security for multi-tenant isolation on knowledge
ALTER TABLE knowledge ENABLE ROW LEVEL SECURITY;
CREATE POLICY knowledge_tenant_isolation ON knowledge
    USING (
        visibility = 'public'
        OR organization_id::text = current_setting('app.current_org_id', true)
    );

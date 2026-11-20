pub mod postgres;
pub mod sqlite;

use async_trait::async_trait;
use brainmesh_core::{
    AgentRecord, AgentRegistrationRequest, AgentRegistrationResponse, AuditLogRecord, AuthContext,
    ChallengeKnowledgeRequest, ChallengeRecord, ContributeKnowledgeRequest,
    CreateRelationshipRequest, ExperienceRecord, ExtractExperienceRequest, FederationNodeRecord,
    FederationNodeRequest, KnowledgeObject, KnowledgeVersionRecord, NetworkStatus, RecallMatch,
    RecallQuery, RelationshipRecord, Result, SubmitExperienceRequest, UpdateKnowledgeRequest,
    VerificationHistoryResponse, VerificationRecord, VerifyKnowledgeRequest, Visibility,
};
use std::sync::Arc;

#[async_trait]
pub trait BrainMeshStore: Send + Sync {
    async fn health_check(&self) -> Result<()>;

    // Auth & Agents
    async fn authenticate_api_key(&self, raw_key: &str) -> Result<AuthContext>;
    async fn register_agent(
        &self,
        req: AgentRegistrationRequest,
    ) -> Result<AgentRegistrationResponse>;
    async fn list_agents(&self, ctx: &AuthContext) -> Result<Vec<AgentRecord>>;
    async fn get_agent(&self, ctx: &AuthContext, id: &str) -> Result<AgentRecord>;
    async fn delete_agent(&self, ctx: &AuthContext, id: &str) -> Result<()>;

    // Knowledge
    async fn contribute_knowledge(
        &self,
        ctx: &AuthContext,
        req: ContributeKnowledgeRequest,
    ) -> Result<KnowledgeObject>;
    async fn get_knowledge(&self, ctx: &AuthContext, id: &str) -> Result<KnowledgeObject>;
    async fn update_knowledge(
        &self,
        ctx: &AuthContext,
        id: &str,
        req: UpdateKnowledgeRequest,
    ) -> Result<KnowledgeObject>;
    async fn delete_knowledge(&self, ctx: &AuthContext, id: &str) -> Result<()>;
    async fn list_knowledge(
        &self,
        ctx: &AuthContext,
        domain: Option<String>,
        visibility: Option<Visibility>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<KnowledgeObject>>;
    async fn get_knowledge_history(
        &self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<Vec<KnowledgeVersionRecord>>;

    // Recall
    async fn recall(&self, ctx: &AuthContext, query: RecallQuery) -> Result<Vec<RecallMatch>>;
    async fn recall_similar(
        &self,
        ctx: &AuthContext,
        id: &str,
        limit: usize,
    ) -> Result<Vec<RecallMatch>>;

    // Experiences
    async fn submit_experience(
        &self,
        ctx: &AuthContext,
        req: SubmitExperienceRequest,
    ) -> Result<ExperienceRecord>;
    async fn get_experience(&self, ctx: &AuthContext, id: &str) -> Result<ExperienceRecord>;
    async fn extract_experience(
        &self,
        ctx: &AuthContext,
        id: &str,
        req: ExtractExperienceRequest,
    ) -> Result<KnowledgeObject>;

    // Trust & Verification
    async fn verify_knowledge(
        &self,
        ctx: &AuthContext,
        id: &str,
        req: VerifyKnowledgeRequest,
    ) -> Result<VerificationRecord>;
    async fn challenge_knowledge(
        &self,
        ctx: &AuthContext,
        id: &str,
        req: ChallengeKnowledgeRequest,
    ) -> Result<ChallengeRecord>;
    async fn get_verification_history(
        &self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<VerificationHistoryResponse>;

    // Graph Relationships
    async fn create_relationship(
        &self,
        ctx: &AuthContext,
        source_id: &str,
        req: CreateRelationshipRequest,
    ) -> Result<RelationshipRecord>;
    async fn get_related(
        &self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<(Vec<RelationshipRecord>, Vec<KnowledgeObject>)>;

    // Federation
    async fn register_federation_node(
        &self,
        ctx: &AuthContext,
        req: FederationNodeRequest,
    ) -> Result<FederationNodeRecord>;
    async fn list_federation_nodes(&self, ctx: &AuthContext) -> Result<Vec<FederationNodeRecord>>;

    // Network & Audit
    async fn network_status(&self) -> Result<NetworkStatus>;
    async fn record_audit(
        &self,
        principal: &str,
        action: &str,
        resource_type: &str,
        resource_id: &str,
        allowed: bool,
        reason: Option<&str>,
    ) -> Result<()>;
    async fn list_audit_logs(&self, limit: usize) -> Result<Vec<AuditLogRecord>>;
    async fn mark_federation_synced(&self, node_id: &str) -> Result<()>;
}

pub use postgres::PostgresStore;
pub use sqlite::SqliteStore;

/// Connects to PostgreSQL (`postgres://` or `postgresql://`) or SQLite (`sqlite://` or file path).
pub async fn connect_store(database_url: &str) -> Result<Arc<dyn BrainMeshStore>> {
    if database_url.starts_with("postgres://") || database_url.starts_with("postgresql://") {
        Ok(Arc::new(PostgresStore::connect(database_url).await?))
    } else {
        Ok(Arc::new(SqliteStore::connect(database_url).await?))
    }
}

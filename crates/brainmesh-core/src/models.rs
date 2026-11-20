use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Six primary BrainMesh knowledge categories.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeCategory {
    Knowledge,
    Experience,
    Skill,
    Solution,
    Discovery,
    Lesson,
}

impl Default for KnowledgeCategory {
    fn default() -> Self {
        Self::Knowledge
    }
}

/// Evidence-based verification states.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Unverified,
    ReportedSuccessful,
    IndependentlyConfirmed,
    VerifiedUnderConditions,
    Disputed,
    Outdated,
    Rejected,
}

impl Default for VerificationStatus {
    fn default() -> Self {
        Self::Unverified
    }
}

/// Privacy & sharing visibility levels. Defaults strictly to Private.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Private,
    Workspace,
    Organization,
    TrustedNetwork,
    Public,
}

impl Default for Visibility {
    fn default() -> Self {
        Self::Private
    }
}

/// Typed relationships in the BrainMesh knowledge graph.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipType {
    RelatedTo,
    Solves,
    Causes,
    DependsOn,
    Improves,
    Replaces,
    Contradicts,
    VerifiedBy,
    DerivedFrom,
    AppliesTo,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VerificationInfo {
    pub status: VerificationStatus,
    pub method: Option<String>,
    #[serde(default)]
    pub evidence: Vec<serde_json::Value>,
    pub verified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    pub agent_id: String,
    pub framework: String,
    pub contributor_id: String,
    #[serde(default)]
    pub organization_id: Option<String>,
    #[serde(default)]
    pub workspace_id: Option<String>,
}

/// Canonical BrainMesh Knowledge Object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeObject {
    pub id: String,
    #[serde(default)]
    pub category: KnowledgeCategory,
    #[serde(rename = "type")]
    pub record_type: String,
    pub title: String,
    pub description: String,
    pub domain: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub content: serde_json::Value,
    #[serde(default)]
    pub content_hash: String,
    #[serde(default)]
    pub verification: VerificationInfo,
    pub provenance: Provenance,
    #[serde(default)]
    pub visibility: Visibility,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub version: u32,
}

impl KnowledgeObject {
    pub fn compute_content_hash(title: &str, domain: &str, content: &serde_json::Value) -> String {
        let mut hasher = Sha256::new();
        hasher.update(title.trim().to_lowercase().as_bytes());
        hasher.update(b"|");
        hasher.update(domain.trim().to_lowercase().as_bytes());
        hasher.update(b"|");
        hasher.update(content.to_string().as_bytes());
        let bytes = hasher.finalize();
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContributeKnowledgeRequest {
    #[serde(default)]
    pub category: KnowledgeCategory,
    #[serde(rename = "type", default = "default_record_type")]
    pub record_type: String,
    pub title: String,
    pub description: String,
    pub domain: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub content: serde_json::Value,
    pub verification: Option<VerificationInfo>,
    #[serde(default)]
    pub visibility: Visibility,
    pub expires_at: Option<DateTime<Utc>>,
}

fn default_record_type() -> String {
    "knowledge".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateKnowledgeRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub content: Option<serde_json::Value>,
    pub tags: Option<Vec<String>>,
    pub visibility: Option<Visibility>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeVersionRecord {
    pub id: String,
    pub knowledge_id: String,
    pub version: u32,
    pub title: String,
    pub description: String,
    pub content: serde_json::Value,
    pub changed_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallQuery {
    pub query: String,
    pub domain: Option<String>,
    pub framework: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub verification_status: Option<VerificationStatus>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreExplanation {
    pub text_score: f64,
    pub verification_boost: f64,
    pub freshness_boost: f64,
    pub total_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallMatch {
    pub knowledge: KnowledgeObject,
    pub score: ScoreExplanation,
    /// Explicit untrusted-data label to mitigate prompt injection in downstream agents.
    pub safety_notice: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRegistrationRequest {
    pub name: String,
    pub framework: String,
    pub model: Option<String>,
    pub organization_id: Option<String>,
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRecord {
    pub id: String,
    pub name: String,
    pub framework: String,
    pub model: Option<String>,
    pub organization_id: Option<String>,
    pub workspace_id: Option<String>,
    pub capabilities: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRegistrationResponse {
    pub agent: AgentRecord,
    /// Returned once upon registration.
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyKnowledgeRequest {
    pub status: VerificationStatus,
    pub method: String,
    #[serde(default)]
    pub evidence: Vec<serde_json::Value>,
    pub conditions: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationRecord {
    pub id: String,
    pub knowledge_id: String,
    pub verifier_agent_id: String,
    pub status: VerificationStatus,
    pub method: String,
    pub evidence: Vec<serde_json::Value>,
    pub conditions: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeKnowledgeRequest {
    pub reason: String,
    #[serde(default)]
    pub evidence: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeRecord {
    pub id: String,
    pub knowledge_id: String,
    pub challenger_agent_id: String,
    pub reason: String,
    pub evidence: Vec<serde_json::Value>,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationHistoryResponse {
    pub knowledge_id: String,
    pub current_status: VerificationStatus,
    pub verifications: Vec<VerificationRecord>,
    pub challenges: Vec<ChallengeRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRelationshipRequest {
    pub target_id: String,
    pub relationship_type: RelationshipType,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationshipRecord {
    pub id: String,
    pub source_id: String,
    pub target_id: String,
    pub relationship_type: RelationshipType,
    pub metadata: serde_json::Value,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitExperienceRequest {
    pub title: String,
    pub outcome: String,
    pub summary: String,
    #[serde(default)]
    pub events: Vec<serde_json::Value>,
    #[serde(default)]
    pub approved_for_publication: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperienceRecord {
    pub id: String,
    pub agent_id: String,
    pub framework: String,
    pub title: String,
    pub outcome: String,
    pub summary: String,
    pub events: Vec<serde_json::Value>,
    pub redacted: bool,
    pub approved_for_publication: bool,
    pub extracted_knowledge_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractExperienceRequest {
    pub domain: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub visibility: Visibility,
    #[serde(default)]
    pub user_approved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationNodeRequest {
    pub name: String,
    pub url: String,
    #[serde(default = "default_trust_tier")]
    pub trust_tier: String,
}

fn default_trust_tier() -> String {
    "trusted_network".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationNodeRecord {
    pub id: String,
    pub name: String,
    pub url: String,
    pub trust_tier: String,
    pub enabled: bool,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStatus {
    pub status: String,
    pub version: String,
    pub total_agents: usize,
    pub total_knowledge: usize,
    pub verified_knowledge: usize,
    pub total_verifications: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogRecord {
    pub id: String,
    pub principal: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub allowed: bool,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationSyncResult {
    pub nodes_contacted: usize,
    pub items_imported: usize,
    pub errors: Vec<String>,
}

pub fn new_uuid() -> String {
    Uuid::new_v4().to_string()
}

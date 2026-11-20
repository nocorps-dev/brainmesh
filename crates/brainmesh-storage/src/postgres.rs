use crate::BrainMeshStore;
use async_trait::async_trait;
use brainmesh_core::{
    hash_api_key, new_uuid, validate_contribution, AgentRecord, AgentRegistrationRequest,
    AgentRegistrationResponse, AuditLogRecord, AuthContext, BrainMeshError,
    ChallengeKnowledgeRequest, ChallengeRecord, ContributeKnowledgeRequest,
    CreateRelationshipRequest, ExperienceRecord, ExtractExperienceRequest, FederationNodeRecord,
    FederationNodeRequest, KnowledgeCategory, KnowledgeObject, KnowledgeVersionRecord,
    NetworkStatus, Permission, PromptInjectionGuard, Provenance, RecallMatch, RecallQuery,
    RelationshipRecord, RelationshipType, Result, ScoreExplanation, SecretRedactor,
    SubmitExperienceRequest, UpdateKnowledgeRequest, VerificationHistoryResponse,
    VerificationInfo, VerificationRecord, VerificationStatus, VerifyKnowledgeRequest, Visibility,
    cosine_similarity, LocalHashEmbeddingProvider,
};
use chrono::{DateTime, Utc};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use uuid::Uuid;

const POSTGRES_SCHEMA: &str = include_str!("../../../migrations/postgres/0001_initial_schema.sql");

#[derive(Clone)]
pub struct PostgresStore {
    pool: PgPool,
}

impl PostgresStore {
    pub async fn connect(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await
            .map_err(|e| BrainMeshError::Storage(format!("postgres connect failed: {e}")))?;

        sqlx::raw_sql(POSTGRES_SCHEMA)
            .execute(&pool)
            .await
            .map_err(|e| BrainMeshError::Storage(format!("postgres migration failed: {e}")))?;

        Ok(Self { pool })
    }

    fn map_err(e: sqlx::Error) -> BrainMeshError {
        BrainMeshError::Storage(e.to_string())
    }

    fn parse_uuid(s: &str) -> Result<Uuid> {
        Uuid::parse_str(s)
            .map_err(|e| BrainMeshError::Validation(format!("invalid UUID '{s}': {e}")))
    }

    fn parse_opt_uuid(s: &Option<String>) -> Result<Option<Uuid>> {
        match s {
            Some(v) => Ok(Some(Self::parse_uuid(v)?)),
            None => Ok(None),
        }
    }

    fn enum_str<T: serde::Serialize>(val: &T) -> String {
        serde_json::to_string(val)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string()
    }

    async fn fetch_tags(&self, knowledge_id: Uuid) -> Result<Vec<String>> {
        let rows = sqlx::query("SELECT tag FROM knowledge_tags WHERE knowledge_id = $1")
            .bind(knowledge_id)
            .fetch_all(&self.pool)
            .await
            .map_err(Self::map_err)?;
        Ok(rows
            .into_iter()
            .filter_map(|r| r.try_get::<String, _>("tag").ok())
            .collect())
    }

    async fn row_to_knowledge(&self, row: &sqlx::postgres::PgRow) -> Result<KnowledgeObject> {
        let id: Uuid = row.try_get("id").map_err(Self::map_err)?;
        let tags = self.fetch_tags(id).await?;
        let category_str: String = row.try_get("category").map_err(Self::map_err)?;
        let verif_status_str: String = row.try_get("verification_status").map_err(Self::map_err)?;
        let vis_str: String = row.try_get("visibility").map_err(Self::map_err)?;
        let ver: i32 = row.try_get("version").map_err(Self::map_err)?;
        let agent_uuid: Option<Uuid> = row.try_get("agent_id").map_err(Self::map_err)?;
        let org_uuid: Option<Uuid> = row.try_get("organization_id").map_err(Self::map_err)?;
        let ws_uuid: Option<Uuid> = row.try_get("workspace_id").map_err(Self::map_err)?;
        let ev_val: serde_json::Value = row
            .try_get("verification_evidence")
            .map_err(Self::map_err)?;
        let evidence = ev_val.as_array().cloned().unwrap_or_default();

        Ok(KnowledgeObject {
            id: id.to_string(),
            category: serde_json::from_str(&format!("\"{category_str}\"")).unwrap_or_default(),
            record_type: row.try_get("record_type").map_err(Self::map_err)?,
            title: row.try_get("title").map_err(Self::map_err)?,
            description: row.try_get("description").map_err(Self::map_err)?,
            domain: row.try_get("domain").map_err(Self::map_err)?,
            tags,
            content: row.try_get("content").map_err(Self::map_err)?,
            content_hash: row.try_get("content_hash").map_err(Self::map_err)?,
            verification: VerificationInfo {
                status: serde_json::from_str(&format!("\"{verif_status_str}\""))
                    .unwrap_or_default(),
                method: row.try_get("verification_method").map_err(Self::map_err)?,
                evidence,
                verified_at: row.try_get("verified_at").map_err(Self::map_err)?,
            },
            provenance: Provenance {
                agent_id: agent_uuid.map(|u| u.to_string()).unwrap_or_default(),
                framework: row.try_get("framework").map_err(Self::map_err)?,
                contributor_id: row.try_get("contributor_id").map_err(Self::map_err)?,
                organization_id: org_uuid.map(|u| u.to_string()),
                workspace_id: ws_uuid.map(|u| u.to_string()),
            },
            visibility: serde_json::from_str(&format!("\"{vis_str}\"")).unwrap_or_default(),
            created_at: row.try_get("created_at").map_err(Self::map_err)?,
            updated_at: row.try_get("updated_at").map_err(Self::map_err)?,
            expires_at: row.try_get("expires_at").map_err(Self::map_err)?,
            version: ver as u32,
        })
    }
}

#[async_trait]
impl BrainMeshStore for PostgresStore {
    async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(Self::map_err)?;
        Ok(())
    }

    async fn authenticate_api_key(&self, raw_key: &str) -> Result<AuthContext> {
        let key_hash = hash_api_key(raw_key);
        let row = sqlx::query(
            "SELECT a.id, a.framework, a.organization_id, a.workspace_id
             FROM agents a
             JOIN agent_credentials c ON c.agent_id = a.id
             WHERE c.key_hash = $1 AND c.revoked_at IS NULL AND a.revoked_at IS NULL",
        )
        .bind(&key_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(Self::map_err)?
        .ok_or_else(|| BrainMeshError::Authentication("invalid or revoked API key".into()))?;

        let id: Uuid = row.try_get("id").map_err(Self::map_err)?;
        let org: Option<Uuid> = row.try_get("organization_id").map_err(Self::map_err)?;
        let ws: Option<Uuid> = row.try_get("workspace_id").map_err(Self::map_err)?;

        Ok(AuthContext {
            agent_id: id.to_string(),
            framework: row.try_get("framework").map_err(Self::map_err)?,
            contributor_id: id.to_string(),
            organization_id: org.map(|u| u.to_string()),
            workspace_id: ws.map(|u| u.to_string()),
            scopes: Permission::standard_agent(),
        })
    }

    async fn register_agent(
        &self,
        req: AgentRegistrationRequest,
    ) -> Result<AgentRegistrationResponse> {
        if req.name.trim().is_empty() || req.framework.trim().is_empty() {
            return Err(BrainMeshError::Validation(
                "agent name and framework must not be empty".into(),
            ));
        }
        let id = Uuid::new_v4();
        let raw_key = format!("bm_{}", new_uuid().replace('-', ""));
        let key_hash = hash_api_key(&raw_key);
        let now = Utc::now();
        let org_uuid = Self::parse_opt_uuid(&req.organization_id)?;
        let ws_uuid = Self::parse_opt_uuid(&req.workspace_id)?;
        let caps_val = serde_json::to_value(&req.capabilities).unwrap_or(serde_json::json!([]));

        sqlx::query(
            "INSERT INTO agents (id, organization_id, workspace_id, name, framework, model, capabilities, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(id)
        .bind(org_uuid)
        .bind(ws_uuid)
        .bind(&req.name)
        .bind(&req.framework)
        .bind(&req.model)
        .bind(&caps_val)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;

        sqlx::query(
            "INSERT INTO agent_credentials (id, agent_id, key_hash, created_at) VALUES ($1, $2, $3, $4)",
        )
        .bind(Uuid::new_v4())
        .bind(id)
        .bind(&key_hash)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;

        Ok(AgentRegistrationResponse {
            agent: AgentRecord {
                id: id.to_string(),
                name: req.name,
                framework: req.framework,
                model: req.model,
                organization_id: req.organization_id,
                workspace_id: req.workspace_id,
                capabilities: req.capabilities,
                created_at: now,
            },
            api_key: raw_key,
        })
    }

    async fn list_agents(&self, _ctx: &AuthContext) -> Result<Vec<AgentRecord>> {
        let rows = sqlx::query(
            "SELECT id, organization_id, workspace_id, name, framework, model, capabilities, created_at
             FROM agents WHERE revoked_at IS NULL ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_err)?;

        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let id: Uuid = r.try_get("id").map_err(Self::map_err)?;
            let org: Option<Uuid> = r.try_get("organization_id").map_err(Self::map_err)?;
            let ws: Option<Uuid> = r.try_get("workspace_id").map_err(Self::map_err)?;
            let caps: serde_json::Value = r.try_get("capabilities").map_err(Self::map_err)?;
            out.push(AgentRecord {
                id: id.to_string(),
                name: r.try_get("name").map_err(Self::map_err)?,
                framework: r.try_get("framework").map_err(Self::map_err)?,
                model: r.try_get("model").map_err(Self::map_err)?,
                organization_id: org.map(|u| u.to_string()),
                workspace_id: ws.map(|u| u.to_string()),
                capabilities: serde_json::from_value(caps).unwrap_or_default(),
                created_at: r.try_get("created_at").map_err(Self::map_err)?,
            });
        }
        Ok(out)
    }

    async fn get_agent(&self, _ctx: &AuthContext, id: &str) -> Result<AgentRecord> {
        let uuid = Self::parse_uuid(id)?;
        let r = sqlx::query(
            "SELECT id, organization_id, workspace_id, name, framework, model, capabilities, created_at
             FROM agents WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(Self::map_err)?
        .ok_or_else(|| BrainMeshError::NotFound(format!("agent '{id}' not found")))?;

        let org: Option<Uuid> = r.try_get("organization_id").map_err(Self::map_err)?;
        let ws: Option<Uuid> = r.try_get("workspace_id").map_err(Self::map_err)?;
        let caps: serde_json::Value = r.try_get("capabilities").map_err(Self::map_err)?;
        Ok(AgentRecord {
            id: id.to_string(),
            name: r.try_get("name").map_err(Self::map_err)?,
            framework: r.try_get("framework").map_err(Self::map_err)?,
            model: r.try_get("model").map_err(Self::map_err)?,
            organization_id: org.map(|u| u.to_string()),
            workspace_id: ws.map(|u| u.to_string()),
            capabilities: serde_json::from_value(caps).unwrap_or_default(),
            created_at: r.try_get("created_at").map_err(Self::map_err)?,
        })
    }

    async fn delete_agent(&self, ctx: &AuthContext, id: &str) -> Result<()> {
        if !ctx.has_permission(Permission::ManageAgents) && ctx.agent_id != id {
            return Err(BrainMeshError::Authorization(
                "cannot revoke another agent".into(),
            ));
        }
        let uuid = Self::parse_uuid(id)?;
        let res = sqlx::query(
            "UPDATE agents SET revoked_at = NOW() WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(uuid)
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;
        if res.rows_affected() == 0 {
            return Err(BrainMeshError::NotFound(format!("agent '{id}' not found")));
        }
        Ok(())
    }

    async fn contribute_knowledge(
        &self,
        ctx: &AuthContext,
        req: ContributeKnowledgeRequest,
    ) -> Result<KnowledgeObject> {
        if !ctx.has_permission(Permission::Contribute) {
            return Err(BrainMeshError::Authorization(
                "lacks contribute permission".into(),
            ));
        }
        validate_contribution(&req)?;

        let safe_title = SecretRedactor::redact(&req.title);
        let safe_desc = SecretRedactor::redact(&req.description);
        let safe_content = SecretRedactor::redact_json(&req.content);
        let content_hash =
            KnowledgeObject::compute_content_hash(&safe_title, &req.domain, &safe_content);

        let id = Uuid::new_v4();
        let now = Utc::now();
        let category_str = Self::enum_str(&req.category);
        let vis_str = Self::enum_str(&req.visibility);
        let verif = req.verification.unwrap_or_default();
        let verif_status_str = Self::enum_str(&verif.status);
        let ev_val = serde_json::to_value(&verif.evidence).unwrap_or(serde_json::json!([]));
        let agent_uuid = Uuid::parse_str(&ctx.agent_id).ok();
        let org_uuid = Self::parse_opt_uuid(&ctx.organization_id)?;
        let ws_uuid = Self::parse_opt_uuid(&ctx.workspace_id)?;

        sqlx::query(
            "INSERT INTO knowledge (
                id, organization_id, workspace_id, category, record_type, title, description, domain,
                content, content_hash, verification_status, verification_method, verification_evidence,
                verified_at, agent_id, framework, contributor_id, visibility, version, created_at, updated_at, expires_at
             ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,1,$19,$20,$21)",
        )
        .bind(id)
        .bind(org_uuid)
        .bind(ws_uuid)
        .bind(&category_str)
        .bind(&req.record_type)
        .bind(&safe_title)
        .bind(&safe_desc)
        .bind(&req.domain)
        .bind(&safe_content)
        .bind(&content_hash)
        .bind(&verif_status_str)
        .bind(&verif.method)
        .bind(&ev_val)
        .bind(verif.verified_at)
        .bind(agent_uuid)
        .bind(&ctx.framework)
        .bind(&ctx.contributor_id)
        .bind(&vis_str)
        .bind(now)
        .bind(now)
        .bind(req.expires_at)
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;

        for tag in &req.tags {
            sqlx::query("INSERT INTO knowledge_tags (knowledge_id, tag) VALUES ($1, $2) ON CONFLICT DO NOTHING")
                .bind(id)
                .bind(tag)
                .execute(&self.pool)
                .await
                .map_err(Self::map_err)?;
        }

        sqlx::query(
            "INSERT INTO knowledge_versions (id, knowledge_id, version, title, description, content, changed_by, created_at)
             VALUES ($1, $2, 1, $3, $4, $5, $6, $7)",
        )
        .bind(Uuid::new_v4())
        .bind(id)
        .bind(&safe_title)
        .bind(&safe_desc)
        .bind(&safe_content)
        .bind(&ctx.agent_id)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;

        self.get_knowledge(ctx, &id.to_string()).await
    }

    async fn get_knowledge(&self, ctx: &AuthContext, id: &str) -> Result<KnowledgeObject> {
        let uuid = Self::parse_uuid(id)?;
        let row = sqlx::query("SELECT * FROM knowledge WHERE id = $1 AND deleted_at IS NULL")
            .bind(uuid)
            .fetch_optional(&self.pool)
            .await
            .map_err(Self::map_err)?
            .ok_or_else(|| BrainMeshError::NotFound(format!("knowledge '{id}' not found")))?;

        let item = self.row_to_knowledge(&row).await?;
        if !ctx.can_access_knowledge(&item) {
            return Err(BrainMeshError::Authorization(format!(
                "knowledge '{id}' is not accessible"
            )));
        }
        Ok(item)
    }

    async fn update_knowledge(
        &self,
        ctx: &AuthContext,
        id: &str,
        req: UpdateKnowledgeRequest,
    ) -> Result<KnowledgeObject> {
        let existing = self.get_knowledge(ctx, id).await?;
        if existing.provenance.agent_id != ctx.agent_id && !ctx.has_permission(Permission::Admin) {
            return Err(BrainMeshError::Authorization("forbidden".into()));
        }
        let uuid = Self::parse_uuid(id)?;
        let title = req
            .title
            .map(|t| SecretRedactor::redact(&t))
            .unwrap_or(existing.title);
        let description = req
            .description
            .map(|d| SecretRedactor::redact(&d))
            .unwrap_or(existing.description);
        let content = req
            .content
            .map(|c| SecretRedactor::redact_json(&c))
            .unwrap_or(existing.content);
        let vis_str = Self::enum_str(&req.visibility.unwrap_or(existing.visibility));
        let new_ver = (existing.version + 1) as i32;

        sqlx::query(
            "UPDATE knowledge SET title = $1, description = $2, content = $3, visibility = $4, version = $5, updated_at = NOW() WHERE id = $6",
        )
        .bind(&title)
        .bind(&description)
        .bind(&content)
        .bind(&vis_str)
        .bind(new_ver)
        .bind(uuid)
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;

        self.get_knowledge(ctx, id).await
    }

    async fn delete_knowledge(&self, ctx: &AuthContext, id: &str) -> Result<()> {
        let existing = self.get_knowledge(ctx, id).await?;
        if existing.provenance.agent_id != ctx.agent_id && !ctx.has_permission(Permission::Admin) {
            return Err(BrainMeshError::Authorization("forbidden".into()));
        }
        let uuid = Self::parse_uuid(id)?;
        sqlx::query("UPDATE knowledge SET deleted_at = NOW() WHERE id = $1")
            .bind(uuid)
            .execute(&self.pool)
            .await
            .map_err(Self::map_err)?;
        Ok(())
    }

    async fn list_knowledge(
        &self,
        ctx: &AuthContext,
        domain: Option<String>,
        visibility: Option<Visibility>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<KnowledgeObject>> {
        let rows = sqlx::query(
            "SELECT * FROM knowledge WHERE deleted_at IS NULL ORDER BY updated_at DESC LIMIT $1 OFFSET $2",
        )
        .bind((limit.max(1) * 3) as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_err)?;

        let mut items = Vec::new();
        for r in rows {
            let item = self.row_to_knowledge(&r).await?;
            if !ctx.can_access_knowledge(&item) {
                continue;
            }
            if let Some(ref d) = domain {
                if &item.domain != d {
                    continue;
                }
            }
            if let Some(v) = visibility {
                if item.visibility != v {
                    continue;
                }
            }
            items.push(item);
            if items.len() >= limit {
                break;
            }
        }
        Ok(items)
    }

    async fn get_knowledge_history(
        &self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<Vec<KnowledgeVersionRecord>> {
        let _ = self.get_knowledge(ctx, id).await?;
        let uuid = Self::parse_uuid(id)?;
        let rows = sqlx::query(
            "SELECT * FROM knowledge_versions WHERE knowledge_id = $1 ORDER BY version ASC",
        )
        .bind(uuid)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_err)?;

        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let vid: Uuid = r.try_get("id").map_err(Self::map_err)?;
            let kid: Uuid = r.try_get("knowledge_id").map_err(Self::map_err)?;
            let ver: i32 = r.try_get("version").map_err(Self::map_err)?;
            out.push(KnowledgeVersionRecord {
                id: vid.to_string(),
                knowledge_id: kid.to_string(),
                version: ver as u32,
                title: r.try_get("title").map_err(Self::map_err)?,
                description: r.try_get("description").map_err(Self::map_err)?,
                content: r.try_get("content").map_err(Self::map_err)?,
                changed_by: r.try_get("changed_by").map_err(Self::map_err)?,
                created_at: r.try_get("created_at").map_err(Self::map_err)?,
            });
        }
        Ok(out)
    }

    async fn recall(&self, ctx: &AuthContext, query: RecallQuery) -> Result<Vec<RecallMatch>> {
        if !ctx.has_permission(Permission::Recall) {
            return Err(BrainMeshError::Authorization(
                "lacks recall permission".into(),
            ));
        }
        let rows = sqlx::query("SELECT * FROM knowledge WHERE deleted_at IS NULL")
            .fetch_all(&self.pool)
            .await
            .map_err(Self::map_err)?;

        let embedder = LocalHashEmbeddingProvider::default();
        let query_vec = embedder.embed_one(&query.query);
        let terms: Vec<String> = query
            .query
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            .filter(|s| s.len() >= 2)
            .map(String::from)
            .collect();

        let mut matches = Vec::new();
        for r in rows {
            let item = self.row_to_knowledge(&r).await?;
            if !ctx.can_access_knowledge(&item) {
                continue;
            }
            if let Some(ref d) = query.domain {
                if &item.domain != d {
                    continue;
                }
            }
            if let Some(ref f) = query.framework {
                if &item.provenance.framework != f {
                    continue;
                }
            }
            if let Some(vs) = query.verification_status {
                if item.verification.status != vs {
                    continue;
                }
            }
            if !query.tags.is_empty() && !query.tags.iter().all(|t| item.tags.contains(t)) {
                continue;
            }

            let haystack = format!(
                "{} {} {} {} {}",
                item.title,
                item.description,
                item.domain,
                item.tags.join(" "),
                item.content
            );
            let lower = haystack.to_lowercase();
            let matched = terms.iter().filter(|t| lower.contains(t.as_str())).count();
            let sem = cosine_similarity(&query_vec, &embedder.embed_one(&haystack));
            if !terms.is_empty() && matched == 0 && sem < 0.15 {
                continue;
            }

            let text_score = if terms.is_empty() {
                0.5
            } else {
                ((matched as f64) / (terms.len() as f64)) * 0.7 + sem.max(0.0) * 0.3
            };
            let verification_boost = match item.verification.status {
                VerificationStatus::IndependentlyConfirmed => 0.4,
                VerificationStatus::VerifiedUnderConditions => 0.3,
                VerificationStatus::ReportedSuccessful => 0.2,
                VerificationStatus::Unverified => 0.0,
                VerificationStatus::Outdated => -0.1,
                VerificationStatus::Disputed => -0.2,
                VerificationStatus::Rejected => -0.5,
            };
            let age_days = (Utc::now() - item.updated_at).num_days().max(0) as f64;
            let freshness_boost = (0.1 / (1.0 + age_days / 30.0)).max(0.0);
            let total_score = text_score + verification_boost + freshness_boost;
            let safety_notice = PromptInjectionGuard::safety_notice_for(&item);

            matches.push(RecallMatch {
                knowledge: item,
                score: ScoreExplanation {
                    text_score,
                    verification_boost,
                    freshness_boost,
                    total_score,
                },
                safety_notice,
            });
        }
        matches.sort_by(|a, b| {
            b.score
                .total_score
                .partial_cmp(&a.score.total_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        matches.truncate(query.limit.max(1));
        Ok(matches)
    }

    async fn recall_similar(
        &self,
        ctx: &AuthContext,
        id: &str,
        limit: usize,
    ) -> Result<Vec<RecallMatch>> {
        let base = self.get_knowledge(ctx, id).await?;
        let mut results = self
            .recall(
                ctx,
                RecallQuery {
                    query: format!("{} {}", base.title, base.tags.join(" ")),
                    domain: Some(base.domain),
                    framework: None,
                    tags: Vec::new(),
                    verification_status: None,
                    limit: limit + 1,
                },
            )
            .await?;
        results.retain(|m| m.knowledge.id != id);
        results.truncate(limit);
        Ok(results)
    }

    async fn submit_experience(
        &self,
        ctx: &AuthContext,
        req: SubmitExperienceRequest,
    ) -> Result<ExperienceRecord> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let safe_title = SecretRedactor::redact(&req.title);
        let safe_outcome = SecretRedactor::redact(&req.outcome);
        let safe_summary = SecretRedactor::redact(&req.summary);
        let safe_events: Vec<serde_json::Value> = req
            .events
            .iter()
            .map(SecretRedactor::redact_json)
            .collect();
        let events_val = serde_json::Value::Array(safe_events.clone());
        let agent_uuid = Uuid::parse_str(&ctx.agent_id).ok();

        sqlx::query(
            "INSERT INTO experiences (id, agent_id, framework, title, outcome, summary, raw_events, redacted, approved_for_publication, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, TRUE, $8, $9)",
        )
        .bind(id)
        .bind(agent_uuid)
        .bind(&ctx.framework)
        .bind(&safe_title)
        .bind(&safe_outcome)
        .bind(&safe_summary)
        .bind(&events_val)
        .bind(req.approved_for_publication)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;

        Ok(ExperienceRecord {
            id: id.to_string(),
            agent_id: ctx.agent_id.clone(),
            framework: ctx.framework.clone(),
            title: safe_title,
            outcome: safe_outcome,
            summary: safe_summary,
            events: safe_events,
            redacted: true,
            approved_for_publication: req.approved_for_publication,
            extracted_knowledge_id: None,
            created_at: now,
        })
    }

    async fn get_experience(&self, ctx: &AuthContext, id: &str) -> Result<ExperienceRecord> {
        let uuid = Self::parse_uuid(id)?;
        let r = sqlx::query("SELECT * FROM experiences WHERE id = $1")
            .bind(uuid)
            .fetch_optional(&self.pool)
            .await
            .map_err(Self::map_err)?
            .ok_or_else(|| BrainMeshError::NotFound(format!("experience '{id}' not found")))?;

        let agent_uuid: Option<Uuid> = r.try_get("agent_id").map_err(Self::map_err)?;
        let agent_id = agent_uuid.map(|u| u.to_string()).unwrap_or_default();
        if agent_id != ctx.agent_id && !ctx.has_permission(Permission::Admin) {
            return Err(BrainMeshError::Authorization("forbidden".into()));
        }
        let raw_events: serde_json::Value = r.try_get("raw_events").map_err(Self::map_err)?;
        let ext_uuid: Option<Uuid> = r.try_get("extracted_knowledge_id").map_err(Self::map_err)?;
        Ok(ExperienceRecord {
            id: id.to_string(),
            agent_id,
            framework: r.try_get("framework").map_err(Self::map_err)?,
            title: r.try_get("title").map_err(Self::map_err)?,
            outcome: r.try_get("outcome").map_err(Self::map_err)?,
            summary: r.try_get("summary").map_err(Self::map_err)?,
            events: raw_events.as_array().cloned().unwrap_or_default(),
            redacted: true,
            approved_for_publication: r
                .try_get("approved_for_publication")
                .map_err(Self::map_err)?,
            extracted_knowledge_id: ext_uuid.map(|u| u.to_string()),
            created_at: r.try_get("created_at").map_err(Self::map_err)?,
        })
    }

    async fn extract_experience(
        &self,
        ctx: &AuthContext,
        id: &str,
        req: ExtractExperienceRequest,
    ) -> Result<KnowledgeObject> {
        let exp = self.get_experience(ctx, id).await?;
        if req.visibility != Visibility::Private && !req.user_approved && !exp.approved_for_publication
        {
            return Err(BrainMeshError::Authorization(
                "explicit approval required".into(),
            ));
        }
        let created = self
            .contribute_knowledge(
                ctx,
                ContributeKnowledgeRequest {
                    category: KnowledgeCategory::Lesson,
                    record_type: "extracted_lesson".into(),
                    title: exp.title,
                    description: exp.summary.clone(),
                    domain: req.domain,
                    tags: req.tags,
                    content: serde_json::json!({ "outcome": exp.outcome, "summary": exp.summary }),
                    verification: None,
                    visibility: req.visibility,
                    expires_at: None,
                },
            )
            .await?;
        let exp_uuid = Self::parse_uuid(id)?;
        let kid_uuid = Self::parse_uuid(&created.id)?;
        sqlx::query("UPDATE experiences SET extracted_knowledge_id = $1 WHERE id = $2")
            .bind(kid_uuid)
            .bind(exp_uuid)
            .execute(&self.pool)
            .await
            .map_err(Self::map_err)?;
        Ok(created)
    }

    async fn verify_knowledge(
        &self,
        ctx: &AuthContext,
        id: &str,
        req: VerifyKnowledgeRequest,
    ) -> Result<VerificationRecord> {
        let item = self.get_knowledge(ctx, id).await?;
        let effective = if item.provenance.agent_id != ctx.agent_id
            && matches!(
                req.status,
                VerificationStatus::ReportedSuccessful | VerificationStatus::IndependentlyConfirmed
            ) {
            VerificationStatus::IndependentlyConfirmed
        } else {
            req.status
        };
        let vid = Uuid::new_v4();
        let kid = Self::parse_uuid(id)?;
        let now = Utc::now();
        let status_str = Self::enum_str(&effective);
        let ev_val = serde_json::to_value(&req.evidence).unwrap_or(serde_json::json!([]));

        sqlx::query(
            "INSERT INTO verification_records (id, knowledge_id, verifier_agent_id, status, method, evidence, conditions, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(vid)
        .bind(kid)
        .bind(&ctx.agent_id)
        .bind(&status_str)
        .bind(&req.method)
        .bind(&ev_val)
        .bind(&req.conditions)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;

        sqlx::query("UPDATE knowledge SET verification_status = $1, verification_method = $2, verified_at = $3, updated_at = $3 WHERE id = $4")
            .bind(&status_str)
            .bind(&req.method)
            .bind(now)
            .bind(kid)
            .execute(&self.pool)
            .await
            .map_err(Self::map_err)?;

        Ok(VerificationRecord {
            id: vid.to_string(),
            knowledge_id: id.to_string(),
            verifier_agent_id: ctx.agent_id.clone(),
            status: effective,
            method: req.method,
            evidence: req.evidence,
            conditions: req.conditions,
            created_at: now,
        })
    }

    async fn challenge_knowledge(
        &self,
        ctx: &AuthContext,
        id: &str,
        req: ChallengeKnowledgeRequest,
    ) -> Result<ChallengeRecord> {
        let _ = self.get_knowledge(ctx, id).await?;
        let cid = Uuid::new_v4();
        let kid = Self::parse_uuid(id)?;
        let now = Utc::now();
        let ev_val = serde_json::to_value(&req.evidence).unwrap_or(serde_json::json!([]));

        sqlx::query(
            "INSERT INTO challenges (id, knowledge_id, challenger_agent_id, reason, evidence, status, created_at)
             VALUES ($1, $2, $3, $4, $5, 'open', $6)",
        )
        .bind(cid)
        .bind(kid)
        .bind(&ctx.agent_id)
        .bind(&req.reason)
        .bind(&ev_val)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;

        sqlx::query("UPDATE knowledge SET verification_status = 'disputed', updated_at = $1 WHERE id = $2")
            .bind(now)
            .bind(kid)
            .execute(&self.pool)
            .await
            .map_err(Self::map_err)?;

        Ok(ChallengeRecord {
            id: cid.to_string(),
            knowledge_id: id.to_string(),
            challenger_agent_id: ctx.agent_id.clone(),
            reason: req.reason,
            evidence: req.evidence,
            status: "open".to_string(),
            created_at: now,
        })
    }

    async fn get_verification_history(
        &self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<VerificationHistoryResponse> {
        let item = self.get_knowledge(ctx, id).await?;
        let kid = Self::parse_uuid(id)?;
        let v_rows = sqlx::query(
            "SELECT * FROM verification_records WHERE knowledge_id = $1 ORDER BY created_at ASC",
        )
        .bind(kid)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_err)?;

        let mut verifications = Vec::new();
        for r in v_rows {
            let vid: Uuid = r.try_get("id").map_err(Self::map_err)?;
            let st_str: String = r.try_get("status").map_err(Self::map_err)?;
            let ev_val: serde_json::Value = r.try_get("evidence").map_err(Self::map_err)?;
            verifications.push(VerificationRecord {
                id: vid.to_string(),
                knowledge_id: id.to_string(),
                verifier_agent_id: r.try_get("verifier_agent_id").map_err(Self::map_err)?,
                status: serde_json::from_str(&format!("\"{st_str}\"")).unwrap_or_default(),
                method: r.try_get("method").map_err(Self::map_err)?,
                evidence: ev_val.as_array().cloned().unwrap_or_default(),
                conditions: r.try_get("conditions").map_err(Self::map_err)?,
                created_at: r.try_get("created_at").map_err(Self::map_err)?,
            });
        }

        let c_rows =
            sqlx::query("SELECT * FROM challenges WHERE knowledge_id = $1 ORDER BY created_at ASC")
                .bind(kid)
                .fetch_all(&self.pool)
                .await
                .map_err(Self::map_err)?;
        let mut challenges = Vec::new();
        for r in c_rows {
            let cid: Uuid = r.try_get("id").map_err(Self::map_err)?;
            let ev_val: serde_json::Value = r.try_get("evidence").map_err(Self::map_err)?;
            challenges.push(ChallengeRecord {
                id: cid.to_string(),
                knowledge_id: id.to_string(),
                challenger_agent_id: r.try_get("challenger_agent_id").map_err(Self::map_err)?,
                reason: r.try_get("reason").map_err(Self::map_err)?,
                evidence: ev_val.as_array().cloned().unwrap_or_default(),
                status: r.try_get("status").map_err(Self::map_err)?,
                created_at: r.try_get("created_at").map_err(Self::map_err)?,
            });
        }

        Ok(VerificationHistoryResponse {
            knowledge_id: item.id,
            current_status: item.verification.status,
            verifications,
            challenges,
        })
    }

    async fn create_relationship(
        &self,
        ctx: &AuthContext,
        source_id: &str,
        req: CreateRelationshipRequest,
    ) -> Result<RelationshipRecord> {
        let _ = self.get_knowledge(ctx, source_id).await?;
        let _ = self.get_knowledge(ctx, &req.target_id).await?;
        let id = Uuid::new_v4();
        let src_uuid = Self::parse_uuid(source_id)?;
        let tgt_uuid = Self::parse_uuid(&req.target_id)?;
        let now = Utc::now();
        let rel_str = Self::enum_str(&req.relationship_type);

        sqlx::query(
            "INSERT INTO knowledge_relationships (id, source_id, target_id, relationship_type, metadata, created_by, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(id)
        .bind(src_uuid)
        .bind(tgt_uuid)
        .bind(&rel_str)
        .bind(&req.metadata)
        .bind(&ctx.agent_id)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;

        Ok(RelationshipRecord {
            id: id.to_string(),
            source_id: source_id.to_string(),
            target_id: req.target_id,
            relationship_type: req.relationship_type,
            metadata: req.metadata,
            created_by: ctx.agent_id.clone(),
            created_at: now,
        })
    }

    async fn get_related(
        &self,
        ctx: &AuthContext,
        id: &str,
    ) -> Result<(Vec<RelationshipRecord>, Vec<KnowledgeObject>)> {
        let _ = self.get_knowledge(ctx, id).await?;
        let uuid = Self::parse_uuid(id)?;
        let rows = sqlx::query(
            "SELECT * FROM knowledge_relationships WHERE source_id = $1 OR target_id = $1",
        )
        .bind(uuid)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_err)?;

        let mut rels = Vec::new();
        let mut items = Vec::new();
        for r in rows {
            let rid: Uuid = r.try_get("id").map_err(Self::map_err)?;
            let src: Uuid = r.try_get("source_id").map_err(Self::map_err)?;
            let tgt: Uuid = r.try_get("target_id").map_err(Self::map_err)?;
            let rel_str: String = r.try_get("relationship_type").map_err(Self::map_err)?;
            let other = if src == uuid { tgt } else { src };
            if let Ok(other_item) = self.get_knowledge(ctx, &other.to_string()).await {
                items.push(other_item);
                rels.push(RelationshipRecord {
                    id: rid.to_string(),
                    source_id: src.to_string(),
                    target_id: tgt.to_string(),
                    relationship_type: serde_json::from_str(&format!("\"{rel_str}\""))
                        .unwrap_or(RelationshipType::RelatedTo),
                    metadata: r.try_get("metadata").map_err(Self::map_err)?,
                    created_by: r.try_get("created_by").map_err(Self::map_err)?,
                    created_at: r.try_get("created_at").map_err(Self::map_err)?,
                });
            }
        }
        Ok((rels, items))
    }

    async fn register_federation_node(
        &self,
        ctx: &AuthContext,
        req: FederationNodeRequest,
    ) -> Result<FederationNodeRecord> {
        if !ctx.has_permission(Permission::ManageFederation) {
            return Err(BrainMeshError::Authorization("forbidden".into()));
        }
        let id = Uuid::new_v4();
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO federation_nodes (id, name, url, trust_tier, enabled, created_at)
             VALUES ($1, $2, $3, $4, TRUE, $5)",
        )
        .bind(id)
        .bind(&req.name)
        .bind(&req.url)
        .bind(&req.trust_tier)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;
        Ok(FederationNodeRecord {
            id: id.to_string(),
            name: req.name,
            url: req.url,
            trust_tier: req.trust_tier,
            enabled: true,
            last_sync_at: None,
            created_at: now,
        })
    }

    async fn list_federation_nodes(&self, _ctx: &AuthContext) -> Result<Vec<FederationNodeRecord>> {
        let rows = sqlx::query("SELECT * FROM federation_nodes ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await
            .map_err(Self::map_err)?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let id: Uuid = r.try_get("id").map_err(Self::map_err)?;
            out.push(FederationNodeRecord {
                id: id.to_string(),
                name: r.try_get("name").map_err(Self::map_err)?,
                url: r.try_get("url").map_err(Self::map_err)?,
                trust_tier: r.try_get("trust_tier").map_err(Self::map_err)?,
                enabled: r.try_get("enabled").map_err(Self::map_err)?,
                last_sync_at: r.try_get("last_sync_at").map_err(Self::map_err)?,
                created_at: r.try_get("created_at").map_err(Self::map_err)?,
            });
        }
        Ok(out)
    }

    async fn network_status(&self) -> Result<NetworkStatus> {
        let agents: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agents WHERE revoked_at IS NULL")
            .fetch_one(&self.pool)
            .await
            .map_err(Self::map_err)?;
        let knowledge: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM knowledge WHERE deleted_at IS NULL")
                .fetch_one(&self.pool)
                .await
                .map_err(Self::map_err)?;
        let verified: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM knowledge WHERE deleted_at IS NULL AND verification_status IN ('reported_successful', 'independently_confirmed', 'verified_under_conditions')",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(Self::map_err)?;
        let verifs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM verification_records")
            .fetch_one(&self.pool)
            .await
            .map_err(Self::map_err)?;

        Ok(NetworkStatus {
            status: "ok".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            total_agents: agents as usize,
            total_knowledge: knowledge as usize,
            verified_knowledge: verified as usize,
            total_verifications: verifs as usize,
        })
    }

    async fn record_audit(
        &self,
        principal: &str,
        action: &str,
        resource_type: &str,
        resource_id: &str,
        allowed: bool,
        reason: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO audit_logs (id, principal, action, resource_type, resource_id, allowed, reason, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(Uuid::new_v4())
        .bind(principal)
        .bind(action)
        .bind(resource_type)
        .bind(resource_id)
        .bind(allowed)
        .bind(reason)
        .bind(Utc::now())
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;
        Ok(())
    }

    async fn list_audit_logs(&self, limit: usize) -> Result<Vec<AuditLogRecord>> {
        let rows = sqlx::query("SELECT * FROM audit_logs ORDER BY created_at DESC LIMIT $1")
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(Self::map_err)?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let id: Uuid = r.try_get("id").map_err(Self::map_err)?;
            out.push(AuditLogRecord {
                id: id.to_string(),
                principal: r.try_get("principal").map_err(Self::map_err)?,
                action: r.try_get("action").map_err(Self::map_err)?,
                resource_type: r.try_get("resource_type").map_err(Self::map_err)?,
                resource_id: r.try_get("resource_id").map_err(Self::map_err)?,
                allowed: r.try_get("allowed").map_err(Self::map_err)?,
                reason: r.try_get("reason").map_err(Self::map_err)?,
                created_at: r.try_get("created_at").map_err(Self::map_err)?,
            });
        }
        Ok(out)
    }

    async fn mark_federation_synced(&self, node_id: &str) -> Result<()> {
        let uuid = Self::parse_uuid(node_id)?;
        sqlx::query("UPDATE federation_nodes SET last_sync_at = NOW() WHERE id = $1")
            .bind(uuid)
            .execute(&self.pool)
            .await
            .map_err(Self::map_err)?;
        Ok(())
    }
}

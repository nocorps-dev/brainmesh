use crate::BrainMeshStore;
use async_trait::async_trait;
use brainmesh_core::{
    cosine_similarity, hash_api_key, new_uuid, validate_contribution, AgentRecord,
    AgentRegistrationRequest, AgentRegistrationResponse, AuditLogRecord, AuthContext,
    BrainMeshError, ChallengeKnowledgeRequest, ChallengeRecord, ContributeKnowledgeRequest,
    CreateRelationshipRequest, ExperienceRecord, ExtractExperienceRequest, FederationNodeRecord,
    FederationNodeRequest, KnowledgeCategory, KnowledgeObject, KnowledgeVersionRecord,
    LocalHashEmbeddingProvider, NetworkStatus, Permission, PromptInjectionGuard, Provenance,
    RecallMatch, RecallQuery, RelationshipRecord, RelationshipType, Result, ScoreExplanation,
    SecretRedactor, SubmitExperienceRequest, UpdateKnowledgeRequest, VerificationHistoryResponse,
    VerificationInfo, VerificationRecord, VerificationStatus, VerifyKnowledgeRequest, Visibility,
};
use chrono::{DateTime, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::str::FromStr;

const SQLITE_SCHEMA: &str = include_str!("../../../migrations/sqlite/0001_initial_schema.sql");

#[derive(Clone)]
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub async fn connect(database_url: &str) -> Result<Self> {
        let opts = SqliteConnectOptions::from_str(database_url)
            .map_err(|e| BrainMeshError::Storage(e.to_string()))?
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await
            .map_err(|e| BrainMeshError::Storage(e.to_string()))?;

        sqlx::raw_sql(SQLITE_SCHEMA)
            .execute(&pool)
            .await
            .map_err(|e| BrainMeshError::Storage(format!("migration failed: {e}")))?;

        Ok(Self { pool })
    }

    fn map_err(e: sqlx::Error) -> BrainMeshError {
        BrainMeshError::Storage(e.to_string())
    }

    fn parse_dt(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now())
    }

    fn row_to_knowledge(row: &sqlx::sqlite::SqliteRow) -> Result<KnowledgeObject> {
        let category_str: String = row.try_get("category").map_err(Self::map_err)?;
        let category: KnowledgeCategory =
            serde_json::from_str(&format!("\"{category_str}\"")).unwrap_or_default();

        let tags_json: String = row.try_get("tags_json").map_err(Self::map_err)?;
        let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();

        let content_json: String = row.try_get("content_json").map_err(Self::map_err)?;
        let content: serde_json::Value =
            serde_json::from_str(&content_json).unwrap_or(serde_json::Value::Null);

        let verif_status_str: String = row.try_get("verification_status").map_err(Self::map_err)?;
        let verif_status: VerificationStatus =
            serde_json::from_str(&format!("\"{verif_status_str}\"")).unwrap_or_default();

        let verif_ev_json: String = row
            .try_get("verification_evidence_json")
            .map_err(Self::map_err)?;
        let evidence: Vec<serde_json::Value> =
            serde_json::from_str(&verif_ev_json).unwrap_or_default();

        let vis_str: String = row.try_get("visibility").map_err(Self::map_err)?;
        let visibility: Visibility =
            serde_json::from_str(&format!("\"{vis_str}\"")).unwrap_or_default();

        let verified_at_str: Option<String> = row.try_get("verified_at").map_err(Self::map_err)?;
        let expires_at_str: Option<String> = row.try_get("expires_at").map_err(Self::map_err)?;
        let created_at_str: String = row.try_get("created_at").map_err(Self::map_err)?;
        let updated_at_str: String = row.try_get("updated_at").map_err(Self::map_err)?;

        let ver: i64 = row.try_get("version").map_err(Self::map_err)?;

        Ok(KnowledgeObject {
            id: row.try_get("id").map_err(Self::map_err)?,
            category,
            record_type: row.try_get("record_type").map_err(Self::map_err)?,
            title: row.try_get("title").map_err(Self::map_err)?,
            description: row.try_get("description").map_err(Self::map_err)?,
            domain: row.try_get("domain").map_err(Self::map_err)?,
            tags,
            content,
            content_hash: row.try_get("content_hash").map_err(Self::map_err)?,
            verification: VerificationInfo {
                status: verif_status,
                method: row.try_get("verification_method").map_err(Self::map_err)?,
                evidence,
                verified_at: verified_at_str.as_deref().map(Self::parse_dt),
            },
            provenance: Provenance {
                agent_id: row.try_get("agent_id").map_err(Self::map_err)?,
                framework: row.try_get("framework").map_err(Self::map_err)?,
                contributor_id: row.try_get("contributor_id").map_err(Self::map_err)?,
                organization_id: row.try_get("organization_id").map_err(Self::map_err)?,
                workspace_id: row.try_get("workspace_id").map_err(Self::map_err)?,
            },
            visibility,
            created_at: Self::parse_dt(&created_at_str),
            updated_at: Self::parse_dt(&updated_at_str),
            expires_at: expires_at_str.as_deref().map(Self::parse_dt),
            version: ver as u32,
        })
    }

    fn enum_str<T: serde::Serialize>(val: &T) -> String {
        serde_json::to_string(val)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string()
    }
}

#[async_trait]
impl BrainMeshStore for SqliteStore {
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
            "SELECT id, framework, organization_id, workspace_id FROM agents WHERE api_key_hash = ? AND revoked_at IS NULL",
        )
        .bind(&key_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(Self::map_err)?
        .ok_or_else(|| BrainMeshError::Authentication("invalid or revoked API key".into()))?;

        let agent_id: String = row.try_get("id").map_err(Self::map_err)?;
        let framework: String = row.try_get("framework").map_err(Self::map_err)?;
        let organization_id: Option<String> =
            row.try_get("organization_id").map_err(Self::map_err)?;
        let workspace_id: Option<String> = row.try_get("workspace_id").map_err(Self::map_err)?;

        Ok(AuthContext {
            agent_id: agent_id.clone(),
            framework,
            contributor_id: agent_id,
            organization_id,
            workspace_id,
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

        let id = new_uuid();
        let raw_key = format!("bm_{}", new_uuid().replace('-', ""));
        let key_hash = hash_api_key(&raw_key);
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        let caps_json = serde_json::to_string(&req.capabilities).unwrap_or_else(|_| "[]".into());

        sqlx::query(
            "INSERT INTO agents (id, organization_id, workspace_id, name, framework, model, capabilities_json, api_key_hash, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&req.organization_id)
        .bind(&req.workspace_id)
        .bind(&req.name)
        .bind(&req.framework)
        .bind(&req.model)
        .bind(&caps_json)
        .bind(&key_hash)
        .bind(&now_str)
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;

        Ok(AgentRegistrationResponse {
            agent: AgentRecord {
                id,
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
            "SELECT id, organization_id, workspace_id, name, framework, model, capabilities_json, created_at
             FROM agents WHERE revoked_at IS NULL ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_err)?;

        let mut agents = Vec::with_capacity(rows.len());
        for row in rows {
            let caps_json: String = row.try_get("capabilities_json").map_err(Self::map_err)?;
            let created_str: String = row.try_get("created_at").map_err(Self::map_err)?;
            agents.push(AgentRecord {
                id: row.try_get("id").map_err(Self::map_err)?,
                name: row.try_get("name").map_err(Self::map_err)?,
                framework: row.try_get("framework").map_err(Self::map_err)?,
                model: row.try_get("model").map_err(Self::map_err)?,
                organization_id: row.try_get("organization_id").map_err(Self::map_err)?,
                workspace_id: row.try_get("workspace_id").map_err(Self::map_err)?,
                capabilities: serde_json::from_str(&caps_json).unwrap_or_default(),
                created_at: Self::parse_dt(&created_str),
            });
        }
        Ok(agents)
    }

    async fn get_agent(&self, _ctx: &AuthContext, id: &str) -> Result<AgentRecord> {
        let row = sqlx::query(
            "SELECT id, organization_id, workspace_id, name, framework, model, capabilities_json, created_at
             FROM agents WHERE id = ? AND revoked_at IS NULL",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Self::map_err)?
        .ok_or_else(|| BrainMeshError::NotFound(format!("agent '{id}' not found")))?;

        let caps_json: String = row.try_get("capabilities_json").map_err(Self::map_err)?;
        let created_str: String = row.try_get("created_at").map_err(Self::map_err)?;
        Ok(AgentRecord {
            id: row.try_get("id").map_err(Self::map_err)?,
            name: row.try_get("name").map_err(Self::map_err)?,
            framework: row.try_get("framework").map_err(Self::map_err)?,
            model: row.try_get("model").map_err(Self::map_err)?,
            organization_id: row.try_get("organization_id").map_err(Self::map_err)?,
            workspace_id: row.try_get("workspace_id").map_err(Self::map_err)?,
            capabilities: serde_json::from_str(&caps_json).unwrap_or_default(),
            created_at: Self::parse_dt(&created_str),
        })
    }

    async fn delete_agent(&self, ctx: &AuthContext, id: &str) -> Result<()> {
        if !ctx.has_permission(Permission::ManageAgents) && ctx.agent_id != id {
            return Err(BrainMeshError::Authorization(
                "cannot revoke another agent".into(),
            ));
        }
        let now = Utc::now().to_rfc3339();
        let res = sqlx::query("UPDATE agents SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL")
            .bind(&now)
            .bind(id)
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

        // Deduplication check for same contributor/hash
        if let Some(existing) = sqlx::query(
            "SELECT * FROM knowledge WHERE content_hash = ? AND agent_id = ? AND deleted_at IS NULL LIMIT 1",
        )
        .bind(&content_hash)
        .bind(&ctx.agent_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(Self::map_err)?
        {
            return Self::row_to_knowledge(&existing);
        }

        let id = new_uuid();
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        let category_str = Self::enum_str(&req.category);
        let vis_str = Self::enum_str(&req.visibility);
        let tags_json = serde_json::to_string(&req.tags).unwrap_or_else(|_| "[]".into());
        let content_json = serde_json::to_string(&safe_content).unwrap_or_else(|_| "{}".into());

        let verif = req.verification.unwrap_or_default();
        let verif_status_str = Self::enum_str(&verif.status);
        let verif_ev_json = serde_json::to_string(&verif.evidence).unwrap_or_else(|_| "[]".into());
        let verified_at_str = verif.verified_at.map(|d| d.to_rfc3339());
        let expires_at_str = req.expires_at.map(|d| d.to_rfc3339());

        sqlx::query(
            "INSERT INTO knowledge (
                id, organization_id, workspace_id, category, record_type, title, description, domain,
                tags_json, content_json, content_hash, verification_status, verification_method,
                verification_evidence_json, verified_at, agent_id, framework, contributor_id,
                visibility, version, created_at, updated_at, expires_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&ctx.organization_id)
        .bind(&ctx.workspace_id)
        .bind(&category_str)
        .bind(&req.record_type)
        .bind(&safe_title)
        .bind(&safe_desc)
        .bind(&req.domain)
        .bind(&tags_json)
        .bind(&content_json)
        .bind(&content_hash)
        .bind(&verif_status_str)
        .bind(&verif.method)
        .bind(&verif_ev_json)
        .bind(&verified_at_str)
        .bind(&ctx.agent_id)
        .bind(&ctx.framework)
        .bind(&ctx.contributor_id)
        .bind(&vis_str)
        .bind(&now_str)
        .bind(&now_str)
        .bind(&expires_at_str)
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;

        // Record initial version
        sqlx::query(
            "INSERT INTO knowledge_versions (id, knowledge_id, version, title, description, content_json, changed_by, created_at)
             VALUES (?, ?, 1, ?, ?, ?, ?, ?)",
        )
        .bind(new_uuid())
        .bind(&id)
        .bind(&safe_title)
        .bind(&safe_desc)
        .bind(&content_json)
        .bind(&ctx.agent_id)
        .bind(&now_str)
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;

        // Index in FTS5
        sqlx::query(
            "INSERT INTO knowledge_fts (knowledge_id, title, description, domain, tags, content_text)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&safe_title)
        .bind(&safe_desc)
        .bind(&req.domain)
        .bind(req.tags.join(" "))
        .bind(&content_json)
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;

        self.get_knowledge(ctx, &id).await
    }

    async fn get_knowledge(&self, ctx: &AuthContext, id: &str) -> Result<KnowledgeObject> {
        let row = sqlx::query("SELECT * FROM knowledge WHERE id = ? AND deleted_at IS NULL")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(Self::map_err)?
            .ok_or_else(|| BrainMeshError::NotFound(format!("knowledge '{id}' not found")))?;

        let item = Self::row_to_knowledge(&row)?;
        if !ctx.can_access_knowledge(&item) {
            return Err(BrainMeshError::Authorization(format!(
                "knowledge '{id}' is not accessible to agent '{}'",
                ctx.agent_id
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
            return Err(BrainMeshError::Authorization(
                "only the contributing agent or admin may update knowledge".into(),
            ));
        }

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
        let tags = req.tags.unwrap_or(existing.tags);
        let visibility = req.visibility.unwrap_or(existing.visibility);
        let new_version = existing.version + 1;
        let now = Utc::now().to_rfc3339();
        let content_json = serde_json::to_string(&content).unwrap_or_else(|_| "{}".into());
        let tags_json = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".into());
        let vis_str = Self::enum_str(&visibility);

        sqlx::query(
            "UPDATE knowledge SET title = ?, description = ?, content_json = ?, tags_json = ?, visibility = ?, version = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(&title)
        .bind(&description)
        .bind(&content_json)
        .bind(&tags_json)
        .bind(&vis_str)
        .bind(new_version as i64)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;

        sqlx::query(
            "INSERT INTO knowledge_versions (id, knowledge_id, version, title, description, content_json, changed_by, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(new_uuid())
        .bind(id)
        .bind(new_version as i64)
        .bind(&title)
        .bind(&description)
        .bind(&content_json)
        .bind(&ctx.agent_id)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;

        self.get_knowledge(ctx, id).await
    }

    async fn delete_knowledge(&self, ctx: &AuthContext, id: &str) -> Result<()> {
        let existing = self.get_knowledge(ctx, id).await?;
        if existing.provenance.agent_id != ctx.agent_id && !ctx.has_permission(Permission::Admin) {
            return Err(BrainMeshError::Authorization(
                "only the contributing agent or admin may delete knowledge".into(),
            ));
        }
        let now = Utc::now().to_rfc3339();
        sqlx::query("UPDATE knowledge SET deleted_at = ? WHERE id = ?")
            .bind(&now)
            .bind(id)
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
            "SELECT * FROM knowledge WHERE deleted_at IS NULL ORDER BY updated_at DESC LIMIT ? OFFSET ?",
        )
        .bind((limit.max(1) * 3) as i64)
        .bind(offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_err)?;

        let mut items = Vec::new();
        for row in rows {
            let item = Self::row_to_knowledge(&row)?;
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
        let rows = sqlx::query(
            "SELECT * FROM knowledge_versions WHERE knowledge_id = ? ORDER BY version ASC",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_err)?;

        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let ver: i64 = r.try_get("version").map_err(Self::map_err)?;
            let content_str: String = r.try_get("content_json").map_err(Self::map_err)?;
            let created_str: String = r.try_get("created_at").map_err(Self::map_err)?;
            out.push(KnowledgeVersionRecord {
                id: r.try_get("id").map_err(Self::map_err)?,
                knowledge_id: r.try_get("knowledge_id").map_err(Self::map_err)?,
                version: ver as u32,
                title: r.try_get("title").map_err(Self::map_err)?,
                description: r.try_get("description").map_err(Self::map_err)?,
                content: serde_json::from_str(&content_str).unwrap_or_default(),
                changed_by: r.try_get("changed_by").map_err(Self::map_err)?,
                created_at: Self::parse_dt(&created_str),
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
        for row in rows {
            let item = Self::row_to_knowledge(&row)?;
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

            let haystack_raw = format!(
                "{} {} {} {} {}",
                item.title,
                item.description,
                item.domain,
                item.tags.join(" "),
                item.content
            );
            let haystack = haystack_raw.to_lowercase();

            let matched_terms = if terms.is_empty() {
                1usize
            } else {
                terms.iter().filter(|t| haystack.contains(t.as_str())).count()
            };
            let sem_sim = cosine_similarity(&query_vec, &embedder.embed_one(&haystack_raw));

            if !terms.is_empty() && matched_terms == 0 && sem_sim < 0.15 {
                continue;
            }

            let text_score = if terms.is_empty() {
                0.5
            } else {
                ((matched_terms as f64) / (terms.len() as f64)) * 0.7 + sem_sim.max(0.0) * 0.3
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
        if !ctx.has_permission(Permission::SubmitExperience) {
            return Err(BrainMeshError::Authorization(
                "lacks submit_experience permission".into(),
            ));
        }

        let id = new_uuid();
        let now = Utc::now();
        let safe_title = SecretRedactor::redact(&req.title);
        let safe_outcome = SecretRedactor::redact(&req.outcome);
        let safe_summary = SecretRedactor::redact(&req.summary);
        let safe_events: Vec<serde_json::Value> = req
            .events
            .iter()
            .map(SecretRedactor::redact_json)
            .collect();
        let events_json = serde_json::to_string(&safe_events).unwrap_or_else(|_| "[]".into());

        sqlx::query(
            "INSERT INTO experiences (id, organization_id, workspace_id, agent_id, framework, title, outcome, summary, raw_events_json, redacted, approved_for_publication, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?)",
        )
        .bind(&id)
        .bind(&ctx.organization_id)
        .bind(&ctx.workspace_id)
        .bind(&ctx.agent_id)
        .bind(&ctx.framework)
        .bind(&safe_title)
        .bind(&safe_outcome)
        .bind(&safe_summary)
        .bind(&events_json)
        .bind(if req.approved_for_publication { 1i64 } else { 0i64 })
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;

        Ok(ExperienceRecord {
            id,
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
        let row = sqlx::query("SELECT * FROM experiences WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(Self::map_err)?
            .ok_or_else(|| BrainMeshError::NotFound(format!("experience '{id}' not found")))?;

        let agent_id: String = row.try_get("agent_id").map_err(Self::map_err)?;
        if agent_id != ctx.agent_id && !ctx.has_permission(Permission::Admin) {
            return Err(BrainMeshError::Authorization(
                "cannot access another agent's raw experience".into(),
            ));
        }

        let events_str: String = row.try_get("raw_events_json").map_err(Self::map_err)?;
        let approved: i64 = row
            .try_get("approved_for_publication")
            .map_err(Self::map_err)?;
        let created_str: String = row.try_get("created_at").map_err(Self::map_err)?;

        Ok(ExperienceRecord {
            id: row.try_get("id").map_err(Self::map_err)?,
            agent_id,
            framework: row.try_get("framework").map_err(Self::map_err)?,
            title: row.try_get("title").map_err(Self::map_err)?,
            outcome: row.try_get("outcome").map_err(Self::map_err)?,
            summary: row.try_get("summary").map_err(Self::map_err)?,
            events: serde_json::from_str(&events_str).unwrap_or_default(),
            redacted: true,
            approved_for_publication: approved != 0,
            extracted_knowledge_id: row
                .try_get("extracted_knowledge_id")
                .map_err(Self::map_err)?,
            created_at: Self::parse_dt(&created_str),
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
                "explicit user approval is required to publish extracted knowledge beyond private visibility".into(),
            ));
        }

        let contrib = ContributeKnowledgeRequest {
            category: KnowledgeCategory::Lesson,
            record_type: "extracted_lesson".to_string(),
            title: exp.title.clone(),
            description: exp.summary.clone(),
            domain: req.domain,
            tags: req.tags,
            content: serde_json::json!({
                "outcome": exp.outcome,
                "summary": exp.summary,
                "source_experience_id": exp.id,
            }),
            verification: None,
            visibility: req.visibility,
            expires_at: None,
        };

        let created = self.contribute_knowledge(ctx, contrib).await?;
        sqlx::query("UPDATE experiences SET extracted_knowledge_id = ? WHERE id = ?")
            .bind(&created.id)
            .bind(id)
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
        if !ctx.has_permission(Permission::Verify) {
            return Err(BrainMeshError::Authorization(
                "lacks verify permission".into(),
            ));
        }
        let item = self.get_knowledge(ctx, id).await?;

        // Promote to IndependentlyConfirmed if verified by a different agent
        let effective_status = if item.provenance.agent_id != ctx.agent_id
            && matches!(
                req.status,
                VerificationStatus::ReportedSuccessful | VerificationStatus::IndependentlyConfirmed
            ) {
            VerificationStatus::IndependentlyConfirmed
        } else {
            req.status
        };

        let verif_id = new_uuid();
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        let ev_json = serde_json::to_string(&req.evidence).unwrap_or_else(|_| "[]".into());
        let status_str = Self::enum_str(&effective_status);

        sqlx::query(
            "INSERT INTO verification_records (id, knowledge_id, verifier_agent_id, status, method, evidence_json, conditions, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&verif_id)
        .bind(id)
        .bind(&ctx.agent_id)
        .bind(&status_str)
        .bind(&req.method)
        .bind(&ev_json)
        .bind(&req.conditions)
        .bind(&now_str)
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;

        sqlx::query(
            "UPDATE knowledge SET verification_status = ?, verification_method = ?, verified_at = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(&status_str)
        .bind(&req.method)
        .bind(&now_str)
        .bind(&now_str)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;

        Ok(VerificationRecord {
            id: verif_id,
            knowledge_id: id.to_string(),
            verifier_agent_id: ctx.agent_id.clone(),
            status: effective_status,
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
        if !ctx.has_permission(Permission::Challenge) {
            return Err(BrainMeshError::Authorization(
                "lacks challenge permission".into(),
            ));
        }
        let _ = self.get_knowledge(ctx, id).await?;

        let ch_id = new_uuid();
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        let ev_json = serde_json::to_string(&req.evidence).unwrap_or_else(|_| "[]".into());

        sqlx::query(
            "INSERT INTO challenges (id, knowledge_id, challenger_agent_id, reason, evidence_json, status, created_at)
             VALUES (?, ?, ?, ?, ?, 'open', ?)",
        )
        .bind(&ch_id)
        .bind(id)
        .bind(&ctx.agent_id)
        .bind(&req.reason)
        .bind(&ev_json)
        .bind(&now_str)
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;

        sqlx::query("UPDATE knowledge SET verification_status = 'disputed', updated_at = ? WHERE id = ?")
            .bind(&now_str)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(Self::map_err)?;

        Ok(ChallengeRecord {
            id: ch_id,
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
        let v_rows = sqlx::query(
            "SELECT * FROM verification_records WHERE knowledge_id = ? ORDER BY created_at ASC",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_err)?;

        let mut verifications = Vec::with_capacity(v_rows.len());
        for r in v_rows {
            let st_str: String = r.try_get("status").map_err(Self::map_err)?;
            let ev_str: String = r.try_get("evidence_json").map_err(Self::map_err)?;
            let created_str: String = r.try_get("created_at").map_err(Self::map_err)?;
            verifications.push(VerificationRecord {
                id: r.try_get("id").map_err(Self::map_err)?,
                knowledge_id: r.try_get("knowledge_id").map_err(Self::map_err)?,
                verifier_agent_id: r.try_get("verifier_agent_id").map_err(Self::map_err)?,
                status: serde_json::from_str(&format!("\"{st_str}\"")).unwrap_or_default(),
                method: r.try_get("method").map_err(Self::map_err)?,
                evidence: serde_json::from_str(&ev_str).unwrap_or_default(),
                conditions: r.try_get("conditions").map_err(Self::map_err)?,
                created_at: Self::parse_dt(&created_str),
            });
        }

        let c_rows = sqlx::query(
            "SELECT * FROM challenges WHERE knowledge_id = ? ORDER BY created_at ASC",
        )
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_err)?;

        let mut challenges = Vec::with_capacity(c_rows.len());
        for r in c_rows {
            let ev_str: String = r.try_get("evidence_json").map_err(Self::map_err)?;
            let created_str: String = r.try_get("created_at").map_err(Self::map_err)?;
            challenges.push(ChallengeRecord {
                id: r.try_get("id").map_err(Self::map_err)?,
                knowledge_id: r.try_get("knowledge_id").map_err(Self::map_err)?,
                challenger_agent_id: r.try_get("challenger_agent_id").map_err(Self::map_err)?,
                reason: r.try_get("reason").map_err(Self::map_err)?,
                evidence: serde_json::from_str(&ev_str).unwrap_or_default(),
                status: r.try_get("status").map_err(Self::map_err)?,
                created_at: Self::parse_dt(&created_str),
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

        let id = new_uuid();
        let now = Utc::now();
        let rel_str = Self::enum_str(&req.relationship_type);
        let meta_str = serde_json::to_string(&req.metadata).unwrap_or_else(|_| "{}".into());

        sqlx::query(
            "INSERT INTO knowledge_relationships (id, source_id, target_id, relationship_type, metadata_json, created_by, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(source_id)
        .bind(&req.target_id)
        .bind(&rel_str)
        .bind(&meta_str)
        .bind(&ctx.agent_id)
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;

        Ok(RelationshipRecord {
            id,
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
        let rows = sqlx::query(
            "SELECT * FROM knowledge_relationships WHERE source_id = ? OR target_id = ?",
        )
        .bind(id)
        .bind(id)
        .fetch_all(&self.pool)
        .await
        .map_err(Self::map_err)?;

        let mut rels = Vec::new();
        let mut related_items = Vec::new();
        for r in rows {
            let source_id: String = r.try_get("source_id").map_err(Self::map_err)?;
            let target_id: String = r.try_get("target_id").map_err(Self::map_err)?;
            let rel_str: String = r.try_get("relationship_type").map_err(Self::map_err)?;
            let meta_str: String = r.try_get("metadata_json").map_err(Self::map_err)?;
            let created_str: String = r.try_get("created_at").map_err(Self::map_err)?;

            let other_id = if source_id == id { &target_id } else { &source_id };
            if let Ok(other_item) = self.get_knowledge(ctx, other_id).await {
                related_items.push(other_item);
                rels.push(RelationshipRecord {
                    id: r.try_get("id").map_err(Self::map_err)?,
                    source_id,
                    target_id,
                    relationship_type: serde_json::from_str(&format!("\"{rel_str}\""))
                        .unwrap_or(RelationshipType::RelatedTo),
                    metadata: serde_json::from_str(&meta_str).unwrap_or_default(),
                    created_by: r.try_get("created_by").map_err(Self::map_err)?,
                    created_at: Self::parse_dt(&created_str),
                });
            }
        }
        Ok((rels, related_items))
    }

    async fn register_federation_node(
        &self,
        ctx: &AuthContext,
        req: FederationNodeRequest,
    ) -> Result<FederationNodeRecord> {
        if !ctx.has_permission(Permission::ManageFederation) {
            return Err(BrainMeshError::Authorization(
                "lacks manage_federation permission".into(),
            ));
        }
        let id = new_uuid();
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO federation_nodes (id, name, url, trust_tier, enabled, created_at)
             VALUES (?, ?, ?, ?, 1, ?)",
        )
        .bind(&id)
        .bind(&req.name)
        .bind(&req.url)
        .bind(&req.trust_tier)
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;

        Ok(FederationNodeRecord {
            id,
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
            let enabled: i64 = r.try_get("enabled").map_err(Self::map_err)?;
            let last_sync: Option<String> = r.try_get("last_sync_at").map_err(Self::map_err)?;
            let created_str: String = r.try_get("created_at").map_err(Self::map_err)?;
            out.push(FederationNodeRecord {
                id: r.try_get("id").map_err(Self::map_err)?,
                name: r.try_get("name").map_err(Self::map_err)?,
                url: r.try_get("url").map_err(Self::map_err)?,
                trust_tier: r.try_get("trust_tier").map_err(Self::map_err)?,
                enabled: enabled != 0,
                last_sync_at: last_sync.as_deref().map(Self::parse_dt),
                created_at: Self::parse_dt(&created_str),
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
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(new_uuid())
        .bind(principal)
        .bind(action)
        .bind(resource_type)
        .bind(resource_id)
        .bind(if allowed { 1i64 } else { 0i64 })
        .bind(reason)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(Self::map_err)?;
        Ok(())
    }

    async fn list_audit_logs(&self, limit: usize) -> Result<Vec<AuditLogRecord>> {
        let rows = sqlx::query("SELECT * FROM audit_logs ORDER BY created_at DESC LIMIT ?")
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(Self::map_err)?;

        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let allowed: i64 = r.try_get("allowed").map_err(Self::map_err)?;
            let created_str: String = r.try_get("created_at").map_err(Self::map_err)?;
            out.push(AuditLogRecord {
                id: r.try_get("id").map_err(Self::map_err)?,
                principal: r.try_get("principal").map_err(Self::map_err)?,
                action: r.try_get("action").map_err(Self::map_err)?,
                resource_type: r.try_get("resource_type").map_err(Self::map_err)?,
                resource_id: r.try_get("resource_id").map_err(Self::map_err)?,
                allowed: allowed != 0,
                reason: r.try_get("reason").map_err(Self::map_err)?,
                created_at: Self::parse_dt(&created_str),
            });
        }
        Ok(out)
    }

    async fn mark_federation_synced(&self, node_id: &str) -> Result<()> {
        sqlx::query("UPDATE federation_nodes SET last_sync_at = ? WHERE id = ?")
            .bind(Utc::now().to_rfc3339())
            .bind(node_id)
            .execute(&self.pool)
            .await
            .map_err(Self::map_err)?;
        Ok(())
    }
}

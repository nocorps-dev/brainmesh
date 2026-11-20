use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use brainmesh_core::{
    AgentRegistrationRequest, AuthContext, BrainMeshError, ChallengeKnowledgeRequest,
    ContributeKnowledgeRequest, CreateRelationshipRequest, ExtractExperienceRequest,
    FederationNodeRequest, FederationSyncResult, KnowledgeCategory, RateLimiter, RecallQuery,
    SubmitExperienceRequest, UpdateKnowledgeRequest, VerifyKnowledgeRequest, Visibility,
};
use brainmesh_mcp::mcp_router;
use brainmesh_storage::BrainMeshStore;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<dyn BrainMeshStore>,
    pub rate_limiter: Arc<RateLimiter>,
    pub require_auth: bool,
}

pub fn api_router(store: Arc<dyn BrainMeshStore>, require_auth: bool) -> Router {
    let state = AppState {
        store: Arc::clone(&store),
        rate_limiter: Arc::new(RateLimiter::default()),
        require_auth,
    };

    let v1 = Router::new()
        // Agents
        .route("/agents/register", post(register_agent))
        .route("/agents", get(list_agents))
        .route("/agents/{id}", get(get_agent).delete(delete_agent))
        // Knowledge
        .route("/knowledge", post(contribute_knowledge).get(list_knowledge))
        .route(
            "/knowledge/{id}",
            get(get_knowledge).patch(update_knowledge).delete(delete_knowledge),
        )
        .route("/knowledge/{id}/history", get(knowledge_history))
        // Recall
        .route("/recall", post(recall))
        .route("/recall/similar/{id}", get(recall_similar))
        // Experiences
        .route("/experiences", post(submit_experience))
        .route("/experiences/{id}", get(get_experience))
        .route("/experiences/{id}/extract", post(extract_experience))
        // Verification
        .route("/knowledge/{id}/verify", post(verify_knowledge))
        .route("/knowledge/{id}/challenge", post(challenge_knowledge))
        .route("/knowledge/{id}/verification", get(get_verification_history))
        // Relationships
        .route("/knowledge/{id}/relationships", post(create_relationship))
        .route("/knowledge/{id}/related", get(get_related))
        // Federation
        .route(
            "/federation/nodes",
            post(register_federation_node).get(list_federation_nodes),
        )
        .route("/federation/sync", post(trigger_federation_sync))
        .route("/federation/status", get(federation_status))
        // System
        .route("/audit", get(list_audit_logs))
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/version", get(version))
        .route("/metrics", get(metrics))
        .route("/openapi.json", get(openapi))
        .with_state(state.clone());

    let a2a = Router::new()
        .route("/.well-known/agent.json", get(a2a_agent_card))
        .route("/a2a/tasks/send", post(a2a_send_task))
        .with_state(state);

    Router::new()
        .nest("/api/v1", v1)
        .merge(a2a)
        .merge(mcp_router(store, require_auth))
}

async fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<AuthContext, BrainMeshError> {
    let ctx = if !state.require_auth {
        AuthContext::local_admin()
    } else {
        let raw = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer ").or_else(|| s.strip_prefix("bearer ")))
            .map(str::trim)
            .ok_or_else(|| BrainMeshError::Authentication("missing Authorization Bearer token".into()))?;
        state.store.authenticate_api_key(raw).await?
    };

    state.rate_limiter.check(&ctx.agent_id, 120)?;
    Ok(ctx)
}

fn err_resp(err: BrainMeshError) -> Response {
    let status =
        StatusCode::from_u16(err.http_status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (
        status,
        Json(serde_json::json!({
            "error": { "code": err.code(), "message": err.to_string() }
        })),
    )
        .into_response()
}

async fn register_agent(
    State(state): State<AppState>,
    Json(req): Json<AgentRegistrationRequest>,
) -> Response {
    match state.store.register_agent(req).await {
        Ok(res) => (StatusCode::CREATED, Json(res)).into_response(),
        Err(e) => err_resp(e),
    }
}

async fn list_agents(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return err_resp(e),
    };
    match state.store.list_agents(&ctx).await {
        Ok(list) => (StatusCode::OK, Json(list)).into_response(),
        Err(e) => err_resp(e),
    }
}

async fn get_agent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return err_resp(e),
    };
    match state.store.get_agent(&ctx, &id).await {
        Ok(a) => (StatusCode::OK, Json(a)).into_response(),
        Err(e) => err_resp(e),
    }
}

async fn delete_agent(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return err_resp(e),
    };
    match state.store.delete_agent(&ctx, &id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => err_resp(e),
    }
}

async fn contribute_knowledge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ContributeKnowledgeRequest>,
) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return err_resp(e),
    };
    match state.store.contribute_knowledge(&ctx, req).await {
        Ok(k) => (StatusCode::CREATED, Json(k)).into_response(),
        Err(e) => err_resp(e),
    }
}

async fn get_knowledge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return err_resp(e),
    };
    match state.store.get_knowledge(&ctx, &id).await {
        Ok(k) => (StatusCode::OK, Json(k)).into_response(),
        Err(e) => err_resp(e),
    }
}

async fn update_knowledge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<UpdateKnowledgeRequest>,
) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return err_resp(e),
    };
    match state.store.update_knowledge(&ctx, &id, req).await {
        Ok(k) => (StatusCode::OK, Json(k)).into_response(),
        Err(e) => err_resp(e),
    }
}

async fn delete_knowledge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return err_resp(e),
    };
    match state.store.delete_knowledge(&ctx, &id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => err_resp(e),
    }
}

#[derive(Deserialize)]
struct ListKnowledgeParams {
    domain: Option<String>,
    visibility: Option<Visibility>,
    limit: Option<usize>,
    offset: Option<usize>,
}

async fn list_knowledge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ListKnowledgeParams>,
) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return err_resp(e),
    };
    match state
        .store
        .list_knowledge(
            &ctx,
            q.domain,
            q.visibility,
            q.limit.unwrap_or(20),
            q.offset.unwrap_or(0),
        )
        .await
    {
        Ok(items) => (StatusCode::OK, Json(items)).into_response(),
        Err(e) => err_resp(e),
    }
}

async fn knowledge_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return err_resp(e),
    };
    match state.store.get_knowledge_history(&ctx, &id).await {
        Ok(h) => (StatusCode::OK, Json(h)).into_response(),
        Err(e) => err_resp(e),
    }
}

async fn recall(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(query): Json<RecallQuery>,
) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return err_resp(e),
    };
    match state.store.recall(&ctx, query).await {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(e) => err_resp(e),
    }
}

async fn recall_similar(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return err_resp(e),
    };
    match state.store.recall_similar(&ctx, &id, 10).await {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(e) => err_resp(e),
    }
}

async fn submit_experience(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<SubmitExperienceRequest>,
) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return err_resp(e),
    };
    match state.store.submit_experience(&ctx, req).await {
        Ok(res) => (StatusCode::CREATED, Json(res)).into_response(),
        Err(e) => err_resp(e),
    }
}

async fn get_experience(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return err_resp(e),
    };
    match state.store.get_experience(&ctx, &id).await {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(e) => err_resp(e),
    }
}

async fn extract_experience(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<ExtractExperienceRequest>,
) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return err_resp(e),
    };
    match state.store.extract_experience(&ctx, &id, req).await {
        Ok(res) => (StatusCode::CREATED, Json(res)).into_response(),
        Err(e) => err_resp(e),
    }
}

async fn verify_knowledge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<VerifyKnowledgeRequest>,
) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return err_resp(e),
    };
    match state.store.verify_knowledge(&ctx, &id, req).await {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(e) => err_resp(e),
    }
}

async fn challenge_knowledge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<ChallengeKnowledgeRequest>,
) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return err_resp(e),
    };
    match state.store.challenge_knowledge(&ctx, &id, req).await {
        Ok(res) => (StatusCode::CREATED, Json(res)).into_response(),
        Err(e) => err_resp(e),
    }
}

async fn get_verification_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return err_resp(e),
    };
    match state.store.get_verification_history(&ctx, &id).await {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(e) => err_resp(e),
    }
}

async fn create_relationship(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(req): Json<CreateRelationshipRequest>,
) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return err_resp(e),
    };
    match state.store.create_relationship(&ctx, &id, req).await {
        Ok(res) => (StatusCode::CREATED, Json(res)).into_response(),
        Err(e) => err_resp(e),
    }
}

async fn get_related(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return err_resp(e),
    };
    match state.store.get_related(&ctx, &id).await {
        Ok((rels, items)) => (
            StatusCode::OK,
            Json(serde_json::json!({ "relationships": rels, "related": items })),
        )
            .into_response(),
        Err(e) => err_resp(e),
    }
}

async fn register_federation_node(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<FederationNodeRequest>,
) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return err_resp(e),
    };
    match state.store.register_federation_node(&ctx, req).await {
        Ok(res) => (StatusCode::CREATED, Json(res)).into_response(),
        Err(e) => err_resp(e),
    }
}

async fn list_federation_nodes(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return err_resp(e),
    };
    match state.store.list_federation_nodes(&ctx).await {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(e) => err_resp(e),
    }
}

async fn trigger_federation_sync(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return err_resp(e),
    };
    let nodes = state
        .store
        .list_federation_nodes(&ctx)
        .await
        .unwrap_or_default();
    let http = reqwest::Client::new();
    let mut imported = 0usize;
    let mut errors = Vec::new();
    let mut contacted = 0usize;

    for node in nodes.into_iter().filter(|n| n.enabled) {
        contacted += 1;
        let url = format!(
            "{}/api/v1/knowledge?visibility=public&limit=50",
            node.url.trim_end_matches('/')
        );
        match http.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(items) = resp.json::<Vec<brainmesh_core::KnowledgeObject>>().await {
                    for item in items {
                        if item.visibility != Visibility::Public
                            && item.visibility != Visibility::TrustedNetwork
                        {
                            continue;
                        }
                        let req = ContributeKnowledgeRequest {
                            category: item.category,
                            record_type: item.record_type,
                            title: item.title,
                            description: item.description,
                            domain: item.domain,
                            tags: item.tags,
                            content: item.content,
                            verification: Some(item.verification),
                            visibility: item.visibility,
                            expires_at: item.expires_at,
                        };
                        if state.store.contribute_knowledge(&ctx, req).await.is_ok() {
                            imported += 1;
                        }
                    }
                    let _ = state.store.mark_federation_synced(&node.id).await;
                }
            }
            Ok(resp) => errors.push(format!("node '{}' returned HTTP {}", node.name, resp.status())),
            Err(e) => errors.push(format!("node '{}' unreachable: {e}", node.name)),
        }
    }

    (
        StatusCode::OK,
        Json(FederationSyncResult {
            nodes_contacted: contacted,
            items_imported: imported,
            errors,
        }),
    )
        .into_response()
}

async fn list_audit_logs(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(e) = authenticate(&state, &headers).await {
        return err_resp(e);
    }
    match state.store.list_audit_logs(100).await {
        Ok(logs) => (StatusCode::OK, Json(logs)).into_response(),
        Err(e) => err_resp(e),
    }
}

async fn a2a_agent_card() -> Response {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "name": "BrainMesh",
            "description": "Collective Intelligence Infrastructure for AI Agents",
            "version": env!("CARGO_PKG_VERSION"),
            "capabilities": { "async_tasks": false, "cancellation": false, "state_transition_history": true },
            "authentication": ["Bearer"],
            "skills": [
                { "id": "recall", "name": "Hybrid Knowledge Recall", "description": "Retrieve verified agent discoveries with provenance." },
                { "id": "contribute", "name": "Knowledge Contribution", "description": "Share structured solutions and lessons." },
                { "id": "verify", "name": "Evidence Verification", "description": "Submit independent verification evidence." }
            ]
        })),
    )
        .into_response()
}

#[derive(Deserialize)]
struct A2aTaskBody {
    capability: String,
    query: Option<String>,
    title: Option<String>,
    description: Option<String>,
    domain: Option<String>,
    content: Option<serde_json::Value>,
    visibility: Option<Visibility>,
}

async fn a2a_send_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<A2aTaskBody>,
) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return err_resp(e),
    };
    match body.capability.as_str() {
        "recall" => {
            let q = RecallQuery {
                query: body.query.unwrap_or_default(),
                domain: body.domain,
                framework: None,
                tags: Vec::new(),
                verification_status: None,
                limit: 10,
            };
            match state.store.recall(&ctx, q).await {
                Ok(matches) => (StatusCode::OK, Json(serde_json::json!({ "status": "completed", "matches": matches }))).into_response(),
                Err(e) => err_resp(e),
            }
        }
        "contribute" => {
            let req = ContributeKnowledgeRequest {
                category: KnowledgeCategory::Knowledge,
                record_type: "knowledge".into(),
                title: body.title.unwrap_or_else(|| "A2A Contribution".into()),
                description: body.description.unwrap_or_default(),
                domain: body.domain.unwrap_or_else(|| "general".into()),
                tags: Vec::new(),
                content: body.content.unwrap_or_else(|| serde_json::json!({})),
                verification: None,
                visibility: body.visibility.unwrap_or(Visibility::Private),
                expires_at: None,
            };
            match state.store.contribute_knowledge(&ctx, req).await {
                Ok(item) => (StatusCode::CREATED, Json(serde_json::json!({ "status": "completed", "knowledge": item }))).into_response(),
                Err(e) => err_resp(e),
            }
        }
        other => err_resp(BrainMeshError::Validation(format!(
            "unsupported A2A capability: {other}"
        ))),
    }
}

async fn federation_status(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let ctx = match authenticate(&state, &headers).await {
        Ok(c) => c,
        Err(e) => return err_resp(e),
    };
    let nodes = state.store.list_federation_nodes(&ctx).await.unwrap_or_default();
    (
        StatusCode::OK,
        Json(serde_json::json!({ "status": "ready", "nodes": nodes.len() })),
    )
        .into_response()
}

async fn health() -> Response {
    (StatusCode::OK, Json(serde_json::json!({ "status": "ok" }))).into_response()
}

async fn ready(State(state): State<AppState>) -> Response {
    match state.store.health_check().await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "status": "ready" }))).into_response(),
        Err(e) => err_resp(e),
    }
}

async fn version() -> Response {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "name": "brainmesh",
            "version": env!("CARGO_PKG_VERSION"),
            "schema_version": 1
        })),
    )
        .into_response()
}

async fn metrics(State(state): State<AppState>) -> Response {
    match state.store.network_status().await {
        Ok(s) => (StatusCode::OK, Json(s)).into_response(),
        Err(e) => err_resp(e),
    }
}

async fn openapi() -> Response {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "openapi": "3.1.0",
            "info": {
                "title": "BrainMesh Collective Intelligence API",
                "version": env!("CARGO_PKG_VERSION")
            },
            "servers": [{ "url": "/api/v1" }]
        })),
    )
        .into_response()
}

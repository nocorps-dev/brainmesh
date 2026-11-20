use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use brainmesh_core::{
    AgentRegistrationRequest, AuthContext, BrainMeshError, ContributeKnowledgeRequest,
    RecallQuery, SubmitExperienceRequest, VerifyKnowledgeRequest,
};
use brainmesh_storage::BrainMeshStore;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub const MCP_PROTOCOL_VERSION: &str = "2025-03-26";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

impl JsonRpcResponse {
    pub fn ok(id: Option<serde_json::Value>, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: Option<serde_json::Value>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

pub struct McpServer {
    store: Arc<dyn BrainMeshStore>,
}

impl McpServer {
    pub fn new(store: Arc<dyn BrainMeshStore>) -> Self {
        Self { store }
    }

    pub fn tool_definitions() -> Vec<serde_json::Value> {
        vec![
            serde_json::json!({
                "name": "brainmesh_search",
                "description": "Search the shared BrainMesh knowledge network.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "domain": { "type": "string" },
                        "limit": { "type": "integer", "default": 10 }
                    },
                    "required": ["query"]
                }
            }),
            serde_json::json!({
                "name": "brainmesh_recall",
                "description": "Retrieve relevant knowledge with provenance, verification status, and safety labels.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "domain": { "type": "string" },
                        "framework": { "type": "string" },
                        "tags": { "type": "array", "items": { "type": "string" } },
                        "limit": { "type": "integer", "default": 10 }
                    },
                    "required": ["query"]
                }
            }),
            serde_json::json!({
                "name": "brainmesh_contribute",
                "description": "Contribute structured knowledge (defaults to private visibility unless specified).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string" },
                        "description": { "type": "string" },
                        "domain": { "type": "string" },
                        "content": { "type": "object" },
                        "tags": { "type": "array", "items": { "type": "string" } },
                        "visibility": { "type": "string", "enum": ["private", "workspace", "organization", "trusted_network", "public"] }
                    },
                    "required": ["title", "description", "domain", "content"]
                }
            }),
            serde_json::json!({
                "name": "brainmesh_learn",
                "description": "Submit a structured agent experience (redacted automatically, opt-in publication).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string" },
                        "outcome": { "type": "string" },
                        "summary": { "type": "string" },
                        "events": { "type": "array" },
                        "approved_for_publication": { "type": "boolean", "default": false }
                    },
                    "required": ["title", "outcome", "summary"]
                }
            }),
            serde_json::json!({
                "name": "brainmesh_verify",
                "description": "Submit independent verification evidence for a knowledge object.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "knowledge_id": { "type": "string" },
                        "status": { "type": "string" },
                        "method": { "type": "string" },
                        "evidence": { "type": "array" },
                        "conditions": { "type": "string" }
                    },
                    "required": ["knowledge_id", "status", "method"]
                }
            }),
            serde_json::json!({
                "name": "brainmesh_related",
                "description": "Explore related knowledge in the BrainMesh relationship graph.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "knowledge_id": { "type": "string" }
                    },
                    "required": ["knowledge_id"]
                }
            }),
            serde_json::json!({
                "name": "brainmesh_agent_register",
                "description": "Register a new AI agent with BrainMesh.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "framework": { "type": "string" },
                        "model": { "type": "string" }
                    },
                    "required": ["name", "framework"]
                }
            }),
            serde_json::json!({
                "name": "brainmesh_network_status",
                "description": "Get current BrainMesh network statistics and health.",
                "inputSchema": { "type": "object", "properties": {} }
            }),
            serde_json::json!({
                "name": "brainmesh_get_knowledge",
                "description": "Retrieve a specific knowledge object by ID.",
                "inputSchema": {
                    "type": "object",
                    "properties": { "knowledge_id": { "type": "string" } },
                    "required": ["knowledge_id"]
                }
            }),
        ]
    }

    pub async fn handle_request(
        &self,
        ctx: &AuthContext,
        req: JsonRpcRequest,
    ) -> Option<JsonRpcResponse> {
        let id = req.id.clone();
        if req.method.starts_with("notifications/") && id.is_none() {
            return None;
        }

        let resp = match req.method.as_str() {
            "initialize" => JsonRpcResponse::ok(
                id,
                serde_json::json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "brainmesh-mcp", "version": env!("CARGO_PKG_VERSION") }
                }),
            ),
            "ping" => JsonRpcResponse::ok(id, serde_json::json!({})),
            "tools/list" => {
                JsonRpcResponse::ok(id, serde_json::json!({ "tools": Self::tool_definitions() }))
            }
            "tools/call" => self.handle_tool_call(id, ctx, req.params).await,
            other => JsonRpcResponse::err(id, -32601, format!("method not found: {other}")),
        };
        Some(resp)
    }

    async fn handle_tool_call(
        &self,
        id: Option<serde_json::Value>,
        ctx: &AuthContext,
        params: serde_json::Value,
    ) -> JsonRpcResponse {
        let tool = match params.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => return JsonRpcResponse::err(id, -32602, "missing tool name"),
        };
        let args = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));

        let res: std::result::Result<serde_json::Value, BrainMeshError> = match tool {
            "brainmesh_search" | "brainmesh_recall" => {
                match serde_json::from_value::<RecallQuery>(args) {
                    Ok(q) => self
                        .store
                        .recall(ctx, q)
                        .await
                        .and_then(|r| serde_json::to_value(r).map_err(|e| BrainMeshError::Internal(e.to_string()))),
                    Err(e) => Err(BrainMeshError::Validation(e.to_string())),
                }
            }
            "brainmesh_contribute" => {
                match serde_json::from_value::<ContributeKnowledgeRequest>(args) {
                    Ok(req) => self
                        .store
                        .contribute_knowledge(ctx, req)
                        .await
                        .and_then(|k| serde_json::to_value(k).map_err(|e| BrainMeshError::Internal(e.to_string()))),
                    Err(e) => Err(BrainMeshError::Validation(e.to_string())),
                }
            }
            "brainmesh_learn" => {
                match serde_json::from_value::<SubmitExperienceRequest>(args) {
                    Ok(req) => self
                        .store
                        .submit_experience(ctx, req)
                        .await
                        .and_then(|ex| serde_json::to_value(ex).map_err(|e| BrainMeshError::Internal(e.to_string()))),
                    Err(e) => Err(BrainMeshError::Validation(e.to_string())),
                }
            }
            "brainmesh_verify" => {
                let kid = args
                    .get("knowledge_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                match serde_json::from_value::<VerifyKnowledgeRequest>(args) {
                    Ok(req) => self
                        .store
                        .verify_knowledge(ctx, &kid, req)
                        .await
                        .and_then(|v| serde_json::to_value(v).map_err(|e| BrainMeshError::Internal(e.to_string()))),
                    Err(e) => Err(BrainMeshError::Validation(e.to_string())),
                }
            }
            "brainmesh_related" => {
                let kid = args
                    .get("knowledge_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                self.store.get_related(ctx, kid).await.map(|(rels, items)| {
                    serde_json::json!({ "relationships": rels, "related": items })
                })
            }
            "brainmesh_agent_register" => {
                match serde_json::from_value::<AgentRegistrationRequest>(args) {
                    Ok(req) => self
                        .store
                        .register_agent(req)
                        .await
                        .and_then(|a| serde_json::to_value(a).map_err(|e| BrainMeshError::Internal(e.to_string()))),
                    Err(e) => Err(BrainMeshError::Validation(e.to_string())),
                }
            }
            "brainmesh_network_status" => self
                .store
                .network_status()
                .await
                .and_then(|s| serde_json::to_value(s).map_err(|e| BrainMeshError::Internal(e.to_string()))),
            "brainmesh_get_knowledge" => {
                let kid = args
                    .get("knowledge_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                self.store
                    .get_knowledge(ctx, kid)
                    .await
                    .and_then(|k| serde_json::to_value(k).map_err(|e| BrainMeshError::Internal(e.to_string())))
            }
            unknown => {
                return JsonRpcResponse::err(id, -32602, format!("unknown tool: {unknown}"));
            }
        };

        match res {
            Ok(structured) => {
                let text = serde_json::to_string_pretty(&structured).unwrap_or_default();
                JsonRpcResponse::ok(
                    id,
                    serde_json::json!({
                        "content": [{ "type": "text", "text": text }],
                        "structuredContent": structured,
                        "isError": false
                    }),
                )
            }
            Err(err) => JsonRpcResponse::ok(
                id,
                serde_json::json!({
                    "content": [{ "type": "text", "text": err.to_string() }],
                    "structuredContent": { "error": err.code(), "message": err.to_string() },
                    "isError": true
                }),
            ),
        }
    }
}

pub async fn serve_stdio(store: Arc<dyn BrainMeshStore>, ctx: AuthContext) -> std::io::Result<()> {
    let server = McpServer::new(store);
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut lines = BufReader::new(stdin).lines();

    while let Some(line) = lines.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let resp_opt = match serde_json::from_str::<JsonRpcRequest>(trimmed) {
            Ok(req) => server.handle_request(&ctx, req).await,
            Err(e) => Some(JsonRpcResponse::err(None, -32700, format!("parse error: {e}"))),
        };
        if let Some(resp) = resp_opt {
            let s = serde_json::to_string(&resp).unwrap_or_default();
            stdout.write_all(s.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
    }
    Ok(())
}

#[derive(Clone)]
struct McpHttpState {
    store: Arc<dyn BrainMeshStore>,
    server: Arc<McpServer>,
    require_auth: bool,
}

pub fn mcp_router(store: Arc<dyn BrainMeshStore>, require_auth: bool) -> Router {
    let server = Arc::new(McpServer::new(Arc::clone(&store)));
    Router::new()
        .route("/mcp", post(handle_mcp_http))
        .with_state(McpHttpState {
            store,
            server,
            require_auth,
        })
}

async fn handle_mcp_http(
    State(state): State<McpHttpState>,
    headers: HeaderMap,
    Json(req): Json<JsonRpcRequest>,
) -> Response {
    let ctx = if state.require_auth {
        let raw = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer ").or_else(|| s.strip_prefix("bearer ")))
            .map(str::trim);
        match raw {
            Some(tok) if !tok.is_empty() => match state.store.authenticate_api_key(tok).await {
                Ok(c) => c,
                Err(e) => {
                    return (
                        StatusCode::UNAUTHORIZED,
                        Json(JsonRpcResponse::err(req.id, -32001, e.to_string())),
                    )
                        .into_response();
                }
            },
            _ => {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(JsonRpcResponse::err(req.id, -32001, "missing bearer token")),
                )
                    .into_response();
            }
        }
    } else {
        AuthContext::local_admin()
    };

    let wants_sse = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.contains("text/event-stream"))
        .unwrap_or(false);

    match state.server.handle_request(&ctx, req).await {
        None => StatusCode::ACCEPTED.into_response(),
        Some(resp) if wants_sse => {
            let payload = serde_json::to_string(&resp).unwrap_or_default();
            let stream = futures::stream::once(async move {
                Ok::<Event, Infallible>(Event::default().event("message").data(payload))
            });
            Sse::new(stream).keep_alive(KeepAlive::default()).into_response()
        }
        Some(resp) => (StatusCode::OK, Json(resp)).into_response(),
    }
}

use brainmesh_api::api_router;
use brainmesh_core::{
    AgentRegistrationRequest, BrainMeshError, ChallengeKnowledgeRequest,
    ContributeKnowledgeRequest, CreateRelationshipRequest, ExtractExperienceRequest,
    KnowledgeCategory, LocalHashEmbeddingProvider, PromptInjectionGuard, RecallQuery,
    RelationshipType, SubmitExperienceRequest, VerificationStatus, Visibility, cosine_similarity,
    EmbeddingProvider,
};
use brainmesh_sdk::BrainMeshClient;
use brainmesh_storage::{BrainMeshStore, SqliteStore};
use std::sync::Arc;

async fn spawn_brainmesh_server() -> String {
    let store: Arc<dyn BrainMeshStore> =
        Arc::new(SqliteStore::connect("sqlite::memory:").await.unwrap());
    let app = api_router(store, true);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn test_cross_agent_opencode_to_hermes_mvp_and_security() {
    let base_url = spawn_brainmesh_server().await;

    // 1. Register Agent A (OpenCode) and Agent B (Hermes Agent)
    let anon = BrainMeshClient::new(&base_url, None);
    let agent_a = anon
        .register_agent(&AgentRegistrationRequest {
            name: "agent-a-opencode".into(),
            framework: "opencode".into(),
            model: Some("claude-3-7-sonnet".into()),
            organization_id: None,
            workspace_id: None,
            capabilities: vec!["debugging".into()],
        })
        .await
        .unwrap();

    let agent_b = anon
        .register_agent(&AgentRegistrationRequest {
            name: "agent-b-hermes".into(),
            framework: "hermes_agent".into(),
            model: Some("gemini-2.5-flash".into()),
            organization_id: None,
            workspace_id: None,
            capabilities: vec!["recall".into(), "verification".into()],
        })
        .await
        .unwrap();

    let client_a = BrainMeshClient::new(&base_url, Some(agent_a.api_key.clone()));
    let client_b = BrainMeshClient::new(&base_url, Some(agent_b.api_key.clone()));

    // 2. Agent A contributes a PUBLIC verified solution for PostgreSQL Docker Compose
    let public_sol = client_a
        .contribute(&ContributeKnowledgeRequest {
            category: KnowledgeCategory::Solution,
            record_type: "verified_solution".into(),
            title: "Fix PostgreSQL connection failure in Docker Compose".into(),
            description: "Use the Docker Compose service hostname instead of localhost. Token sk-secret1234567890abcd".into(),
            domain: "software_engineering".into(),
            tags: vec!["rust".into(), "postgresql".into(), "docker".into()],
            content: serde_json::json!({
                "problem": "Application cannot connect to PostgreSQL.",
                "cause": "Incorrect database hostname (localhost instead of service name).",
                "solution": "Use the Docker Compose service hostname (e.g. postgres:5432)."
            }),
            verification: None,
            visibility: Visibility::Public,
            expires_at: None,
        })
        .await
        .unwrap();

    // Secret redaction check
    assert!(!public_sol.description.contains("sk-secret1234567890abcd"));
    assert!(public_sol.description.contains("[REDACTED_KEY]"));

    // 3. Agent A also contributes a PRIVATE note (default visibility)
    let private_note = client_a
        .contribute(&ContributeKnowledgeRequest {
            category: KnowledgeCategory::Lesson,
            record_type: "lesson".into(),
            title: "Internal secret PostgreSQL credentials".into(),
            description: "Private internal memory".into(),
            domain: "software_engineering".into(),
            tags: vec!["postgresql".into()],
            content: serde_json::json!({"note": "private only"}),
            verification: None,
            visibility: Visibility::Private,
            expires_at: None,
        })
        .await
        .unwrap();

    // 4. Unauthorized access check: Agent B must NOT be able to read Agent A's private knowledge
    let err = client_b.get_knowledge(&private_note.id).await.unwrap_err();
    assert!(matches!(err, BrainMeshError::Authorization(_)));

    // 5. Agent B searches BrainMesh through MCP (POST /mcp)
    let http = reqwest::Client::new();
    let mcp_recall: serde_json::Value = http
        .post(format!("{base_url}/mcp"))
        .bearer_auth(&agent_b.api_key)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "brainmesh_recall",
                "arguments": {
                    "query": "PostgreSQL connection failure Docker",
                    "limit": 5
                }
            }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(mcp_recall["result"]["isError"], false);
    let recalled = mcp_recall["result"]["structuredContent"]
        .as_array()
        .unwrap();
    // Only the public solution is returned; private note is isolated!
    assert_eq!(recalled.len(), 1);
    assert_eq!(recalled[0]["knowledge"]["id"], public_sol.id);
    assert!(recalled[0]["safety_notice"]
        .as_str()
        .unwrap()
        .contains(PromptInjectionGuard::SAFETY_NOTICE));

    // 6. Agent B verifies the solution via MCP -> promoted to independently_confirmed
    let mcp_verify: serde_json::Value = http
        .post(format!("{base_url}/mcp"))
        .bearer_auth(&agent_b.api_key)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "brainmesh_verify",
                "arguments": {
                    "knowledge_id": public_sol.id,
                    "status": "reported_successful",
                    "method": "integration_test",
                    "evidence": [{ "exit_code": 0 }]
                }
            }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        mcp_verify["result"]["structuredContent"]["status"],
        "independently_confirmed"
    );

    // 7. Challenge preservation: challenging records dispute without erasing verification history
    client_b
        .challenge(
            &public_sol.id,
            &ChallengeKnowledgeRequest {
                reason: "Fails when host network mode is enabled".into(),
                evidence: vec![serde_json::json!({"network_mode": "host"})],
            },
        )
        .await
        .unwrap();

    let history = client_b.verification_history(&public_sol.id).await.unwrap();
    assert_eq!(history.current_status, VerificationStatus::Disputed);
    assert_eq!(history.verifications.len(), 1);
    assert_eq!(history.challenges.len(), 1);

    // 8. Experience submission -> redaction -> opt-in extraction -> relationship graph
    let exp = client_a
        .submit_experience(&SubmitExperienceRequest {
            title: "Docker Compose DNS debugging".into(),
            outcome: "success".into(),
            summary: "Resolved DNS issue for user@example.com".into(),
            events: vec![serde_json::json!({"step": "checked compose network"})],
            approved_for_publication: false,
        })
        .await
        .unwrap();
    assert!(exp.summary.contains("[REDACTED_EMAIL]"));

    // Opt-in enforcement: cannot extract to public without user_approved=true
    let unapproved_err = client_a
        .extract_experience(
            &exp.id,
            &ExtractExperienceRequest {
                domain: "devops".into(),
                tags: vec!["docker".into()],
                visibility: Visibility::Public,
                user_approved: false,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(unapproved_err, BrainMeshError::Authorization(_)));

    let extracted = client_a
        .extract_experience(
            &exp.id,
            &ExtractExperienceRequest {
                domain: "devops".into(),
                tags: vec!["docker".into()],
                visibility: Visibility::Public,
                user_approved: true,
            },
        )
        .await
        .unwrap();

    let rel = client_a
        .add_relationship(
            &extracted.id,
            &CreateRelationshipRequest {
                target_id: public_sol.id.clone(),
                relationship_type: RelationshipType::RelatedTo,
                metadata: serde_json::json!({}),
            },
        )
        .await
        .unwrap();
    assert_eq!(rel.source_id, extracted.id);

    // 9. Network status & Rust SDK recall
    let matches = client_b
        .recall(&RecallQuery {
            query: "PostgreSQL".into(),
            domain: Some("software_engineering".into()),
            framework: None,
            tags: Vec::new(),
            verification_status: None,
            limit: 5,
        })
        .await
        .unwrap();
    assert_eq!(matches.len(), 1);

    // 10. A2A Agent Card + Task Recall
    let card: serde_json::Value = http
        .get(format!("{base_url}/.well-known/agent.json"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(card["name"], "BrainMesh");

    let a2a_recall: serde_json::Value = http
        .post(format!("{base_url}/a2a/tasks/send"))
        .bearer_auth(&agent_b.api_key)
        .json(&serde_json::json!({
            "capability": "recall",
            "query": "PostgreSQL"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(a2a_recall["status"], "completed");
    assert_eq!(a2a_recall["matches"].as_array().unwrap().len(), 1);

    // 11. LocalHashEmbeddingProvider deterministic similarity
    let embedder = LocalHashEmbeddingProvider::default();
    let vecs = embedder
        .embed(&[
            "postgresql docker connection".into(),
            "postgresql docker network".into(),
        ])
        .await
        .unwrap();
    assert!(cosine_similarity(&vecs[0], &vecs[1]) > 0.4);
}

use brainmesh_core::{
    AgentRecord, AgentRegistrationRequest, AgentRegistrationResponse, BrainMeshError,
    ChallengeKnowledgeRequest, ChallengeRecord, ContributeKnowledgeRequest,
    CreateRelationshipRequest, ExperienceRecord, ExtractExperienceRequest, KnowledgeObject,
    NetworkStatus, RecallMatch, RecallQuery, RelationshipRecord, Result, SubmitExperienceRequest,
    VerificationHistoryResponse, VerificationRecord, VerifyKnowledgeRequest,
};

#[derive(Clone)]
pub struct BrainMeshClient {
    base_url: String,
    api_key: Option<String>,
    http: reqwest::Client,
}

impl BrainMeshClient {
    pub fn new(base_url: impl Into<String>, api_key: Option<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key,
            http: reqwest::Client::new(),
        }
    }

    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    fn apply_auth(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.api_key {
            Some(k) => builder.bearer_auth(k),
            None => builder,
        }
    }

    async fn decode<T: serde::de::DeserializeOwned>(&self, resp: reqwest::Response) -> Result<T> {
        let status = resp.status();
        if status.is_success() {
            return resp
                .json::<T>()
                .await
                .map_err(|e| BrainMeshError::Internal(format!("decode error: {e}")));
        }
        let body = resp.text().await.unwrap_or_default();
        Err(match status.as_u16() {
            401 => BrainMeshError::Authentication(body),
            403 => BrainMeshError::Authorization(body),
            404 => BrainMeshError::NotFound(body),
            429 => BrainMeshError::RateLimited(body),
            400 | 422 => BrainMeshError::Validation(body),
            _ => BrainMeshError::Internal(format!("HTTP {}: {}", status.as_u16(), body)),
        })
    }

    pub async fn register_agent(
        &self,
        req: &AgentRegistrationRequest,
    ) -> Result<AgentRegistrationResponse> {
        let resp = self
            .http
            .post(format!("{}/api/v1/agents/register", self.base_url))
            .json(req)
            .send()
            .await
            .map_err(|e| BrainMeshError::Internal(e.to_string()))?;
        self.decode(resp).await
    }

    pub async fn list_agents(&self) -> Result<Vec<AgentRecord>> {
        let resp = self
            .apply_auth(self.http.get(format!("{}/api/v1/agents", self.base_url)))
            .send()
            .await
            .map_err(|e| BrainMeshError::Internal(e.to_string()))?;
        self.decode(resp).await
    }

    pub async fn contribute(
        &self,
        req: &ContributeKnowledgeRequest,
    ) -> Result<KnowledgeObject> {
        let resp = self
            .apply_auth(
                self.http
                    .post(format!("{}/api/v1/knowledge", self.base_url))
                    .json(req),
            )
            .send()
            .await
            .map_err(|e| BrainMeshError::Internal(e.to_string()))?;
        self.decode(resp).await
    }

    pub async fn get_knowledge(&self, id: &str) -> Result<KnowledgeObject> {
        let resp = self
            .apply_auth(
                self.http
                    .get(format!("{}/api/v1/knowledge/{id}", self.base_url)),
            )
            .send()
            .await
            .map_err(|e| BrainMeshError::Internal(e.to_string()))?;
        self.decode(resp).await
    }

    pub async fn recall(&self, query: &RecallQuery) -> Result<Vec<RecallMatch>> {
        let resp = self
            .apply_auth(
                self.http
                    .post(format!("{}/api/v1/recall", self.base_url))
                    .json(query),
            )
            .send()
            .await
            .map_err(|e| BrainMeshError::Internal(e.to_string()))?;
        self.decode(resp).await
    }

    pub async fn verify(
        &self,
        knowledge_id: &str,
        req: &VerifyKnowledgeRequest,
    ) -> Result<VerificationRecord> {
        let resp = self
            .apply_auth(
                self.http
                    .post(format!(
                        "{}/api/v1/knowledge/{knowledge_id}/verify",
                        self.base_url
                    ))
                    .json(req),
            )
            .send()
            .await
            .map_err(|e| BrainMeshError::Internal(e.to_string()))?;
        self.decode(resp).await
    }

    pub async fn challenge(
        &self,
        knowledge_id: &str,
        req: &ChallengeKnowledgeRequest,
    ) -> Result<ChallengeRecord> {
        let resp = self
            .apply_auth(
                self.http
                    .post(format!(
                        "{}/api/v1/knowledge/{knowledge_id}/challenge",
                        self.base_url
                    ))
                    .json(req),
            )
            .send()
            .await
            .map_err(|e| BrainMeshError::Internal(e.to_string()))?;
        self.decode(resp).await
    }

    pub async fn verification_history(
        &self,
        knowledge_id: &str,
    ) -> Result<VerificationHistoryResponse> {
        let resp = self
            .apply_auth(self.http.get(format!(
                "{}/api/v1/knowledge/{knowledge_id}/verification",
                self.base_url
            )))
            .send()
            .await
            .map_err(|e| BrainMeshError::Internal(e.to_string()))?;
        self.decode(resp).await
    }

    pub async fn submit_experience(
        &self,
        req: &SubmitExperienceRequest,
    ) -> Result<ExperienceRecord> {
        let resp = self
            .apply_auth(
                self.http
                    .post(format!("{}/api/v1/experiences", self.base_url))
                    .json(req),
            )
            .send()
            .await
            .map_err(|e| BrainMeshError::Internal(e.to_string()))?;
        self.decode(resp).await
    }

    pub async fn extract_experience(
        &self,
        experience_id: &str,
        req: &ExtractExperienceRequest,
    ) -> Result<KnowledgeObject> {
        let resp = self
            .apply_auth(
                self.http
                    .post(format!(
                        "{}/api/v1/experiences/{experience_id}/extract",
                        self.base_url
                    ))
                    .json(req),
            )
            .send()
            .await
            .map_err(|e| BrainMeshError::Internal(e.to_string()))?;
        self.decode(resp).await
    }

    pub async fn add_relationship(
        &self,
        source_id: &str,
        req: &CreateRelationshipRequest,
    ) -> Result<RelationshipRecord> {
        let resp = self
            .apply_auth(
                self.http
                    .post(format!(
                        "{}/api/v1/knowledge/{source_id}/relationships",
                        self.base_url
                    ))
                    .json(req),
            )
            .send()
            .await
            .map_err(|e| BrainMeshError::Internal(e.to_string()))?;
        self.decode(resp).await
    }

    pub async fn network_status(&self) -> Result<NetworkStatus> {
        let resp = self
            .http
            .get(format!("{}/api/v1/metrics", self.base_url))
            .send()
            .await
            .map_err(|e| BrainMeshError::Internal(e.to_string()))?;
        self.decode(resp).await
    }
}

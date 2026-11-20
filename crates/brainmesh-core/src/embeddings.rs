use crate::error::{BrainMeshError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Pluggable embedding provider interface.
/// BrainMesh works without any external LLM/embedding provider using LocalHashEmbeddingProvider,
/// and supports OpenAI-compatible `/v1/embeddings` endpoints (OpenAI, Ollama, vLLM, etc.).
#[async_trait::async_trait]
pub trait EmbeddingProvider: Send + Sync {
    fn model_name(&self) -> &str;
    fn dimensions(&self) -> usize;
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}

/// Deterministic local feature-hashing embedding provider (zero external dependencies).
#[derive(Debug, Clone)]
pub struct LocalHashEmbeddingProvider {
    dims: usize,
    model: String,
}

impl Default for LocalHashEmbeddingProvider {
    fn default() -> Self {
        Self {
            dims: 64,
            model: "local-hash-v1".to_string(),
        }
    }
}

impl LocalHashEmbeddingProvider {
    pub fn new(dims: usize) -> Self {
        Self {
            dims: dims.max(8),
            model: "local-hash-v1".to_string(),
        }
    }

    pub fn embed_one(&self, text: &str) -> Vec<f32> {
        let mut vec = vec![0.0f32; self.dims];
        for token in text
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
            .filter(|t| t.len() >= 2)
        {
            let mut hasher = Sha256::new();
            hasher.update(token.as_bytes());
            let digest = hasher.finalize();
            let idx = u16::from_le_bytes([digest[0], digest[1]]) as usize % self.dims;
            let sign = if digest[2] & 1 == 0 { 1.0 } else { -1.0 };
            vec[idx] += sign;
        }
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut vec {
                *x /= norm;
            }
        }
        vec
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for LocalHashEmbeddingProvider {
    fn model_name(&self) -> &str {
        &self.model
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| self.embed_one(t)).collect())
    }
}

/// Configuration for OpenAI-compatible embedding endpoints (OpenAI, Ollama, vLLM, LM Studio).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiEmbeddingConfig {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub dimensions: usize,
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let mut dot = 0.0f64;
    let mut na = 0.0f64;
    let mut nb = 0.0f64;
    for (x, y) in a.iter().zip(b.iter()) {
        let xf = *x as f64;
        let yf = *y as f64;
        dot += xf * yf;
        na += xf * xf;
        nb += yf * yf;
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        (dot / denom).clamp(-1.0, 1.0)
    }
}

pub fn validate_embedding_dimensions(vec: &[f32], expected: usize) -> Result<()> {
    if vec.len() != expected {
        return Err(BrainMeshError::Validation(format!(
            "embedding dimension mismatch: expected {expected}, got {}",
            vec.len()
        )));
    }
    Ok(())
}

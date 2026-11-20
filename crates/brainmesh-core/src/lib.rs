pub mod embeddings;
pub mod error;
pub mod models;
pub mod security;

pub use embeddings::{
    cosine_similarity, validate_embedding_dimensions, EmbeddingProvider,
    LocalHashEmbeddingProvider, OpenAiEmbeddingConfig,
};
pub use error::{BrainMeshError, Result};
pub use models::*;
pub use security::{
    hash_api_key, validate_contribution, AuthContext, Permission, PromptInjectionGuard,
    RateLimiter, SecretRedactor,
};

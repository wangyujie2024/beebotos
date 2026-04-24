//! Embedding Providers
//!
//! Abstraction layer for text embedding models, supporting both cloud-based
//! (OpenAI, etc.) and local (llama.cpp, sentence-transformers) providers.
//!
//! # Supported Providers
//!
//! - **OpenAI**: text-embedding-3-small, text-embedding-3-large,
//!   text-embedding-ada-002
//! - **Local**: llama.cpp with embedding support
//! - **Mock**: For testing without external dependencies

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Default embedding dimension (OpenAI ada-002)
pub const DEFAULT_EMBEDDING_DIMENSION: usize = 1536;
/// Default request timeout
pub const DEFAULT_EMBEDDING_TIMEOUT_SECS: u64 = 30;
/// Maximum text length for embedding
pub const MAX_EMBEDDING_TEXT_LENGTH: usize = 8192;

/// Embedding provider trait
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embedding for text
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Generate embeddings for multiple texts (batch processing)
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed(text).await?);
        }
        Ok(results)
    }

    /// Get embedding dimension
    fn dimension(&self) -> usize;

    /// Get provider name
    fn name(&self) -> &str;

    /// Check if provider is healthy/available
    async fn health_check(&self) -> bool {
        true
    }
}

/// Embedding provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Provider type
    pub provider_type: ProviderType,
    /// API key (for cloud providers)
    pub api_key: Option<String>,
    /// API base URL (optional, for custom endpoints)
    pub api_base: Option<String>,
    /// Model name
    pub model: String,
    /// Request timeout
    pub timeout_secs: u64,
    /// Dimension override (optional)
    pub dimension: Option<usize>,
    /// Local model path (for local providers)
    pub local_model_path: Option<PathBuf>,
}

/// Provider type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderType {
    OpenAI,
    AzureOpenAI,
    LocalLlama,
    Mock,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider_type: ProviderType::OpenAI,
            api_key: None,
            api_base: None,
            model: "text-embedding-3-small".to_string(),
            timeout_secs: DEFAULT_EMBEDDING_TIMEOUT_SECS,
            dimension: None,
            local_model_path: None,
        }
    }
}

impl EmbeddingConfig {
    /// Create OpenAI configuration
    pub fn openai(api_key: impl Into<String>, model: Option<String>) -> Self {
        Self {
            provider_type: ProviderType::OpenAI,
            api_key: Some(api_key.into()),
            model: model.unwrap_or_else(|| "text-embedding-3-small".to_string()),
            ..Default::default()
        }
    }

    /// Create local llama.cpp configuration
    pub fn local_llama(model_path: impl Into<PathBuf>) -> Self {
        Self {
            provider_type: ProviderType::LocalLlama,
            local_model_path: Some(model_path.into()),
            model: "local".to_string(),
            ..Default::default()
        }
    }

    /// Create mock configuration for testing
    pub fn mock(dimension: usize) -> Self {
        Self {
            provider_type: ProviderType::Mock,
            dimension: Some(dimension),
            model: "mock".to_string(),
            ..Default::default()
        }
    }

    /// Get dimension for the configured model
    pub fn get_dimension(&self) -> usize {
        self.dimension.unwrap_or_else(|| match self.model.as_str() {
            "text-embedding-3-small" => 1536,
            "text-embedding-3-large" => 3072,
            "text-embedding-ada-002" => 1536,
            _ => DEFAULT_EMBEDDING_DIMENSION,
        })
    }
}

/// OpenAI embedding provider
pub struct OpenAIEmbeddingProvider {
    config: EmbeddingConfig,
    client: reqwest::Client,
    dimension: usize,
}

impl OpenAIEmbeddingProvider {
    /// Create new OpenAI provider
    pub fn new(config: EmbeddingConfig) -> Result<Self> {
        let api_key = config.api_key.as_ref().ok_or_else(|| {
            crate::error::AgentError::configuration("OpenAI API key is required".to_string())
        })?;

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .default_headers({
                let mut headers = reqwest::header::HeaderMap::new();
                headers.insert(
                    "Authorization",
                    format!("Bearer {}", api_key).parse().unwrap(),
                );
                headers
            })
            .build()
            .map_err(|e| {
                crate::error::AgentError::configuration(format!(
                    "Failed to create HTTP client: {}",
                    e
                ))
            })?;

        let dimension = config.get_dimension();

        Ok(Self {
            config,
            client,
            dimension,
        })
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAIEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Truncate text if too long
        let text = if text.len() > MAX_EMBEDDING_TEXT_LENGTH {
            &text[..MAX_EMBEDDING_TEXT_LENGTH]
        } else {
            text
        };

        let request = OpenAIEmbeddingRequest {
            model: self.config.model.clone(),
            input: text.to_string(),
        };

        let base_url = self
            .config
            .api_base
            .as_deref()
            .unwrap_or("https://api.openai.com/v1");
        let url = format!("{}/embeddings", base_url);

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                crate::error::AgentError::platform(format!(
                    "Failed to send embedding request: {}",
                    e
                ))
            })?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(crate::error::AgentError::platform(format!(
                "OpenAI API error: {}",
                error_text
            )));
        }

        let embedding_response: OpenAIEmbeddingResponse = response.json().await.map_err(|e| {
            crate::error::AgentError::platform(format!("Failed to parse embedding response: {}", e))
        })?;

        if let Some(data) = embedding_response.data.first() {
            Ok(data.embedding.clone())
        } else {
            Err(crate::error::AgentError::platform(
                "Empty embedding response".to_string(),
            ))
        }
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        // Truncate texts if too long
        let texts: Vec<String> = texts
            .iter()
            .map(|t| {
                if t.len() > MAX_EMBEDDING_TEXT_LENGTH {
                    t[..MAX_EMBEDDING_TEXT_LENGTH].to_string()
                } else {
                    t.clone()
                }
            })
            .collect();

        let request = OpenAIEmbeddingBatchRequest {
            model: self.config.model.clone(),
            input: texts,
        };

        let base_url = self
            .config
            .api_base
            .as_deref()
            .unwrap_or("https://api.openai.com/v1");
        let url = format!("{}/embeddings", base_url);

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                crate::error::AgentError::platform(format!(
                    "Failed to send batch embedding request: {}",
                    e
                ))
            })?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(crate::error::AgentError::platform(format!(
                "OpenAI API error: {}",
                error_text
            )));
        }

        let embedding_response: OpenAIEmbeddingBatchResponse =
            response.json().await.map_err(|e| {
                crate::error::AgentError::platform(format!(
                    "Failed to parse batch embedding response: {}",
                    e
                ))
            })?;

        let mut embeddings: Vec<Vec<f32>> = Vec::with_capacity(embedding_response.data.len());
        for data in embedding_response.data {
            embeddings.push(data.embedding);
        }

        Ok(embeddings)
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn name(&self) -> &str {
        "openai"
    }

    async fn health_check(&self) -> bool {
        // Simple health check by embedding a short text
        self.embed("test").await.is_ok()
    }
}

/// OpenAI API request structures
#[derive(Debug, Serialize)]
struct OpenAIEmbeddingRequest {
    model: String,
    input: String,
}

#[derive(Debug, Serialize)]
struct OpenAIEmbeddingBatchRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIEmbeddingResponse {
    data: Vec<EmbeddingData>,
    #[allow(dead_code)]
    model: String,
    #[allow(dead_code)]
    usage: Usage,
}

#[derive(Debug, Deserialize)]
struct OpenAIEmbeddingBatchResponse {
    data: Vec<EmbeddingData>,
    #[allow(dead_code)]
    model: String,
    #[allow(dead_code)]
    usage: Usage,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
    #[allow(dead_code)]
    index: usize,
    #[allow(dead_code)]
    object: String,
}

#[derive(Debug, Deserialize)]
struct Usage {
    #[allow(dead_code)]
    prompt_tokens: usize,
    #[allow(dead_code)]
    total_tokens: usize,
}

/// Local llama.cpp embedding provider
pub struct LocalLlamaEmbeddingProvider {
    config: EmbeddingConfig,
    client: reqwest::Client,
}

impl LocalLlamaEmbeddingProvider {
    /// Create new local llama provider
    pub fn new(config: EmbeddingConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| {
                crate::error::AgentError::configuration(format!(
                    "Failed to create HTTP client: {}",
                    e
                ))
            })?;

        Ok(Self { config, client })
    }
}

#[async_trait]
impl EmbeddingProvider for LocalLlamaEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let base_url = self
            .config
            .api_base
            .as_deref()
            .unwrap_or("http://localhost:8080");
        let url = format!("{}/embedding", base_url);

        let request = LlamaEmbeddingRequest {
            content: text.to_string(),
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                crate::error::AgentError::platform(format!(
                    "Failed to send embedding request to local llama: {}",
                    e
                ))
            })?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(crate::error::AgentError::platform(format!(
                "Local llama API error: {}",
                error_text
            )));
        }

        let embedding_response: LlamaEmbeddingResponse = response.json().await.map_err(|e| {
            crate::error::AgentError::platform(format!(
                "Failed to parse local llama embedding response: {}",
                e
            ))
        })?;

        Ok(embedding_response.embedding)
    }

    fn dimension(&self) -> usize {
        self.config.get_dimension()
    }

    fn name(&self) -> &str {
        "local_llama"
    }

    async fn health_check(&self) -> bool {
        let base_url = self
            .config
            .api_base
            .as_deref()
            .unwrap_or("http://localhost:8080");
        let health_url = format!("{}/health", base_url);

        match self.client.get(&health_url).send().await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }
}

#[derive(Debug, Serialize)]
struct LlamaEmbeddingRequest {
    content: String,
}

#[derive(Debug, Deserialize)]
struct LlamaEmbeddingResponse {
    embedding: Vec<f32>,
}

/// Mock embedding provider for testing
pub struct MockEmbeddingProvider {
    dimension: usize,
    #[allow(dead_code)]
    seed: std::sync::atomic::AtomicU64,
}

impl MockEmbeddingProvider {
    /// Create new mock provider
    pub fn new(dimension: usize) -> Self {
        Self {
            dimension,
            seed: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Generate deterministic mock embedding from text
    fn generate_mock_embedding(&self, text: &str) -> Vec<f32> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let seed = hasher.finish();

        // Generate pseudo-random but deterministic vector
        let mut embedding = Vec::with_capacity(self.dimension);
        for i in 0..self.dimension {
            let value = ((seed.wrapping_add(i as u64) % 1000) as f32 / 1000.0) * 2.0 - 1.0;
            embedding.push(value);
        }

        // Normalize
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            embedding.iter_mut().for_each(|x| *x /= norm);
        }

        embedding
    }
}

#[async_trait]
impl EmbeddingProvider for MockEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        Ok(self.generate_mock_embedding(text))
    }

    fn dimension(&self) -> usize {
        self.dimension
    }

    fn name(&self) -> &str {
        "mock"
    }
}

/// Embedding provider factory
pub struct EmbeddingProviderFactory;

impl EmbeddingProviderFactory {
    /// Create provider from configuration
    pub fn create(config: EmbeddingConfig) -> Result<Box<dyn EmbeddingProvider>> {
        match config.provider_type {
            ProviderType::OpenAI | ProviderType::AzureOpenAI => {
                Ok(Box::new(OpenAIEmbeddingProvider::new(config)?))
            }
            ProviderType::LocalLlama => Ok(Box::new(LocalLlamaEmbeddingProvider::new(config)?)),
            ProviderType::Mock => {
                let dimension = config.get_dimension();
                Ok(Box::new(MockEmbeddingProvider::new(dimension)))
            }
        }
    }
}

/// Cached embedding provider with LRU cache
pub struct CachedEmbeddingProvider {
    inner: Box<dyn EmbeddingProvider>,
    cache: std::sync::Mutex<lru::LruCache<String, Vec<f32>>>,
}

impl CachedEmbeddingProvider {
    /// Create new cached provider
    pub fn new(inner: Box<dyn EmbeddingProvider>, cache_size: usize) -> Self {
        let cache_size = std::num::NonZeroUsize::new(cache_size)
            .unwrap_or(std::num::NonZeroUsize::new(100).unwrap());
        Self {
            inner,
            cache: std::sync::Mutex::new(lru::LruCache::new(cache_size)),
        }
    }

    /// Clear cache
    pub fn clear_cache(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
    }

    /// Get cache stats
    pub fn cache_stats(&self) -> (usize, usize) {
        let cache = self.cache.lock().unwrap();
        (cache.len(), cache.cap().get())
    }
}

#[async_trait]
impl EmbeddingProvider for CachedEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // Check cache
        {
            let mut cache = self.cache.lock().unwrap();
            if let Some(embedding) = cache.get(text) {
                return Ok(embedding.clone());
            }
        }

        // Get embedding from inner provider
        let embedding = self.inner.embed(text).await?;

        // Store in cache
        {
            let mut cache = self.cache.lock().unwrap();
            cache.put(text.to_string(), embedding.clone());
        }

        Ok(embedding)
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        // Check cache for each text
        let mut results: Vec<Option<Vec<f32>>> = vec![None; texts.len()];
        let mut uncached_indices = Vec::new();
        let mut uncached_texts = Vec::new();

        {
            let mut cache = self.cache.lock().unwrap();
            for (i, text) in texts.iter().enumerate() {
                if let Some(embedding) = cache.get(text) {
                    results[i] = Some(embedding.clone());
                } else {
                    uncached_indices.push(i);
                    uncached_texts.push(text.clone());
                }
            }
        }

        // Get uncached embeddings
        if !uncached_texts.is_empty() {
            let embeddings = self.inner.embed_batch(&uncached_texts).await?;

            // Store in cache and results
            let mut cache = self.cache.lock().unwrap();
            for (idx, embedding) in uncached_indices.iter().zip(embeddings.iter()) {
                cache.put(texts[*idx].clone(), embedding.clone());
                results[*idx] = Some(embedding.clone());
            }
        }

        // Unwrap results
        results
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                crate::error::AgentError::platform("Some embeddings were not generated".to_string())
            })
    }

    fn dimension(&self) -> usize {
        self.inner.dimension()
    }

    fn name(&self) -> &str {
        self.inner.name()
    }

    async fn health_check(&self) -> bool {
        self.inner.health_check().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_provider() {
        let provider = MockEmbeddingProvider::new(128);

        let embedding1 = provider.embed("Hello world").await.unwrap();
        assert_eq!(embedding1.len(), 128);

        // Should be deterministic
        let embedding2 = provider.embed("Hello world").await.unwrap();
        assert_eq!(embedding1, embedding2);

        // Different text should produce different embedding
        let embedding3 = provider.embed("Different text").await.unwrap();
        assert_ne!(embedding1, embedding3);
    }

    #[test]
    fn test_embedding_config() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.get_dimension(), 1536);

        let config =
            EmbeddingConfig::openai("test-key", Some("text-embedding-3-large".to_string()));
        assert_eq!(config.get_dimension(), 3072);

        let config = EmbeddingConfig::mock(256);
        assert_eq!(config.get_dimension(), 256);
    }

    #[tokio::test]
    async fn test_cached_provider() {
        let inner = Box::new(MockEmbeddingProvider::new(64));
        let cached = CachedEmbeddingProvider::new(inner, 100);

        let text = "Test text for caching";

        // First call - should cache
        let embedding1 = cached.embed(text).await.unwrap();

        // Second call - should hit cache
        let embedding2 = cached.embed(text).await.unwrap();

        assert_eq!(embedding1, embedding2);

        // Check cache stats
        let (len, cap) = cached.cache_stats();
        assert_eq!(len, 1);
        assert_eq!(cap, 100);
    }

    #[tokio::test]
    async fn test_batch_embedding() {
        let provider = MockEmbeddingProvider::new(64);

        let texts = vec![
            "First text".to_string(),
            "Second text".to_string(),
            "Third text".to_string(),
        ];

        let embeddings = provider.embed_batch(&texts).await.unwrap();
        assert_eq!(embeddings.len(), 3);
        assert_eq!(embeddings[0].len(), 64);
    }
}

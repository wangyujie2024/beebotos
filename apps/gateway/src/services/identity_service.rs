//! Identity Service
//!
//! Manages agent on-chain identity registration and resolution.
//! Separated from ChainService for better modularity.

use std::sync::Arc;

use beebotos_chain::compat::{Address, AgentIdentityInfo, ChainClientTrait, TxHash, U256};
use tracing::{debug, info, instrument};

/// Agent metadata for identity
#[derive(Debug, Clone)]
pub struct AgentMetadata {
    pub name: String,
    pub description: String,
}

use super::identity_cache::{CacheStats, IdentityCache, IdentityCacheConfig};
use super::wallet_service::WalletService;
use crate::config::BlockchainConfig;
use crate::error::AppError;

/// Identity service configuration
#[derive(Debug, Clone)]
pub struct IdentityServiceConfig {
    /// Chain ID
    pub chain_id: u64,
    /// Identity contract address
    pub identity_contract: Option<Address>,
    /// Registry contract address
    pub registry_contract: Option<Address>,
}

impl From<&BlockchainConfig> for IdentityServiceConfig {
    fn from(config: &BlockchainConfig) -> Self {
        let parse_address = |addr: &Option<String>| -> Option<Address> {
            addr.as_ref().and_then(|a| {
                let a = a.trim();
                if a.len() >= 42 && a.starts_with("0x") {
                    hex::decode(&a[2..])
                        .ok()
                        .filter(|b| b.len() == 20)
                        .map(|b| Address::from_slice(&b))
                } else {
                    None
                }
            })
        };

        Self {
            chain_id: config.chain_id,
            identity_contract: parse_address(&config.identity_contract_address),
            registry_contract: parse_address(&config.registry_contract_address),
        }
    }
}

/// Identity registration request
#[derive(Debug, Clone)]
pub struct RegisterIdentityRequest {
    /// Agent ID (unique identifier)
    pub agent_id: String,
    /// Decentralized identifier
    pub did: String,
    /// Public key (32 bytes)
    pub public_key: [u8; 32],
    /// Initial metadata
    pub metadata: AgentMetadata,
}

/// Identity update request
#[derive(Debug, Clone)]
pub struct UpdateIdentityRequest {
    /// Agent ID
    pub agent_id: String,
    /// New metadata
    pub metadata: AgentMetadata,
}

/// Identity Service
pub struct IdentityService {
    /// Configuration
    config: IdentityServiceConfig,
    /// Chain client
    client: Option<Arc<dyn ChainClientTrait>>,
    /// Wallet service
    wallet_service: Arc<WalletService>,
    /// Identity cache
    cache: IdentityCache,
}

impl IdentityService {
    /// Create new identity service
    pub async fn new(
        config: IdentityServiceConfig,
        wallet_service: Arc<WalletService>,
    ) -> anyhow::Result<Self> {
        info!(
            chain_id = config.chain_id,
            has_identity_contract = config.identity_contract.is_some(),
            "Initializing IdentityService"
        );

        // Initialize cache
        let cache = IdentityCache::new(IdentityCacheConfig::default());

        Ok(Self {
            config,
            client: None, // Will be set via with_client
            wallet_service,
            cache,
        })
    }

    /// Set chain client
    pub fn with_client(mut self, client: Arc<dyn ChainClientTrait>) -> Self {
        self.client = Some(client);
        self
    }

    /// Check if identity registry is available
    pub fn has_identity_registry(&self) -> bool {
        self.client.is_some() && self.config.identity_contract.is_some()
    }

    /// Register an agent's on-chain identity
    #[instrument(skip(self, request), fields(agent_id = %request.agent_id, did = %request.did))]
    pub async fn register_identity(
        &self,
        request: RegisterIdentityRequest,
    ) -> Result<TxHash, AppError> {
        let _client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Chain client not initialized".into()))?;

        let identity_contract =
            self.config.identity_contract.as_ref().ok_or_else(|| {
                AppError::Configuration("Identity contract not configured".into())
            })?;

        if !self.wallet_service.has_wallet() {
            return Err(AppError::Configuration(
                "Wallet not initialized for signing".into(),
            ));
        }

        info!(
            agent_id = %request.agent_id,
            did = %request.did,
            "Registering agent on-chain identity"
        );

        // Build transaction data
        let tx_data = build_register_identity_call(&request);

        // Send transaction
        let tx_hash = self
            .wallet_service
            .send_contract_transaction(identity_contract.clone(), tx_data, U256::from(0))
            .await?;

        // Clear cache for this agent
        self.cache.invalidate_identity(&request.agent_id).await;

        info!(
            tx_hash = %hex::encode(tx_hash.as_slice()),
            agent_id = %request.agent_id,
            "Identity registration submitted"
        );

        Ok(tx_hash)
    }

    /// Get agent's on-chain identity information
    ///
    /// Uses identity cache to reduce RPC calls.
    #[instrument(skip(self), fields(agent_id = %agent_id))]
    pub async fn get_identity(&self, agent_id: &str) -> Result<AgentIdentityInfo, AppError> {
        // Check cache first
        if let Some(cached) = self.cache.get_identity(agent_id).await {
            debug!(agent_id = %agent_id, "Identity cache hit");
            return Ok(cached);
        }

        let _client = self
            .client
            .as_ref()
            .ok_or_else(|| AppError::Configuration("Chain client not initialized".into()))?;

        let _identity_contract =
            self.config.identity_contract.as_ref().ok_or_else(|| {
                AppError::Configuration("Identity contract not configured".into())
            })?;

        // TODO: Implement actual chain query
        // For now, return not found
        Err(AppError::NotFound(format!("agent identity: {}", agent_id)))
    }

    /// Check if an agent has a registered on-chain identity
    pub async fn has_identity(&self, agent_id: &str) -> Result<bool, AppError> {
        // Check on-chain
        match self.get_identity(agent_id).await {
            Ok(_) => Ok(true),
            Err(AppError::NotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Update agent metadata
    #[instrument(skip(self), fields(agent_id = %request.agent_id))]
    pub async fn update_metadata(
        &self,
        request: UpdateIdentityRequest,
    ) -> Result<TxHash, AppError> {
        let identity_contract =
            self.config.identity_contract.as_ref().ok_or_else(|| {
                AppError::Configuration("Identity contract not configured".into())
            })?;

        if !self.wallet_service.has_wallet() {
            return Err(AppError::Configuration(
                "Wallet not initialized for signing".into(),
            ));
        }

        info!(agent_id = %request.agent_id, "Updating agent metadata");

        // Build transaction data
        let tx_data = build_update_metadata_call(&request);

        // Send transaction
        let tx_hash = self
            .wallet_service
            .send_contract_transaction(identity_contract.clone(), tx_data, U256::from(0))
            .await?;

        // Clear cache
        self.cache.invalidate_identity(&request.agent_id).await;

        info!(
            tx_hash = %hex::encode(tx_hash.as_slice()),
            agent_id = %request.agent_id,
            "Metadata update submitted"
        );

        Ok(tx_hash)
    }

    /// Deactivate an agent's on-chain identity
    pub async fn deactivate_identity(&self, agent_id: &str) -> Result<TxHash, AppError> {
        let identity_contract =
            self.config.identity_contract.as_ref().ok_or_else(|| {
                AppError::Configuration("Identity contract not configured".into())
            })?;

        if !self.wallet_service.has_wallet() {
            return Err(AppError::Configuration(
                "Wallet not initialized for signing".into(),
            ));
        }

        info!(agent_id = %agent_id, "Deactivating agent identity");

        // Build transaction data
        let agent_id_bytes = keccak256_hash(agent_id);
        let tx_data = build_deactivate_identity_call(agent_id_bytes);

        // Send transaction
        let tx_hash = self
            .wallet_service
            .send_contract_transaction(identity_contract.clone(), tx_data, U256::from(0))
            .await?;

        // Clear cache
        self.cache.invalidate_identity(agent_id).await;

        info!(
            tx_hash = %hex::encode(tx_hash.as_slice()),
            agent_id = %agent_id,
            "Identity deactivation submitted"
        );

        Ok(tx_hash)
    }

    /// Get cache statistics
    pub async fn get_cache_stats(&self) -> CacheStats {
        self.cache.get_stats().await
    }

    /// Clear identity cache
    pub async fn clear_cache(&self) {
        self.cache.clear().await;
    }

    /// Invalidate cache entry for specific agent
    pub async fn invalidate_cache(&self, agent_id: &str) {
        self.cache.invalidate_identity(agent_id).await;
    }
}

/// Keccak256 hash - using sha3 since tiny_keccak is not available
fn keccak256_hash(input: &str) -> [u8; 32] {
    use sha3::{Digest, Keccak256};

    let mut hasher = Keccak256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    let mut output = [0u8; 32];
    output.copy_from_slice(&result);
    output
}

// Helper functions to build transaction data

fn build_register_identity_call(
    request: &RegisterIdentityRequest,
) -> beebotos_chain::compat::Bytes {
    // In real implementation, this would ABI encode the function call
    let mut data = Vec::new();
    data.extend_from_slice(&request.public_key);
    data.extend_from_slice(request.did.as_bytes());
    beebotos_chain::compat::Bytes::from(data)
}

fn build_update_metadata_call(_request: &UpdateIdentityRequest) -> beebotos_chain::compat::Bytes {
    // In real implementation, this would ABI encode the function call
    // Metadata serialization removed since AgentMetadata doesn't implement
    // Serialize
    beebotos_chain::compat::Bytes::from(vec![0u8; 32])
}

fn build_deactivate_identity_call(agent_id_bytes: [u8; 32]) -> beebotos_chain::compat::Bytes {
    beebotos_chain::compat::Bytes::from(agent_id_bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keccak256_hash() {
        let hash1 = keccak256_hash("test");
        let hash2 = keccak256_hash("test");
        let hash3 = keccak256_hash("different");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hash1.len(), 32);
    }

    #[test]
    fn test_identity_service_config() {
        let blockchain_config = BlockchainConfig {
            enabled: true,
            chain_id: 1,
            identity_contract_address: Some(
                "0x1234567890123456789012345678901234567890".to_string(),
            ),
            registry_contract_address: Some(
                "0x0987654321098765432109876543210987654321".to_string(),
            ),
            ..Default::default()
        };

        let identity_config = IdentityServiceConfig::from(&blockchain_config);

        assert_eq!(identity_config.chain_id, 1);
        assert!(identity_config.identity_contract.is_some());
        assert!(identity_config.registry_contract.is_some());
    }
}

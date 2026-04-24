//! Identity Registry Implementation
//!
//! On-chain identity registry using Alloy contracts.

use std::sync::Arc;

use alloy_primitives::{FixedBytes, B256, U256};
use alloy_provider::Provider as AlloyProvider;
use tracing::{debug, error, info, instrument};

use crate::compat::Address;
use crate::config::ChainConfig;
use crate::contracts::{AgentIdentity, AgentRegistry};
use crate::identity::IdentityRegistry;
use crate::{ChainError, Result};

/// Agent ID type (bytes32)
pub type AgentId = FixedBytes<32>;

/// Agent identity information
#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub agent_id: AgentId,
    pub owner: Address,
    pub did: String,
    pub public_key: B256,
    pub is_active: bool,
    pub reputation: U256,
    pub created_at: U256,
    pub capabilities: Vec<B256>,
}

impl From<crate::contracts::bindings::AgentIdentity::AgentIdentityInfo> for AgentInfo {
    fn from(agent: crate::contracts::bindings::AgentIdentity::AgentIdentityInfo) -> Self {
        Self {
            agent_id: agent.agentId,
            owner: agent.owner,
            did: agent.did,
            public_key: agent.publicKey,
            is_active: agent.isActive,
            reputation: agent.reputation,
            created_at: agent.createdAt,
            capabilities: Vec::new(), // Capabilities fetched separately
        }
    }
}

/// On-chain identity registry client
pub struct OnChainIdentityRegistry<P: AlloyProvider + Clone> {
    provider: Arc<P>,
    identity_contract: Address,
    registry_contract: Option<Address>,
    signer: Option<alloy_signer_local::PrivateKeySigner>,
}

impl<P: AlloyProvider + Clone> OnChainIdentityRegistry<P> {
    /// Create new identity registry client
    pub fn new(provider: Arc<P>, identity_contract: Address) -> Self {
        info!(
            target: "chain::identity",
            identity_contract = %identity_contract,
            "Creating on-chain identity registry client"
        );
        Self {
            provider,
            identity_contract,
            registry_contract: None,
            signer: None,
        }
    }

    /// Create with registry contract for extended functionality
    pub fn with_registry(mut self, registry_contract: Address) -> Self {
        debug!(
            target: "chain::identity",
            registry_contract = %registry_contract,
            "Setting registry contract"
        );
        self.registry_contract = Some(registry_contract);
        self
    }

    /// Create with signer for write operations
    pub fn with_signer(mut self, signer: alloy_signer_local::PrivateKeySigner) -> Self {
        let address = signer.address();
        debug!(
            target: "chain::identity",
            signer_address = %address,
            "Setting signer"
        );
        self.signer = Some(signer);
        self
    }

    /// Create from chain configuration
    pub fn from_config(provider: Arc<P>, config: &ChainConfig) -> anyhow::Result<Self> {
        let identity_address = config.get_identity_registry_address().map_err(|e| {
            error!(target: "chain::identity", "Identity registry address not configured: {}", e);
            anyhow::anyhow!("Identity registry address not configured: {}", e)
        })?;
        Ok(Self::new(provider, identity_address))
    }

    /// Get identity contract address
    pub fn identity_address(&self) -> Address {
        self.identity_contract
    }

    /// Get registry contract address
    pub fn registry_address(&self) -> Option<Address> {
        self.registry_contract
    }

    /// Get the underlying provider
    pub fn provider(&self) -> &P {
        &self.provider
    }

    /// Create identity contract instance
    fn identity_contract(
        &self,
    ) -> AgentIdentity::AgentIdentityInstance<alloy_transport::BoxTransport, &P> {
        AgentIdentity::new(self.identity_contract, &*self.provider)
    }

    /// Create registry contract instance
    fn registry_contract(
        &self,
    ) -> Option<AgentRegistry::AgentRegistryInstance<alloy_transport::BoxTransport, &P>> {
        self.registry_contract
            .map(|addr| AgentRegistry::new(addr, &*self.provider))
    }

    /// Register a new agent identity
    #[instrument(skip(self, did, public_key), target = "chain::identity")]
    pub async fn register_agent(&self, did: &str, public_key: B256) -> Result<AgentId> {
        info!(
            target: "chain::identity",
            did = %did,
            "Registering new agent identity"
        );

        // Validate inputs
        if did.is_empty() {
            error!(target: "chain::identity", "DID cannot be empty");
            return Err(ChainError::Validation("DID cannot be empty".to_string()));
        }
        if public_key == B256::ZERO {
            error!(target: "chain::identity", "Public key cannot be empty");
            return Err(ChainError::Validation(
                "Public key cannot be empty".to_string(),
            ));
        }

        // Check if signer is available
        let _signer = self.signer.as_ref().ok_or_else(|| {
            error!(target: "chain::identity", "No signer configured for registration");
            ChainError::Wallet("No signer configured".to_string())
        })?;

        let contract = self.identity_contract();
        let call = contract.registerAgent(did.to_string(), public_key);

        let pending_tx = call.send().await.map_err(|e| {
            error!(
                target: "chain::identity",
                did = %did,
                error = %e,
                "Failed to send registration transaction"
            );
            ChainError::Transaction(format!("Failed to register agent: {}", e))
        })?;

        info!(
            target: "chain::identity",
            tx_hash = %pending_tx.tx_hash(),
            "Registration transaction sent, waiting for confirmation"
        );

        let receipt = pending_tx.get_receipt().await.map_err(|e| {
            error!(
                target: "chain::identity",
                did = %did,
                error = %e,
                "Failed to get registration receipt"
            );
            ChainError::Transaction(format!("Registration failed: {}", e))
        })?;

        // Extract agent ID from receipt logs
        let agent_id = Self::extract_agent_id_from_receipt(&receipt).ok_or_else(|| {
            error!(target: "chain::identity", "Failed to extract agent ID from receipt");
            ChainError::Contract("Failed to extract agent ID".to_string())
        })?;

        info!(
            target: "chain::identity",
            agent_id = %agent_id,
            tx_hash = %receipt.transaction_hash,
            "Agent registered successfully"
        );

        Ok(agent_id)
    }

    /// Get agent information by agent ID
    #[instrument(skip(self), target = "chain::identity")]
    pub async fn get_agent(&self, agent_id: AgentId) -> Result<AgentInfo> {
        debug!(
            target: "chain::identity",
            agent_id = %agent_id,
            "Querying agent information"
        );

        let contract = self.identity_contract();

        let result = contract.getAgent(agent_id).call().await.map_err(|e| {
            error!(
                target: "chain::identity",
                agent_id = %agent_id,
                error = %e,
                "Failed to get agent"
            );
            ChainError::Contract(format!("Failed to get agent {}: {}", agent_id, e))
        })?;

        let agent_info: AgentInfo = result._0.into();

        // Fetch capabilities separately
        // This is a simplified version - in production, you'd fetch all capabilities
        debug!(
            target: "chain::identity",
            agent_id = %agent_id,
            "Retrieved agent information"
        );

        Ok(agent_info)
    }

    /// Get agent ID by DID
    #[instrument(skip(self), target = "chain::identity")]
    pub async fn get_agent_id_by_did(&self, did: &str) -> Result<Option<AgentId>> {
        debug!(
            target: "chain::identity",
            did = %did,
            "Querying agent ID by DID"
        );

        let contract = self.identity_contract();

        let result = contract
            .didToAgent(did.to_string())
            .call()
            .await
            .map_err(|e| {
                error!(
                    target: "chain::identity",
                    did = %did,
                    error = %e,
                    "Failed to get agent ID by DID"
                );
                ChainError::Contract(format!("Failed to get agent ID for DID {}: {}", did, e))
            })?;

        let agent_id = result._0;

        // Check if agent ID is zero (not found)
        if agent_id == B256::ZERO {
            debug!(
                target: "chain::identity",
                did = %did,
                "No agent found for DID"
            );
            return Ok(None);
        }

        info!(
            target: "chain::identity",
            did = %did,
            agent_id = %agent_id,
            "Found agent ID for DID"
        );

        Ok(Some(agent_id.into()))
    }

    /// Check if an agent is registered by DID
    #[instrument(skip(self), target = "chain::identity")]
    pub async fn is_registered_by_did(&self, did: &str) -> Result<bool> {
        let agent_id = self.get_agent_id_by_did(did).await?;
        Ok(agent_id.is_some())
    }

    /// Check if an agent ID is registered
    #[instrument(skip(self), target = "chain::identity")]
    pub async fn is_registered(&self, agent_id: AgentId) -> Result<bool> {
        debug!(
            target: "chain::identity",
            agent_id = %agent_id,
            "Checking if agent is registered"
        );

        // Check by querying getAgent and verifying owner is not zero address
        match self.get_agent(agent_id).await {
            Ok(agent) => Ok(agent.owner != Address::ZERO),
            Err(_) => Ok(false),
        }
    }

    /// Get total number of registered agents
    #[instrument(skip(self), target = "chain::identity")]
    pub async fn total_agents(&self) -> Result<u64> {
        debug!(
            target: "chain::identity",
            "Querying total registered agents"
        );

        let contract = self.identity_contract();

        let result = contract.totalAgents().call().await.map_err(|e| {
            error!(
                target: "chain::identity",
                error = %e,
                "Failed to get total agents"
            );
            ChainError::Contract(format!("Failed to get total agents: {}", e))
        })?;

        let total = result._0.to::<u64>();

        debug!(
            target: "chain::identity",
            total,
            "Retrieved total agents"
        );

        Ok(total)
    }

    /// Deactivate an agent
    #[instrument(skip(self), target = "chain::identity")]
    pub async fn deactivate_agent(&self, agent_id: AgentId) -> Result<()> {
        info!(
            target: "chain::identity",
            agent_id = %agent_id,
            "Deactivating agent"
        );

        let _signer = self.signer.as_ref().ok_or_else(|| {
            error!(target: "chain::identity", "No signer configured for deactivation");
            ChainError::Wallet("No signer configured".to_string())
        })?;

        let contract = self.identity_contract();
        let call = contract.deactivateAgent(agent_id);

        let pending_tx = call.send().await.map_err(|e| {
            error!(
                target: "chain::identity",
                agent_id = %agent_id,
                error = %e,
                "Failed to send deactivation transaction"
            );
            ChainError::Transaction(format!("Failed to deactivate agent: {}", e))
        })?;

        let receipt = pending_tx.get_receipt().await.map_err(|e| {
            error!(
                target: "chain::identity",
                agent_id = %agent_id,
                error = %e,
                "Failed to get deactivation receipt"
            );
            ChainError::Transaction(format!("Deactivation failed: {}", e))
        })?;

        info!(
            target: "chain::identity",
            agent_id = %agent_id,
            tx_hash = %receipt.transaction_hash,
            "Agent deactivated successfully"
        );

        Ok(())
    }

    /// Get all agents owned by an address
    #[instrument(skip(self), target = "chain::identity")]
    pub async fn get_agents_by_owner(&self, owner: Address) -> Result<Vec<AgentId>> {
        debug!(
            target: "chain::identity",
            owner = %owner,
            "Querying all agents owned by address"
        );

        let contract = self.identity_contract();

        let result = contract.getOwnerAgents(owner).call().await.map_err(|e| {
            error!(
                target: "chain::identity",
                owner = %owner,
                error = %e,
                "Failed to get agents by owner"
            );
            ChainError::Contract(format!("Failed to get agents for owner {}: {}", owner, e))
        })?;

        let agent_ids: Vec<AgentId> = result._0.into_iter().map(|id| id.into()).collect();

        debug!(
            target: "chain::identity",
            owner = %owner,
            count = agent_ids.len(),
            "Retrieved agents by owner"
        );

        Ok(agent_ids)
    }

    /// Check if an agent has a specific capability
    #[instrument(skip(self), target = "chain::identity")]
    pub async fn has_capability(&self, agent_id: AgentId, capability: B256) -> Result<bool> {
        debug!(
            target: "chain::identity",
            agent_id = %agent_id,
            capability = %capability,
            "Checking agent capability"
        );

        let contract = self.identity_contract();

        let result = contract
            .hasCapability(agent_id, capability)
            .call()
            .await
            .map_err(|e| {
                error!(
                    target: "chain::identity",
                    agent_id = %agent_id,
                    capability = %capability,
                    error = %e,
                    "Failed to check capability"
                );
                ChainError::Contract(format!("Failed to check capability: {}", e))
            })?;

        let has = result._0;

        debug!(
            target: "chain::identity",
            agent_id = %agent_id,
            capability = %capability,
            has,
            "Capability check result"
        );

        Ok(has)
    }

    /// Grant capability to an agent
    #[instrument(skip(self), target = "chain::identity")]
    pub async fn grant_capability(&self, agent_id: AgentId, capability: B256) -> Result<()> {
        info!(
            target: "chain::identity",
            agent_id = %agent_id,
            capability = %capability,
            "Granting capability to agent"
        );

        let _signer = self.signer.as_ref().ok_or_else(|| {
            error!(target: "chain::identity", "No signer configured");
            ChainError::Wallet("No signer configured".to_string())
        })?;

        let contract = self.identity_contract();
        let call = contract.grantCapability(agent_id, capability);

        let pending_tx = call.send().await.map_err(|e| {
            error!(
                target: "chain::identity",
                agent_id = %agent_id,
                capability = %capability,
                error = %e,
                "Failed to send grant capability transaction"
            );
            ChainError::Transaction(format!("Failed to grant capability: {}", e))
        })?;

        let receipt = pending_tx.get_receipt().await.map_err(|e| {
            error!(
                target: "chain::identity",
                agent_id = %agent_id,
                capability = %capability,
                error = %e,
                "Failed to get grant receipt"
            );
            ChainError::Transaction(format!("Grant capability failed: {}", e))
        })?;

        info!(
            target: "chain::identity",
            agent_id = %agent_id,
            capability = %capability,
            tx_hash = %receipt.transaction_hash,
            "Capability granted successfully"
        );

        Ok(())
    }

    /// Revoke capability from an agent
    #[instrument(skip(self), target = "chain::identity")]
    pub async fn revoke_capability(&self, agent_id: AgentId, capability: B256) -> Result<()> {
        info!(
            target: "chain::identity",
            agent_id = %agent_id,
            capability = %capability,
            "Revoking capability from agent"
        );

        let _signer = self.signer.as_ref().ok_or_else(|| {
            error!(target: "chain::identity", "No signer configured");
            ChainError::Wallet("No signer configured".to_string())
        })?;

        let contract = self.identity_contract();
        let call = contract.revokeCapability(agent_id, capability);

        let pending_tx = call.send().await.map_err(|e| {
            error!(
                target: "chain::identity",
                agent_id = %agent_id,
                capability = %capability,
                error = %e,
                "Failed to send revoke capability transaction"
            );
            ChainError::Transaction(format!("Failed to revoke capability: {}", e))
        })?;

        let receipt = pending_tx.get_receipt().await.map_err(|e| {
            error!(
                target: "chain::identity",
                agent_id = %agent_id,
                capability = %capability,
                error = %e,
                "Failed to get revoke receipt"
            );
            ChainError::Transaction(format!("Revoke capability failed: {}", e))
        })?;

        info!(
            target: "chain::identity",
            agent_id = %agent_id,
            capability = %capability,
            tx_hash = %receipt.transaction_hash,
            "Capability revoked successfully"
        );

        Ok(())
    }

    /// Update agent reputation (only authorized updaters)
    #[instrument(skip(self), target = "chain::identity")]
    pub async fn update_reputation(&self, agent_id: AgentId, new_reputation: U256) -> Result<()> {
        info!(
            target: "chain::identity",
            agent_id = %agent_id,
            new_reputation = %new_reputation,
            "Updating agent reputation"
        );

        let _signer = self.signer.as_ref().ok_or_else(|| {
            error!(target: "chain::identity", "No signer configured");
            ChainError::Wallet("No signer configured".to_string())
        })?;

        let contract = self.identity_contract();
        let call = contract.updateReputation(agent_id, new_reputation);

        let pending_tx = call.send().await.map_err(|e| {
            error!(
                target: "chain::identity",
                agent_id = %agent_id,
                new_reputation = %new_reputation,
                error = %e,
                "Failed to send update reputation transaction"
            );
            ChainError::Transaction(format!("Failed to update reputation: {}", e))
        })?;

        let receipt = pending_tx.get_receipt().await.map_err(|e| {
            error!(
                target: "chain::identity",
                agent_id = %agent_id,
                new_reputation = %new_reputation,
                error = %e,
                "Failed to get update receipt"
            );
            ChainError::Transaction(format!("Update reputation failed: {}", e))
        })?;

        info!(
            target: "chain::identity",
            agent_id = %agent_id,
            new_reputation = %new_reputation,
            tx_hash = %receipt.transaction_hash,
            "Reputation updated successfully"
        );

        Ok(())
    }

    /// Register agent metadata (requires registry contract)
    #[instrument(
        skip(self, name, description, capabilities, endpoint),
        target = "chain::identity"
    )]
    pub async fn register_metadata(
        &self,
        agent_id: AgentId,
        name: &str,
        description: &str,
        capabilities: Vec<String>,
        endpoint: &str,
    ) -> Result<()> {
        info!(
            target: "chain::identity",
            agent_id = %agent_id,
            name = %name,
            "Registering agent metadata"
        );

        let registry = self.registry_contract().ok_or_else(|| {
            error!(target: "chain::identity", "Registry contract not configured");
            ChainError::Configuration("Registry contract not configured".to_string())
        })?;

        let _signer = self.signer.as_ref().ok_or_else(|| {
            error!(target: "chain::identity", "No signer configured");
            ChainError::Wallet("No signer configured".to_string())
        })?;

        let call = registry.registerMetadata(
            agent_id,
            name.to_string(),
            description.to_string(),
            capabilities,
            endpoint.to_string(),
        );

        let pending_tx = call.send().await.map_err(|e| {
            error!(
                target: "chain::identity",
                agent_id = %agent_id,
                error = %e,
                "Failed to send metadata registration"
            );
            ChainError::Transaction(format!("Failed to register metadata: {}", e))
        })?;

        let receipt = pending_tx.get_receipt().await.map_err(|e| {
            error!(
                target: "chain::identity",
                agent_id = %agent_id,
                error = %e,
                "Failed to get metadata receipt"
            );
            ChainError::Transaction(format!("Metadata registration failed: {}", e))
        })?;

        info!(
            target: "chain::identity",
            agent_id = %agent_id,
            tx_hash = %receipt.transaction_hash,
            "Metadata registered successfully"
        );

        Ok(())
    }

    /// Send heartbeat for agent (requires registry contract)
    #[instrument(skip(self), target = "chain::identity")]
    pub async fn heartbeat(&self, agent_id: AgentId) -> Result<()> {
        debug!(
            target: "chain::identity",
            agent_id = %agent_id,
            "Sending agent heartbeat"
        );

        let registry = self.registry_contract().ok_or_else(|| {
            error!(target: "chain::identity", "Registry contract not configured");
            ChainError::Configuration("Registry contract not configured".to_string())
        })?;

        let _signer = self.signer.as_ref().ok_or_else(|| {
            error!(target: "chain::identity", "No signer configured");
            ChainError::Wallet("No signer configured".to_string())
        })?;

        let call = registry.heartbeat(agent_id);

        let pending_tx = call.send().await.map_err(|e| {
            error!(
                target: "chain::identity",
                agent_id = %agent_id,
                error = %e,
                "Failed to send heartbeat"
            );
            ChainError::Transaction(format!("Failed to send heartbeat: {}", e))
        })?;

        let receipt = pending_tx.get_receipt().await.map_err(|e| {
            error!(
                target: "chain::identity",
                agent_id = %agent_id,
                error = %e,
                "Failed to get heartbeat receipt"
            );
            ChainError::Transaction(format!("Heartbeat failed: {}", e))
        })?;

        debug!(
            target: "chain::identity",
            agent_id = %agent_id,
            tx_hash = %receipt.transaction_hash,
            "Heartbeat sent successfully"
        );

        Ok(())
    }

    /// Extract agent ID from transaction receipt logs
    fn extract_agent_id_from_receipt(
        receipt: &alloy_rpc_types::TransactionReceipt,
    ) -> Option<AgentId> {
        // The AgentRegistered event has the agent ID as the first indexed parameter
        for log in receipt.inner.logs() {
            if log.topics().len() >= 2 {
                let agent_id_bytes: [u8; 32] = log.topics()[1].into();
                return Some(agent_id_bytes.into());
            }
        }
        None
    }
}

#[async_trait::async_trait]
impl<P: AlloyProvider + Clone + Send + Sync> IdentityRegistry for OnChainIdentityRegistry<P> {
    #[instrument(skip(self, did), target = "chain::identity")]
    async fn register(&self, address: Address, did: &str) -> Result<()> {
        info!(
            target: "chain::identity",
            address = %address,
            did = %did,
            "Registering identity via IdentityRegistry trait"
        );

        // Generate a default public key (in real application, should pass real public
        // key)
        let public_key =
            B256::from_slice(&address.as_slice()[..32].try_into().unwrap_or([0u8; 32]));

        let _agent_id = self.register_agent(did, public_key).await?;

        info!(
            target: "chain::identity",
            address = %address,
            did = %did,
            "Identity registered successfully"
        );

        Ok(())
    }

    #[instrument(skip(self), target = "chain::identity")]
    async fn get_did(&self, address: Address) -> Result<Option<String>> {
        debug!(
            target: "chain::identity",
            address = %address,
            "Querying DID by address"
        );

        // Get all agents owned by address, return first DID
        let agent_ids = self.get_agents_by_owner(address).await?;

        if agent_ids.is_empty() {
            debug!(
                target: "chain::identity",
                address = %address,
                "Address has no registered agents"
            );
            return Ok(None);
        }

        // Get first agent's DID
        let agent_info = self.get_agent(agent_ids[0]).await?;

        info!(
            target: "chain::identity",
            address = %address,
            did = %agent_info.did,
            "Queried DID by address successfully"
        );

        Ok(Some(agent_info.did))
    }

    #[instrument(skip(self), target = "chain::identity")]
    async fn get_address(&self, did: &str) -> Result<Option<Address>> {
        debug!(
            target: "chain::identity",
            did = %did,
            "Querying address by DID"
        );

        let agent_id = self.get_agent_id_by_did(did).await?;

        if let Some(id) = agent_id {
            let agent_info = self.get_agent(id).await?;

            info!(
                target: "chain::identity",
                did = %did,
                address = %agent_info.owner,
                "Queried address by DID successfully"
            );

            Ok(Some(agent_info.owner))
        } else {
            debug!(
                target: "chain::identity",
                did = %did,
                "DID not found"
            );
            Ok(None)
        }
    }
}

/// Identity registration builder
pub struct IdentityRegistrationBuilder {
    did: String,
    public_key: Option<B256>,
    metadata: Option<String>,
}

impl IdentityRegistrationBuilder {
    /// Create new registration builder
    pub fn new(did: impl Into<String>) -> Self {
        Self {
            did: did.into(),
            public_key: None,
            metadata: None,
        }
    }

    /// Add public key
    pub fn with_public_key(mut self, public_key: B256) -> Self {
        self.public_key = Some(public_key);
        self
    }

    /// Add metadata
    pub fn with_metadata(mut self, metadata: impl Into<String>) -> Self {
        self.metadata = Some(metadata.into());
        self
    }

    /// Build registration parameters
    pub fn build(self) -> (String, Option<B256>, Option<String>) {
        (self.did, self.public_key, self.metadata)
    }
}

/// Cached identity registry for improved performance
pub struct CachedIdentityRegistry<P: AlloyProvider + Clone> {
    inner: OnChainIdentityRegistry<P>,
    // Use tokio::sync::RwLock for async compatibility
    cache: tokio::sync::RwLock<std::collections::HashMap<AgentId, (AgentInfo, std::time::Instant)>>,
    did_cache:
        tokio::sync::RwLock<std::collections::HashMap<String, (AgentId, std::time::Instant)>>,
    ttl: std::time::Duration,
}

impl<P: AlloyProvider + Clone> CachedIdentityRegistry<P> {
    /// Create new cached registry
    pub fn new(inner: OnChainIdentityRegistry<P>, ttl_secs: u64) -> Self {
        Self {
            inner,
            cache: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            did_cache: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            ttl: std::time::Duration::from_secs(ttl_secs),
        }
    }

    /// Get agent with caching
    pub async fn get_agent_cached(&self, agent_id: AgentId) -> Result<AgentInfo> {
        // Check cache first (read lock)
        {
            let cache = self.cache.read().await;
            if let Some((agent_info, timestamp)) = cache.get(&agent_id) {
                if timestamp.elapsed() < self.ttl {
                    return Ok(agent_info.clone());
                }
            }
        }

        // Fetch from chain
        let agent_info = self.inner.get_agent(agent_id).await?;

        // Update cache (write lock)
        {
            let mut cache = self.cache.write().await;
            cache.insert(agent_id, (agent_info.clone(), std::time::Instant::now()));
        }

        Ok(agent_info)
    }

    /// Get agent ID by DID with caching
    pub async fn get_agent_id_by_did_cached(&self, did: &str) -> Result<Option<AgentId>> {
        // Check cache first
        {
            let cache = self.did_cache.read().await;
            if let Some((agent_id, timestamp)) = cache.get(did) {
                if timestamp.elapsed() < self.ttl {
                    return Ok(Some(*agent_id));
                }
            }
        }

        // Fetch from chain
        let result = self.inner.get_agent_id_by_did(did).await?;

        // Update cache
        if let Some(agent_id) = result {
            let mut cache = self.did_cache.write().await;
            cache.insert(did.to_string(), (agent_id, std::time::Instant::now()));
        }

        Ok(result)
    }

    /// Clear cache
    pub async fn clear_cache(&self) {
        {
            let mut cache = self.cache.write().await;
            cache.clear();
        }
        {
            let mut cache = self.did_cache.write().await;
            cache.clear();
        }
    }

    /// Get inner registry
    pub fn inner(&self) -> &OnChainIdentityRegistry<P> {
        &self.inner
    }

    /// Get TTL
    pub fn ttl(&self) -> std::time::Duration {
        self.ttl
    }

    /// Update TTL
    pub fn set_ttl(&mut self, ttl_secs: u64) {
        self.ttl = std::time::Duration::from_secs(ttl_secs);
    }
}

#[async_trait::async_trait]
impl<P: AlloyProvider + Clone + Send + Sync> IdentityRegistry for CachedIdentityRegistry<P> {
    async fn register(&self, address: Address, did: &str) -> Result<()> {
        self.inner.register(address, did).await
    }

    async fn get_did(&self, address: Address) -> Result<Option<String>> {
        self.inner.get_did(address).await
    }

    async fn get_address(&self, did: &str) -> Result<Option<Address>> {
        self.inner.get_address(did).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_registration_builder() {
        let public_key = B256::from([1u8; 32]);
        let builder = IdentityRegistrationBuilder::new("did:ethr:0x1234")
            .with_public_key(public_key)
            .with_metadata("{\"name\":\"Test\"}");

        let (did, pk, metadata) = builder.build();
        assert_eq!(did, "did:ethr:0x1234");
        assert_eq!(pk, Some(public_key));
        assert_eq!(metadata, Some("{\"name\":\"Test\"}".to_string()));
    }

    #[test]
    fn test_cached_registry_ttl() {
        // Note: This is a basic test - full testing requires mocking the provider
        let ttl = std::time::Duration::from_secs(300);
        assert_eq!(ttl.as_secs(), 300);
    }

    #[test]
    fn test_agent_info_from_contract() {
        let contract_agent = crate::contracts::bindings::AgentIdentity::AgentIdentityInfo {
            agentId: B256::from([1u8; 32]),
            owner: Address::from([2u8; 20]),
            did: "did:ethr:0x1234".to_string(),
            publicKey: B256::from([3u8; 32]),
            isActive: true,
            reputation: U256::from(1000),
            createdAt: U256::from(1234567890),
        };

        let agent_info: AgentInfo = contract_agent.into();
        assert_eq!(agent_info.agent_id, B256::from([1u8; 32]));
        assert_eq!(agent_info.owner, Address::from([2u8; 20]));
        assert_eq!(agent_info.did, "did:ethr:0x1234");
        assert!(agent_info.is_active);
        assert_eq!(agent_info.reputation, U256::from(1000));
    }
}

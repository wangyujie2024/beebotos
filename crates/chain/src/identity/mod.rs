//! Identity Module

use serde::{Deserialize, Serialize};

use crate::compat::Address;
use crate::Result;

/// DID Document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DIDDocument {
    pub id: String,
    pub controller: Option<String>,
    pub verification_methods: Vec<VerificationMethod>,
    pub authentication: Vec<String>,
    pub assertion_method: Vec<String>,
}

/// Verification method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationMethod {
    pub id: String,
    pub type_: String,
    pub controller: String,
    pub public_key: Vec<u8>,
}

/// DID Resolver trait
#[async_trait::async_trait]
pub trait DIDResolver: Send + Sync {
    async fn resolve(&self, did: &str) -> Result<Option<DIDDocument>>;
}

/// Identity registry trait
#[async_trait::async_trait]
pub trait IdentityRegistry: Send + Sync {
    /// Register identity
    async fn register(&self, address: Address, did: &str) -> Result<()>;

    /// Get DID for address
    async fn get_did(&self, address: Address) -> Result<Option<String>>;

    /// Get address for DID
    async fn get_address(&self, did: &str) -> Result<Option<Address>>;
}

pub mod credentials;
pub mod did;
pub mod registry;
pub mod resolver;

// Re-export main types
pub use registry::OnChainIdentityRegistry;
pub use resolver::SimpleDIDResolver;

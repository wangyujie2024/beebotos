//! Credential encryption helpers for channel configurations.
//!
//! The actual encryption should be performed by the caller (Gateway/CLI)
//! using `crates/crypto` or any other trusted implementation.  This module
//! provides a type alias and a no-op helper for testing.

use std::sync::Arc;

use crate::communication::user_channel::UserChannelConfig;
use crate::error::Result;

/// Encryptor trait for `UserChannelConfig`.
/// Implementations are expected to serialize the config to JSON (or other
/// canonical representation), encrypt it, and return a base64-encoded or
/// otherwise self-describing ciphertext string.
pub type ChannelConfigEncryptor = Arc<dyn Fn(&UserChannelConfig) -> Result<String> + Send + Sync>;

/// Returns a no-op encryptor that simply JSON-serializes the config.
///
/// ⚠️ **Do not use in production** – this stores credentials in plaintext.
pub fn plaintext_encryptor() -> ChannelConfigEncryptor {
    Arc::new(|config| {
        serde_json::to_string(config)
            .map_err(|e| crate::error::AgentError::serialization(e.to_string()))
    })
}

//! Generic Contract Interface for EVM Chains
//!
//! Provides chain-agnostic contract interaction primitives.

use crate::chains::common::EvmError;
use crate::compat::Address;

/// Contract instance reference
#[derive(Debug, Clone)]
pub struct ContractInstance {
    address: Address,
    abi: Option<serde_json::Value>,
}

impl ContractInstance {
    pub fn new(address: Address) -> Self {
        Self { address, abi: None }
    }

    pub fn with_abi(mut self, abi: serde_json::Value) -> Self {
        self.abi = Some(abi);
        self
    }

    pub fn address(&self) -> Address {
        self.address
    }

    pub fn abi(&self) -> Option<&serde_json::Value> {
        self.abi.as_ref()
    }
}

/// Contract call specification
#[derive(Debug, Clone)]
pub struct ContractCall {
    to: Address,
    data: Vec<u8>,
    value: Option<alloy_primitives::U256>,
    gas_limit: Option<u64>,
}

impl ContractCall {
    pub fn new(to: Address, data: Vec<u8>) -> Self {
        Self {
            to,
            data,
            value: None,
            gas_limit: None,
        }
    }

    pub fn with_value(mut self, value: alloy_primitives::U256) -> Self {
        self.value = Some(value);
        self
    }

    pub fn with_gas_limit(mut self, gas: u64) -> Self {
        self.gas_limit = Some(gas);
        self
    }

    pub fn to(&self) -> Address {
        self.to
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn value(&self) -> Option<alloy_primitives::U256> {
        self.value
    }

    pub fn gas_limit(&self) -> Option<u64> {
        self.gas_limit
    }
}

/// Contract deployment specification
#[derive(Debug, Clone)]
pub struct ContractDeploy {
    bytecode: Vec<u8>,
    constructor_args: Vec<u8>,
    salt: Option<[u8; 32]>, // For CREATE2
}

impl ContractDeploy {
    pub fn new(bytecode: Vec<u8>) -> Self {
        Self {
            bytecode,
            constructor_args: Vec::new(),
            salt: None,
        }
    }

    pub fn with_args(mut self, args: Vec<u8>) -> Self {
        self.constructor_args = args;
        self
    }

    pub fn with_salt(mut self, salt: [u8; 32]) -> Self {
        self.salt = Some(salt);
        self
    }

    pub fn bytecode(&self) -> &[u8] {
        &self.bytecode
    }

    pub fn constructor_args(&self) -> &[u8] {
        &self.constructor_args
    }

    pub fn salt(&self) -> Option<[u8; 32]> {
        self.salt
    }

    /// Get deployment data (bytecode + constructor args)
    pub fn deployment_data(&self) -> Vec<u8> {
        let mut data = self.bytecode.clone();
        data.extend_from_slice(&self.constructor_args);
        data
    }
}

/// Contract interface trait - implemented by all chain clients
#[async_trait::async_trait]
pub trait ContractInterface {
    /// Call a contract method (read-only)
    async fn call(&self, call: &ContractCall) -> Result<Vec<u8>, EvmError>;

    /// Send a transaction to a contract (state-changing)
    async fn send(&self, call: &ContractCall) -> Result<String, EvmError>;

    /// Deploy a contract
    async fn deploy(&self, deploy: &ContractDeploy) -> Result<Address, EvmError>;

    /// Estimate gas for a call
    async fn estimate_gas(&self, call: &ContractCall) -> Result<u64, EvmError>;
}

/// Get events helper
pub async fn get_events<D: alloy_sol_types::SolEvent>(
    _from_block: u64,
    _to_block: u64,
) -> anyhow::Result<Vec<D>> {
    // Implementation will be added
    Ok(Vec::new())
}

/// Decode contract return data
pub fn decode_return_data<T: alloy_sol_types::SolType>(
    data: &[u8],
) -> Result<T::RustType, EvmError> {
    T::abi_decode(data, false).map_err(|e| EvmError::ContractError(format!("Decode error: {}", e)))
}

/// Encode contract call data
pub fn encode_call_data<T: alloy_sol_types::SolCall>(call: &T) -> Vec<u8> {
    call.abi_encode()
}

/// Compute CREATE2 address
pub fn compute_create2_address(
    deployer: Address,
    salt: [u8; 32],
    bytecode_hash: [u8; 32],
) -> Address {
    use alloy_primitives::keccak256;
    // use alloy_sol_types::SolValue;

    // CREATE2 address = keccak256(0xff ++ deployer ++ salt ++
    // keccak256(bytecode))[12:]
    let mut data = Vec::with_capacity(85);
    data.push(0xff);
    data.extend_from_slice(deployer.as_slice());
    data.extend_from_slice(&salt);
    data.extend_from_slice(&bytecode_hash);

    let hash = keccak256(&data);
    Address::from_slice(&hash[12..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contract_call_builder() {
        let addr = Address::ZERO;
        let data = vec![0x12, 0x34];
        let call = ContractCall::new(addr, data.clone()).with_gas_limit(100_000);

        assert_eq!(call.to(), addr);
        assert_eq!(call.data(), data.as_slice());
        assert_eq!(call.gas_limit(), Some(100_000));
    }

    #[test]
    fn test_contract_deploy() {
        let bytecode = vec![0x60, 0x80, 0x60]; // Simple bytecode
        let args = vec![0x00, 0x01];

        let deploy = ContractDeploy::new(bytecode.clone()).with_args(args.clone());

        assert_eq!(deploy.bytecode(), bytecode.as_slice());
        assert_eq!(deploy.constructor_args(), args.as_slice());

        let data = deploy.deployment_data();
        assert_eq!(data.len(), bytecode.len() + args.len());
    }
}

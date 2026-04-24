//! Mock Provider for Testing
//!
//! Provides a mock implementation for unit tests.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use alloy_primitives::{Address, B256, U256};
use alloy_rpc_types::{
    Block, BlockNumberOrTag, BlockTransactionsKind, Filter, Log, Transaction, TransactionReceipt,
    TransactionRequest,
};
use alloy_transport::TransportError;

/// Mock provider for testing
#[derive(Debug, Clone)]
pub struct MockProvider {
    state: Arc<Mutex<MockProviderState>>,
}

#[derive(Debug)]
struct MockProviderState {
    accounts: Vec<Address>,
    block_number: u64,
    chain_id: u64,
    gas_price: u128,
    balance: HashMap<Address, U256>,
    logs: Vec<Log>,
    transactions: HashMap<B256, Transaction>,
    receipts: HashMap<B256, TransactionReceipt>,
    blocks: HashMap<u64, Block>,
    return_errors: bool,
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockProvider {
    /// Create a new mock provider with default values
    pub fn new() -> Self {
        let state = MockProviderState {
            accounts: vec![
                Address::from_str("0x1234567890123456789012345678901234567890").unwrap(),
                Address::from_str("0x0987654321098765432109876543210987654321").unwrap(),
            ],
            block_number: 1000,
            chain_id: 1,
            gas_price: 20_000_000_000, // 20 gwei
            balance: HashMap::new(),
            logs: Vec::new(),
            transactions: HashMap::new(),
            receipts: HashMap::new(),
            blocks: HashMap::new(),
            return_errors: false,
        };

        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }

    /// Set the block number
    pub fn set_block_number(&self, number: u64) {
        self.state.lock().unwrap().block_number = number;
    }

    /// Set the chain ID
    pub fn set_chain_id(&self, chain_id: u64) {
        self.state.lock().unwrap().chain_id = chain_id;
    }

    /// Set the gas price
    pub fn set_gas_price(&self, price: u128) {
        self.state.lock().unwrap().gas_price = price;
    }

    /// Set balance for an address
    pub fn set_balance(&self, address: Address, balance: U256) {
        self.state.lock().unwrap().balance.insert(address, balance);
    }

    /// Add a log entry
    pub fn add_log(&self, log: Log) {
        self.state.lock().unwrap().logs.push(log);
    }

    /// Set return errors flag
    pub fn set_return_errors(&self, return_errors: bool) {
        self.state.lock().unwrap().return_errors = return_errors;
    }

    /// Get current state
    pub fn state(&self) -> MockProviderState {
        self.state.lock().unwrap().clone()
    }

    /// Get accounts
    pub async fn get_accounts(&self) -> Result<Vec<Address>, TransportError> {
        let state = self.state.lock().unwrap();
        if state.return_errors {
            return Err(TransportError::local_usage_str("Mock error"));
        }
        Ok(state.accounts.clone())
    }

    /// Get block number
    pub async fn get_block_number(&self) -> Result<u64, TransportError> {
        let state = self.state.lock().unwrap();
        if state.return_errors {
            return Err(TransportError::local_usage_str("Mock error"));
        }
        Ok(state.block_number)
    }

    /// Get chain ID
    pub async fn get_chain_id(&self) -> Result<u64, TransportError> {
        let state = self.state.lock().unwrap();
        if state.return_errors {
            return Err(TransportError::local_usage_str("Mock error"));
        }
        Ok(state.chain_id)
    }

    /// Get gas price
    pub async fn get_gas_price(&self) -> Result<u128, TransportError> {
        let state = self.state.lock().unwrap();
        if state.return_errors {
            return Err(TransportError::local_usage_str("Mock error"));
        }
        Ok(state.gas_price)
    }

    /// Get balance for an address
    pub async fn get_balance(&self, address: Address) -> Result<U256, TransportError> {
        let state = self.state.lock().unwrap();
        if state.return_errors {
            return Err(TransportError::local_usage_str("Mock error"));
        }
        Ok(state.balance.get(&address).copied().unwrap_or(U256::ZERO))
    }

    /// Get logs
    pub async fn get_logs(&self, _filter: &Filter) -> Result<Vec<Log>, TransportError> {
        let state = self.state.lock().unwrap();
        if state.return_errors {
            return Err(TransportError::local_usage_str("Mock error"));
        }
        Ok(state.logs.clone())
    }

    /// Estimate gas
    pub async fn estimate_gas(&self, _tx: &TransactionRequest) -> Result<u64, TransportError> {
        let state = self.state.lock().unwrap();
        if state.return_errors {
            return Err(TransportError::local_usage_str("Mock error"));
        }
        Ok(21000) // Standard gas limit for simple transfer
    }

    /// Get transaction receipt
    pub async fn get_transaction_receipt(
        &self,
        tx_hash: B256,
    ) -> Result<Option<TransactionReceipt>, TransportError> {
        let state = self.state.lock().unwrap();
        if state.return_errors {
            return Err(TransportError::local_usage_str("Mock error"));
        }
        Ok(state.receipts.get(&tx_hash).cloned())
    }

    /// Get transaction by hash
    pub async fn get_transaction_by_hash(
        &self,
        tx_hash: B256,
    ) -> Result<Option<Transaction>, TransportError> {
        let state = self.state.lock().unwrap();
        if state.return_errors {
            return Err(TransportError::local_usage_str("Mock error"));
        }
        Ok(state.transactions.get(&tx_hash).cloned())
    }

    /// Get block by number
    pub async fn get_block_by_number(
        &self,
        number: BlockNumberOrTag,
        _tx_kind: BlockTransactionsKind,
    ) -> Result<Option<Block>, TransportError> {
        let state = self.state.lock().unwrap();
        if state.return_errors {
            return Err(TransportError::local_usage_str("Mock error"));
        }

        let block_num = match number {
            BlockNumberOrTag::Number(n) => n,
            BlockNumberOrTag::Latest => state.block_number,
            BlockNumberOrTag::Earliest => 0,
            BlockNumberOrTag::Pending => state.block_number + 1,
            BlockNumberOrTag::Safe => state.block_number.saturating_sub(12),
            BlockNumberOrTag::Finalized => state.block_number.saturating_sub(12),
        };

        Ok(state.blocks.get(&block_num).cloned())
    }
}

// Clone implementation for state
impl Clone for MockProviderState {
    fn clone(&self) -> Self {
        Self {
            accounts: self.accounts.clone(),
            block_number: self.block_number,
            chain_id: self.chain_id,
            gas_price: self.gas_price,
            balance: self.balance.clone(),
            logs: self.logs.clone(),
            transactions: self.transactions.clone(),
            receipts: self.receipts.clone(),
            blocks: self.blocks.clone(),
            return_errors: self.return_errors,
        }
    }
}

/// Builder for creating mock provider scenarios
pub struct MockProviderBuilder {
    provider: MockProvider,
}

impl Default for MockProviderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl MockProviderBuilder {
    pub fn new() -> Self {
        Self {
            provider: MockProvider::new(),
        }
    }

    pub fn with_chain_id(mut self, chain_id: u64) -> Self {
        self.provider.set_chain_id(chain_id);
        self
    }

    pub fn with_block_number(mut self, block_number: u64) -> Self {
        self.provider.set_block_number(block_number);
        self
    }

    pub fn with_gas_price(mut self, gas_price: u128) -> Self {
        self.provider.set_gas_price(gas_price);
        self
    }

    pub fn with_balance(mut self, address: Address, balance: U256) -> Self {
        self.provider.set_balance(address, balance);
        self
    }

    pub fn with_log(mut self, log: Log) -> Self {
        self.provider.add_log(log);
        self
    }

    pub fn with_error_mode(mut self, return_errors: bool) -> Self {
        self.provider.set_return_errors(return_errors);
        self
    }

    pub fn build(self) -> MockProvider {
        self.provider
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_provider_basic() {
        let provider = MockProvider::new();

        let block_number = provider.get_block_number().await.unwrap();
        assert_eq!(block_number, 1000);

        let chain_id = provider.get_chain_id().await.unwrap();
        assert_eq!(chain_id, 1);

        let gas_price = provider.get_gas_price().await.unwrap();
        assert_eq!(gas_price, 20_000_000_000);
    }

    #[tokio::test]
    async fn test_mock_provider_balance() {
        let provider = MockProvider::new();
        let address = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();

        // Default balance is 0
        let balance = provider.get_balance(address).await.unwrap();
        assert_eq!(balance, U256::ZERO);

        // Set balance
        provider.set_balance(address, U256::from(1000));
        let balance = provider.get_balance(address).await.unwrap();
        assert_eq!(balance, U256::from(1000));
    }

    #[tokio::test]
    async fn test_mock_provider_builder() {
        let provider = MockProviderBuilder::new()
            .with_chain_id(1337)
            .with_block_number(5000)
            .with_gas_price(50_000_000_000u128)
            .build();

        assert_eq!(provider.get_chain_id().await.unwrap(), 1337);
        assert_eq!(provider.get_block_number().await.unwrap(), 5000);
        assert_eq!(provider.get_gas_price().await.unwrap(), 50_000_000_000);
    }

    #[tokio::test]
    async fn test_mock_provider_errors() {
        let provider = MockProviderBuilder::new().with_error_mode(true).build();

        assert!(provider.get_block_number().await.is_err());
        assert!(provider.get_chain_id().await.is_err());
    }
}

//! Integration Tests
//!
//! 🔧 FIX: Comprehensive integration tests for all fixed components.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    // Test configuration persistence
    #[tokio::test]
    async fn test_config_persistence() {
        // This test would require a test database
        // For now, verify the types compile correctly
        let _config = crate::state_manager::PersistedAgentConfig {
            agent_id: "test-agent".to_string(),
            name: "Test Agent".to_string(),
            description: "Test".to_string(),
            version: "1.0.0".to_string(),
            capabilities: vec!["chat".to_string()],
            model_config: crate::ModelConfig::default(),
            memory_config: crate::MemoryConfig::default(),
            personality_config: crate::PersonalityConfig::default(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Verify serialization
        let json = serde_json::to_string(&_config).expect("Should serialize");
        let _deserialized: crate::state_manager::PersistedAgentConfig =
            serde_json::from_str(&json).expect("Should deserialize");
    }

    // Test error conversion
    #[test]
    fn test_error_conversion() {
        use beebotos_core::{BeeBotOSError, ErrorCode};

        use crate::error::AgentError;
        use crate::error_integration::AgentErrorExt;

        // Test AgentError -> BeeBotOSError
        let agent_err = AgentError::NotFound("test".to_string());
        let beebotos_err: BeeBotOSError = agent_err.into();
        assert!(matches!(beebotos_err.code, ErrorCode::NotFound));

        // Test HTTP status mapping
        let err = AgentError::NotFound("test".to_string());
        assert_eq!(err.http_status(), 404);

        let err = AgentError::RateLimited("test".to_string());
        assert_eq!(err.http_status(), 429);

        // Test retryable detection
        let err = AgentError::Timeout("test".to_string());
        assert!(err.is_retryable());

        let err = AgentError::NotFound("test".to_string());
        assert!(!err.is_retryable());
    }

    // Test metrics collection
    #[test]
    fn test_metrics_collection() {
        use crate::metrics::MetricsCollector;

        let metrics = MetricsCollector::new();

        // Record task metrics
        metrics.record_task_started("agent-1", "llm_chat");
        metrics.record_task_completed("agent-1", "llm_chat", 150);
        metrics.record_task_failed("agent-1", "llm_chat", "timeout");

        // Record chain metrics
        metrics.record_chain_tx_submitted("agent-1", 1);
        metrics.record_chain_tx_confirmed("agent-1", 1, 2000);
        metrics.record_chain_tx_failed("agent-1", 1, "revert");

        // Export and verify
        let output = metrics.export_prometheus();
        assert!(output.contains("agent_tasks_started_total"));
        assert!(output.contains("agent_chain_tx_submitted_total"));
    }

    // Test wallet configuration
    #[test]
    fn test_wallet_config_validation() {
        use crate::wallet::WalletConfig;

        // Valid config
        let config = WalletConfig::new(1, "m/44'/60'/0'/0", 0);
        assert!(config.validate().is_ok());

        // Invalid chain_id
        let config = WalletConfig {
            chain_id: 0,
            derivation_path_prefix: "m/44'/60'/0'/0".to_string(),
            default_account_index: 0,
            rpc_url: None,
            default_gas_limit: 100_000,
            max_priority_fee_gwei: None,
            min_tx_interval_secs: 1,
            incoming_transfer_poll_interval_secs: 15,
        };
        assert!(config.validate().is_err());

        // Invalid derivation path
        let config = WalletConfig {
            chain_id: 1,
            derivation_path_prefix: "invalid".to_string(),
            default_account_index: 0,
            rpc_url: None,
            default_gas_limit: 100_000,
            max_priority_fee_gwei: None,
            min_tx_interval_secs: 1,
            incoming_transfer_poll_interval_secs: 15,
        };
        assert!(config.validate().is_err());
    }

    // Test state transitions
    #[tokio::test]
    async fn test_state_transitions() {
        use crate::state_manager::{AgentState, AgentStateManager, StateTransition};

        let manager = AgentStateManager::new(None);

        // Register agent
        manager
            .register_agent("test-agent", HashMap::new())
            .await
            .unwrap();
        assert_eq!(
            manager.get_state("test-agent").await.unwrap(),
            AgentState::Registered
        );

        // Start -> Initializing
        manager
            .transition("test-agent", StateTransition::Start)
            .await
            .unwrap();
        assert_eq!(
            manager.get_state("test-agent").await.unwrap(),
            AgentState::Initializing
        );

        // InitializationComplete -> Idle
        manager
            .transition("test-agent", StateTransition::InitializationComplete)
            .await
            .unwrap();
        assert_eq!(
            manager.get_state("test-agent").await.unwrap(),
            AgentState::Idle
        );

        // BeginTask -> Working
        manager
            .transition(
                "test-agent",
                StateTransition::BeginTask {
                    task_id: "task-1".to_string(),
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            manager.get_state("test-agent").await.unwrap(),
            AgentState::Working { .. }
        ));

        // CompleteTask -> Idle
        manager
            .transition(
                "test-agent",
                StateTransition::CompleteTask { success: true },
            )
            .await
            .unwrap();
        assert_eq!(
            manager.get_state("test-agent").await.unwrap(),
            AgentState::Idle
        );

        // Shutdown -> Stopped
        manager
            .transition("test-agent", StateTransition::Shutdown)
            .await
            .unwrap();
        assert_eq!(
            manager.get_state("test-agent").await.unwrap(),
            AgentState::ShuttingDown
        );

        manager
            .transition("test-agent", StateTransition::Stopped)
            .await
            .unwrap();
        assert_eq!(
            manager.get_state("test-agent").await.unwrap(),
            AgentState::Stopped
        );
    }

    // Test invalid state transitions
    #[tokio::test]
    async fn test_invalid_state_transitions() {
        use crate::state_manager::{AgentStateManager, StateTransition};

        let manager = AgentStateManager::new(None);
        manager
            .register_agent("test-agent", HashMap::new())
            .await
            .unwrap();

        // Cannot go from Registered directly to Working
        let result = manager
            .transition(
                "test-agent",
                StateTransition::BeginTask {
                    task_id: "task-1".to_string(),
                },
            )
            .await;
        assert!(result.is_err());
    }

    // Test system stats
    #[tokio::test]
    async fn test_system_stats() {
        use crate::state_manager::{AgentStateManager, StateTransition};

        let manager = AgentStateManager::new(None);

        // Register multiple agents
        for i in 0..3 {
            manager
                .register_agent(format!("agent-{}", i), HashMap::new())
                .await
                .unwrap();
        }

        // Complete workflow for first agent
        manager
            .transition("agent-0", StateTransition::Start)
            .await
            .unwrap();
        manager
            .transition("agent-0", StateTransition::InitializationComplete)
            .await
            .unwrap();
        manager
            .transition(
                "agent-0",
                StateTransition::BeginTask {
                    task_id: "t1".to_string(),
                },
            )
            .await
            .unwrap();
        manager
            .transition("agent-0", StateTransition::CompleteTask { success: true })
            .await
            .unwrap();

        // Get stats
        let stats = manager.get_system_stats().await;

        assert_eq!(stats.total_agents, 3);
        assert_eq!(stats.total_tasks, 1);
        assert_eq!(stats.total_successful, 1);
        assert_eq!(stats.success_rate, 1.0);
    }

    // Test message bus creation
    #[tokio::test]
    async fn test_message_bus_creation() {
        use std::sync::Arc;

        use beebotos_message_bus::{DefaultMessageBus, JsonCodec, MemoryTransport};

        // Create message bus - this just verifies the types are compatible
        let _bus: Arc<DefaultMessageBus<MemoryTransport>> = Arc::new(DefaultMessageBus::new(
            MemoryTransport::new(),
            Box::new(JsonCodec),
            None,
        ));
    }

    // Test agent health calculation
    #[tokio::test]
    async fn test_agent_health() {
        use crate::state_manager::{AgentHealth, AgentStateManager, StateTransition};

        let manager = AgentStateManager::new(None);
        manager
            .register_agent("healthy-agent", HashMap::new())
            .await
            .unwrap();

        // Start and complete initialization
        manager
            .transition("healthy-agent", StateTransition::Start)
            .await
            .unwrap();
        manager
            .transition("healthy-agent", StateTransition::InitializationComplete)
            .await
            .unwrap();

        let health = manager.get_agent_health("healthy-agent").await.unwrap();
        assert!(matches!(health, AgentHealth::Healthy));

        // Register agent and transition to error state
        manager
            .register_agent("error-agent", HashMap::new())
            .await
            .unwrap();
        manager
            .transition("error-agent", StateTransition::Start)
            .await
            .unwrap();
        manager
            .transition(
                "error-agent",
                StateTransition::Error {
                    message: "test error".to_string(),
                },
            )
            .await
            .unwrap();

        let health = manager.get_agent_health("error-agent").await.unwrap();
        assert!(matches!(health, AgentHealth::Unhealthy { .. }));
    }

    // Test batch transitions
    #[tokio::test]
    async fn test_batch_transitions() {
        use crate::state_manager::{AgentStateManager, StateTransition};

        let manager = AgentStateManager::new(None);

        // Register agents
        for i in 0..3 {
            manager
                .register_agent(format!("agent-{}", i), HashMap::new())
                .await
                .unwrap();
        }

        // Batch start all agents
        let agent_ids: Vec<String> = (0..3).map(|i| format!("agent-{}", i)).collect();
        let results = manager
            .batch_transition(&agent_ids, StateTransition::Start)
            .await;

        assert_eq!(results.len(), 3);
        for (id, result) in results {
            assert!(result.is_ok(), "Failed to start {}", id);
        }
    }

    // Test unified error macros
    #[test]
    fn test_unified_error_macros() {
        use beebotos_core::{BeeBotOSError, ErrorCode};

        use crate::{unified_bail, unified_err};

        // Test unified_err macro
        let err = unified_err!(ErrorCode::Database, "test error");
        assert!(matches!(err.code, ErrorCode::Database));

        let err = unified_err!(ErrorCode::Agent, "error: {}", "details");
        assert!(matches!(err.code, ErrorCode::Agent));

        // Test unified_bail macro - use a function that returns Result
        fn test_bail() -> Result<(), BeeBotOSError> {
            unified_bail!(ErrorCode::InvalidInput, "bail test");
        }
        let result = test_bail();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::InvalidInput);
    }

    // Test transaction receipt
    #[test]
    fn test_transaction_receipt() {
        use beebotos_chain::compat::B256;

        use crate::wallet::TransactionReceipt;

        let receipt = TransactionReceipt {
            tx_hash: B256::ZERO,
            block_number: Some(12345),
            gas_used: 21000,
            status: true,
        };

        assert_eq!(receipt.block_number, Some(12345));
        assert_eq!(receipt.gas_used, 21000);
        assert!(receipt.status);
    }

    // Test wallet events
    #[test]
    fn test_wallet_events() {
        use beebotos_chain::compat::{Address, B256, U256};

        use crate::wallet::{TransactionReceipt, TransferEvent, WalletEvent};

        let event = WalletEvent::TransactionConfirmed(TransactionReceipt {
            tx_hash: B256::ZERO,
            block_number: Some(100),
            gas_used: 21000,
            status: true,
        });

        assert!(matches!(event, WalletEvent::TransactionConfirmed(_)));

        let event = WalletEvent::IncomingTransfer(TransferEvent {
            tx_hash: B256::ZERO,
            from: Address::ZERO,
            to: Address::ZERO,
            amount: U256::from(1000),
            token: None,
            block_number: 100,
        });

        assert!(matches!(event, WalletEvent::IncomingTransfer(_)));
    }

    // Test config center integration types
    #[tokio::test]
    async fn test_config_center_types() {
        // This test verifies the types compile correctly
        use beebotos_core::ConfigCenter;

        // Just verify we can create the center - using _ to avoid unused warning
        let _center = ConfigCenter::from_env();
    }
}

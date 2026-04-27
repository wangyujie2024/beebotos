//! Configuration Management Example
//!
//! This example demonstrates how to use the configuration system:
//! - Loading from YAML files
//! - Loading from environment variables
//! - Programmatic configuration

use beebotos_message_bus::config::MessageBusConfig;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== BeeBotOS Message Bus - Configuration Example ===\n");

    // Example 1: Default configuration
    println!("1. Default Configuration");
    let config = MessageBusConfig::new();
    println!("   Transport: {}", config.transport);
    println!(
        "   Max message size: {} bytes",
        config.limits.max_message_size
    );
    println!("   gRPC bind address: {}", config.grpc.bind_addr);
    println!("   Metrics enabled: {}", config.metrics.enabled);
    println!();

    // Example 2: Programmatic configuration
    println!("2. Programmatic Configuration");
    let mut config = MessageBusConfig::new();
    config.transport = "grpc".to_string();
    config.grpc.node_id = Some("my-node-1".to_string());
    config.grpc.cluster_seeds = vec!["10.0.1.10:50051".parse()?, "10.0.1.11:50051".parse()?];
    config.limits.max_message_size = 50 * 1024 * 1024; // 50 MB
    config.logging.level = "debug".to_string();

    println!("   Transport: {}", config.transport);
    println!("   Node ID: {:?}", config.grpc.node_id);
    println!("   Cluster seeds: {:?}", config.grpc.cluster_seeds);
    println!(
        "   Max message size: {} bytes",
        config.limits.max_message_size
    );
    println!("   Log level: {}", config.logging.level);
    println!();

    // Example 3: Configuration from YAML string
    println!("3. Configuration from YAML");
    let yaml_config = r#"
transport: grpc
grpc:
  bind_addr: "0.0.0.0:50051"
  node_id: "example-node"
  cluster_seeds:
    - "192.168.1.100:50051"
    - "192.168.1.101:50051"
  keepalive_interval: "30s"
  max_message_size: 20971520
limits:
  max_message_size: 20971520
  max_topics: 5000
  max_subscriptions_per_topic: 500
  queue_depth: 5000
metrics:
  enabled: true
  endpoint: "0.0.0.0:9090"
  interval: "30s"
logging:
  level: "info"
  format: "json"
"#;

    let config: MessageBusConfig = serde_yaml::from_str(yaml_config)?;
    println!("   Parsed from YAML:");
    println!("   Transport: {}", config.transport);
    println!("   Node ID: {:?}", config.grpc.node_id);
    println!("   Cluster seeds: {:?}", config.grpc.cluster_seeds);
    println!("   Max message size: {}", config.limits.max_message_size);
    println!();

    // Example 4: Save configuration to file
    println!("4. Save Configuration to File");
    let config_yaml = serde_yaml::to_string(&config)?;
    let config_path = "data/message-bus-config.yaml";
    tokio::fs::write(config_path, config_yaml).await?;
    println!("   Configuration saved to: {}", config_path);
    println!();

    // Example 5: Load configuration from file
    println!("5. Load Configuration from File");
    let loaded_config = MessageBusConfig::from_file(config_path).await?;
    println!("   Loaded transport: {}", loaded_config.transport);
    println!("   Loaded node ID: {:?}", loaded_config.grpc.node_id);
    println!();

    // Cleanup
    tokio::fs::remove_file(config_path).await.ok();

    // Example 6: Configuration validation
    println!("6. Configuration Validation");
    let valid_config = MessageBusConfig::new();
    match valid_config.validate() {
        Ok(_) => println!("   ✓ Valid configuration"),
        Err(e) => println!("   ✗ Invalid: {}", e),
    }

    let mut invalid_config = MessageBusConfig::new();
    invalid_config.transport = "invalid_transport".to_string();
    match invalid_config.validate() {
        Ok(_) => println!("   ✓ Unexpectedly valid"),
        Err(e) => println!("   ✓ Correctly detected invalid: {}", e),
    }

    let mut invalid_config = MessageBusConfig::new();
    invalid_config.limits.max_message_size = 0;
    match invalid_config.validate() {
        Ok(_) => println!("   ✓ Unexpectedly valid"),
        Err(e) => println!("   ✓ Correctly detected invalid: {}", e),
    }
    println!();

    // Example 7: Environment variable loading
    println!("7. Environment Variable Loading");
    println!("   Set these environment variables to configure:");
    println!("   - MESSAGE_BUS_TRANSPORT (memory/grpc)");
    println!("   - MESSAGE_BUS_GRPC_BIND_ADDR (e.g., 0.0.0.0:50051)");
    println!("   - MESSAGE_BUS_GRPC_NODE_ID (node identifier)");
    println!("   - MESSAGE_BUS_GRPC_CLUSTER_SEEDS (comma-separated addresses)");
    println!("   - MESSAGE_BUS_LIMITS_MAX_MESSAGE_SIZE (e.g., 10MB)");
    println!();

    // Demonstrate env loading (with test values)
    std::env::set_var("MESSAGE_BUS_TRANSPORT", "grpc");
    std::env::set_var("MESSAGE_BUS_GRPC_BIND_ADDR", "0.0.0.0:60051");
    std::env::set_var("MESSAGE_BUS_GRPC_NODE_ID", "env-node");
    std::env::set_var("MESSAGE_BUS_LIMITS_MAX_MESSAGE_SIZE", "20MB");

    let env_config = MessageBusConfig::from_env()?;
    println!("   Loaded from environment:");
    println!("   Transport: {}", env_config.transport);
    println!("   Bind addr: {}", env_config.grpc.bind_addr);
    println!("   Node ID: {:?}", env_config.grpc.node_id);
    println!(
        "   Max message size: {}",
        env_config.limits.max_message_size
    );
    println!();

    // Cleanup env vars
    std::env::remove_var("MESSAGE_BUS_TRANSPORT");
    std::env::remove_var("MESSAGE_BUS_GRPC_BIND_ADDR");
    std::env::remove_var("MESSAGE_BUS_GRPC_NODE_ID");
    std::env::remove_var("MESSAGE_BUS_LIMITS_MAX_MESSAGE_SIZE");

    println!("=== Example Complete ===");
    Ok(())
}

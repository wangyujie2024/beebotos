//! Distributed Cluster Example
//!
//! This example demonstrates the gRPC cluster mode:
//! - Starting multiple nodes
//! - Auto-discovery
//! - Cross-node message routing
//!
//! Note: This example requires the "grpc-cluster" feature
//!
//! Run with:
//!   cargo run --example distributed_cluster --features grpc-cluster

#[cfg(feature = "grpc-cluster")]
use std::time::Duration;

#[cfg(feature = "grpc-cluster")]
use beebotos_message_bus::{GrpcConfig, GrpcTransport, Message, MessageBus};

#[cfg(feature = "grpc-cluster")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::net::SocketAddr;

    tracing_subscriber::fmt::init();

    println!("=== BeeBotOS Message Bus - Distributed Cluster Example ===\n");
    println!("Starting a 3-node cluster...\n");

    // Node 1: Seed node (leader)
    println!("[Node-1] Starting on port 50051 as seed node...");
    let config1 = GrpcConfig {
        bind_addr: "127.0.0.1:50051".parse()?,
        cluster_addrs: vec![], // No seeds for first node
        node_id: "node-1".to_string(),
        keepalive_interval: Duration::from_secs(5),
        connect_timeout: Duration::from_secs(5),
        max_message_size: 10 * 1024 * 1024,
        tls_enabled: false,
        tls_cert_path: None,
        tls_key_path: None,
    };

    let node1 = GrpcTransport::new(config1).await?;

    // Subscribe node-1 to all agent events
    let (_sub1, mut stream1) = node1.subscribe("agent/#").await?;
    let node1_handle = tokio::spawn(async move {
        println!("[Node-1] Subscribed to 'agent/#'");
        while let Some(msg) = stream1.recv().await {
            println!(
                "[Node-1] Received from {}: {:?}",
                msg.metadata.topic,
                String::from_utf8_lossy(&msg.payload)
            );
        }
    });

    // Wait for node-1 to be ready
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Node 2: Joins via node-1
    println!("[Node-2] Starting on port 50052, joining via node-1...");
    let config2 = GrpcConfig {
        bind_addr: "127.0.0.1:50052".parse()?,
        cluster_addrs: vec!["127.0.0.1:50051".parse()?],
        node_id: "node-2".to_string(),
        keepalive_interval: Duration::from_secs(5),
        connect_timeout: Duration::from_secs(5),
        max_message_size: 10 * 1024 * 1024,
        tls_enabled: false,
        tls_cert_path: None,
        tls_key_path: None,
    };

    let node2 = GrpcTransport::new(config2).await?;

    // Subscribe node-2 to task events
    let (_sub2, mut stream2) = node2.subscribe("agent/+/task/+").await?;
    let node2_handle = tokio::spawn(async move {
        println!("[Node-2] Subscribed to 'agent/+/task/+'");
        while let Some(msg) = stream2.recv().await {
            println!(
                "[Node-2] Received from {}: {:?}",
                msg.metadata.topic,
                String::from_utf8_lossy(&msg.payload)
            );
        }
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Node 3: Joins via either node-1 or node-2
    println!("[Node-3] Starting on port 50053, joining cluster...");
    let config3 = GrpcConfig {
        bind_addr: "127.0.0.1:50053".parse()?,
        cluster_addrs: vec!["127.0.0.1:50051".parse()?],
        node_id: "node-3".to_string(),
        keepalive_interval: Duration::from_secs(5),
        connect_timeout: Duration::from_secs(5),
        max_message_size: 10 * 1024 * 1024,
        tls_enabled: false,
        tls_cert_path: None,
        tls_key_path: None,
    };

    let node3 = GrpcTransport::new(config3).await?;

    // Subscribe node-3 to status updates
    let (_sub3, mut stream3) = node3.subscribe("agent/+/status").await?;
    let node3_handle = tokio::spawn(async move {
        println!("[Node-3] Subscribed to 'agent/+/status'");
        while let Some(msg) = stream3.recv().await {
            println!(
                "[Node-3] Received from {}: {:?}",
                msg.metadata.topic,
                String::from_utf8_lossy(&msg.payload)
            );
        }
    });

    tokio::time::sleep(Duration::from_secs(2)).await;
    println!("\n✓ All nodes started and joined cluster\n");

    // Demonstrate cross-node messaging
    println!("=== Cross-Node Messaging Demo ===\n");

    // Publish from node-1 (should reach all nodes)
    println!("[Node-1] Publishing agent status...");
    let msg = Message::new("agent/100/status", "Agent 100 is online");
    node1.publish("agent/100/status", msg).await?;

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Publish from node-2
    println!("\n[Node-2] Publishing task events...");
    let msg = Message::new("agent/101/task/start", "Task started on agent 101");
    node2.publish("agent/101/task/start", msg).await?;

    let msg = Message::new("agent/101/task/complete", "Task completed on agent 101");
    node2.publish("agent/101/task/complete", msg).await?;

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Publish from node-3
    println!("\n[Node-3] Publishing status updates...");
    let msg = Message::new("agent/102/status", "Agent 102 is busy");
    node3.publish("agent/102/status", msg).await?;

    tokio::time::sleep(Duration::from_millis(500)).await;

    println!("\n=== Request-Reply Across Nodes ===\n");

    // Set up a responder on node-2
    let node2_responder = node2.clone();
    let responder_handle = tokio::spawn(async move {
        let (sub, mut stream) = node2_responder.subscribe("cluster/ping").await.unwrap();

        while let Some(request) = stream.recv().await {
            if let Some(reply_to) = &request.metadata.reply_to {
                let payload = String::from_utf8_lossy(&request.payload);
                let response = format!("Pong from node-2: received '{}'", payload);
                let msg = Message::new(reply_to, response);
                let _ = node2_responder.publish(reply_to, msg).await;
            }
        }

        node2_responder.unsubscribe(sub).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send request from node-1 to node-2
    println!("[Node-1] Sending request to 'cluster/ping'...");
    let request = Message::new("cluster/ping", "Hello from node-1");

    match node1
        .request("cluster/ping", request, Duration::from_secs(5))
        .await
    {
        Ok(response) => {
            let resp_str = String::from_utf8_lossy(&response.payload);
            println!("[Node-1] Received response: {}", resp_str);
        }
        Err(e) => {
            println!("[Node-1] Request failed: {}", e);
        }
    }

    println!("\n=== Demo Complete ===");
    println!("Press Ctrl+C to stop all nodes...");

    // Keep running
    tokio::signal::ctrl_c().await?;

    node1_handle.abort();
    node2_handle.abort();
    node3_handle.abort();
    responder_handle.abort();

    println!("\nAll nodes stopped.");
    Ok(())
}

#[cfg(not(feature = "grpc-cluster"))]
#[tokio::main]
async fn main() {
    eprintln!("This example requires the 'grpc-cluster' feature.");
    eprintln!("Run with: cargo run --example distributed_cluster --features grpc-cluster");
}

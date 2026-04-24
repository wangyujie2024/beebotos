//! Basic Publish-Subscribe Example
//!
//! This example demonstrates the basic pub/sub pattern:
//! - Publishing messages to topics
//! - Subscribing with exact topic matches
//! - Subscribing with wildcard patterns

use std::time::Duration;

use beebotos_message_bus::{DefaultMessageBus, JsonCodec, MemoryTransport, Message, MessageBus};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for logging
    tracing_subscriber::fmt::init();

    println!("=== BeeBotOS Message Bus - Basic Pub/Sub Example ===\n");

    // Create a message bus with in-memory transport
    let bus = DefaultMessageBus::new(MemoryTransport::new(), Box::new(JsonCodec::new()), None);

    // Example 1: Exact topic subscription
    println!("1. Exact Topic Subscription");
    println!("   Subscribing to 'agent/123/status'...");

    let (sub1, mut stream1) = bus.subscribe("agent/123/status").await?;

    // Publish a message to the exact topic
    let msg = Message::new("agent/123/status", b"Agent is online".to_vec());
    bus.publish("agent/123/status", msg).await?;

    // Receive the message
    if let Some(received) = stream1.recv().await {
        println!(
            "   ✓ Received: {:?}",
            String::from_utf8_lossy(&received.payload)
        );
    }

    bus.unsubscribe(sub1).await?;
    println!();

    // Example 2: Single-level wildcard (+)
    println!("2. Single-Level Wildcard (+)");
    println!("   Subscribing to 'agent/+/status'...");

    let (sub2, mut stream2) = bus.subscribe("agent/+/status").await?;

    // Publish to different agents
    for agent_id in ["100", "101", "102"] {
        let topic = format!("agent/{}/status", agent_id);
        let payload = format!("Agent {} is active", agent_id);
        let msg = Message::new(&topic, payload.into_bytes());
        bus.publish(&topic, msg).await?;
    }

    // Receive all messages
    for _ in 0..3 {
        if let Some(received) = tokio::time::timeout(Duration::from_secs(1), stream2.recv()).await?
        {
            println!(
                "   ✓ Received from {}: {:?}",
                received.metadata.topic,
                String::from_utf8_lossy(&received.payload)
            );
        }
    }

    bus.unsubscribe(sub2).await?;
    println!();

    // Example 3: Multi-level wildcard (#)
    println!("3. Multi-Level Wildcard (#)");
    println!("   Subscribing to 'agent/123/#'...");

    let (sub3, mut stream3) = bus.subscribe("agent/123/#").await?;

    // Publish to various sub-topics
    let messages = [
        ("agent/123/status", "Status update"),
        ("agent/123/task/start", "Task started"),
        ("agent/123/task/progress", "50% complete"),
        ("agent/123/task/complete", "Task done"),
        ("agent/123/metrics/cpu", "CPU: 45%"),
    ];

    for (topic, payload) in &messages {
        let msg = Message::new(*topic, payload.to_string().into_bytes());
        bus.publish(topic, msg).await?;
    }

    // Receive all messages
    for _ in 0..messages.len() {
        if let Some(received) = tokio::time::timeout(Duration::from_secs(1), stream3.recv()).await?
        {
            println!(
                "   ✓ Received from {}: {:?}",
                received.metadata.topic,
                String::from_utf8_lossy(&received.payload)
            );
        }
    }

    bus.unsubscribe(sub3).await?;
    println!();

    // Example 4: Multiple subscribers on same topic
    println!("4. Multiple Subscribers");
    println!("   Creating 3 subscribers to 'notifications'...");

    let (sub_a, mut stream_a) = bus.subscribe("notifications").await?;
    let (sub_b, mut stream_b) = bus.subscribe("notifications").await?;
    let (sub_c, mut stream_c) = bus.subscribe("notifications").await?;

    let msg = Message::new("notifications", b"Important announcement!".to_vec());
    bus.publish("notifications", msg).await?;

    // All subscribers should receive
    let a = stream_a.recv().await;
    let b = stream_b.recv().await;
    let c = stream_c.recv().await;

    println!("   ✓ Subscriber A received: {}", a.is_some());
    println!("   ✓ Subscriber B received: {}", b.is_some());
    println!("   ✓ Subscriber C received: {}", c.is_some());

    bus.unsubscribe(sub_a).await?;
    bus.unsubscribe(sub_b).await?;
    bus.unsubscribe(sub_c).await?;
    println!();

    println!("=== Example Complete ===");
    Ok(())
}

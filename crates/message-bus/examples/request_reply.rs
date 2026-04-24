//! Request-Reply Pattern Example
//!
//! This example demonstrates the request-reply (RPC) pattern:
//! - Setting up a responder
//! - Sending requests with timeout
//! - Handling responses

use std::time::Duration;

use beebotos_message_bus::{DefaultMessageBus, JsonCodec, MemoryTransport, Message, MessageBus};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("=== BeeBotOS Message Bus - Request/Reply Example ===\n");

    let bus = DefaultMessageBus::new(MemoryTransport::new(), Box::new(JsonCodec::new()), None);

    // Example 1: Basic echo service
    println!("1. Basic Echo Service");
    println!("   Starting echo responder...");

    let bus_clone = bus.clone();
    let echo_handle = tokio::spawn(async move {
        let (sub_id, mut stream) = bus_clone.subscribe("rpc/echo").await.unwrap();

        while let Some(request) = stream.recv().await {
            let payload = String::from_utf8_lossy(&request.payload);
            println!("   [Echo Server] Received: {}", payload);

            // Send response back using reply_to
            if let Some(reply_to) = request.metadata.headers.get("reply_to") {
                let correlation_id = request
                    .metadata
                    .correlation_id
                    .clone()
                    .unwrap_or_else(|| request.id().to_string());

                let response = Message::new(reply_to, format!("Echo: {}", payload).into_bytes());
                // In real implementation, would use bus.respond()
                let _ = bus_clone.publish(reply_to, response).await;
                println!("   [Echo Server] Sent response");
            }
        }

        bus_clone.unsubscribe(sub_id).await.unwrap();
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send requests
    for i in 1..=3 {
        let request = Message::new("rpc/echo", format!("Hello {}", i).into_bytes());
        println!("   [Client] Sending request: Hello {}", i);

        match bus
            .request("rpc/echo", request, Duration::from_secs(5))
            .await
        {
            Ok(response) => {
                let resp_str = String::from_utf8_lossy(&response.payload);
                println!("   [Client] Received response: {}\n", resp_str);
            }
            Err(e) => {
                eprintln!("   [Client] Request failed: {}\n", e);
            }
        }
    }

    echo_handle.abort();
    println!();

    // Example 2: Calculator service
    println!("2. Calculator Service");
    println!("   Starting calculator responder...");

    let bus_calc = bus.clone();
    let calc_handle = tokio::spawn(async move {
        let (sub_id, mut stream) = bus_calc.subscribe("rpc/calc").await.unwrap();

        while let Some(request) = stream.recv().await {
            let payload = String::from_utf8_lossy(&request.payload);

            // Parse calculation request (simple format: "5 + 3")
            let result = if let Some(reply_to) = request.metadata.headers.get("reply_to") {
                let parts: Vec<&str> = payload.split_whitespace().collect();
                if parts.len() == 3 {
                    let a: f64 = parts[0].parse().unwrap_or(0.0);
                    let op = parts[1];
                    let b: f64 = parts[2].parse().unwrap_or(0.0);

                    let answer = match op {
                        "+" => a + b,
                        "-" => a - b,
                        "*" => a * b,
                        "/" => {
                            if b != 0.0 {
                                a / b
                            } else {
                                f64::NAN
                            }
                        }
                        _ => f64::NAN,
                    };

                    let response = Message::new(reply_to, format!("{}", answer).into_bytes());
                    let _ = bus_calc.publish(reply_to, response).await;
                    Some(format!("{} {} {} = {}", a, op, b, answer))
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(msg) = result {
                println!("   [Calc Server] {}\n", msg);
            }
        }

        bus_calc.unsubscribe(sub_id).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Test calculations
    let calculations = vec!["10 + 5", "20 - 8", "6 * 7", "100 / 4"];

    for calc in calculations {
        let request = Message::new("rpc/calc", calc.to_string().into_bytes());
        println!("   [Client] Calculate: {}", calc);

        match bus
            .request("rpc/calc", request, Duration::from_secs(5))
            .await
        {
            Ok(response) => {
                let result = String::from_utf8_lossy(&response.payload);
                println!("   [Client] Result: {}\n", result);
            }
            Err(e) => {
                eprintln!("   [Client] Calculation failed: {}\n", e);
            }
        }
    }

    calc_handle.abort();
    println!();

    // Example 3: Timeout handling
    println!("3. Timeout Handling");
    println!("   Sending request to non-existent service (will timeout)...");

    let slow_request = Message::new("rpc/nonexistent", "test".as_bytes().to_vec());

    match bus
        .request("rpc/nonexistent", slow_request, Duration::from_millis(500))
        .await
    {
        Ok(_) => println!("   ✓ Unexpected success"),
        Err(e) => println!("   ✓ Expected timeout: {}", e),
    }
    println!();

    // Example 4: JSON request/response
    println!("4. JSON Request/Response");
    println!("   Starting JSON API responder...");

    let bus_json = bus.clone();
    let json_handle = tokio::spawn(async move {
        let (sub_id, mut stream) = bus_json.subscribe("api/users").await.unwrap();

        while let Some(request) = stream.recv().await {
            if let Some(reply_to) = request.metadata.headers.get("reply_to") {
                // Parse JSON request
                let req_json: serde_json::Value = serde_json::from_slice(&request.payload)
                    .unwrap_or(json!({"action": "unknown"}));

                let action = req_json["action"].as_str().unwrap_or("unknown");

                let response_json = match action {
                    "get_user" => {
                        let user_id = req_json["user_id"].as_str().unwrap_or("0");
                        json!({
                            "user_id": user_id,
                            "name": format!("User {}", user_id),
                            "email": format!("user{}@example.com", user_id),
                            "status": "active"
                        })
                    }
                    "list_users" => {
                        json!({
                            "users": [
                                {"id": "1", "name": "Alice"},
                                {"id": "2", "name": "Bob"},
                                {"id": "3", "name": "Charlie"}
                            ],
                            "total": 3
                        })
                    }
                    _ => json!({"error": "Unknown action"}),
                };

                let response = Message::new(reply_to, serde_json::to_vec(&response_json).unwrap());
                let _ = bus_json.publish(reply_to, response).await;
            }
        }

        bus_json.unsubscribe(sub_id).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Get user request
    let get_user_req = json!({
        "action": "get_user",
        "user_id": "42"
    });
    let request = Message::new("api/users", serde_json::to_vec(&get_user_req)?);
    println!("   [Client] Get user 42");

    if let Ok(response) = bus
        .request("api/users", request, Duration::from_secs(5))
        .await
    {
        let resp_json: serde_json::Value = serde_json::from_slice(&response.payload)?;
        println!(
            "   [Client] Response: {}",
            serde_json::to_string_pretty(&resp_json)?
        );
    }

    // List users request
    let list_req = json!({"action": "list_users"});
    let request = Message::new("api/users", serde_json::to_vec(&list_req)?);
    println!("\n   [Client] List all users");

    if let Ok(response) = bus
        .request("api/users", request, Duration::from_secs(5))
        .await
    {
        let resp_json: serde_json::Value = serde_json::from_slice(&response.payload)?;
        println!(
            "   [Client] Response: {}",
            serde_json::to_string_pretty(&resp_json)?
        );
    }

    json_handle.abort();
    println!();

    println!("=== Example Complete ===");
    Ok(())
}

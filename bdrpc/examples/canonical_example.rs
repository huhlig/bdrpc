//
// Copyright 2026 Hans W. Uhlig. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//

//! # Canonical BDRPC Example
//!
//! This example demonstrates the recommended patterns for using BDRPC with:
//! 1. **RPC Service** - Request/response pattern using the `#[service]` macro
//! 2. **Bidirectional Messages** - Async message passing without request/response correlation
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                         Client                              │
//! │  ┌──────────────────┐         ┌─────────────────────────┐   │
//! │  │ CalculatorClient │         │ NotificationProtocol    │   │
//! │  │  (RPC Service)   │         │ (Bidirectional Channel) │   │
//! │  └──────────────────┘         └─────────────────────────┘   │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              │ TCP Connection
//!                              │
//! ┌─────────────────────────────────────────────────────────────┐
//! │                         Server                              │
//! │  ┌──────────────────┐         ┌─────────────────────────┐   │
//! │  │ CalculatorServer │         │ NotificationProtocol    │   │
//! │  │  (RPC Service)   │         │ (Bidirectional Channel) │   │
//! │  └──────────────────┘         └─────────────────────────┘   │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Key Patterns
//!
//! ### 1. RPC Service Pattern (Request/Response)
//!
//! Use the `#[service]` macro for request/response RPC:
//! - Automatic correlation ID management
//! - Type-safe client stubs
//! - Concurrent request support
//! - Timeout handling
//!
//! ### 2. Bidirectional Message Pattern
//!
//! Use manual Protocol implementation for async messages:
//! - No request/response correlation
//! - Fire-and-forget semantics
//! - Both sides can send messages independently
//! - Useful for notifications, events, streaming
//!
//! ## Running the Example
//!
//! ```bash
//! cargo run --example canonical_example
//! ```

use bdrpc::channel::{ChannelId, Protocol};
use bdrpc::endpoint::EndpointBuilder;
use bdrpc::serialization::PostcardSerializer;
use bdrpc::service;
use tokio::time::{Duration, sleep};

// ============================================================================
// Part 1: RPC Service Definition (Request/Response Pattern)
// ============================================================================

/// Calculator service demonstrating RPC request/response pattern.
///
/// The `#[service]` macro generates:
/// - `CalculatorServiceProtocol` enum with request/response variants
/// - `CalculatorServiceClient` with async methods that handle correlation
/// - `CalculatorServiceServer` trait for implementation
/// - `CalculatorServiceDispatcher` for routing requests to handlers
#[service(direction = "bidirectional", version = 1)]
#[allow(dead_code)]
trait CalculatorService {
    /// Add two numbers and return the result.
    async fn add(&self, a: i32, b: i32) -> Result<i32, String>;

    /// Multiply two numbers with optional delay for testing.
    async fn multiply(&self, a: i32, b: i32, delay_ms: Option<u64>) -> Result<i32, String>;

    /// Divide two numbers, returns error on division by zero.
    async fn divide(&self, a: i32, b: i32) -> Result<f64, String>;
}

/// Implementation of the calculator service.
#[allow(dead_code)]
struct CalculatorImpl {
    name: String,
}

impl CalculatorImpl {
    fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

impl CalculatorServiceServer for CalculatorImpl {
    async fn add(&self, a: i32, b: i32) -> Result<i32, String> {
        println!("[{}] Processing add({}, {})", self.name, a, b);
        Ok(a + b)
    }

    async fn multiply(&self, a: i32, b: i32, delay_ms: Option<u64>) -> Result<i32, String> {
        println!("[{}] Processing multiply({}, {})", self.name, a, b);
        if let Some(delay) = delay_ms {
            sleep(Duration::from_millis(delay)).await;
        }
        Ok(a * b)
    }

    async fn divide(&self, a: i32, b: i32) -> Result<f64, String> {
        println!("[{}] Processing divide({}, {})", self.name, a, b);
        if b == 0 {
            Err("Division by zero".to_string())
        } else {
            Ok(a as f64 / b as f64)
        }
    }
}

// ============================================================================
// Part 2: Bidirectional Message Protocol (Non-RPC Pattern)
// ============================================================================

/// Notification protocol for bidirectional async messaging.
///
/// This demonstrates the non-RPC pattern where messages are sent
/// asynchronously without expecting a correlated response.
///
/// Use cases:
/// - Server-initiated notifications
/// - Event streaming
/// - Heartbeats
/// - Status updates
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
enum NotificationProtocol {
    /// Server sends status update to client
    StatusUpdate { message: String, timestamp: u64 },

    /// Client sends heartbeat to server
    Heartbeat { client_id: String },

    /// Server acknowledges heartbeat
    HeartbeatAck { server_time: u64 },

    /// Generic event notification
    Event { event_type: String, data: String },
}

impl Protocol for NotificationProtocol {
    fn method_name(&self) -> &'static str {
        match self {
            Self::StatusUpdate { .. } => "status_update",
            Self::Heartbeat { .. } => "heartbeat",
            Self::HeartbeatAck { .. } => "heartbeat_ack",
            Self::Event { .. } => "event",
        }
    }

    fn is_request(&self) -> bool {
        // None of these are requests - they're all async messages
        false
    }
}

// ============================================================================
// Server Implementation
// ============================================================================

/// Run the server side of the example.
async fn run_server() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Starting Server ===\n");

    // Create server endpoint using EndpointBuilder
    let server = EndpointBuilder::server(PostcardSerializer::default())
        // Register RPC service as responder
        .with_responder("CalculatorService", 1)
        // Register notification protocol as bidirectional
        .with_bidirectional("NotificationProtocol", 1)
        // Add TCP listener
        .with_tcp_listener("127.0.0.1:9090")
        .build()
        .await?;

    println!("Server listening on 127.0.0.1:9090");

    // Accept incoming connection
    println!("Waiting for client connection...");
    let connection = server.accept().await?;
    println!("Client connected: {}\n", connection.id());

    // Create channels for the connection
    let calc_channel_id = ChannelId::from(1);
    let notif_channel_id = ChannelId::from(2);

    // Create RPC service channel
    server
        .channel_manager()
        .create_channel::<CalculatorServiceProtocol>(calc_channel_id, 100)
        .await?;

    let calc_sender = server
        .channel_manager()
        .get_sender::<CalculatorServiceProtocol>(calc_channel_id)
        .await?;

    let mut calc_receiver = server
        .channel_manager()
        .get_receiver::<CalculatorServiceProtocol>(calc_channel_id)
        .await?;

    // Create notification channel
    server
        .channel_manager()
        .create_channel::<NotificationProtocol>(notif_channel_id, 100)
        .await?;

    let notif_sender = server
        .channel_manager()
        .get_sender::<NotificationProtocol>(notif_channel_id)
        .await?;

    let mut notif_receiver = server
        .channel_manager()
        .get_receiver::<NotificationProtocol>(notif_channel_id)
        .await?;

    // Create service implementation and dispatcher
    let service = CalculatorImpl::new("Server");
    let dispatcher = CalculatorServiceDispatcher::new(service);

    // Spawn RPC service handler
    let rpc_handle = tokio::spawn(async move {
        println!("[Server] RPC service handler started");
        while let Some(envelope) = calc_receiver.recv_envelope().await {
            let response_envelope = dispatcher.dispatch_envelope(envelope).await;

            // Send response with correlation ID
            if let Some(correlation_id) = response_envelope.correlation_id {
                if calc_sender
                    .send_response(response_envelope.payload, correlation_id)
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
        println!("[Server] RPC service handler stopped");
    });

    // Spawn notification handler
    let notif_handle = tokio::spawn(async move {
        println!("[Server] Notification handler started");

        // Send periodic status updates
        let status_sender = notif_sender.clone();
        tokio::spawn(async move {
            for i in 1..=3 {
                sleep(Duration::from_secs(2)).await;
                let msg = NotificationProtocol::StatusUpdate {
                    message: format!("Server status update #{}", i),
                    timestamp: i * 2,
                };
                if status_sender.send(msg).await.is_err() {
                    break;
                }
                println!("[Server] Sent status update #{}", i);
            }
        });

        // Handle incoming notifications
        while let Some(msg) = notif_receiver.recv().await {
            match msg {
                NotificationProtocol::Heartbeat { client_id } => {
                    println!("[Server] Received heartbeat from: {}", client_id);

                    // Send acknowledgment
                    let ack = NotificationProtocol::HeartbeatAck {
                        server_time: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                    };
                    if notif_sender.send(ack).await.is_err() {
                        break;
                    }
                }
                other => {
                    println!("[Server] Received notification: {:?}", other);
                }
            }
        }
        println!("[Server] Notification handler stopped");
    });

    // Wait for handlers to complete
    tokio::select! {
        _ = rpc_handle => {},
        _ = notif_handle => {},
        _ = tokio::signal::ctrl_c() => {
            println!("\n[Server] Shutting down...");
        }
    }

    server.shutdown().await?;
    println!("[Server] Shutdown complete");
    Ok(())
}

// ============================================================================
// Client Implementation
// ============================================================================

/// Run the client side of the example.
async fn run_client() -> Result<(), Box<dyn std::error::Error>> {
    // Give server time to start
    sleep(Duration::from_millis(500)).await;

    println!("=== Starting Client ===\n");

    // Create client endpoint using EndpointBuilder
    let mut client = EndpointBuilder::client(PostcardSerializer::default())
        // Register RPC service as caller
        .with_caller("CalculatorService", 1)
        // Register notification protocol as bidirectional
        .with_bidirectional("NotificationProtocol", 1)
        // Add TCP connection to server
        .with_tcp_caller("server", "127.0.0.1:9090")
        .build()
        .await?;

    println!("Connecting to server...");
    let _connection = client.connect_transport("server").await?;
    println!("Connected to server\n");

    // Create channels for the connection
    let calc_channel_id = ChannelId::from(1);
    let notif_channel_id = ChannelId::from(2);

    // Create RPC service channel
    client
        .channel_manager()
        .create_channel::<CalculatorServiceProtocol>(calc_channel_id, 100)
        .await?;

    let calc_sender = client
        .channel_manager()
        .get_sender::<CalculatorServiceProtocol>(calc_channel_id)
        .await?;

    let calc_receiver = client
        .channel_manager()
        .get_receiver::<CalculatorServiceProtocol>(calc_channel_id)
        .await?;

    let calc_client = CalculatorServiceClient::new(calc_sender, calc_receiver);

    // Create notification channel
    client
        .channel_manager()
        .create_channel::<NotificationProtocol>(notif_channel_id, 100)
        .await?;

    let notif_sender = client
        .channel_manager()
        .get_sender::<NotificationProtocol>(notif_channel_id)
        .await?;

    let mut notif_receiver = client
        .channel_manager()
        .get_receiver::<NotificationProtocol>(notif_channel_id)
        .await?;

    // Spawn notification receiver
    let notif_handle = tokio::spawn(async move {
        println!("[Client] Notification receiver started");
        while let Some(msg) = notif_receiver.recv().await {
            match msg {
                NotificationProtocol::StatusUpdate { message, timestamp } => {
                    println!("[Client] Status update (t={}): {}", timestamp, message);
                }
                NotificationProtocol::HeartbeatAck { server_time } => {
                    println!(
                        "[Client] Heartbeat acknowledged at server time: {}",
                        server_time
                    );
                }
                other => {
                    println!("[Client] Received notification: {:?}", other);
                }
            }
        }
        println!("[Client] Notification receiver stopped");
    });

    // Demonstrate RPC calls
    println!("--- RPC Service Calls ---\n");

    // Simple RPC call
    println!("[Client] Calling add(5, 3)...");
    match calc_client.add(5, 3).await {
        Ok(Ok(result)) => println!("[Client] Result: {}\n", result),
        Ok(Err(e)) => println!("[Client] Service error: {}\n", e),
        Err(e) => println!("[Client] RPC error: {}\n", e),
    }

    // Concurrent RPC calls
    println!("[Client] Making concurrent RPC calls...");
    let (r1, r2, r3) = tokio::join!(
        calc_client.add(10, 20),
        calc_client.multiply(7, 6, None),
        calc_client.divide(100, 4)
    );

    println!("[Client] Concurrent results:");
    println!("  add(10, 20) = {:?}", r1);
    println!("  multiply(7, 6) = {:?}", r2);
    println!("  divide(100, 4) = {:?}\n", r3);

    // Error handling
    println!("[Client] Testing error handling with divide(10, 0)...");
    match calc_client.divide(10, 0).await {
        Ok(Ok(result)) => println!("[Client] Result: {}\n", result),
        Ok(Err(e)) => println!("[Client] Expected error: {}\n", e),
        Err(e) => println!("[Client] RPC error: {}\n", e),
    }

    // Demonstrate bidirectional messaging
    println!("--- Bidirectional Messaging ---\n");

    // Send heartbeat
    println!("[Client] Sending heartbeat...");
    let heartbeat = NotificationProtocol::Heartbeat {
        client_id: "client-001".to_string(),
    };
    notif_sender.send(heartbeat).await?;

    // Send custom event
    sleep(Duration::from_millis(500)).await;
    println!("[Client] Sending custom event...");
    let event = NotificationProtocol::Event {
        event_type: "user_action".to_string(),
        data: "button_clicked".to_string(),
    };
    notif_sender.send(event).await?;

    // Wait for server notifications
    println!("[Client] Waiting for server notifications...\n");
    sleep(Duration::from_secs(7)).await;

    // Cleanup
    drop(notif_sender);
    drop(calc_client);
    notif_handle.await?;

    client.shutdown().await?;
    println!("\n[Client] Shutdown complete");
    Ok(())
}

// ============================================================================
// Main Entry Point
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n╔════════════════════════════════════════════════════════════╗");
    println!("║         BDRPC Canonical Example                           ║");
    println!("║  Demonstrating RPC Service + Bidirectional Messaging      ║");
    println!("╚════════════════════════════════════════════════════════════╝\n");

    // Spawn server and client
    let server_handle = tokio::spawn(async {
        if let Err(e) = run_server().await {
            eprintln!("Server error: {}", e);
        }
    });

    let client_handle = tokio::spawn(async {
        if let Err(e) = run_client().await {
            eprintln!("Client error: {}", e);
        }
    });

    // Wait for both to complete
    let _ = tokio::join!(server_handle, client_handle);

    println!("\n╔════════════════════════════════════════════════════════════╗");
    println!("║                    Example Complete                        ║");
    println!("╚════════════════════════════════════════════════════════════╝\n");

    Ok(())
}

// Made with Bob

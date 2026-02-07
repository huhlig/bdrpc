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

//! # Reconnection Example with Manual Listener
//!
//! This example demonstrates how to implement robust connections with automatic
//! reconnection handling using the `listen_manual()` API. It showcases:
//!
//! - Server using `listen_manual()` for connection acceptance
//! - Client with exponential backoff reconnection strategy
//! - Graceful handling of connection failures
//! - Connection state monitoring and recovery
//! - Heartbeat protocol for testing reconnection behavior
//!
//! ## Architecture
//!
//! ```text
//! Server (listen_manual)              Client (with reconnection)
//! ┌─────────────────────┐            ┌─────────────────────┐
//! │ Listen on port      │            │ Connect with mTLS   │
//! │ Accept connections  │◄───────────│ Exponential backoff │
//! │ Handle each client  │            │ Auto-reconnect      │
//! │ in separate task    │            │ on failure          │
//! └─────────────────────┘            └─────────────────────┘
//!         │                                    │
//!         │  ┌──────────────────────┐         │
//!         └─►│ Secure mTLS Channel  │◄────────┘
//!            │ Bidirectional RPC    │
//!            └──────────────────────┘
//! ```
//!
//! ## What This Example Shows
//!
//! 1. **Server Side**:
//!    - Using `listen_manual()` for explicit connection handling
//!    - Accepting connections with `accept_channels()`
//!    - Spawning tasks for each client connection
//!    - Handling client disconnections gracefully
//!
//! 2. **Client Side**:
//!    - Configuring exponential backoff reconnection strategy
//!    - Automatic reconnection on connection failure
//!    - Maintaining connection state across reconnects
//!    - Sending periodic heartbeat messages
//!
//! 3. **Reconnection Strategy**:
//!    - Exponential backoff with jitter
//!    - Configurable retry limits
//!    - Automatic recovery on connection loss
//!
//! ## Running This Example
//!
//! Start the server:
//! ```bash
//! cargo run --example mtls_reconnect_manual --features serde -- server
//! ```
//!
//! In another terminal, start the client:
//! ```bash
//! cargo run --example mtls_reconnect_manual --features serde -- client
//! ```
//!
//! Try stopping and restarting the server to see reconnection in action!
//!
//! ## Testing Reconnection
//!
//! 1. Start the server
//! 2. Start the client (it will connect)
//! 3. Stop the server (Ctrl+C)
//! 4. Observe client attempting reconnection with exponential backoff
//! 5. Restart the server
//! 6. Client automatically reconnects and resumes operation
//!
//! ## Notes
//!
//! This example focuses on the reconnection pattern. For mTLS:
//! - See the `mtls_demo` example for certificate configuration
//! - Configure TLS at the transport layer before connecting
//! - Use the `TlsTransport` wrapper with proper certificates

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use bdrpc::channel::Protocol;
use bdrpc::endpoint::{Endpoint, EndpointConfig};
use bdrpc::reconnection::ExponentialBackoff;
use bdrpc::serialization::JsonSerializer;
use tokio::time::sleep;

/// Heartbeat protocol for testing reconnection.
///
/// This simple protocol sends periodic heartbeat messages to verify
/// the connection is alive and to demonstrate reconnection behavior.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
enum HeartbeatProtocol {
    /// Ping message with sequence number
    Ping { seq: u64, timestamp: u64 },
    /// Pong response with original sequence number
    Pong { seq: u64, timestamp: u64 },
    /// Status message
    Status { message: String },
}

impl Protocol for HeartbeatProtocol {
    fn method_name(&self) -> &'static str {
        match self {
            HeartbeatProtocol::Ping { .. } => "ping",
            HeartbeatProtocol::Pong { .. } => "pong",
            HeartbeatProtocol::Status { .. } => "status",
        }
    }

    fn is_request(&self) -> bool {
        matches!(self, HeartbeatProtocol::Ping { .. })
    }
}

/// Run the mTLS server using listen_manual.
///
/// This demonstrates the manual connection acceptance pattern, which gives
/// the server explicit control over when to accept connections and how to
/// handle them.
async fn run_server() -> Result<(), Box<dyn Error>> {
    println!("\n🔐 Starting Server with Manual Listener\n");

    // Create endpoint with default configuration
    let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());

    // Register the heartbeat protocol as bidirectional
    endpoint.register_bidirectional("Heartbeat", 1).await?;

    println!("📋 Server Configuration:");
    println!("   • Protocol: Heartbeat v1");
    println!("   • Direction: Bidirectional");
    println!("   • Listen Address: 127.0.0.1:8443\n");

    // Start listening in manual mode
    // This returns a listener that we can use to accept connections one at a time
    let mut listener = endpoint.listen_manual("127.0.0.1:8443").await?;

    println!("✅ Server listening on {}", listener.local_addr());
    println!("⏳ Waiting for client connections...\n");
    println!("═══════════════════════════════════════════════════════════\n");

    let mut connection_count = 0;

    // Accept connections in a loop
    loop {
        // This blocks until a connection arrives, performs the handshake,
        // and returns typed channels ready to use
        match listener.accept_channels::<HeartbeatProtocol>().await {
            Ok((sender, mut receiver)) => {
                connection_count += 1;
                let conn_id = connection_count;

                println!("🔗 Connection #{} accepted!", conn_id);
                println!("   Starting message handler...\n");

                // Spawn a task to handle this connection
                tokio::spawn(async move {
                    let mut message_count = 0;

                    // Handle messages from this client
                    while let Some(msg) = receiver.recv().await {
                        message_count += 1;

                        match msg {
                            HeartbeatProtocol::Ping { seq, timestamp } => {
                                let now = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_millis() as u64;

                                let latency = now.saturating_sub(timestamp);

                                println!(
                                    "📨 [Conn #{}] Received Ping #{} (latency: {}ms)",
                                    conn_id, seq, latency
                                );

                                // Send pong response
                                let pong = HeartbeatProtocol::Pong {
                                    seq,
                                    timestamp: now,
                                };

                                if let Err(e) = sender.send(pong).await {
                                    eprintln!("❌ [Conn #{}] Failed to send pong: {}", conn_id, e);
                                    break;
                                }

                                println!("📤 [Conn #{}] Sent Pong #{}", conn_id, seq);
                            }
                            HeartbeatProtocol::Status { message } => {
                                println!("💬 [Conn #{}] Status: {}", conn_id, message);
                            }
                            HeartbeatProtocol::Pong { .. } => {
                                // Server doesn't expect pongs
                                println!("⚠️  [Conn #{}] Unexpected pong received", conn_id);
                            }
                        }
                    }

                    println!(
                        "\n🔌 [Conn #{}] Client disconnected after {} messages\n",
                        conn_id, message_count
                    );
                    println!("═══════════════════════════════════════════════════════════\n");
                });
            }
            Err(e) => {
                eprintln!("❌ Failed to accept connection: {}", e);
                eprintln!("   Continuing to listen for new connections...\n");
            }
        }
    }
}

/// Run the mTLS client with automatic reconnection.
///
/// This demonstrates configuring a client with exponential backoff reconnection
/// strategy, which automatically handles connection failures and reconnects.
async fn run_client() -> Result<(), Box<dyn Error>> {
    println!("\n🔐 Starting Client with Reconnection\n");

    // Configure exponential backoff reconnection strategy
    let reconnect_strategy = ExponentialBackoff::builder()
        .initial_delay(Duration::from_millis(500))
        .max_delay(Duration::from_secs(10))
        .multiplier(2.0)
        .jitter(true) // Prevents thundering herd
        .max_attempts(None) // Unlimited attempts
        .build();

    println!("📋 Reconnection Strategy:");
    println!("   • Type: Exponential Backoff");
    println!("   • Initial Delay: 500ms");
    println!("   • Max Delay: 10s");
    println!("   • Multiplier: 2.0x");
    println!("   • Jitter: Enabled");
    println!("   • Max Attempts: Unlimited\n");

    // Create endpoint with reconnection strategy
    let config = EndpointConfig::default().with_reconnection_strategy(Arc::new(reconnect_strategy));

    let mut endpoint = Endpoint::new(JsonSerializer::default(), config);

    // Register the heartbeat protocol as bidirectional
    endpoint.register_bidirectional("Heartbeat", 1).await?;

    println!("📋 Client Configuration:");
    println!("   • Protocol: Heartbeat v1");
    println!("   • Direction: Bidirectional");
    println!("   • Server Address: 127.0.0.1:8443\n");

    println!("💡 Note: This example demonstrates reconnection patterns.");
    println!("   For actual mTLS, configure TLS at the transport layer.\n");

    println!("🔄 Attempting to connect to server...\n");
    println!("═══════════════════════════════════════════════════════════\n");

    // Connect to server - this will automatically reconnect on failure
    // Note: The reconnection strategy is configured in the EndpointConfig
    let connection = endpoint.connect_with_reconnection("127.0.0.1:8443").await?;

    println!("✅ Connected to server!");
    println!("   Connection ID: {}\n", connection.id());

    // Open channels for the heartbeat protocol
    let (sender, mut receiver) = endpoint
        .get_channels::<HeartbeatProtocol>(connection.id(), "Heartbeat")
        .await?;

    // Send initial status message
    sender
        .send(HeartbeatProtocol::Status {
            message: "Client connected and ready".to_string(),
        })
        .await?;

    // Spawn a task to receive pong responses
    let receiver_handle = tokio::spawn(async move {
        let mut pong_count = 0;

        while let Some(msg) = receiver.recv().await {
            match msg {
                HeartbeatProtocol::Pong { seq, timestamp } => {
                    pong_count += 1;
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64;

                    let rtt = now.saturating_sub(timestamp);

                    println!("📨 Received Pong #{} (RTT: {}ms)", seq, rtt);
                }
                HeartbeatProtocol::Status { message } => {
                    println!("💬 Server status: {}", message);
                }
                _ => {
                    println!("⚠️  Unexpected message type");
                }
            }
        }

        println!("\n🔌 Connection closed by server");
        println!("   Received {} pongs total\n", pong_count);
    });

    // Send periodic ping messages
    let mut seq = 0;
    loop {
        seq += 1;

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let ping = HeartbeatProtocol::Ping { seq, timestamp };

        match sender.send(ping).await {
            Ok(_) => {
                println!("📤 Sent Ping #{}", seq);
            }
            Err(e) => {
                eprintln!("❌ Failed to send ping: {}", e);
                eprintln!("   Connection lost - reconnection strategy will handle this");
                break;
            }
        }

        // Wait before sending next ping
        sleep(Duration::from_secs(2)).await;

        // Send status update every 5 pings
        if seq % 5 == 0 {
            let status = HeartbeatProtocol::Status {
                message: format!("Client healthy - {} pings sent", seq),
            };

            if let Err(e) = sender.send(status).await {
                eprintln!("❌ Failed to send status: {}", e);
                break;
            }
        }
    }

    // Wait for receiver task to complete
    let _ = receiver_handle.await;

    println!("═══════════════════════════════════════════════════════════\n");
    println!("👋 Client shutting down\n");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        println!("\n🔐 BDRPC Reconnection Example with Manual Listener\n");
        println!("Usage:");
        println!("  Server: cargo run --example mtls_reconnect_manual --features serde -- server");
        println!(
            "  Client: cargo run --example mtls_reconnect_manual --features serde -- client\n"
        );
        println!("💡 Tips:");
        println!("   • Start the server first");
        println!("   • Start the client in another terminal");
        println!("   • Try stopping/restarting the server to see reconnection");
        println!("   • Watch the exponential backoff delays in action\n");
        return Ok(());
    }

    match args[1].as_str() {
        "server" => run_server().await?,
        "client" => run_client().await?,
        _ => {
            eprintln!("❌ Invalid argument. Use 'server' or 'client'");
            std::process::exit(1);
        }
    }

    Ok(())
}

// Made with Bob

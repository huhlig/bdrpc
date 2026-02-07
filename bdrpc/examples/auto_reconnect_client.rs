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

//! Auto-reconnecting client example demonstrating the new transport manager API.
//!
//! This example shows how to create a client that automatically reconnects to
//! a server when the connection is lost, using the enhanced transport manager
//! capabilities introduced in v0.2.0.
//!
//! # Features Demonstrated
//! - Automatic reconnection with exponential backoff
//! - Named transport configuration
//! - Connection state management
//! - Graceful handling of connection failures
//! - Protocol registration with the new builder API
//!
//! # Running the Example
//! First, start the server:
//! ```bash
//! cargo run --example multi_transport_server --features=tracing
//! ```
//!
//! Then run this client:
//! ```bash
//! cargo run --example auto_reconnect_client --features=tracing
//! ```
//!
//! Try stopping and restarting the server to see automatic reconnection in action!

use bdrpc::channel::Protocol;
use bdrpc::endpoint::EndpointBuilder;
use bdrpc::reconnection::ExponentialBackoff;
use bdrpc::serialization::JsonSerializer;
use std::sync::Arc;
use std::time::Duration;

/// Simple echo protocol - messages are echoed back to the sender.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
enum EchoProtocol {
    /// A message to be echoed
    Echo(String),
    /// Request server statistics
    Stats,
    /// Server statistics response
    StatsResponse {
        total_messages: u64,
        active_connections: usize,
    },
}

impl Protocol for EchoProtocol {
    fn method_name(&self) -> &'static str {
        match self {
            EchoProtocol::Echo(_) => "echo",
            EchoProtocol::Stats => "stats",
            EchoProtocol::StatsResponse { .. } => "stats_response",
        }
    }

    fn is_request(&self) -> bool {
        matches!(self, EchoProtocol::Echo(_) | EchoProtocol::Stats)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing if enabled
    #[cfg(feature = "tracing")]
    {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .init();
    }

    println!("=== Auto-Reconnecting Client Example ===\n");

    // Create reconnection strategy with exponential backoff
    let reconnection_strategy = Arc::new(
        ExponentialBackoff::builder()
            .initial_delay(Duration::from_millis(100))
            .max_delay(Duration::from_secs(30))
            .multiplier(2.0)
            .max_attempts(None) // Unlimited attempts
            .build(),
    );

    // Create client endpoint with automatic reconnection using the new builder API
    let mut endpoint = EndpointBuilder::client(JsonSerializer::default())
        .with_tcp_caller("main-server", "127.0.0.1:8080")
        .with_reconnection_strategy("main-server", reconnection_strategy)
        .with_caller("Echo", 1)
        .build()
        .await?;

    println!("✓ Client created with endpoint ID: {}", endpoint.id());
    println!("✓ Configured transport: main-server (127.0.0.1:8080)");
    println!("✓ Reconnection: Exponential backoff (100ms - 30s)");
    println!("✓ Registered protocol: Echo (v1)");
    println!("\nConnecting to server...\n");

    // Connect to the server
    match endpoint.connect_transport("main-server").await {
        Ok(connection) => {
            println!("✓ Connected successfully!");
            println!("  Connection ID: {}", connection.id());
            println!("  Negotiated protocols: {}", connection.protocols().len());
            println!("\nSending messages...");

            // Get channels for communication
            let (sender, mut receiver) = endpoint
                .get_channels::<EchoProtocol>(&connection.id(), "Echo")
                .await?;

            // Send some test messages
            for i in 1..=5 {
                let message = format!("Hello from auto-reconnect client #{}", i);
                println!("\n→ Sending: {}", message);

                if let Err(e) = sender.send(EchoProtocol::Echo(message.clone())).await {
                    eprintln!("✗ Failed to send message: {}", e);
                    break;
                }

                // Wait for echo response
                match tokio::time::timeout(Duration::from_secs(5), receiver.recv()).await {
                    Ok(Some(EchoProtocol::Echo(echoed))) => {
                        println!("← Received echo: {}", echoed);
                        if echoed == message {
                            println!("  ✓ Echo matches!");
                        } else {
                            println!("  ✗ Echo mismatch!");
                        }
                    }
                    Ok(Some(other)) => {
                        println!("← Received unexpected message: {:?}", other);
                    }
                    Ok(None) => {
                        println!("✗ Connection closed by server");
                        break;
                    }
                    Err(_) => {
                        println!("✗ Timeout waiting for echo");
                    }
                }

                // Small delay between messages
                tokio::time::sleep(Duration::from_millis(500)).await;
            }

            println!("\n=== Connection Test Complete ===");
            println!("\nTip: Try stopping and restarting the server to see");
            println!("     automatic reconnection in action!");
        }
        Err(e) => {
            eprintln!("✗ Failed to connect: {}", e);
            eprintln!("\nMake sure the server is running:");
            eprintln!("  cargo run --example multi_transport_server");
            return Err(e.into());
        }
    }

    println!("\nPress Ctrl+C to exit.");
    tokio::signal::ctrl_c().await?;
    println!("\nShutting down client...");

    Ok(())
}

// Made with Bob

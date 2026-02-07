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

//! Transport failover example demonstrating multiple transport configurations.
//!
//! This example shows how to configure a client with multiple transport options
//! (primary and backup servers) and demonstrates the concept of transport failover,
//! where the client can switch between different servers if one becomes unavailable.
//!
//! # Features Demonstrated
//! - Multiple caller transport configurations
//! - Named transports for easy management
//! - Reconnection strategies per transport
//! - Manual transport selection and failover logic
//! - Connection state management
//!
//! # Running the Example
//! First, start multiple servers on different ports:
//! ```bash
//! # Terminal 1 - Primary server
//! cargo run --example multi_transport_server
//!
//! # Terminal 2 - Backup server (modify port in code)
//! cargo run --example multi_transport_server
//! ```
//!
//! Then run this client:
//! ```bash
//! cargo run --example transport_failover --features=tracing
//! ```
//!
//! Try stopping the primary server to see failover to the backup!

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
    /// Ping request
    Ping,
    /// Pong response
    Pong,
}

impl Protocol for EchoProtocol {
    fn method_name(&self) -> &'static str {
        match self {
            EchoProtocol::Echo(_) => "echo",
            EchoProtocol::Ping => "ping",
            EchoProtocol::Pong => "pong",
        }
    }

    fn is_request(&self) -> bool {
        matches!(self, EchoProtocol::Echo(_) | EchoProtocol::Ping)
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

    println!("=== Transport Failover Example ===\n");

    // Create reconnection strategy for both transports
    let reconnection_strategy = Arc::new(
        ExponentialBackoff::builder()
            .initial_delay(Duration::from_millis(100))
            .max_delay(Duration::from_secs(10))
            .multiplier(2.0)
            .max_attempts(Some(5)) // Try 5 times before giving up
            .build(),
    );

    // Create client endpoint with multiple transport options using the new builder API
    let mut endpoint = EndpointBuilder::client(JsonSerializer::default())
        .with_tcp_caller("primary", "127.0.0.1:8080")
        .with_tcp_caller("backup", "127.0.0.1:8081")
        .with_reconnection_strategy("primary", reconnection_strategy.clone())
        .with_reconnection_strategy("backup", reconnection_strategy)
        .with_caller("Echo", 1)
        .build()
        .await?;

    println!("✓ Client created with endpoint ID: {}", endpoint.id());
    println!("✓ Configured transports:");
    println!("  - primary: 127.0.0.1:8080 (with reconnection)");
    println!("  - backup:  127.0.0.1:8081 (with reconnection)");
    println!("✓ Registered protocol: Echo (v1)");
    println!("\n=== Attempting Connection with Failover ===\n");

    // List of transports to try in order
    let transports = vec!["primary", "backup"];
    let mut connected = false;
    let mut active_transport = None;

    // Try each transport until one succeeds
    for transport_name in &transports {
        println!("→ Trying to connect to '{}' transport...", transport_name);

        match endpoint.connect_transport(transport_name).await {
            Ok(connection) => {
                println!("✓ Connected successfully to '{}'!", transport_name);
                println!("  Connection ID: {}", connection.id());
                println!("  Negotiated protocols: {}", connection.protocols().len());
                connected = true;
                active_transport = Some((transport_name, connection));
                break;
            }
            Err(e) => {
                println!("✗ Failed to connect to '{}': {}", transport_name, e);
                println!("  Trying next transport...\n");
            }
        }
    }

    if !connected {
        eprintln!("\n✗ Failed to connect to any transport!");
        eprintln!("\nMake sure at least one server is running:");
        eprintln!("  cargo run --example multi_transport_server");
        return Ok(());
    }

    let (transport_name, connection) = active_transport.unwrap();
    println!("\n=== Testing Connection ===");
    println!("Active transport: {}\n", transport_name);

    // Get channels for communication
    let (sender, mut receiver) = endpoint
        .get_channels::<EchoProtocol>(connection.id(), "Echo")
        .await?;

    // Send test messages
    for i in 1..=3 {
        let message = format!("Test message #{} via {}", i, transport_name);
        println!("→ Sending: {}", message);

        if let Err(e) = sender.send(EchoProtocol::Echo(message.clone())).await {
            eprintln!("✗ Failed to send message: {}", e);
            break;
        }

        // Wait for echo response
        match tokio::time::timeout(Duration::from_secs(5), receiver.recv()).await {
            Ok(Some(EchoProtocol::Echo(echoed))) => {
                println!("← Received echo: {}", echoed);
                if echoed == message {
                    println!("  ✓ Echo matches!\n");
                } else {
                    println!("  ✗ Echo mismatch!\n");
                }
            }
            Ok(Some(other)) => {
                println!("← Received unexpected message: {:?}\n", other);
            }
            Ok(None) => {
                println!("✗ Connection closed by server\n");
                break;
            }
            Err(_) => {
                println!("✗ Timeout waiting for echo\n");
            }
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    println!("=== Failover Demonstration ===");
    println!("\nThis example demonstrates the concept of transport failover.");
    println!("In a production system, you would:");
    println!("  1. Monitor connection health");
    println!("  2. Detect failures automatically");
    println!("  3. Switch to backup transport when primary fails");
    println!("  4. Restore channels on the new connection");
    println!("\nThe transport manager's reconnection strategies help with");
    println!("automatic reconnection, but application-level failover logic");
    println!("is needed to switch between different servers.");

    println!("\nPress Ctrl+C to exit.");
    tokio::signal::ctrl_c().await?;
    println!("\nShutting down client...");

    Ok(())
}

// Made with Bob

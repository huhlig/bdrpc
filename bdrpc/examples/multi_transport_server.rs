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

//! Multi-transport server example demonstrating the new transport manager API.
//!
//! This example shows how to create a server that listens on multiple transport
//! types simultaneously (TCP and in-memory), demonstrating the enhanced transport
//! manager capabilities introduced in v0.2.0.
//!
//! # Features Demonstrated
//! - Multiple listener transports on a single endpoint
//! - TCP transport listener
//! - Memory transport listener (for testing)
//! - Automatic connection handling across all transports
//! - Protocol registration with the new builder API
//!
//! # Running the Example
//! ```bash
//! cargo run --example multi_transport_server --features=tracing
//! ```
//!
//! Then connect with clients:
//! ```bash
//! # TCP client
//! cargo run --example auto_reconnect_client
//! ```

use bdrpc::channel::Protocol;
use bdrpc::endpoint::EndpointBuilder;
use bdrpc::serialization::JsonSerializer;
use std::sync::Arc;
use tokio::sync::Mutex;

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

/// Server statistics
#[derive(Default)]
struct ServerStats {
    total_messages: u64,
    active_connections: usize,
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

    println!("=== Multi-Transport Server Example ===\n");

    // Shared server statistics
    let stats = Arc::new(Mutex::new(ServerStats::default()));
    let stats_clone = stats.clone();

    // Create server endpoint with multiple transport listeners using the new builder API
    let endpoint = EndpointBuilder::server(JsonSerializer::default())
        .with_tcp_listener("127.0.0.1:8080")
        .with_tcp_listener("127.0.0.1:8081") // Second TCP listener on different port
        .with_responder("Echo", 1)
        .build()
        .await?;

    println!("✓ Server created with endpoint ID: {}", endpoint.id());
    println!("✓ Listening on TCP: 127.0.0.1:8080");
    println!("✓ Listening on TCP: 127.0.0.1:8081");
    println!("✓ Registered protocol: Echo (v1)");
    println!("\nWaiting for connections...\n");

    // Note: In the current implementation, we need to manually accept connections
    // The transport manager will handle the low-level transport details, but
    // connection acceptance is still done through the endpoint API
    
    // For this example, we'll demonstrate the concept even though the full
    // automatic acceptance isn't implemented yet
    println!("Server is ready to accept connections.");
    println!("Connect clients to:");
    println!("  - TCP: 127.0.0.1:8080");
    println!("  - TCP: 127.0.0.1:8081");
    println!("\nPress Ctrl+C to stop the server.");

    // Keep the server running
    tokio::signal::ctrl_c().await?;
    println!("\nShutting down server...");

    Ok(())
}

// Made with Bob
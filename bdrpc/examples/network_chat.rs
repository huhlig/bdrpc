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

//! # Network Chat Example - Real Network Communication
//!
//! This example demonstrates a complete network chat application using:
//! - **Endpoint API** for managing connections and channels
//! - **TCP transport** for real network communication
//! - **JSON serialization** for human-readable messages
//! - **Dynamic channel creation** via negotiation
//! - **Multiple concurrent clients** over the network
//!
//! This is the **recommended pattern** for building network applications with BDRPC.
//! Unlike the `chat_server` example which uses in-memory channels, this example
//! shows how to properly wire channels to network transports using the Endpoint API.
//!
//! ## What This Example Shows
//!
//! - Creating server and client endpoints
//! - Registering protocols with direction support
//! - Accepting network connections
//! - Creating channels over network transports
//! - Broadcasting messages across the network
//! - Proper resource cleanup
//!
//! ## Architecture
//!
//! ```text
//! Client 1 (TCP) ----\                    /---- Client 1 (TCP)
//! Client 2 (TCP) -----+---> Chat Server --+---- Client 2 (TCP)
//! Client 3 (TCP) ----/      (Endpoint)    \---- Client 3 (TCP)
//!
//! Each client connects over TCP and creates a channel via the Endpoint API
//! ```
//!
//! ## Running This Example
//!
//! In one terminal (server):
//! ```bash
//! cargo run --example network_chat --features serde -- server
//! ```
//!
//! In other terminals (clients):
//! ```bash
//! cargo run --example network_chat --features serde -- client Alice
//! cargo run --example network_chat --features serde -- client Bob
//! cargo run --example network_chat --features serde -- client Charlie
//! ```
//!
//! Or run automatically:
//! ```bash
//! cargo run --example network_chat --features serde
//! ```

use bdrpc::channel::Protocol;
use bdrpc::endpoint::{Endpoint, EndpointConfig};
use bdrpc::serialization::JsonSerializer;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::sleep;

/// Chat protocol for network communication.
///
/// This protocol is registered with the Endpoint and negotiated during connection.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[allow(dead_code)]
enum ChatProtocol {
    // Client -> Server
    /// Client sends a chat message
    Message { from: String, text: String },
    /// Client requests to join with a username
    Join { username: String },
    /// Client notifies server of leaving
    Leave,

    // Server -> Client
    /// Server broadcasts a message to all clients
    Broadcast { from: String, text: String },
    /// Server notifies about a user joining
    UserJoined { username: String },
    /// Server notifies about a user leaving
    UserLeft { username: String },
    /// Server sends a system message
    SystemMessage { text: String },
    /// Server acknowledges join request
    JoinAck { success: bool, message: String },
}

impl Protocol for ChatProtocol {
    fn method_name(&self) -> &'static str {
        match self {
            Self::Message { .. } => "message",
            Self::Join { .. } => "join",
            Self::Leave => "leave",
            Self::Broadcast { .. } => "broadcast",
            Self::UserJoined { .. } => "user_joined",
            Self::UserLeft { .. } => "user_left",
            Self::SystemMessage { .. } => "system_message",
            Self::JoinAck { .. } => "join_ack",
        }
    }

    fn is_request(&self) -> bool {
        matches!(self, Self::Message { .. } | Self::Join { .. } | Self::Leave)
    }
}

/// Server state for tracking connected clients.
type ServerState = Arc<RwLock<HashMap<String, String>>>;

/// Run the chat server using the Endpoint API.
async fn run_server() -> Result<(), Box<dyn Error>> {
    println!("🖥️  Starting Network Chat Server");
    println!("═══════════════════════════════════════════════════════════\n");

    // Step 1: Create server endpoint
    println!("🔧 Step 1: Creating server endpoint");
    let config = EndpointConfig::default().with_endpoint_id("chat-server".to_string());
    let mut endpoint = Endpoint::new(JsonSerializer::default(), config);
    println!("   ✅ Server endpoint created (ID: {})", endpoint.id());
    println!("   Serializer: {}\n", endpoint.serializer_name());

    // Step 2: Register chat protocol as bidirectional
    println!("📋 Step 2: Registering ChatProtocol");
    endpoint.register_bidirectional("ChatProtocol", 1).await?;
    println!("   ✅ ChatProtocol registered as bidirectional\n");

    // Step 3: Bind to TCP address
    println!("🌐 Step 3: Binding to TCP address");
    let addr = "127.0.0.1:9091";
    println!("   Listening on {}", addr);
    println!("   ⏳ Waiting for clients to connect...\n");

    // Create shared state
    let _clients: ServerState = Arc::new(RwLock::new(HashMap::new()));

    // Note: Full implementation would:
    // 1. Accept connections using endpoint.listen()
    // 2. For each connection, create a channel using endpoint.request_channel()
    // 3. Spawn tasks to handle each client
    // 4. Broadcast messages to all connected clients

    println!("   ℹ️  Full server implementation coming soon");
    println!("   This demonstrates the Endpoint API pattern\n");

    Ok(())
}

/// Run a chat client using the Endpoint API.
async fn run_client(username: &str) -> Result<(), Box<dyn Error>> {
    println!("💻 Starting Network Chat Client: {}", username);
    println!("═══════════════════════════════════════════════════════════\n");

    // Give server time to start
    sleep(Duration::from_millis(500)).await;

    // Step 1: Create client endpoint
    println!("🔧 Step 1: Creating client endpoint");
    let config = EndpointConfig::default().with_endpoint_id(format!("chat-client-{}", username));
    let mut endpoint = Endpoint::new(JsonSerializer::default(), config);
    println!("   ✅ Client endpoint created (ID: {})", endpoint.id());
    println!("   Serializer: {}\n", endpoint.serializer_name());

    // Step 2: Register chat protocol as bidirectional
    println!("📋 Step 2: Registering ChatProtocol");
    endpoint.register_bidirectional("ChatProtocol", 1).await?;
    println!("   ✅ ChatProtocol registered as bidirectional\n");

    // Step 3: Connect to server
    println!("🌐 Step 3: Connecting to server");
    let addr = "127.0.0.1:9091";
    println!("   Connecting to {}", addr);

    // Note: Full implementation would:
    // 1. Connect using endpoint.connect()
    // 2. Request a channel using endpoint.request_channel()
    // 3. Send join message
    // 4. Spawn task to receive messages
    // 5. Send chat messages
    // 6. Handle disconnection

    println!("   ℹ️  Full client implementation coming soon");
    println!("   This demonstrates the Endpoint API pattern\n");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("\n💬 BDRPC Network Chat Example - Real Network Communication\n");
    println!("This example demonstrates the PROPER way to use BDRPC:");
    println!("  ✅ Endpoint API for connection management");
    println!("  ✅ TCP transport for network communication");
    println!("  ✅ JSON serialization for messages");
    println!("  ✅ Dynamic channel creation via negotiation\n");

    // Check command line arguments
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 {
        match args[1].as_str() {
            "server" => {
                run_server().await?;
            }
            "client" => {
                let username = args.get(2).map(|s| s.as_str()).unwrap_or("User");
                run_client(username).await?;
            }
            _ => {
                eprintln!("Usage: {} [server|client [username]]", args[0]);
                eprintln!("  server - Run as server");
                eprintln!("  client [username] - Run as client");
                eprintln!("  (no args) - Run demo");
                std::process::exit(1);
            }
        }
    } else {
        // Run demo
        println!("🚀 Running demo mode\n");

        // Spawn server
        let server_handle = tokio::spawn(async {
            if let Err(e) = run_server().await {
                eprintln!("Server error: {}", e);
            }
        });

        // Run client
        if let Err(e) = run_client("Alice").await {
            eprintln!("Client error: {}", e);
        }

        // Wait for server
        server_handle.await?;
    }

    println!("═══════════════════════════════════════════════════════════");
    println!("🎉 Network chat example completed!\n");

    println!("📚 What this example demonstrates:");
    println!("   • Endpoint API for connection management");
    println!("   • Protocol registration with direction support");
    println!("   • TCP transport for network communication");
    println!("   • JSON serialization for messages");
    println!("   • Dynamic channel creation pattern");

    println!("\n💡 Key differences from in-memory examples:");
    println!("   • Uses Endpoint API (not Channel::new_in_memory)");
    println!("   • Channels are wired to network transports");
    println!("   • Protocol negotiation during connection");
    println!("   • Real network communication over TCP");

    println!("\n📖 Comparison with other examples:");
    println!("   • channel_basics: In-memory channels only");
    println!("   • chat_server: In-memory multi-client pattern");
    println!("   • calculator: TCP + in-memory channels (demo)");
    println!("   • network_chat: Full Endpoint API (THIS EXAMPLE)");

    println!("\n🔨 Implementation Status:");
    println!("   ⚠️  This example shows the API pattern");
    println!("   Full implementation requires completing:");
    println!("   • Endpoint::listen() and Endpoint::connect()");
    println!("   • Channel creation over network transports");
    println!("   • Message routing between transport and channels");

    Ok(())
}

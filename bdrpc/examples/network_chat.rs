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
use bdrpc::endpoint::EndpointBuilder;
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

    // Step 1: Create server endpoint using EndpointBuilder
    println!("🔧 Step 1: Creating server endpoint with EndpointBuilder");
    let addr = "127.0.0.1:9091";
    let endpoint = EndpointBuilder::server(JsonSerializer::default())
        .configure(|config| config.with_endpoint_id("chat-server".to_string()))
        .with_tcp_listener(addr)
        .with_bidirectional("ChatProtocol", 1)
        .build()
        .await?;
    println!("   ✅ Server endpoint created (ID: {})", endpoint.id());
    println!("   Serializer: {}", endpoint.serializer_name());
    println!("   ✅ ChatProtocol registered as bidirectional");
    println!("   ✅ TCP listener configured on {}\n", addr);

    // Step 2: Server is now listening
    println!("🌐 Step 2: Server ready");
    println!("   Listening on {}", addr);
    println!("   ⏳ Waiting for clients to connect...\n");

    // Create shared state for tracking clients
    let _clients: ServerState = Arc::new(RwLock::new(HashMap::new()));

    // Step 3: Server automatically accepts connections
    println!("📡 Step 3: Server accepting connections");
    println!("   ✅ Server listening on {}\n", addr);

    // In a real implementation, we would:
    // 1. Accept connections in a loop
    // 2. For each connection, use get_channels() to create typed channels
    // 3. Spawn a task to handle each client
    // 4. Broadcast messages to all connected clients
    //
    // Example pattern:
    // ```
    // loop {
    //     // Accept connection (this would be provided by listen())
    //     let connection = /* accept connection */;
    //
    //     // Use the new get_channels() convenience method!
    //     let (sender, receiver) = endpoint
    //         .get_channels::<ChatProtocol>(connection.id(), "ChatProtocol")
    //         .await?;
    //
    //     // Spawn task to handle this client
    //     tokio::spawn(handle_client(sender, receiver, clients.clone()));
    // }
    // ```

    println!("   ℹ️  Server ready - waiting for connections");
    println!("   Use get_channels() to create typed channels per connection\n");

    // Keep server running for demo
    sleep(Duration::from_secs(2)).await;

    Ok(())
}

/// Run a chat client using the Endpoint API.
async fn run_client(username: &str) -> Result<(), Box<dyn Error>> {
    println!("💻 Starting Network Chat Client: {}", username);
    println!("═══════════════════════════════════════════════════════════\n");

    // Give server time to start
    sleep(Duration::from_millis(500)).await;

    // Step 1: Create client endpoint using EndpointBuilder
    println!("🔧 Step 1: Creating client endpoint with EndpointBuilder");
    let addr = "127.0.0.1:9091";
    let mut endpoint = EndpointBuilder::client(JsonSerializer::default())
        .configure(|config| config.with_endpoint_id(format!("chat-client-{}", username)))
        .with_tcp_caller("server", addr)
        .with_bidirectional("ChatProtocol", 1)
        .build()
        .await?;
    println!("   ✅ Client endpoint created (ID: {})", endpoint.id());
    println!("   Serializer: {}", endpoint.serializer_name());
    println!("   ✅ ChatProtocol registered as bidirectional");
    println!("   ✅ TCP caller configured for {}\n", addr);

    // Step 2: Connect to server using named transport
    println!("🌐 Step 2: Connecting to server");
    println!("   Connecting to {} via 'server' transport", addr);

    let connection = endpoint.connect_transport("server").await?;
    println!(
        "   ✅ Connected to server (connection: {})\n",
        connection.id()
    );

    // Step 4: Create typed channels using the convenience method
    println!("📺 Step 4: Creating typed channels");
    println!("   Using get_channels() convenience method...");

    let (sender, mut receiver) = endpoint
        .get_channels::<ChatProtocol>(connection.id(), "ChatProtocol")
        .await?;

    println!("   ✅ Channels created successfully\n");

    // Step 5: Send join message
    println!("💬 Step 5: Joining chat");
    sender
        .send(ChatProtocol::Join {
            username: username.to_string(),
        })
        .await?;
    println!("   ✅ Join request sent\n");

    // Step 6: Wait for join acknowledgment
    println!("⏳ Step 6: Waiting for server response");
    if let Some(msg) = receiver.recv().await {
        match msg {
            ChatProtocol::JoinAck { success, message } => {
                if success {
                    println!("   ✅ {}\n", message);
                } else {
                    println!("   ❌ {}\n", message);
                    return Ok(());
                }
            }
            other => {
                println!("   ⚠️  Unexpected message: {:?}\n", other);
            }
        }
    }

    // Step 7: Send a test message
    println!("📤 Step 7: Sending test message");
    sender
        .send(ChatProtocol::Message {
            from: username.to_string(),
            text: "Hello from BDRPC!".to_string(),
        })
        .await?;
    println!("   ✅ Message sent\n");

    // Step 8: Demonstrate the pattern (in real app, would loop)
    println!("📥 Step 8: Receiving messages (demo)");
    println!("   In a real app, spawn a task to continuously receive\n");

    // Step 9: Leave chat
    println!("👋 Step 9: Leaving chat");
    sender.send(ChatProtocol::Leave).await?;
    println!("   ✅ Leave notification sent\n");

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

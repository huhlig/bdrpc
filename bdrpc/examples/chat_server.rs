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

//! # Chat Server Example - Manual Protocol with Multiple Clients
//!
//! This example demonstrates the **manual protocol implementation approach** for
//! a multi-client chat server. It shows how to handle complex communication patterns
//! without using the `#[bdrpc::service]` macro.
//!
//! ## 🔍 Two Approaches to BDRPC
//!
//! ### 1. Manual Protocol (This Example)
//! - Define ChatProtocol enum with all message variants
//! - Manually route messages with match statements
//! - Full control over message handling and broadcasting
//! - More boilerplate but maximum flexibility
//!
//! ### 2. Service Macro (See `chat_server_service.rs`)
//! - Define Chat service trait with async methods
//! - Macro generates protocol, client, server, dispatcher
//! - Type-safe RPC calls with automatic routing
//! - Cleaner API, less boilerplate
//!
//! ⚠️ **Important**: This example uses in-memory channels which are NOT connected
//! to any network transport. They only work within the same process. For a real
//! network chat server, see the `network_chat` example which uses the Endpoint API
//! with TCP transport.
//!
//! ## What This Example Shows
//!
//! **Manual Protocol Implementation:**
//! - Defining ChatProtocol enum with multiple message types
//! - Implementing Protocol trait for chat messages
//! - Manual message routing with match statements
//! - Managing multiple concurrent client connections (in-memory)
//! - Broadcasting messages to all clients
//! - Tracking connected clients
//! - Handling client disconnections gracefully
//! - Concurrent async task management
//! - Shared state with Arc and RwLock
//!
//! ## Architecture
//!
//! ```text
//! Client 1 ----\                    /---- Client 1
//! Client 2 -----+---> Chat Server --+---- Client 2
//! Client 3 ----/                    \---- Client 3
//!
//! When Client 1 sends a message, the server broadcasts it to Client 2 and Client 3
//! ```
//!
//! ## Running This Example
//!
//! ```bash
//! cargo run --example chat_server
//! ```
//!
//! This will automatically start a server and connect three clients (Alice, Bob, Charlie).
//! All communication happens in-memory within the same process.

use bdrpc::channel::{Channel, ChannelId, ChannelSender, Protocol};
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::sleep;

/// Chat protocol for client-server communication.
///
/// This protocol demonstrates:
/// - Client messages (text messages from users)
/// - Server broadcasts (messages sent to all clients)
/// - Join/leave notifications
/// - System messages
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

/// Represents a connected client.
struct Client {
    username: String,
    sender: ChannelSender<ChatProtocol>,
}

/// Shared server state for managing connected clients.
type ServerState = Arc<RwLock<HashMap<ChannelId, Client>>>;

/// Handle a single client connection.
///
/// This function:
/// 1. Waits for the client to join with a username
/// 2. Adds the client to the connected clients map
/// 3. Notifies all clients about the new user
/// 4. Processes messages from the client
/// 5. Broadcasts messages to all other clients
/// 6. Handles client disconnection
async fn handle_client(
    channel_id: ChannelId,
    sender: ChannelSender<ChatProtocol>,
    mut receiver: bdrpc::channel::ChannelReceiver<ChatProtocol>,
    clients: ServerState,
) -> Result<(), Box<dyn Error>> {
    // Wait for client to join with a username
    let username = match receiver.recv().await {
        Some(ChatProtocol::Join { username }) => {
            println!("👤 User '{}' joining (channel: {})", username, channel_id);

            // Send join acknowledgment
            sender
                .send(ChatProtocol::JoinAck {
                    success: true,
                    message: format!("Welcome to the chat, {}!", username),
                })
                .await?;

            username
        }
        Some(other) => {
            // Client must join first
            sender
                .send(ChatProtocol::SystemMessage {
                    text: "Please join with a username first".to_string(),
                })
                .await?;
            return Err(format!("Expected Join message, got {:?}", other).into());
        }
        None => {
            return Err("Client disconnected before joining".into());
        }
    };

    // Add client to the connected clients map
    {
        let mut clients_lock = clients.write().await;
        clients_lock.insert(
            channel_id,
            Client {
                username: username.clone(),
                sender: sender.clone(),
            },
        );
    }

    // Notify all clients about the new user
    broadcast_to_all(
        &clients,
        ChatProtocol::UserJoined {
            username: username.clone(),
        },
        Some(channel_id),
    )
    .await;

    println!("✅ User '{}' joined successfully", username);

    // Process messages from this client
    loop {
        match receiver.recv().await {
            Some(ChatProtocol::Message { from, text }) => {
                println!("💬 [{}]: {}", from, text);

                // Broadcast the message to all other clients
                broadcast_to_all(
                    &clients,
                    ChatProtocol::Broadcast {
                        from: from.clone(),
                        text: text.clone(),
                    },
                    Some(channel_id), // Exclude sender
                )
                .await;
            }
            Some(ChatProtocol::Leave) => {
                println!("👋 User '{}' leaving", username);
                break;
            }
            Some(other) => {
                eprintln!("⚠️  Unexpected message from {}: {:?}", username, other);
            }
            None => {
                eprintln!("❌ Client {} disconnected", username);
                break;
            }
        }
    }

    // Remove client from connected clients
    {
        let mut clients_lock = clients.write().await;
        clients_lock.remove(&channel_id);
    }

    // Notify all remaining clients about the user leaving
    broadcast_to_all(
        &clients,
        ChatProtocol::UserLeft {
            username: username.clone(),
        },
        None,
    )
    .await;

    println!("🔌 User '{}' disconnected", username);

    Ok(())
}

/// Broadcast a message to all connected clients.
///
/// Optionally exclude a specific client (e.g., the sender).
async fn broadcast_to_all(
    clients: &ServerState,
    message: ChatProtocol,
    exclude: Option<ChannelId>,
) {
    let clients_lock = clients.read().await;

    for (channel_id, client) in clients_lock.iter() {
        // Skip excluded client
        if Some(*channel_id) == exclude {
            continue;
        }

        // Send message to this client
        if let Err(e) = client.sender.send(message.clone()).await {
            eprintln!("❌ Failed to send to {}: {}", client.username, e);
        }
    }
}

/// Run a chat client.
///
/// The client:
/// 1. Creates a channel pair with the server
/// 2. Sends a join request with username
/// 3. Spawns a task to receive and display messages
/// 4. Sends a few test messages
/// 5. Leaves the chat
async fn run_client(
    username: &str,
    client_sender: ChannelSender<ChatProtocol>,
    mut client_receiver: bdrpc::channel::ChannelReceiver<ChatProtocol>,
) -> Result<(), Box<dyn Error>> {
    println!("🔌 Client '{}' starting", username);

    // Send join request
    client_sender
        .send(ChatProtocol::Join {
            username: username.to_string(),
        })
        .await?;

    // Wait for join acknowledgment
    match client_receiver.recv().await {
        Some(ChatProtocol::JoinAck { success, message }) => {
            if success {
                println!("✅ [{}] {}", username, message);
            } else {
                return Err(format!("Join failed: {}", message).into());
            }
        }
        Some(other) => {
            return Err(format!("Expected JoinAck, got {:?}", other).into());
        }
        None => {
            return Err("Server disconnected".into());
        }
    }

    // Spawn a task to receive and display messages
    let recv_username = username.to_string();
    let recv_task = tokio::spawn(async move {
        loop {
            match client_receiver.recv().await {
                Some(ChatProtocol::Broadcast { from, text }) => {
                    println!("💬 [{}] received from [{}]: {}", recv_username, from, text);
                }
                Some(ChatProtocol::UserJoined { username: user }) => {
                    println!("👤 [{}] {} joined the chat", recv_username, user);
                }
                Some(ChatProtocol::UserLeft { username: user }) => {
                    println!("👋 [{}] {} left the chat", recv_username, user);
                }
                Some(ChatProtocol::SystemMessage { text }) => {
                    println!("🔔 [{}] System: {}", recv_username, text);
                }
                Some(other) => {
                    println!("⚠️  [{}] Unexpected message: {:?}", recv_username, other);
                }
                None => {
                    eprintln!("❌ [{}] Server disconnected", recv_username);
                    break;
                }
            }
        }
    });

    // Send some test messages
    sleep(Duration::from_millis(500)).await;

    let messages = vec![
        format!("Hello from {}!", username),
        format!("{} says hi to everyone!", username),
        format!("This is {}'s last message", username),
    ];

    for (i, text) in messages.iter().enumerate() {
        client_sender
            .send(ChatProtocol::Message {
                from: username.to_string(),
                text: text.clone(),
            })
            .await?;

        println!("📤 [{}] Sent: {}", username, text);

        // Wait a bit between messages
        if i < messages.len() - 1 {
            sleep(Duration::from_millis(800)).await;
        }
    }

    // Wait a bit to receive responses
    sleep(Duration::from_secs(2)).await;

    // Leave the chat
    println!("👋 [{}] Leaving chat...", username);
    client_sender.send(ChatProtocol::Leave).await?;

    // Give the receive task a moment to finish
    sleep(Duration::from_millis(100)).await;
    recv_task.abort();

    println!("✅ [{}] Disconnected", username);

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("🎭 BDRPC Chat Server Demo (In-Memory)");
    println!("═══════════════════════════════════════════════════════════");
    println!("⚠️  Note: This uses IN-MEMORY channels only");
    println!("   For network chat, see the 'network_chat' example\n");

    // Create shared state for tracking clients
    let clients: ServerState = Arc::new(RwLock::new(HashMap::new()));

    println!("🚀 Starting in-memory chat server...\n");

    // Create multiple clients
    let client_names = vec!["Alice", "Bob", "Charlie"];
    let mut client_handles = Vec::new();

    for name in &client_names {
        // Create bidirectional in-memory channels for this client
        // Server sends to client, client sends to server
        let channel_id = ChannelId::new();
        let (server_to_client_sender, server_to_client_receiver) =
            Channel::<ChatProtocol>::new_in_memory(channel_id, 100);
        let (client_to_server_sender, client_to_server_receiver) =
            Channel::<ChatProtocol>::new_in_memory(channel_id, 100);

        println!(
            "📡 Created in-memory channels for client '{}' (ID: {})",
            name, channel_id
        );

        // Spawn server handler for this client
        let clients_clone = clients.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_client(
                channel_id,
                server_to_client_sender,
                client_to_server_receiver,
                clients_clone,
            )
            .await
            {
                eprintln!("❌ Error handling client {}: {}", channel_id, e);
            }
        });

        // Spawn client task
        let name = name.to_string();
        let handle = tokio::spawn(async move {
            // Stagger client starts
            sleep(Duration::from_millis(300)).await;

            if let Err(e) =
                run_client(&name, client_to_server_sender, server_to_client_receiver).await
            {
                eprintln!("❌ Client {} error: {}", name, e);
            }
        });
        client_handles.push(handle);
    }

    println!("\n✅ All clients connected\n");
    println!("═══════════════════════════════════════════════════════════");
    println!("Chat session in progress...");
    println!("═══════════════════════════════════════════════════════════\n");

    // Wait for all clients to finish
    for handle in client_handles {
        let _ = handle.await;
    }

    // Give server time to process final messages
    sleep(Duration::from_secs(1)).await;

    println!("\n═══════════════════════════════════════════════════════════");
    println!("✅ Chat demo completed!");
    println!("═══════════════════════════════════════════════════════════");

    println!("\n📚 What this example demonstrated (Manual Protocol):");
    println!("   • ChatProtocol enum with multiple message variants");
    println!("   • Manual Protocol trait implementation");
    println!("   • Match statements for message routing");
    println!("   • Multiple concurrent clients (in-memory)");
    println!("   • Broadcasting messages to all clients");
    println!("   • Join/leave notifications");
    println!("   • Shared state management with Arc<RwLock>");
    println!("   • Manual client connection handling");

    println!("\n🔄 Alternative: Service Macro Approach");
    println!("   See 'chat_server_service.rs' for the same functionality using:");
    println!("   • #[bdrpc::service] macro for code generation");
    println!("   • Chat service trait with join/send_message/leave methods");
    println!("   • Generated dispatcher for automatic routing");
    println!("   • Type-safe client API");
    println!("   • Less boilerplate code");

    println!("\n📊 Manual vs Service Macro for Chat:");
    println!("   Manual Protocol (this example):");
    println!("     ✓ Full control over message broadcasting");
    println!("     ✓ Custom notification patterns");
    println!("     ✓ Flexible client state management");
    println!("     ✗ More code for message routing");
    println!("   Service Macro (chat_server_service.rs):");
    println!("     ✓ Cleaner service interface");
    println!("     ✓ Type-safe method calls");
    println!("     ✓ Automatic request routing");
    println!("     ✗ Less flexibility for custom patterns");

    println!("\n⚠️  Important distinction:");
    println!("   • This example: In-memory channels (same process only)");
    println!("   • Network chat: Use 'network_chat' example with Endpoint API");

    println!("\n📖 Next steps:");
    println!("   • Compare with 'chat_server_service.rs' (service macro version)");
    println!("   • Try 'network_chat' for real network communication");
    println!("   • See 'calculator' for bi-directional RPC over TCP");
    println!("   • Check 'advanced_channels' for channel management");

    Ok(())
}

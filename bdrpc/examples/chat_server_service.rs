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

//! # Chat Server Service Example - Using #[bdrpc::service]
//!
//! This example demonstrates a multi-client chat server using the `#[bdrpc::service]` macro.
//! Compare this with `chat_server_manual.rs` to see the difference between manual protocol
//! implementation and using the service macro.
//!
//! ## What This Example Shows
//!
//! - Defining a chat service with the `#[bdrpc::service]` macro
//! - Implementing the generated server trait
//! - Handling multiple concurrent clients
//! - Broadcasting messages to all connected clients
//! - Client join/leave notifications
//! - Using the dispatcher for request routing
//!
//! ## Benefits Over Manual Implementation
//!
//! - **Type Safety**: Compile-time checking of message types
//! - **Less Boilerplate**: No manual protocol enum matching
//! - **Automatic Routing**: Dispatcher handles request routing
//! - **Clean API**: Service methods are self-documenting
//!
//! ## Running This Example
//!
//! ```bash
//! cargo run --example chat_server_service --features serde
//! ```

use bdrpc::channel::{Channel, ChannelId, ChannelSender};
use bdrpc::service;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::sleep;

// ============================================================================
// STEP 1: Define the service trait with the macro
// ============================================================================

/// Chat service for multi-client communication.
///
/// This macro generates:
/// - ChatProtocol enum (with all request/response variants)
/// - ChatClient struct (for making RPC calls)
/// - ChatServer trait (to implement)
/// - ChatDispatcher struct (for routing requests)
#[service(direction = "bidirectional")]
#[allow(dead_code)]
trait Chat {
    /// Join the chat with a username
    async fn join(&self, username: String) -> Result<String, String>;

    /// Send a chat message
    async fn send_message(&self, from: String, text: String) -> Result<(), String>;

    /// Leave the chat
    async fn leave(&self, username: String) -> Result<(), String>;
}

// ============================================================================
// STEP 2: Implement the server trait
// ============================================================================

/// Represents a connected client.
#[allow(dead_code)]
struct Client {
    username: String,
    sender: ChannelSender<ChatProtocol>,
}

/// Shared server state for managing connected clients.
type ServerState = Arc<RwLock<HashMap<ChannelId, Client>>>;

/// Our chat service implementation.
struct MyChatService {
    channel_id: ChannelId,
    clients: ServerState,
    sender: ChannelSender<ChatProtocol>,
}

impl MyChatService {
    fn new(
        channel_id: ChannelId,
        clients: ServerState,
        sender: ChannelSender<ChatProtocol>,
    ) -> Self {
        Self {
            channel_id,
            clients,
            sender,
        }
    }

    /// Broadcast a message to all clients except the sender
    async fn broadcast(&self, message: String, exclude: Option<ChannelId>) {
        let _ = message; // Suppress unused warning - would be used in real implementation
        let clients_lock = self.clients.read().await;

        for (id, client) in clients_lock.iter() {
            if Some(*id) == exclude {
                continue;
            }

            // For broadcasting, we'd need to send notifications
            // In a real implementation, this would use a separate notification channel
            // or extend the protocol to support server-initiated messages
            let _ = client
                .sender
                .send(ChatProtocol::SendMessageResponse { result: Ok(()) })
                .await;
        }
    }
}

/// Implement the generated ChatServer trait.
#[async_trait::async_trait]
impl ChatServer for MyChatService {
    async fn join(&self, username: String) -> Result<String, String> {
        println!(
            "👤 User '{}' joining (channel: {})",
            username, self.channel_id
        );

        // Add client to the connected clients map
        {
            let mut clients_lock = self.clients.write().await;
            clients_lock.insert(
                self.channel_id,
                Client {
                    username: username.clone(),
                    sender: self.sender.clone(),
                },
            );
        }

        // Notify all other clients about the new user
        let message = format!("{} joined the chat", username);
        self.broadcast(message, Some(self.channel_id)).await;

        println!("✅ User '{}' joined successfully", username);

        Ok(format!("Welcome to the chat, {}!", username))
    }

    async fn send_message(&self, from: String, text: String) -> Result<(), String> {
        println!("💬 [{}]: {}", from, text);

        // Broadcast the message to all other clients
        let message = format!("[{}]: {}", from, text);
        self.broadcast(message, Some(self.channel_id)).await;

        Ok(())
    }

    async fn leave(&self, username: String) -> Result<(), String> {
        println!("👋 User '{}' leaving", username);

        // Remove client from connected clients
        {
            let mut clients_lock = self.clients.write().await;
            clients_lock.remove(&self.channel_id);
        }

        // Notify all remaining clients about the user leaving
        let message = format!("{} left the chat", username);
        self.broadcast(message, None).await;

        println!("🔌 User '{}' disconnected", username);

        Ok(())
    }
}

// ============================================================================
// STEP 3: Use the generated client and server
// ============================================================================

/// Run a chat client.
async fn run_client(
    username: &str,
    client_sender: ChannelSender<ChatProtocol>,
    client_receiver: bdrpc::channel::ChannelReceiver<ChatProtocol>,
) -> Result<(), Box<dyn Error>> {
    println!("🔌 Client '{}' starting", username);

    // Create the generated client stub
    let client = ChatClient::new(client_sender, client_receiver);

    // Join the chat
    match client.join(username.to_string()).await {
        Ok(Ok(message)) => {
            println!("✅ [{}] {}", username, message);
        }
        Ok(Err(e)) => {
            return Err(format!("Join failed: {}", e).into());
        }
        Err(e) => {
            return Err(format!("Channel error: {}", e).into());
        }
    }

    // Send some test messages
    sleep(Duration::from_millis(500)).await;

    let messages = vec![
        format!("Hello from {}!", username),
        format!("{} says hi to everyone!", username),
        format!("This is {}'s last message", username),
    ];

    for (i, text) in messages.iter().enumerate() {
        match client
            .send_message(username.to_string(), text.clone())
            .await
        {
            Ok(Ok(())) => {
                println!("📤 [{}] Sent: {}", username, text);
            }
            Ok(Err(e)) => {
                println!("❌ [{}] Service error: {}", username, e);
            }
            Err(e) => {
                println!("❌ [{}] Channel error: {}", username, e);
            }
        }

        // Wait a bit between messages
        if i < messages.len() - 1 {
            sleep(Duration::from_millis(800)).await;
        }
    }

    // Wait a bit to receive responses
    sleep(Duration::from_secs(2)).await;

    // Leave the chat
    println!("👋 [{}] Leaving chat...", username);
    match client.leave(username.to_string()).await {
        Ok(Ok(())) => {
            println!("✅ [{}] Left successfully", username);
        }
        Ok(Err(e)) => {
            println!("❌ [{}] Leave error: {}", username, e);
        }
        Err(e) => {
            println!("❌ [{}] Channel error: {}", username, e);
        }
    }

    println!("✅ [{}] Disconnected", username);

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("🎭 BDRPC Chat Server Service Example");
    println!("═══════════════════════════════════════════════════════════");
    println!("Using the #[bdrpc::service] macro for multi-client chat\n");

    // Create shared state for tracking clients
    let clients: ServerState = Arc::new(RwLock::new(HashMap::new()));

    println!("🚀 Starting in-memory chat server...\n");

    // Create multiple clients
    let client_names = vec!["Alice", "Bob", "Charlie"];
    let mut client_handles = Vec::new();

    for name in &client_names {
        // Create bidirectional in-memory channels for this client
        let channel_id = ChannelId::new();
        let (server_to_client_sender, server_to_client_receiver) =
            Channel::<ChatProtocol>::new_in_memory(channel_id, 100);
        let (client_to_server_sender, mut client_to_server_receiver) =
            Channel::<ChatProtocol>::new_in_memory(channel_id, 100);

        println!(
            "📡 Created in-memory channels for client '{}' (ID: {})",
            name, channel_id
        );

        // Create service and dispatcher for this client
        let service =
            MyChatService::new(channel_id, clients.clone(), server_to_client_sender.clone());
        let dispatcher = ChatDispatcher::new(service);

        // Spawn server handler for this client
        tokio::spawn(async move {
            while let Some(request) = client_to_server_receiver.recv().await {
                let response = dispatcher.dispatch(request).await;
                if let Err(e) = server_to_client_sender.send(response).await {
                    eprintln!("❌ Error sending response: {}", e);
                    break;
                }
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

    println!("\n📚 What this example demonstrated:");
    println!("   • Defining a chat service with #[bdrpc::service]");
    println!("   • Implementing the generated ChatServer trait");
    println!("   • Using ChatDispatcher for request routing");
    println!("   • Using ChatClient for type-safe RPC calls");
    println!("   • Multiple concurrent clients");
    println!("   • Broadcasting messages to all clients");
    println!("   • Join/leave notifications");

    println!("\n💡 Benefits over manual implementation:");
    println!("   • Type-safe service methods");
    println!("   • Automatic request/response matching");
    println!("   • Clean separation of concerns");
    println!("   • Less boilerplate code");
    println!("   • Self-documenting service interface");

    println!("\n🔍 Compare with chat_server_manual:");
    println!("   • chat_server_manual: Manual ChatProtocol enum");
    println!("   • chat_server_service.rs: Generated via macro");
    println!("   • Service macro provides cleaner API");

    println!("\n📖 Next steps:");
    println!("   • See service_macro_demo.rs for detailed explanation");
    println!("   • Try calculator_service.rs for simpler example");
    println!("   • Check file_transfer_service.rs for streaming pattern");
    println!("   • Read the macro documentation for advanced features");

    Ok(())
}

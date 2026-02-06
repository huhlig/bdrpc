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

//! # Advanced Channels Example
//!
//! This example demonstrates advanced channel management features:
//! - Creating in-memory channels dynamically using ChannelManager
//! - Managing multiple channels with different protocols
//! - Unidirectional channels (one-way communication pattern)
//! - Bidirectional channels (two-way communication pattern)
//! - Properly destroying channels when done
//! - Handling channel lifecycle and cleanup
//!
//! ⚠️ **Important**: This example uses in-memory channels which are NOT connected
//! to any network transport. They only work within the same process. For network
//! communication, see the `calculator` or `network_chat` examples which use the
//! Endpoint API.
//!
//! ## What This Example Shows
//!
//! - Dynamic in-memory channel creation and destruction
//! - Channel manager for organizing multiple channels
//! - Different protocol types on different channels
//! - Proper resource cleanup and channel lifecycle
//! - Error handling for channel operations
//!
//! ## Running This Example
//!
//! ```bash
//! cargo run --example advanced_channels
//! ```

use bdrpc::channel::{ChannelId, ChannelManager, Protocol};
use std::error::Error;

/// A notification protocol for unidirectional messages (server -> client only).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
enum NotificationProtocol {
    /// Server sends a notification to client
    Alert { level: String, message: String },
    /// Server sends a status update
    StatusUpdate { status: String },
}

impl Protocol for NotificationProtocol {
    fn method_name(&self) -> &'static str {
        match self {
            Self::Alert { .. } => "alert",
            Self::StatusUpdate { .. } => "status_update",
        }
    }

    fn is_request(&self) -> bool {
        false // Notifications are not requests
    }
}

/// A command protocol for unidirectional messages (client -> server only).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
enum CommandProtocol {
    /// Client sends a command to server
    Execute { command: String },
    /// Client requests server shutdown
    Shutdown,
}

impl Protocol for CommandProtocol {
    fn method_name(&self) -> &'static str {
        match self {
            Self::Execute { .. } => "execute",
            Self::Shutdown => "shutdown",
        }
    }

    fn is_request(&self) -> bool {
        true // Commands are requests
    }
}

/// A chat protocol for bidirectional communication.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
enum ChatProtocol {
    /// Send a message
    Message { from: String, text: String },
    /// Acknowledge receipt of a message
    Ack { message_id: u64 },
}

impl Protocol for ChatProtocol {
    fn method_name(&self) -> &'static str {
        match self {
            Self::Message { .. } => "message",
            Self::Ack { .. } => "ack",
        }
    }

    fn is_request(&self) -> bool {
        matches!(self, Self::Message { .. })
    }
}

/// A request-response protocol for bidirectional RPC.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
enum RpcProtocol {
    /// Request to add two numbers
    Add { a: i32, b: i32 },
    /// Response with the result
    Result { value: i32 },
    /// Error response
    Error { message: String },
}

impl Protocol for RpcProtocol {
    fn method_name(&self) -> &'static str {
        match self {
            Self::Add { .. } => "add",
            Self::Result { .. } => "result",
            Self::Error { .. } => "error",
        }
    }

    fn is_request(&self) -> bool {
        matches!(self, Self::Add { .. })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("🚀 BDRPC Advanced Channels Example\n");
    println!("⚠️  Note: This example uses IN-MEMORY channels only");
    println!("   For network communication, see 'calculator' or 'network_chat' examples\n");
    println!("This example demonstrates:");
    println!("  • Unidirectional channels (one-way communication pattern)");
    println!("  • Bidirectional channels (two-way communication pattern)");
    println!("  • Dynamic channel creation and destruction");
    println!("  • Channel manager for organizing multiple channels");
    println!("  • Managing multiple protocols\n");

    // Step 1: Create channel manager
    println!("🔧 Step 1: Creating channel manager");
    let manager = ChannelManager::new();
    println!("   ✅ Created channel manager");
    println!("   ℹ️  Channels created will be in-memory only\n");

    // ========================================================================
    // UNIDIRECTIONAL CHANNELS
    // ========================================================================

    println!("═══════════════════════════════════════════════════════════");
    println!("📤 UNIDIRECTIONAL CHANNELS (One-Way Communication Pattern)");
    println!("═══════════════════════════════════════════════════════════\n");

    // Step 2: Create notification channel (server -> client pattern)
    println!("🔔 Step 2: Creating notification channel (server → client pattern)");
    let notification_id = ChannelId::new();
    println!("   Channel ID: {}", notification_id);
    println!("   Direction: One sender, one receiver");

    let notification_sender = manager
        .create_channel::<NotificationProtocol>(notification_id, 50)
        .await?;
    let mut notification_receiver = manager
        .get_receiver::<NotificationProtocol>(notification_id)
        .await?;

    println!("   ✅ Notification channel created\n");

    // Use notification channel
    println!("   Testing notification channel:");

    // Spawn receiver task
    let notification_handler = tokio::spawn(async move {
        if let Some(msg) = notification_receiver.recv().await {
            match msg {
                NotificationProtocol::Alert { level, message } => {
                    println!("   Receiver: 🔔 Received alert [{}]: {}", level, message);
                }
                _ => {}
            }
        }
        if let Some(msg) = notification_receiver.recv().await {
            match msg {
                NotificationProtocol::StatusUpdate { status } => {
                    println!("   Receiver: 📊 Received status update: {}", status);
                }
                _ => {}
            }
        }
    });

    // Sender sends notifications
    println!("   Sender: Sending alert notification");
    notification_sender
        .send(NotificationProtocol::Alert {
            level: "WARNING".to_string(),
            message: "System maintenance in 5 minutes".to_string(),
        })
        .await?;

    println!("   Sender: Sending status update");
    notification_sender
        .send(NotificationProtocol::StatusUpdate {
            status: "All systems operational".to_string(),
        })
        .await?;

    notification_handler.await?;
    println!("   ✅ Notification channel test complete\n");

    // Step 3: Create command channel (client -> server pattern)
    println!("⚡ Step 3: Creating command channel (client → server pattern)");
    let command_id = ChannelId::new();
    println!("   Channel ID: {}", command_id);
    println!("   Direction: One sender, one receiver");

    let command_sender = manager
        .create_channel::<CommandProtocol>(command_id, 50)
        .await?;
    let mut command_receiver = manager.get_receiver::<CommandProtocol>(command_id).await?;

    println!("   ✅ Command channel created\n");

    // Use command channel
    println!("   Testing command channel:");

    // Spawn receiver task
    let command_handler = tokio::spawn(async move {
        if let Some(msg) = command_receiver.recv().await {
            match msg {
                CommandProtocol::Execute { command } => {
                    println!("   Receiver: ⚡ Executing command: {}", command);
                }
                _ => {}
            }
        }
        if let Some(msg) = command_receiver.recv().await {
            match msg {
                CommandProtocol::Shutdown => {
                    println!("   Receiver: 🛑 Received shutdown command");
                }
                _ => {}
            }
        }
    });

    // Sender sends commands
    println!("   Sender: Sending execute command");
    command_sender
        .send(CommandProtocol::Execute {
            command: "backup_database".to_string(),
        })
        .await?;

    println!("   Sender: Sending shutdown command");
    command_sender.send(CommandProtocol::Shutdown).await?;

    command_handler.await?;
    println!("   ✅ Command channel test complete\n");

    // ========================================================================
    // BIDIRECTIONAL CHANNELS
    // ========================================================================

    println!("═══════════════════════════════════════════════════════════");
    println!("🔄 BIDIRECTIONAL CHANNELS (Two-Way Communication Pattern)");
    println!("═══════════════════════════════════════════════════════════\n");

    // Step 4: Create chat channel (bidirectional)
    println!("💬 Step 4: Creating chat channel (bidirectional pattern)");
    let chat_id = ChannelId::new();
    println!("   Channel ID: {}", chat_id);
    println!("   Direction: Multiple senders and receivers can communicate");

    let chat_sender = manager.create_channel::<ChatProtocol>(chat_id, 50).await?;
    let mut chat_receiver = manager.get_receiver::<ChatProtocol>(chat_id).await?;

    println!("   ✅ Chat channel created\n");

    // Use chat channel bidirectionally
    println!("   Testing bidirectional chat:");

    // Clone sender for response
    let chat_sender_clone = chat_sender.clone();

    // Spawn handler task
    let chat_handler = tokio::spawn(async move {
        if let Some(msg) = chat_receiver.recv().await {
            match msg {
                ChatProtocol::Message { from, text } => {
                    println!("   Handler: 💬 Received from '{}': '{}'", from, text);
                    // Send acknowledgment
                    chat_sender_clone
                        .send(ChatProtocol::Ack { message_id: 1 })
                        .await
                        .ok();
                }
                ChatProtocol::Ack { message_id } => {
                    println!("   Handler: ✅ Received ack for message {}", message_id);
                }
            }
        }
        // Receive the ack
        if let Some(msg) = chat_receiver.recv().await {
            match msg {
                ChatProtocol::Ack { message_id } => {
                    println!("   Handler: ✅ Received ack for message {}", message_id);
                }
                _ => {}
            }
        }
    });

    // Send message
    println!("   Sender: Sending chat message");
    chat_sender
        .send(ChatProtocol::Message {
            from: "Alice".to_string(),
            text: "Hello, World!".to_string(),
        })
        .await?;

    chat_handler.await?;
    println!("   ✅ Chat channel test complete\n");

    // Step 5: Create RPC channel (bidirectional request-response)
    println!("🔧 Step 5: Creating RPC channel (bidirectional request-response)");
    let rpc_id = ChannelId::new();
    println!("   Channel ID: {}", rpc_id);
    println!("   Direction: Request-response pattern");

    let rpc_sender = manager.create_channel::<RpcProtocol>(rpc_id, 50).await?;
    let mut rpc_receiver = manager.get_receiver::<RpcProtocol>(rpc_id).await?;

    println!("   ✅ RPC channel created\n");

    // Use RPC channel
    println!("   Testing RPC request-response:");

    // Clone sender for response
    let rpc_sender_clone = rpc_sender.clone();

    // Spawn handler task
    let rpc_handler = tokio::spawn(async move {
        if let Some(msg) = rpc_receiver.recv().await {
            match msg {
                RpcProtocol::Add { a, b } => {
                    println!("   Handler: 🔧 Processing Add({}, {})", a, b);
                    let result = a + b;
                    rpc_sender_clone
                        .send(RpcProtocol::Result { value: result })
                        .await
                        .ok();
                }
                RpcProtocol::Result { value } => {
                    println!("   Handler: ✅ Received result: {}", value);
                }
                _ => {}
            }
        }
        // Receive the result
        if let Some(msg) = rpc_receiver.recv().await {
            match msg {
                RpcProtocol::Result { value } => {
                    println!("   Handler: ✅ Received result: {}", value);
                }
                _ => {}
            }
        }
    });

    // Send RPC request
    println!("   Sender: Sending RPC request: Add(5, 3)");
    rpc_sender.send(RpcProtocol::Add { a: 5, b: 3 }).await?;

    rpc_handler.await?;
    println!("   ✅ RPC channel test complete\n");

    // ========================================================================
    // CHANNEL LIFECYCLE MANAGEMENT
    // ========================================================================

    println!("═══════════════════════════════════════════════════════════");
    println!("🗑️  CHANNEL LIFECYCLE MANAGEMENT");
    println!("═══════════════════════════════════════════════════════════\n");

    // Step 6: List all active channels
    println!("📋 Step 6: Listing active channels");
    let channel_ids = manager.channel_ids().await;
    println!("   Active channels: {}", channel_ids.len());
    for id in &channel_ids {
        println!("     • Channel {}", id);
    }
    println!();

    // Step 7: Destroy specific channels
    println!("🗑️  Step 7: Destroying notification and command channels");

    // Drop senders to close channels
    drop(notification_sender);
    drop(command_sender);

    // Remove from manager
    manager.remove_channel(notification_id).await?;
    manager.remove_channel(command_id).await?;

    println!("   ✅ Unidirectional channels destroyed");
    println!("   Active channels: {}\n", manager.channel_count().await);

    // Step 8: Cleanup closed channels
    println!("🧹 Step 8: Cleaning up closed channels");
    let removed = manager.cleanup_closed_channels().await;
    println!("   Cleaned up {} closed channels", removed);
    println!("   Active channels: {}\n", manager.channel_count().await);

    // Step 9: Destroy remaining channels
    println!("🗑️  Step 9: Destroying remaining bidirectional channels");

    drop(chat_sender);
    drop(rpc_sender);

    // Only remove if they still exist (cleanup_closed_channels may have removed them)
    if manager.has_channel(chat_id).await {
        manager.remove_channel(chat_id).await?;
    }
    if manager.has_channel(rpc_id).await {
        manager.remove_channel(rpc_id).await?;
    }

    println!("   ✅ All channels destroyed");
    println!("   Active channels: {}\n", manager.channel_count().await);

    // Summary
    println!("═══════════════════════════════════════════════════════════");
    println!("🎉 Example completed successfully!\n");

    println!("📚 What we demonstrated:");
    println!("   1. Unidirectional Channels:");
    println!("      • Notification channel (one-way pattern)");
    println!("      • Command channel (one-way pattern)");
    println!("   2. Bidirectional Channels:");
    println!("      • Chat channel (both directions)");
    println!("      • RPC channel (request-response pattern)");
    println!("   3. Channel Management:");
    println!("      • Dynamic creation and destruction");
    println!("      • ChannelManager for organizing channels");
    println!("      • Proper resource cleanup");

    println!("\n💡 Key concepts:");
    println!("   • Unidirectional: One sender, one receiver pattern");
    println!("   • Bidirectional: Multiple senders can communicate");
    println!("   • Channels can be created/destroyed dynamically");
    println!("   • ChannelManager organizes multiple channels");
    println!("   • Each channel has its own protocol type");

    println!("\n⚠️  Important distinction:");
    println!("   • This example: In-memory channels (same process only)");
    println!("   • Network channels: Use Endpoint API with transports");

    println!("\n📖 Next steps:");
    println!("   • Try 'channel_basics' for simpler channel examples");
    println!("   • See 'calculator' for network RPC with Endpoint API");
    println!("   • Check 'network_chat' for multi-client network example");
    println!("   • Read the documentation for advanced features");

    Ok(())
}

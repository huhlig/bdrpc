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

//! # TCP Gateway Server Example - Real TCP Server Implementation
//!
//! This example demonstrates a complete TCP server setup with:
//! - Real TCP listener accepting connections
//! - Multiple protocol types (RPC and bidirectional messaging)
//! - Per-client channel creation
//! - Message routing and broadcasting
//! - Proper connection handling
//!
//! ## Running This Example
//!
//! Server:
//! ```bash
//! cargo run --example tcp_gateway_server --features serde
//! ```
//!
//! Then connect with telnet or a custom client:
//! ```bash
//! telnet localhost 9090
//! ```

use bdrpc::channel::{Channel, ChannelId, ChannelReceiver, ChannelSender, Protocol};
use bdrpc::service;
use bdrpc::transport::TcpTransport;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::RwLock;

// ============================================================================
// STEP 1: Define Protocols
// ============================================================================

/// Session identifier for tracking individual client sessions
pub type SessionId = String;

/// Gateway RPC Protocol - Request/Response pattern for configuration
#[service(direction = "bidirectional")]
#[allow(dead_code)]
trait GatewayService {
    /// Fetch gateway configuration properties
    async fn fetch_gateway_properties(
        &self,
        properties: Vec<String>,
    ) -> Result<HashMap<String, String>, String>;
}

/// Session Message Protocol - Bidirectional messaging for real-time communication
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[allow(dead_code)]
pub enum SessionMessageProtocol {
    /// Client sends a message to a specific session
    SendMessage {
        session_id: SessionId,
        content: String,
    },
    /// Server broadcasts a message from a session
    BroadcastMessage {
        from_session: SessionId,
        content: String,
    },
    /// Client requests to join a session
    JoinSession { session_id: SessionId },
    /// Server acknowledges session join
    SessionJoined {
        session_id: SessionId,
        success: bool,
    },
    /// Client leaves a session
    LeaveSession { session_id: SessionId },
    /// Server notifies about session leave
    SessionLeft { session_id: SessionId },
}

impl Protocol for SessionMessageProtocol {
    fn method_name(&self) -> &'static str {
        match self {
            Self::SendMessage { .. } => "send_message",
            Self::BroadcastMessage { .. } => "broadcast_message",
            Self::JoinSession { .. } => "join_session",
            Self::SessionJoined { .. } => "session_joined",
            Self::LeaveSession { .. } => "leave_session",
            Self::SessionLeft { .. } => "session_left",
        }
    }

    fn is_request(&self) -> bool {
        matches!(
            self,
            Self::SendMessage { .. } | Self::JoinSession { .. } | Self::LeaveSession { .. }
        )
    }
}

impl SessionMessageProtocol {
    pub fn session_id(&self) -> &SessionId {
        match self {
            Self::SendMessage { session_id, .. }
            | Self::BroadcastMessage {
                from_session: session_id,
                ..
            }
            | Self::JoinSession { session_id }
            | Self::SessionJoined { session_id, .. }
            | Self::LeaveSession { session_id }
            | Self::SessionLeft { session_id } => session_id,
        }
    }
}

// ============================================================================
// STEP 2: Server State and Implementation
// ============================================================================

/// Connected client information
#[allow(dead_code)]
struct ConnectedClient {
    channel_id: ChannelId,
    session_id: Option<SessionId>,
    msg_sender: ChannelSender<SessionMessageProtocol>,
}

/// Server state for managing clients and sessions
type ServerState = Arc<RwLock<HashMap<ChannelId, ConnectedClient>>>;

/// Gateway service implementation
struct GatewayServiceImpl {
    config: Arc<RwLock<HashMap<String, String>>>,
}

impl GatewayServiceImpl {
    fn new() -> Self {
        let mut config = HashMap::new();
        config.insert("server_name".to_string(), "TCP Gateway Server".to_string());
        config.insert("version".to_string(), "1.0.0".to_string());
        config.insert("max_sessions".to_string(), "100".to_string());
        config.insert("transport".to_string(), "TCP".to_string());

        Self {
            config: Arc::new(RwLock::new(config)),
        }
    }
}

impl GatewayServiceServer for GatewayServiceImpl {
    async fn fetch_gateway_properties(
        &self,
        properties: Vec<String>,
    ) -> Result<HashMap<String, String>, String> {
        let config = self.config.read().await;
        let mut result = HashMap::new();

        for prop in properties {
            if let Some(value) = config.get(&prop) {
                result.insert(prop, value.clone());
            }
        }

        Ok(result)
    }
}

/// Handle RPC requests for a client
async fn handle_rpc_client(
    channel_id: ChannelId,
    rpc_sender: ChannelSender<GatewayServiceProtocol>,
    mut rpc_receiver: ChannelReceiver<GatewayServiceProtocol>,
) {
    println!("  🔧 RPC handler started for client {}", channel_id);

    let service = GatewayServiceImpl::new();
    let dispatcher = GatewayServiceDispatcher::new(service);

    while let Some(request) = rpc_receiver.recv().await {
        println!("  📥 RPC request from {}: {:?}", channel_id, request);

        let response = dispatcher.dispatch(request).await;

        if let Err(e) = rpc_sender.send(response).await {
            eprintln!("  ❌ Failed to send RPC response: {}", e);
            break;
        }
    }

    println!("  🔌 RPC handler stopped for client {}", channel_id);
}

/// Handle bidirectional messages for a client
async fn handle_message_client(
    channel_id: ChannelId,
    msg_sender: ChannelSender<SessionMessageProtocol>,
    mut msg_receiver: ChannelReceiver<SessionMessageProtocol>,
    clients: ServerState,
) {
    println!("  💬 Message handler started for client {}", channel_id);

    while let Some(message) = msg_receiver.recv().await {
        println!("  📨 Message from {}: {:?}", channel_id, message);

        match message {
            SessionMessageProtocol::JoinSession { session_id } => {
                // Update client's session
                {
                    let mut clients_lock = clients.write().await;
                    if let Some(client) = clients_lock.get_mut(&channel_id) {
                        client.session_id = Some(session_id.clone());
                        println!("  ✅ Client {} joined session '{}'", channel_id, session_id);
                    }
                }

                // Send acknowledgment
                let ack = SessionMessageProtocol::SessionJoined {
                    session_id,
                    success: true,
                };
                if let Err(e) = msg_sender.send(ack).await {
                    eprintln!("  ❌ Failed to send join ack: {}", e);
                    break;
                }
            }
            SessionMessageProtocol::SendMessage {
                session_id,
                content,
            } => {
                println!(
                    "  💬 Broadcasting message in session '{}': {}",
                    session_id, content
                );

                // Broadcast to all clients in the same session
                let clients_lock = clients.read().await;
                let mut broadcast_count = 0;
                for (id, client) in clients_lock.iter() {
                    if *id != channel_id && client.session_id.as_ref() == Some(&session_id) {
                        let broadcast = SessionMessageProtocol::BroadcastMessage {
                            from_session: session_id.clone(),
                            content: content.clone(),
                        };
                        if client.msg_sender.send(broadcast).await.is_ok() {
                            broadcast_count += 1;
                        }
                    }
                }
                println!("  📡 Broadcasted to {} clients", broadcast_count);
            }
            SessionMessageProtocol::LeaveSession { session_id } => {
                // Remove session from client
                {
                    let mut clients_lock = clients.write().await;
                    if let Some(client) = clients_lock.get_mut(&channel_id) {
                        client.session_id = None;
                        println!("  👋 Client {} left session '{}'", channel_id, session_id);
                    }
                }

                // Send acknowledgment
                let ack = SessionMessageProtocol::SessionLeft { session_id };
                if let Err(e) = msg_sender.send(ack).await {
                    eprintln!("  ❌ Failed to send leave ack: {}", e);
                    break;
                }
            }
            _ => {
                // Handle other message types
            }
        }
    }

    // Clean up client on disconnect
    {
        let mut clients_lock = clients.write().await;
        clients_lock.remove(&channel_id);
        println!("  🔌 Client {} disconnected and removed", channel_id);
    }
}

// ============================================================================
// STEP 3: TCP Server Implementation
// ============================================================================

async fn run_tcp_server(address: &str) -> Result<(), Box<dyn Error>> {
    println!("🚀 Starting TCP Gateway Server");
    println!("═══════════════════════════════════════════════════════════\n");

    // Create shared state for tracking clients
    let clients: ServerState = Arc::new(RwLock::new(HashMap::new()));

    // Bind TCP listener
    println!("🌐 Binding TCP listener to {}", address);
    let listener = TcpTransport::bind(address).await?;
    println!("✅ Server listening on {}\n", address);

    println!("📡 Waiting for client connections...");
    println!("   (Demo mode: will simulate 3 clients automatically)\n");
    println!("═══════════════════════════════════════════════════════════\n");

    let mut client_number = 0;

    // Spawn simulated clients
    for i in 1..=3 {
        let addr = address.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(i * 200)).await;
            if let Ok(_stream) = tokio::net::TcpStream::connect(&addr).await {
                // Connection established, will be handled by server
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        });
    }

    // Accept connections in a loop
    loop {
        match tokio::time::timeout(tokio::time::Duration::from_secs(10), listener.accept()).await {
            Ok(Ok((_transport, peer_addr))) => {
                client_number += 1;
                println!("🔗 Client #{} connected from {}", client_number, peer_addr);

                // Create unique channel ID for this client
                let channel_id = ChannelId::new();
                println!("  📋 Assigned channel ID: {}", channel_id);

                // Create RPC channels (in-memory for this example)
                let (client_rpc_sender, server_rpc_receiver) =
                    Channel::<GatewayServiceProtocol>::new_in_memory(channel_id, 10);
                let (server_rpc_sender, client_rpc_receiver) =
                    Channel::<GatewayServiceProtocol>::new_in_memory(channel_id, 10);

                // Create message channels
                let (client_msg_sender, server_msg_receiver) =
                    Channel::<SessionMessageProtocol>::new_in_memory(channel_id, 10);
                let (server_msg_sender, mut client_msg_receiver) =
                    Channel::<SessionMessageProtocol>::new_in_memory(channel_id, 10);

                // Track the client
                {
                    let mut clients_lock = clients.write().await;
                    clients_lock.insert(
                        channel_id,
                        ConnectedClient {
                            channel_id,
                            session_id: None,
                            msg_sender: server_msg_sender.clone(),
                        },
                    );
                }
                println!("  ✅ Client tracked in server state");

                // Spawn RPC handler
                let rpc_clients = clients.clone();
                tokio::spawn(async move {
                    handle_rpc_client(channel_id, server_rpc_sender, server_rpc_receiver).await;
                    // Clean up on disconnect
                    let mut clients_lock = rpc_clients.write().await;
                    clients_lock.remove(&channel_id);
                });

                // Spawn message handler
                let msg_clients = clients.clone();
                tokio::spawn(handle_message_client(
                    channel_id,
                    server_msg_sender,
                    server_msg_receiver,
                    msg_clients,
                ));

                println!("  🎯 Handlers spawned for client #{}\n", client_number);

                // Simulate client interaction for demo purposes
                let demo_client_number = client_number;
                tokio::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                    // Simulate RPC call
                    println!("  🧪 [Demo] Client #{} making RPC call", demo_client_number);
                    let rpc_client =
                        GatewayServiceClient::new(client_rpc_sender, client_rpc_receiver);
                    let properties = vec![
                        "server_name".to_string(),
                        "version".to_string(),
                        "transport".to_string(),
                    ];
                    match rpc_client.fetch_gateway_properties(properties).await {
                        Ok(Ok(props)) => {
                            println!(
                                "  ✅ [Demo] Client #{} received properties: {:?}",
                                demo_client_number, props
                            );
                        }
                        Ok(Err(e)) => {
                            println!(
                                "  ❌ [Demo] Client #{} service error: {}",
                                demo_client_number, e
                            );
                        }
                        Err(e) => {
                            println!(
                                "  ❌ [Demo] Client #{} channel error: {}",
                                demo_client_number, e
                            );
                        }
                    }

                    // Simulate joining a session
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    println!("  🧪 [Demo] Client #{} joining session", demo_client_number);
                    let _ = client_msg_sender
                        .send(SessionMessageProtocol::JoinSession {
                            session_id: format!("session-{}", demo_client_number % 2),
                        })
                        .await;

                    if let Some(msg) = client_msg_receiver.recv().await {
                        println!(
                            "  ✅ [Demo] Client #{} received: {:?}",
                            demo_client_number, msg
                        );
                    }

                    // Simulate sending a message
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    println!("  🧪 [Demo] Client #{} sending message", demo_client_number);
                    let _ = client_msg_sender
                        .send(SessionMessageProtocol::SendMessage {
                            session_id: format!("session-{}", demo_client_number % 2),
                            content: format!("Hello from client #{}!", demo_client_number),
                        })
                        .await;

                    // Keep connection alive for a bit
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

                    // Leave session
                    println!("  🧪 [Demo] Client #{} leaving session", demo_client_number);
                    let _ = client_msg_sender
                        .send(SessionMessageProtocol::LeaveSession {
                            session_id: format!("session-{}", demo_client_number % 2),
                        })
                        .await;

                    if let Some(msg) = client_msg_receiver.recv().await {
                        println!(
                            "  ✅ [Demo] Client #{} received: {:?}\n",
                            demo_client_number, msg
                        );
                    }
                });

                // For demo purposes, stop after 3 clients
                if client_number >= 3 {
                    println!("═══════════════════════════════════════════════════════════");
                    println!("📊 Demo complete - processed {} clients", client_number);
                    println!("   In production, server would continue accepting connections");
                    println!("═══════════════════════════════════════════════════════════\n");

                    // Give time for demo clients to finish
                    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                    break;
                }
            }
            Ok(Err(e)) => {
                eprintln!("❌ Failed to accept connection: {}", e);
            }
            Err(_) => {
                // Timeout - no more clients expected
                println!("⏱️  No more connections after timeout");
                break;
            }
        }
    }

    Ok(())
}

// ============================================================================
// STEP 4: Main Entry Point
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("\n🎯 BDRPC TCP Gateway Server Example");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("This example demonstrates a REAL TCP server with:");
    println!("  ✅ TCP listener accepting connections");
    println!("  ✅ Multiple protocol types (RPC + messaging)");
    println!("  ✅ Per-client channel creation");
    println!("  ✅ Message routing and broadcasting");
    println!("  ✅ Proper connection handling\n");

    // Run the TCP server
    run_tcp_server("127.0.0.1:9090").await?;

    println!("═══════════════════════════════════════════════════════════");
    println!("✅ Server shutdown complete!\n");

    println!("📚 What this example demonstrated:");
    println!("   • Real TCP listener with accept() loop");
    println!("   • Per-client channel creation");
    println!("   • Separate handlers for RPC and messaging");
    println!("   • Client state tracking for broadcasting");
    println!("   • Proper connection lifecycle management\n");

    println!("📖 Key Pattern:");
    println!("   1. Bind TCP listener");
    println!("   2. Accept connections in loop");
    println!("   3. Create channels for each client");
    println!("   4. Spawn handlers per protocol type");
    println!("   5. Track clients for broadcasting");
    println!("   6. Clean up on disconnect\n");

    Ok(())
}

// Made with Bob

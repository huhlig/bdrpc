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

//! # Gateway Server Example - Complete Server Setup Pattern
//!
//! This example demonstrates the proper way to set up a BDRPC server with:
//! - Multiple protocol types (RPC and bidirectional messaging)
//! - Connection acceptance and management
//! - Per-client channel creation
//! - Message routing and broadcasting
//!
//! ## Architecture
//!
//! ```text
//! Client 1 ----\                    /---- RPC Handler
//! Client 2 -----+---> Gateway Server +---- Message Router
//! Client 3 ----/      (Endpoint)     \---- Session Manager
//! ```
//!
//! ## Running This Example
//!
//! Server:
//! ```bash
//! cargo run --example gateway_server_example --features serde
//! ```
//!
//! Client (in another terminal):
//! ```bash
//! cargo run --example gateway_client_example --features serde
//! ```

use bdrpc::channel::{Channel, ChannelId, ChannelReceiver, ChannelSender, Protocol};
use bdrpc::endpoint::{EndpointBuilder, EndpointError};
use bdrpc::serialization::PostcardSerializer;
use bdrpc::service;
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
// STEP 2: Implement Server Components
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
        config.insert("server_name".to_string(), "Gateway Server".to_string());
        config.insert("version".to_string(), "1.0.0".to_string());
        config.insert("max_sessions".to_string(), "100".to_string());

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
    println!("🔧 RPC handler started for client {}", channel_id);

    let service = GatewayServiceImpl::new();
    let dispatcher = GatewayServiceDispatcher::new(service);

    while let Some(request) = rpc_receiver.recv().await {
        println!("📥 RPC request received: {:?}", request);

        let response = dispatcher.dispatch(request).await;

        if let Err(e) = rpc_sender.send(response).await {
            eprintln!("❌ Failed to send RPC response: {}", e);
            break;
        }
    }

    println!("🔌 RPC handler stopped for client {}", channel_id);
}

/// Handle bidirectional messages for a client
async fn handle_message_client(
    channel_id: ChannelId,
    msg_sender: ChannelSender<SessionMessageProtocol>,
    mut msg_receiver: ChannelReceiver<SessionMessageProtocol>,
    clients: ServerState,
) {
    println!("💬 Message handler started for client {}", channel_id);

    while let Some(message) = msg_receiver.recv().await {
        println!("📨 Message received: {:?}", message);

        match message {
            SessionMessageProtocol::JoinSession { session_id } => {
                // Update client's session
                {
                    let mut clients_lock = clients.write().await;
                    if let Some(client) = clients_lock.get_mut(&channel_id) {
                        client.session_id = Some(session_id.clone());
                    }
                }

                // Send acknowledgment
                let ack = SessionMessageProtocol::SessionJoined {
                    session_id,
                    success: true,
                };
                if let Err(e) = msg_sender.send(ack).await {
                    eprintln!("❌ Failed to send join ack: {}", e);
                    break;
                }
            }
            SessionMessageProtocol::SendMessage {
                session_id,
                content,
            } => {
                // Broadcast to all clients in the same session
                let clients_lock = clients.read().await;
                for (id, client) in clients_lock.iter() {
                    if *id != channel_id && client.session_id.as_ref() == Some(&session_id) {
                        let broadcast = SessionMessageProtocol::BroadcastMessage {
                            from_session: session_id.clone(),
                            content: content.clone(),
                        };
                        let _ = client.msg_sender.send(broadcast).await;
                    }
                }
            }
            SessionMessageProtocol::LeaveSession { session_id } => {
                // Remove session from client
                {
                    let mut clients_lock = clients.write().await;
                    if let Some(client) = clients_lock.get_mut(&channel_id) {
                        client.session_id = None;
                    }
                }

                // Send acknowledgment
                let ack = SessionMessageProtocol::SessionLeft { session_id };
                if let Err(e) = msg_sender.send(ack).await {
                    eprintln!("❌ Failed to send leave ack: {}", e);
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
    }

    println!("🔌 Message handler stopped for client {}", channel_id);
}

// ============================================================================
// STEP 3: Server Setup (The Pattern You're Looking For)
// ============================================================================

/// Server endpoint wrapper
pub struct ServerEndpoint {
    // In a real implementation, you'd store the endpoint here
    // endpoint: Endpoint<PostcardSerializer>,
}

impl ServerEndpoint {
    /// Create and configure a new server endpoint
    ///
    /// This demonstrates the proper pattern for setting up a server with:
    /// - Multiple protocol types
    /// - Connection acceptance
    /// - Per-client channel creation
    pub async fn new(address: &str) -> Result<Self, EndpointError> {
        println!("🚀 Creating Gateway Server");
        println!("═══════════════════════════════════════════════════════════\n");

        // STEP 1: Build the endpoint with protocols registered
        println!("📋 Step 1: Building endpoint with protocols");
        let _endpoint = EndpointBuilder::server(PostcardSerializer::default())
            .with_responder("GatewayRPCProtocol", 1)
            .with_bidirectional("SessionMessageProtocol", 1)
            .with_tcp_listener(address)
            .build()
            .await?;

        println!("   ✅ Endpoint created");
        println!("   ✅ GatewayRPCProtocol registered (responder)");
        println!("   ✅ SessionMessageProtocol registered (bidirectional)");
        println!("   ✅ TCP listener configured on {}\n", address);

        // STEP 2: Create shared state for tracking clients
        println!("📊 Step 2: Initializing server state");
        let clients: ServerState = Arc::new(RwLock::new(HashMap::new()));
        println!("   ✅ Client tracking initialized\n");

        // STEP 3: Accept connections in a loop
        println!("🌐 Step 3: Connection acceptance pattern");
        println!("   In a real implementation, you would:");
        println!("   1. Accept connections in a loop");
        println!("   2. Create channels for each connection");
        println!("   3. Spawn handlers for each client\n");

        println!("   Example pattern:");
        println!("   ```rust");
        println!("   loop {{");
        println!("       // Accept connection from transport");
        println!("       let connection = endpoint.accept().await?;");
        println!("       ");
        println!("       // Create RPC channels");
        println!("       let (rpc_sender, rpc_receiver) = endpoint");
        println!("           .get_channels::<GatewayServiceProtocol>(");
        println!("               connection.id(),");
        println!("               \"GatewayRPCProtocol\"");
        println!("           )");
        println!("           .await?;");
        println!("       ");
        println!("       // Create message channels");
        println!("       let (msg_sender, msg_receiver) = endpoint");
        println!("           .get_channels::<SessionMessageProtocol>(");
        println!("               connection.id(),");
        println!("               \"SessionMessageProtocol\"");
        println!("           )");
        println!("           .await?;");
        println!("       ");
        println!("       // Track the client");
        println!("       clients.write().await.insert(");
        println!("           connection.id(),");
        println!("           ConnectedClient {{");
        println!("               channel_id: connection.id(),");
        println!("               session_id: None,");
        println!("               msg_sender: msg_sender.clone(),");
        println!("           }}");
        println!("       );");
        println!("       ");
        println!("       // Spawn RPC handler");
        println!("       tokio::spawn(handle_rpc_client(");
        println!("           connection.id(),");
        println!("           rpc_sender,");
        println!("           rpc_receiver,");
        println!("       ));");
        println!("       ");
        println!("       // Spawn message handler");
        println!("       tokio::spawn(handle_message_client(");
        println!("           connection.id(),");
        println!("           msg_sender,");
        println!("           msg_receiver,");
        println!("           clients.clone(),");
        println!("       ));");
        println!("   }}");
        println!("   ```\n");

        Ok(Self {
            // endpoint,
        })
    }

    /// Demonstrate the pattern with in-memory channels (for testing)
    pub async fn demo_with_in_memory_client() -> Result<(), Box<dyn Error>> {
        println!("🧪 Demo: Simulating client connection with in-memory channels\n");

        let clients: ServerState = Arc::new(RwLock::new(HashMap::new()));
        let channel_id = ChannelId::new();

        // Create RPC channels
        let (client_rpc_sender, server_rpc_receiver) =
            Channel::<GatewayServiceProtocol>::new_in_memory(channel_id, 10);
        let (server_rpc_sender, client_rpc_receiver) =
            Channel::<GatewayServiceProtocol>::new_in_memory(channel_id, 10);

        // Create message channels
        let (client_msg_sender, server_msg_receiver) =
            Channel::<SessionMessageProtocol>::new_in_memory(channel_id, 10);
        let (server_msg_sender, mut client_msg_receiver) =
            Channel::<SessionMessageProtocol>::new_in_memory(channel_id, 10);

        // Track client
        clients.write().await.insert(
            channel_id,
            ConnectedClient {
                channel_id,
                session_id: None,
                msg_sender: server_msg_sender.clone(),
            },
        );

        // Spawn server handlers
        tokio::spawn(handle_rpc_client(
            channel_id,
            server_rpc_sender,
            server_rpc_receiver,
        ));

        tokio::spawn(handle_message_client(
            channel_id,
            server_msg_sender,
            server_msg_receiver,
            clients.clone(),
        ));

        // Simulate client operations
        println!("📞 Client: Making RPC call");
        let rpc_client = GatewayServiceClient::new(client_rpc_sender, client_rpc_receiver);
        let properties = vec!["server_name".to_string(), "version".to_string()];
        match rpc_client.fetch_gateway_properties(properties).await {
            Ok(Ok(props)) => {
                println!("   ✅ Received properties: {:?}\n", props);
            }
            Ok(Err(e)) => {
                println!("   ❌ Service error: {}\n", e);
            }
            Err(e) => {
                println!("   ❌ Channel error: {}\n", e);
            }
        }

        println!("💬 Client: Joining session");
        client_msg_sender
            .send(SessionMessageProtocol::JoinSession {
                session_id: "test-session".to_string(),
            })
            .await?;

        // Wait for acknowledgment
        if let Some(msg) = client_msg_receiver.recv().await {
            println!("   ✅ Received: {:?}\n", msg);
        }

        println!("📤 Client: Sending message");
        client_msg_sender
            .send(SessionMessageProtocol::SendMessage {
                session_id: "test-session".to_string(),
                content: "Hello from client!".to_string(),
            })
            .await?;

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        println!("👋 Client: Leaving session");
        client_msg_sender
            .send(SessionMessageProtocol::LeaveSession {
                session_id: "test-session".to_string(),
            })
            .await?;

        // Wait for acknowledgment
        if let Some(msg) = client_msg_receiver.recv().await {
            println!("   ✅ Received: {:?}\n", msg);
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        Ok(())
    }
}

// ============================================================================
// STEP 4: Main Entry Point
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("\n🎯 BDRPC Gateway Server Example");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("This example demonstrates the proper server setup pattern:\n");
    println!("  1. Build endpoint with protocols registered");
    println!("  2. Configure transport listeners");
    println!("  3. Accept connections in a loop");
    println!("  4. Create channels per connection");
    println!("  5. Spawn handlers for each client\n");

    // Show the pattern
    let _server = ServerEndpoint::new("127.0.0.1:9090").await?;

    println!("═══════════════════════════════════════════════════════════");
    println!("🧪 Running in-memory demo\n");

    // Run demo with in-memory channels
    ServerEndpoint::demo_with_in_memory_client().await?;

    println!("═══════════════════════════════════════════════════════════");
    println!("✅ Example completed!\n");

    println!("📚 Key Takeaways:");
    println!("   • Use EndpointBuilder to configure server");
    println!("   • Register protocols with appropriate directions");
    println!("   • Accept connections in a loop");
    println!("   • Use get_channels() to create typed channels");
    println!("   • Spawn separate handlers for each protocol type");
    println!("   • Track clients in shared state for broadcasting\n");

    println!("📖 Next Steps:");
    println!("   • Implement connection acceptance loop");
    println!("   • Add proper error handling");
    println!("   • Implement graceful shutdown");
    println!("   • Add connection limits and rate limiting");

    Ok(())
}

// Made with Bob

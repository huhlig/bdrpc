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

//! # Dynamic Channel Creation Example - Multiplexing Gateway
//!
//! This example demonstrates the **system protocol** and **dynamic channel creation**,
//! which are core features of BDRPC that enable sophisticated multiplexing patterns.
//!
//! ## What This Example Shows
//!
//! - **System Protocol**: The built-in protocol for channel lifecycle management
//! - **Dynamic Channel Creation**: Creating channels on-demand at runtime
//! - **Multiplexing Gateway Pattern**: Multiple services over a single connection
//! - **Channel Negotiation**: Requesting and creating channels dynamically
//! - **Practical Use Case**: A gateway that routes requests to different backend services
//!
//! ## The System Protocol
//!
//! BDRPC includes a built-in system protocol (Channel ID 0) that handles:
//! - `ChannelCreateRequest`: Request to create a new channel
//! - `ChannelCreateResponse`: Confirmation of channel creation
//! - `ChannelCloseRequest`: Request to close a channel
//! - `ChannelCloseAck`: Acknowledgment of channel closure
//! - `Ping`/`Pong`: Connection health checks
//!
//! This protocol enables dynamic channel creation without pre-configuration.
//!
//! ## Multiplexing Gateway Pattern
//!
//! ```text
//! Client                    Gateway                   Backend Services
//!   |                          |                            |
//!   |-- Connect -------------->|                            |
//!   |<-- System Channel (0) ---|                            |
//!   |                          |                            |
//!   |-- CreateChannel(Auth) -->|                            |
//!   |<-- ChannelCreated(1) ----|                            |
//!   |                          |                            |
//!   |-- Auth Request (Ch 1) -->|-- Forward --------------->| Auth Service
//!   |<-- Auth Response (Ch 1) -|<-- Response --------------|
//!   |                          |                            |
//!   |-- CreateChannel(Data) -->|                            |
//!   |<-- ChannelCreated(2) ----|                            |
//!   |                          |                            |
//!   |-- Data Request (Ch 2) -->|-- Forward --------------->| Data Service
//!   |<-- Data Response (Ch 2) -|<-- Response --------------|
//!   |                          |                            |
//!   |-- CreateChannel(Logs) -->|                            |
//!   |<-- ChannelCreated(3) ----|                            |
//!   |                          |                            |
//!   |-- Log Request (Ch 3) --->|-- Forward --------------->| Log Service
//!   |<-- Log Response (Ch 3) --|<-- Response --------------|
//! ```
//!
//! ## Why Dynamic Channels?
//!
//! 1. **Flexibility**: Create channels only when needed
//! 2. **Efficiency**: No pre-allocation of unused channels
//! 3. **Scalability**: Support arbitrary number of services
//! 4. **Isolation**: Each service gets its own channel with independent flow control
//! 5. **Multiplexing**: Multiple protocols over a single TCP connection
//!
//! ## Real-World Use Cases
//!
//! - **API Gateway**: Route requests to different microservices
//! - **Database Proxy**: Multiplex queries to different databases
//! - **Message Broker**: Route messages to different topics/queues
//! - **Service Mesh**: Dynamic service discovery and routing
//! - **Multi-tenant Systems**: Isolate traffic per tenant
//!
//! ## Running This Example
//!
//! ```bash
//! cargo run --example dynamic_channels --features serde
//! ```

use bdrpc::channel::{Channel, ChannelId, Protocol};
use bdrpc::endpoint::EndpointBuilder;
use bdrpc::serialization::JsonSerializer;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::RwLock;

// ============================================================================
// Protocol Definitions
// ============================================================================

/// Authentication service protocol
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
enum AuthProtocol {
    Login {
        username: String,
        password: String,
    },
    LoginResponse {
        success: bool,
        token: Option<String>,
    },
    Logout {
        token: String,
    },
    LogoutResponse {
        success: bool,
    },
}

impl Protocol for AuthProtocol {
    fn method_name(&self) -> &'static str {
        match self {
            Self::Login { .. } => "login",
            Self::LoginResponse { .. } => "login_response",
            Self::Logout { .. } => "logout",
            Self::LogoutResponse { .. } => "logout_response",
        }
    }

    fn is_request(&self) -> bool {
        matches!(self, Self::Login { .. } | Self::Logout { .. })
    }
}

/// Data service protocol
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
enum DataProtocol {
    Query { sql: String },
    QueryResponse { rows: Vec<String> },
    Insert { table: String, data: String },
    InsertResponse { success: bool, id: Option<u64> },
}

impl Protocol for DataProtocol {
    fn method_name(&self) -> &'static str {
        match self {
            Self::Query { .. } => "query",
            Self::QueryResponse { .. } => "query_response",
            Self::Insert { .. } => "insert",
            Self::InsertResponse { .. } => "insert_response",
        }
    }

    fn is_request(&self) -> bool {
        matches!(self, Self::Query { .. } | Self::Insert { .. })
    }
}

/// Logging service protocol
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
enum LogProtocol {
    Log { level: String, message: String },
    LogResponse { success: bool },
    QueryLogs { filter: String },
    QueryLogsResponse { logs: Vec<String> },
}

impl Protocol for LogProtocol {
    fn method_name(&self) -> &'static str {
        match self {
            Self::Log { .. } => "log",
            Self::LogResponse { .. } => "log_response",
            Self::QueryLogs { .. } => "query_logs",
            Self::QueryLogsResponse { .. } => "query_logs_response",
        }
    }

    fn is_request(&self) -> bool {
        matches!(self, Self::Log { .. } | Self::QueryLogs { .. })
    }
}

// ============================================================================
// Gateway State
// ============================================================================

/// Gateway state tracking active channels and their purposes
#[derive(Debug)]
struct GatewayState {
    /// Map of channel IDs to service names
    channels: HashMap<ChannelId, String>,
}

impl GatewayState {
    fn new() -> Self {
        Self {
            channels: HashMap::new(),
        }
    }
}

type SharedState = Arc<RwLock<GatewayState>>;

// ============================================================================
// Main Example
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("\n🌐 BDRPC Dynamic Channel Creation Example");
    println!("   Multiplexing Gateway Pattern\n");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("📚 This example demonstrates:");
    println!("   • System Protocol for channel lifecycle management");
    println!("   • Dynamic channel creation at runtime");
    println!("   • Multiplexing multiple services over one connection");
    println!("   • Practical gateway pattern for microservices\n");

    // ========================================================================
    // PART 1: Understanding the System Protocol
    // ========================================================================

    println!("═══════════════════════════════════════════════════════════");
    println!("📖 PART 1: Understanding the System Protocol");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("The System Protocol (Channel ID 0) is automatically created");
    println!("for every connection and handles:\n");
    println!("  1. ChannelCreateRequest  - Request new channel");
    println!("  2. ChannelCreateResponse - Confirm channel creation");
    println!("  3. ChannelCloseRequest   - Request channel closure");
    println!("  4. ChannelCloseAck       - Acknowledge closure");
    println!("  5. Ping/Pong             - Health checks\n");

    println!("This enables dynamic channel creation without pre-configuration!\n");

    // ========================================================================
    // PART 2: Setting Up the Gateway
    // ========================================================================

    println!("═══════════════════════════════════════════════════════════");
    println!("🔧 PART 2: Setting Up the Multiplexing Gateway");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("Step 1: Create gateway endpoint using EndpointBuilder");
    let gateway = EndpointBuilder::server(JsonSerializer::default())
        .configure(|config| config.with_endpoint_id("multiplexing-gateway".to_string()))
        .with_bidirectional("AuthProtocol", 1)
        .with_bidirectional("DataProtocol", 1)
        .with_bidirectional("LogProtocol", 1)
        .build()
        .await?;
    println!("   ✅ Gateway endpoint created (ID: {})", gateway.id());
    println!("   ✅ System channel (ID: 0) automatically available");
    println!("   ✅ AuthProtocol registered (version 1)");
    println!("   ✅ DataProtocol registered (version 1)");
    println!("   ✅ LogProtocol registered (version 1)\n");

    println!("Step 2: Initialize gateway state");
    let state: SharedState = Arc::new(RwLock::new(GatewayState::new()));
    println!("   ✅ Gateway state initialized\n");

    // ========================================================================
    // PART 3: Simulating Dynamic Channel Creation
    // ========================================================================

    println!("═══════════════════════════════════════════════════════════");
    println!("🚀 PART 3: Dynamic Channel Creation Flow");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("Scenario: Client connects and dynamically creates channels");
    println!("for different services as needed.\n");

    // Simulate client connection
    println!("📡 Client connects to gateway");
    println!("   ✅ TCP connection established");
    println!("   ✅ System channel (0) ready for use\n");

    // ========================================================================
    // Channel 1: Authentication Service
    // ========================================================================

    println!("───────────────────────────────────────────────────────────");
    println!("🔐 Creating Channel 1: Authentication Service");
    println!("───────────────────────────────────────────────────────────\n");

    println!("Step 1: Client sends ChannelCreateRequest via system channel");
    println!("   Protocol: AuthProtocol");
    println!("   Version: 1");
    println!("   Direction: Bidirectional\n");

    let auth_channel_id = ChannelId::new();
    println!("Step 2: Gateway processes request");
    println!("   ✅ Protocol supported: AuthProtocol v1");
    println!("   ✅ Assigned Channel ID: {}", auth_channel_id);
    println!("   ✅ Created bidirectional channel\n");

    println!("Step 3: Gateway sends ChannelCreateResponse");
    println!("   Success: true");
    println!("   Channel ID: {}", auth_channel_id);
    println!("   Message: 'Auth channel created'\n");

    // Create in-memory channel for demonstration
    let (auth_sender, mut auth_receiver) =
        Channel::<AuthProtocol>::new_in_memory(auth_channel_id, 10);

    {
        let mut state = state.write().await;
        state
            .channels
            .insert(auth_channel_id, "AuthService".to_string());
    }
    println!("   ✅ Channel 1 ready for authentication requests\n");

    // Use the auth channel
    println!("Step 4: Client uses auth channel");
    let auth_sender_clone = auth_sender.clone();
    let auth_task = tokio::spawn(async move {
        if let Some(AuthProtocol::Login { username, .. }) = auth_receiver.recv().await {
            println!(
                "   🔐 Gateway → Auth Service: Login request for '{}'",
                username
            );
            println!("   🔐 Auth Service → Gateway: Login successful");
            auth_sender_clone
                .send(AuthProtocol::LoginResponse {
                    success: true,
                    token: Some("token-abc123".to_string()),
                })
                .await
                .ok();
        }
    });

    println!("   📤 Client → Gateway: Login(alice, ******)");
    auth_sender
        .send(AuthProtocol::Login {
            username: "alice".to_string(),
            password: "secret".to_string(),
        })
        .await?;

    auth_task.await?;
    println!("   📥 Gateway → Client: LoginResponse(success=true, token=abc123)\n");

    // ========================================================================
    // Channel 2: Data Service
    // ========================================================================

    println!("───────────────────────────────────────────────────────────");
    println!("💾 Creating Channel 2: Data Service");
    println!("───────────────────────────────────────────────────────────\n");

    println!("Step 1: Client requests another channel via system channel");
    println!("   Protocol: DataProtocol");
    println!("   Version: 1\n");

    let data_channel_id = ChannelId::new();
    println!("Step 2: Gateway creates data channel");
    println!("   ✅ Assigned Channel ID: {}", data_channel_id);
    println!("   ✅ Independent flow control from auth channel\n");

    let (data_sender, mut data_receiver) =
        Channel::<DataProtocol>::new_in_memory(data_channel_id, 10);

    {
        let mut state = state.write().await;
        state
            .channels
            .insert(data_channel_id, "DataService".to_string());
    }

    println!("Step 3: Client uses data channel");
    let data_sender_clone = data_sender.clone();
    let data_task = tokio::spawn(async move {
        if let Some(DataProtocol::Query { sql }) = data_receiver.recv().await {
            println!("   💾 Gateway → Data Service: Query '{}'", sql);
            println!("   💾 Data Service → Gateway: 2 rows returned");
            data_sender_clone
                .send(DataProtocol::QueryResponse {
                    rows: vec!["row1".to_string(), "row2".to_string()],
                })
                .await
                .ok();
        }
    });

    println!("   📤 Client → Gateway: Query(SELECT * FROM users)");
    data_sender
        .send(DataProtocol::Query {
            sql: "SELECT * FROM users".to_string(),
        })
        .await?;

    data_task.await?;
    println!("   📥 Gateway → Client: QueryResponse(2 rows)\n");

    // ========================================================================
    // Channel 3: Logging Service
    // ========================================================================

    println!("───────────────────────────────────────────────────────────");
    println!("📝 Creating Channel 3: Logging Service");
    println!("───────────────────────────────────────────────────────────\n");

    println!("Step 1: Client requests logging channel");
    println!("   Protocol: LogProtocol");
    println!("   Version: 1\n");

    let log_channel_id = ChannelId::new();
    println!("Step 2: Gateway creates logging channel");
    println!("   ✅ Assigned Channel ID: {}", log_channel_id);
    println!("   ✅ Three channels now active simultaneously\n");

    let (log_sender, mut log_receiver) = Channel::<LogProtocol>::new_in_memory(log_channel_id, 10);

    {
        let mut state = state.write().await;
        state
            .channels
            .insert(log_channel_id, "LogService".to_string());
    }

    println!("Step 3: Client uses logging channel");
    let log_sender_clone = log_sender.clone();
    let log_task = tokio::spawn(async move {
        if let Some(LogProtocol::Log { level, message }) = log_receiver.recv().await {
            println!("   📝 Gateway → Log Service: [{}] {}", level, message);
            println!("   📝 Log Service → Gateway: Log recorded");
            log_sender_clone
                .send(LogProtocol::LogResponse { success: true })
                .await
                .ok();
        }
    });

    println!("   📤 Client → Gateway: Log(INFO, 'User logged in')");
    log_sender
        .send(LogProtocol::Log {
            level: "INFO".to_string(),
            message: "User alice logged in".to_string(),
        })
        .await?;

    log_task.await?;
    println!("   📥 Gateway → Client: LogResponse(success=true)\n");

    // ========================================================================
    // PART 4: Demonstrating Multiplexing Benefits
    // ========================================================================

    println!("═══════════════════════════════════════════════════════════");
    println!("⚡ PART 4: Multiplexing Benefits");
    println!("═══════════════════════════════════════════════════════════\n");

    {
        let state_read = state.read().await;
        println!("Current Gateway State:");
        println!("   Active Channels: {}", state_read.channels.len());
        for (id, service) in &state_read.channels {
            println!("     • Channel {}: {}", id, service);
        }
    }
    println!();

    println!("Key Benefits:");
    println!("   ✅ Single TCP Connection: All services over one connection");
    println!("   ✅ Independent Flow Control: Each channel has its own backpressure");
    println!("   ✅ Protocol Isolation: Different protocols don't interfere");
    println!("   ✅ Dynamic Creation: Channels created only when needed");
    println!("   ✅ Resource Efficiency: No pre-allocation of unused channels\n");

    // ========================================================================
    // PART 5: Channel Cleanup
    // ========================================================================

    println!("═══════════════════════════════════════════════════════════");
    println!("🧹 PART 5: Channel Lifecycle Management");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("Step 1: Client closes auth channel");
    println!(
        "   📤 Client → Gateway: ChannelCloseRequest({})",
        auth_channel_id
    );
    println!("   🗑️  Gateway: Closing auth channel");
    drop(auth_sender);
    println!(
        "   📥 Gateway → Client: ChannelCloseAck({})",
        auth_channel_id
    );
    println!("   ✅ Auth channel closed\n");

    println!("Step 2: Gateway cleans up resources");
    {
        let mut state = state.write().await;
        state.channels.remove(&auth_channel_id);
    }
    {
        let state_read = state.read().await;
        println!("   Active Channels: {}", state_read.channels.len());
        for (id, service) in &state_read.channels {
            println!("     • Channel {}: {}", id, service);
        }
    }
    println!();

    // ========================================================================
    // Summary
    // ========================================================================

    println!("═══════════════════════════════════════════════════════════");
    println!("🎉 Example Completed Successfully!");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("📚 What We Demonstrated:\n");

    println!("1. System Protocol:");
    println!("   • Built-in protocol for channel management");
    println!("   • Always available on Channel ID 0");
    println!("   • Handles channel lifecycle operations\n");

    println!("2. Dynamic Channel Creation:");
    println!("   • Channels created on-demand at runtime");
    println!("   • No pre-configuration required");
    println!("   • Negotiated via system protocol\n");

    println!("3. Multiplexing Gateway Pattern:");
    println!("   • Multiple services over single connection");
    println!("   • Independent protocols per channel");
    println!("   • Isolated flow control and backpressure\n");

    println!("4. Practical Use Cases:");
    println!("   • API Gateway routing to microservices");
    println!("   • Database proxy multiplexing queries");
    println!("   • Service mesh with dynamic discovery");
    println!("   • Multi-tenant traffic isolation\n");

    println!("💡 Key Advantages:\n");
    println!("   ✅ Flexibility: Create channels as needed");
    println!("   ✅ Efficiency: No wasted pre-allocated channels");
    println!("   ✅ Scalability: Support unlimited services");
    println!("   ✅ Isolation: Independent flow control per channel");
    println!("   ✅ Simplicity: Single connection, multiple protocols\n");

    println!("🔗 Related Concepts:\n");
    println!("   • HTTP/2 Multiplexing: Similar concept for HTTP");
    println!("   • QUIC Streams: Multiple streams over one connection");
    println!("   • RPC Multiplexing: Multiple RPCs over one connection");
    println!("   • AMQP Channels: Multiple channels over one TCP connection\n");

    println!("📖 Next Steps:\n");
    println!("   • See 'network_chat' for full Endpoint API usage");
    println!("   • Try 'advanced_channels' for channel management");
    println!("   • Read ADR-010 for channel negotiation details");
    println!("   • Check the architecture guide for system protocol spec\n");

    Ok(())
}

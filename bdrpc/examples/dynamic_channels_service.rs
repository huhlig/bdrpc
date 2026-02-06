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

//! # Dynamic Channel Creation Example - Service Macro Version
//!
//! This example demonstrates the **system protocol** and **dynamic channel creation**
//! using the `#[bdrpc::service]` macro for cleaner, type-safe service definitions.
//!
//! Compare this with `dynamic_channels_manual.rs` to see the difference between manual protocol
//! implementation and using the service macro.
//!
//! ## What This Example Shows
//!
//! - **Service Macro**: Using `#[bdrpc::service]` for multiple services
//! - **System Protocol**: Built-in protocol for channel lifecycle management
//! - **Dynamic Channel Creation**: Creating channels on-demand at runtime
//! - **Multiplexing Gateway Pattern**: Multiple services over a single connection
//! - **Type-Safe APIs**: Generated client stubs and server traits
//!
//! ## Benefits Over Manual Implementation
//!
//! - **Type Safety**: Compile-time checking of service calls
//! - **Less Boilerplate**: No manual protocol enums or matching
//! - **Automatic Routing**: Dispatchers handle request routing
//! - **Clean API**: Service methods are self-documenting
//! - **Error Handling**: Built-in Result types for service errors
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
//!   |-- Login Request (Ch 1) ->|-- Forward --------------->| Auth Service
//!   |<-- Login Response (Ch 1)-|<-- Response --------------|
//!   |                          |                            |
//!   |-- CreateChannel(Data) -->|                            |
//!   |<-- ChannelCreated(2) ----|                            |
//!   |                          |                            |
//!   |-- Query Request (Ch 2) ->|-- Forward --------------->| Data Service
//!   |<-- Query Response (Ch 2)-|<-- Response --------------|
//!   |                          |                            |
//!   |-- CreateChannel(Log) --->|                            |
//!   |<-- ChannelCreated(3) ----|                            |
//!   |                          |                            |
//!   |-- Log Request (Ch 3) --->|-- Forward --------------->| Log Service
//!   |<-- Log Response (Ch 3) --|<-- Response --------------|
//! ```
//!
//! ## Running This Example
//!
//! ```bash
//! cargo run --example dynamic_channels_service --features serde
//! ```

use bdrpc::channel::{Channel, ChannelId};
use bdrpc::endpoint::{Endpoint, EndpointConfig};
use bdrpc::serialization::JsonSerializer;
use bdrpc::service;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::RwLock;

// ============================================================================
// Service Definitions Using #[bdrpc::service] Macro
// ============================================================================

/// Authentication service for user login/logout.
///
/// This macro generates:
/// - AuthProtocol enum (with LoginRequest, LoginResponse, etc.)
/// - AuthClient struct (for making RPC calls)
/// - AuthServer trait (to implement)
/// - AuthDispatcher struct (for routing requests)
#[service(direction = "bidirectional")]
#[allow(dead_code)]
trait Auth {
    /// Authenticate a user with username and password
    async fn login(&self, username: String, password: String) -> Result<String, String>;

    /// Logout a user with their token
    async fn logout(&self, token: String) -> Result<(), String>;
}

/// Data service for database operations.
///
/// This macro generates:
/// - DataProtocol enum (with QueryRequest, QueryResponse, etc.)
/// - DataClient struct (for making RPC calls)
/// - DataServer trait (to implement)
/// - DataDispatcher struct (for routing requests)
#[service(direction = "bidirectional")]
#[allow(dead_code)]
trait Data {
    /// Execute a SQL query
    async fn query(&self, sql: String) -> Result<Vec<String>, String>;

    /// Insert data into a table
    async fn insert(&self, table: String, data: String) -> Result<u64, String>;
}

/// Logging service for application logs.
///
/// This macro generates:
/// - LogProtocol enum (with LogRequest, LogResponse, etc.)
/// - LogClient struct (for making RPC calls)
/// - LogServer trait (to implement)
/// - LogDispatcher struct (for routing requests)
#[service(direction = "bidirectional")]
#[allow(dead_code)]
trait Log {
    /// Log a message with a level
    async fn log(&self, level: String, message: String) -> Result<(), String>;

    /// Query logs with a filter
    async fn query_logs(&self, filter: String) -> Result<Vec<String>, String>;
}

// ============================================================================
// Service Implementations
// ============================================================================

/// Authentication service implementation
struct AuthService {
    name: String,
}

impl AuthService {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl AuthServer for AuthService {
    async fn login(&self, username: String, password: String) -> Result<String, String> {
        println!(
            "[{}] 🔐 Login request: username='{}', password='{}'",
            self.name,
            username,
            "*".repeat(password.len())
        );

        // Simulate authentication logic
        if password == "secret" {
            let token = format!("token-{}-{}", username, "abc123");
            println!("[{}] ✅ Login successful, token: {}", self.name, token);
            Ok(token)
        } else {
            println!("[{}] ❌ Login failed: invalid credentials", self.name);
            Err("Invalid credentials".to_string())
        }
    }

    async fn logout(&self, token: String) -> Result<(), String> {
        println!("[{}] 🔓 Logout request: token='{}'", self.name, token);
        println!("[{}] ✅ Logout successful", self.name);
        Ok(())
    }
}

/// Data service implementation
struct DataService {
    name: String,
}

impl DataService {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl DataServer for DataService {
    async fn query(&self, sql: String) -> Result<Vec<String>, String> {
        println!("[{}] 💾 Query request: {}", self.name, sql);

        // Simulate query execution
        let rows = vec![
            "row1: alice, alice@example.com".to_string(),
            "row2: bob, bob@example.com".to_string(),
        ];

        println!("[{}] ✅ Query returned {} rows", self.name, rows.len());
        Ok(rows)
    }

    async fn insert(&self, table: String, data: String) -> Result<u64, String> {
        println!(
            "[{}] 💾 Insert request: table='{}', data='{}'",
            self.name, table, data
        );

        // Simulate insert operation
        let id = 42;
        println!("[{}] ✅ Insert successful, ID: {}", self.name, id);
        Ok(id)
    }
}

/// Logging service implementation
struct LogService {
    name: String,
}

impl LogService {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl LogServer for LogService {
    async fn log(&self, level: String, message: String) -> Result<(), String> {
        println!("[{}] 📝 Log: [{}] {}", self.name, level, message);
        println!("[{}] ✅ Log recorded", self.name);
        Ok(())
    }

    async fn query_logs(&self, filter: String) -> Result<Vec<String>, String> {
        println!("[{}] 📝 Query logs: filter='{}'", self.name, filter);

        // Simulate log query
        let logs = vec![
            "[INFO] User alice logged in".to_string(),
            "[INFO] Query executed successfully".to_string(),
            "[INFO] User alice logged out".to_string(),
        ];

        println!("[{}] ✅ Found {} log entries", self.name, logs.len());
        Ok(logs)
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
    println!("\n🌐 BDRPC Dynamic Channel Creation Example (Service Macro Version)");
    println!("   Multiplexing Gateway Pattern with Type-Safe Services\n");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("📚 This example demonstrates:");
    println!("   • Using #[bdrpc::service] for multiple services");
    println!("   • System Protocol for channel lifecycle management");
    println!("   • Dynamic channel creation at runtime");
    println!("   • Multiplexing multiple services over one connection");
    println!("   • Type-safe service APIs with generated code\n");

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

    println!("Step 1: Create gateway endpoint");
    let config = EndpointConfig::default().with_endpoint_id("multiplexing-gateway".to_string());
    let mut gateway = Endpoint::new(JsonSerializer::default(), config);
    println!("   ✅ Gateway endpoint created (ID: {})", gateway.id());
    println!("   ✅ System channel (ID: 0) automatically available\n");

    println!("Step 2: Register supported protocols");
    println!("   The gateway supports multiple backend services:");

    gateway.register_bidirectional("AuthProtocol", 1).await?;
    println!("   ✅ AuthProtocol registered (version 1)");

    gateway.register_bidirectional("DataProtocol", 1).await?;
    println!("   ✅ DataProtocol registered (version 1)");

    gateway.register_bidirectional("LogProtocol", 1).await?;
    println!("   ✅ LogProtocol registered (version 1)\n");

    println!("Step 3: Initialize gateway state");
    let state: SharedState = Arc::new(RwLock::new(GatewayState::new()));
    println!("   ✅ Gateway state initialized\n");

    // ========================================================================
    // PART 3: Dynamic Channel Creation with Service Macro
    // ========================================================================

    println!("═══════════════════════════════════════════════════════════");
    println!("🚀 PART 3: Dynamic Channel Creation with Service Macro");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("Scenario: Client connects and dynamically creates channels");
    println!("for different services using type-safe generated APIs.\n");

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

    // Create bidirectional in-memory channels for auth service
    let (client_to_server_sender, client_to_server_receiver) =
        Channel::<AuthProtocol>::new_in_memory(auth_channel_id, 10);
    let (server_to_client_sender, server_to_client_receiver) =
        Channel::<AuthProtocol>::new_in_memory(auth_channel_id, 10);

    {
        let mut state = state.write().await;
        state
            .channels
            .insert(auth_channel_id, "AuthService".to_string());
    }

    println!("Step 3: Create service implementation and dispatcher");
    let auth_service = AuthService::new("AuthService");
    let auth_dispatcher = AuthDispatcher::new(auth_service);
    println!("   ✅ AuthService and AuthDispatcher created\n");

    // Spawn server handler for auth service
    let _auth_server_task = tokio::spawn(async move {
        let mut receiver = client_to_server_receiver;
        while let Some(request) = receiver.recv().await {
            let response = auth_dispatcher.dispatch(request).await;
            if server_to_client_sender.send(response).await.is_err() {
                break;
            }
        }
    });

    println!("Step 4: Create type-safe client and make RPC calls");
    let auth_client = AuthClient::new(client_to_server_sender.clone(), server_to_client_receiver);

    println!("   📤 Client → Gateway: login('alice', '******')");
    match auth_client.login("alice".to_string(), "secret".to_string()).await {
        Ok(Ok(token)) => {
            println!("   📥 Gateway → Client: LoginResponse(token='{}')", token);
        }
        Ok(Err(e)) => {
            println!("   📥 Gateway → Client: LoginError('{}')", e);
        }
        Err(e) => {
            println!("   ❌ Channel error: {}", e);
        }
    }
    println!();

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

    // Create bidirectional in-memory channels for data service
    let (data_client_to_server_sender, data_client_to_server_receiver) =
        Channel::<DataProtocol>::new_in_memory(data_channel_id, 10);
    let (data_server_to_client_sender, data_server_to_client_receiver) =
        Channel::<DataProtocol>::new_in_memory(data_channel_id, 10);

    {
        let mut state = state.write().await;
        state
            .channels
            .insert(data_channel_id, "DataService".to_string());
    }

    println!("Step 3: Create service implementation and dispatcher");
    let data_service = DataService::new("DataService");
    let data_dispatcher = DataDispatcher::new(data_service);
    println!("   ✅ DataService and DataDispatcher created\n");

    // Spawn server handler for data service
    let _data_server_task = tokio::spawn(async move {
        let mut receiver = data_client_to_server_receiver;
        while let Some(request) = receiver.recv().await {
            let response = data_dispatcher.dispatch(request).await;
            if data_server_to_client_sender.send(response).await.is_err() {
                break;
            }
        }
    });

    println!("Step 4: Create type-safe client and make RPC calls");
    let data_client = DataClient::new(data_client_to_server_sender.clone(), data_server_to_client_receiver);

    println!("   📤 Client → Gateway: query('SELECT * FROM users')");
    match data_client.query("SELECT * FROM users".to_string()).await {
        Ok(Ok(rows)) => {
            println!("   📥 Gateway → Client: QueryResponse({} rows)", rows.len());
            for row in rows {
                println!("      • {}", row);
            }
        }
        Ok(Err(e)) => {
            println!("   📥 Gateway → Client: QueryError('{}')", e);
        }
        Err(e) => {
            println!("   ❌ Channel error: {}", e);
        }
    }
    println!();

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

    // Create bidirectional in-memory channels for log service
    let (log_client_to_server_sender, log_client_to_server_receiver) =
        Channel::<LogProtocol>::new_in_memory(log_channel_id, 10);
    let (log_server_to_client_sender, log_server_to_client_receiver) =
        Channel::<LogProtocol>::new_in_memory(log_channel_id, 10);

    {
        let mut state = state.write().await;
        state
            .channels
            .insert(log_channel_id, "LogService".to_string());
    }

    println!("Step 3: Create service implementation and dispatcher");
    let log_service = LogService::new("LogService");
    let log_dispatcher = LogDispatcher::new(log_service);
    println!("   ✅ LogService and LogDispatcher created\n");

    // Spawn server handler for log service
    let _log_server_task = tokio::spawn(async move {
        let mut receiver = log_client_to_server_receiver;
        while let Some(request) = receiver.recv().await {
            let response = log_dispatcher.dispatch(request).await;
            if log_server_to_client_sender.send(response).await.is_err() {
                break;
            }
        }
    });

    println!("Step 4: Create type-safe client and make RPC calls");
    let log_client = LogClient::new(log_client_to_server_sender.clone(), log_server_to_client_receiver);

    println!("   📤 Client → Gateway: log('INFO', 'User alice logged in')");
    match log_client
        .log("INFO".to_string(), "User alice logged in".to_string())
        .await
    {
        Ok(Ok(())) => {
            println!("   📥 Gateway → Client: LogResponse(success=true)");
        }
        Ok(Err(e)) => {
            println!("   📥 Gateway → Client: LogError('{}')", e);
        }
        Err(e) => {
            println!("   ❌ Channel error: {}", e);
        }
    }
    println!();

    // ========================================================================
    // PART 4: Demonstrating Multiplexing Benefits
    // ========================================================================

    println!("═══════════════════════════════════════════════════════════");
    println!("⚡ PART 4: Multiplexing Benefits with Service Macro");
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
    println!("   ✅ Type-Safe APIs: Compile-time checking of service calls");
    println!("   ✅ Independent Flow Control: Each channel has its own backpressure");
    println!("   ✅ Protocol Isolation: Different protocols don't interfere");
    println!("   ✅ Dynamic Creation: Channels created only when needed");
    println!("   ✅ Less Boilerplate: Generated client/server/dispatcher code");
    println!("   ✅ Clean API: Service methods are self-documenting\n");

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
    drop(client_to_server_sender);
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

    // Clean up remaining channels gracefully
    drop(data_client_to_server_sender);
    drop(log_client_to_server_sender);

    // Server tasks will end when channels are closed
    // We don't need to wait for them as they'll clean up automatically
    println!("   ✅ All channels closed and cleaned up\n");

    // ========================================================================
    // Summary
    // ========================================================================

    println!("═══════════════════════════════════════════════════════════");
    println!("🎉 Example Completed Successfully!");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("📚 What We Demonstrated:\n");

    println!("1. Service Macro Benefits:");
    println!("   • Type-safe service definitions with #[bdrpc::service]");
    println!("   • Generated client stubs for RPC calls");
    println!("   • Generated server traits to implement");
    println!("   • Generated dispatchers for request routing\n");

    println!("2. System Protocol:");
    println!("   • Built-in protocol for channel management");
    println!("   • Always available on Channel ID 0");
    println!("   • Handles channel lifecycle operations\n");

    println!("3. Dynamic Channel Creation:");
    println!("   • Channels created on-demand at runtime");
    println!("   • No pre-configuration required");
    println!("   • Negotiated via system protocol\n");

    println!("4. Multiplexing Gateway Pattern:");
    println!("   • Multiple services over single connection");
    println!("   • Independent protocols per channel");
    println!("   • Isolated flow control and backpressure\n");

    println!("💡 Advantages Over Manual Implementation:\n");
    println!("   ✅ Type Safety: Compile-time checking of service calls");
    println!("   ✅ Less Boilerplate: No manual protocol enums");
    println!("   ✅ Automatic Routing: Dispatchers handle requests");
    println!("   ✅ Clean API: Service methods are self-documenting");
    println!("   ✅ Error Handling: Built-in Result types");
    println!("   ✅ Maintainability: Changes to service interface are type-checked\n");

    println!("🔍 Compare with dynamic_channels.rs:\n");
    println!("   • dynamic_channels.rs: Manual protocol enums and matching");
    println!("   • dynamic_channels_service.rs: Generated via #[bdrpc::service]");
    println!("   • Service macro provides cleaner, type-safe API");
    println!("   • Both achieve the same functionality\n");

    println!("📖 Next Steps:\n");
    println!("   • See 'calculator_service' for simpler service example");
    println!("   • Try 'chat_server_service' for multi-client pattern");
    println!("   • Check 'service_macro_demo' for detailed macro explanation");
    println!("   • Read ADR-010 for channel negotiation details");
    println!("   • Check the architecture guide for system protocol spec\n");

    Ok(())
}

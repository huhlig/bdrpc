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

//! Complete Server-Client Demo
//!
//! This example demonstrates a full client-server application using:
//! - EndpointBuilder for easy setup
//! - Service macro for RPC-style communication
//! - Message protocol for event notifications
//! - Server accept() pattern
//! - Proper error handling and cleanup

use bdrpc::channel::Protocol;
use bdrpc::endpoint::EndpointBuilder;
use bdrpc::serialization::PostcardSerializer;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// ============================================================================
// Service Protocol (RPC-style with request/response)
// ============================================================================

/// User management service using the service macro
#[bdrpc::service]
trait UserService {
    /// Create a new user
    async fn create_user(&self, username: String, email: String) -> Result<(u64, String), String>;
    
    /// Get user information
    async fn get_user(&self, user_id: u64) -> Result<(u64, String, String), String>;
    
    /// Delete a user
    async fn delete_user(&self, user_id: u64) -> Result<u64, String>;
}

// ============================================================================
// Message Protocol (Event notifications, no response expected)
// ============================================================================

/// Event notification protocol for broadcasting events
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum EventProtocol {
    UserLoggedIn { user_id: u64, timestamp: u64 },
    UserLoggedOut { user_id: u64, timestamp: u64 },
    SystemAlert { level: String, message: String },
}

impl Protocol for EventProtocol {
    fn method_name(&self) -> &'static str {
        match self {
            Self::UserLoggedIn { .. } => "user_logged_in",
            Self::UserLoggedOut { .. } => "user_logged_out",
            Self::SystemAlert { .. } => "system_alert",
        }
    }

    fn is_request(&self) -> bool {
        false // Events are one-way messages
    }
}

// ============================================================================
// Server Implementation
// ============================================================================

/// Simple in-memory user database
#[derive(Clone)]
struct UserDatabase {
    users: Arc<RwLock<HashMap<u64, (String, String)>>>,
    next_id: Arc<RwLock<u64>>,
}

impl UserDatabase {
    fn new() -> Self {
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(RwLock::new(1)),
        }
    }

    async fn create_user(&self, username: String, email: String) -> u64 {
        let mut next_id = self.next_id.write().await;
        let user_id = *next_id;
        *next_id += 1;
        
        let mut users = self.users.write().await;
        users.insert(user_id, (username, email));
        user_id
    }

    async fn get_user(&self, user_id: u64) -> Option<(String, String)> {
        let users = self.users.read().await;
        users.get(&user_id).cloned()
    }

    async fn delete_user(&self, user_id: u64) -> bool {
        let mut users = self.users.write().await;
        users.remove(&user_id).is_some()
    }
}

/// Server implementation of UserService
struct MyUserService {
    db: UserDatabase,
}

impl MyUserService {
    fn new() -> Self {
        Self {
            db: UserDatabase::new(),
        }
    }
}

impl UserServiceServer for MyUserService {
    async fn create_user(&self, username: String, email: String) -> Result<(u64, String), String> {
        let user_id = self.db.create_user(username.clone(), email).await;
        println!("✅ Server: Created user {} with ID {}", username, user_id);
        Ok((user_id, username))
    }

    async fn get_user(&self, user_id: u64) -> Result<(u64, String, String), String> {
        match self.db.get_user(user_id).await {
            Some((username, email)) => {
                println!("✅ Server: Retrieved user {}", user_id);
                Ok((user_id, username, email))
            }
            None => {
                println!("❌ Server: User {} not found", user_id);
                Err(format!("User {} not found", user_id))
            }
        }
    }

    async fn delete_user(&self, user_id: u64) -> Result<u64, String> {
        if self.db.delete_user(user_id).await {
            println!("✅ Server: Deleted user {}", user_id);
            Ok(user_id)
        } else {
            println!("❌ Server: User {} not found", user_id);
            Err(format!("User {} not found", user_id))
        }
    }
}

/// Run the server
async fn run_server() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting server...");

    // Build server endpoint using EndpointBuilder
    let mut endpoint = EndpointBuilder::new(PostcardSerializer::default())
        .with_responder("user_service", 1)
        .with_responder("events", 1)
        .with_tcp_listener("127.0.0.1:8080")
        .build()
        .await?;

    println!("✅ Server listening on 127.0.0.1:8080");

    // Accept connections in a loop
    loop {
        println!("\n⏳ Waiting for client connection...");
        
        let connection = match endpoint.accept().await {
            Ok(conn) => {
                println!("✅ Client connected! Connection ID: {}", conn.id());
                conn
            }
            Err(e) => {
                eprintln!("❌ Accept error: {}", e);
                continue;
            }
        };

        // Give channels time to be created
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Get channels for this connection
        let (user_sender, user_receiver) = match endpoint
            .get_channels::<UserServiceProtocol>(connection.id(), "user_service")
            .await
        {
            Ok(channels) => channels,
            Err(e) => {
                eprintln!("❌ Failed to get user service channels: {}", e);
                continue;
            }
        };

        let (event_sender, _event_receiver) = match endpoint
            .get_channels::<EventProtocol>(connection.id(), "events")
            .await
        {
            Ok(channels) => channels,
            Err(e) => {
                eprintln!("❌ Failed to get event channels: {}", e);
                continue;
            }
        };

        println!("✅ Channels established");

        // Create service and dispatcher
        let service = MyUserService::new();
        let dispatcher = UserServiceDispatcher::new(service);
        let conn_id = connection.id();

        // Spawn a task to handle this connection
        tokio::spawn(async move {
            println!("🔄 Handler task started for connection {}", conn_id);

            // Send a welcome event
            if let Err(e) = event_sender
                .send(EventProtocol::SystemAlert {
                    level: "info".to_string(),
                    message: "Welcome to the user service!".to_string(),
                })
                .await
            {
                eprintln!("❌ Failed to send welcome event: {}", e);
            }

            // Handle requests using the dispatcher
            let mut request_count = 0;
            while let Some(request) = user_receiver.recv().await {
                request_count += 1;
                println!("📨 Request #{}", request_count);
                
                #[allow(deprecated)]
                let response = dispatcher.dispatch(request).await;
                
                if let Err(e) = user_sender.send(response).await {
                    eprintln!("❌ Failed to send response: {}", e);
                    break;
                }
            }

            println!("🔌 Connection {} closed after {} requests", conn_id, request_count);
        });
    }
}

// ============================================================================
// Client Implementation
// ============================================================================

/// Run the client
async fn run_client() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting client...");

    // Build client endpoint using EndpointBuilder
    let mut endpoint = EndpointBuilder::new(PostcardSerializer::default())
        .with_caller("user_service", 1)
        .with_caller("events", 1)
        .build()
        .await?;

    println!("✅ Client endpoint created");

    // Connect to server
    println!("🔌 Connecting to server at 127.0.0.1:8080...");
    let connection = endpoint.connect("127.0.0.1:8080").await?;
    println!("✅ Connected! Connection ID: {}", connection.id());

    // Give channels time to be created
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Get the user service client
    let (user_sender, user_receiver) = endpoint
        .get_channels::<UserServiceProtocol>(connection.id(), "user_service")
        .await?;
    let user_service = UserServiceClient::new(user_sender, user_receiver);
    println!("✅ User service client ready");

    // Get event channel to receive notifications
    let (_event_sender, mut event_receiver) = endpoint
        .get_channels::<EventProtocol>(connection.id(), "events")
        .await?;
    println!("✅ Event channel ready");

    // Spawn task to handle events
    tokio::spawn(async move {
        while let Some(event_envelope) = event_receiver.recv().await {
            let event = event_envelope.into_payload();
            match event {
                EventProtocol::SystemAlert { level, message } => {
                    println!("🔔 [{}] {}", level.to_uppercase(), message);
                }
                EventProtocol::UserLoggedIn { user_id, timestamp } => {
                    println!("🔔 User {} logged in at {}", user_id, timestamp);
                }
                EventProtocol::UserLoggedOut { user_id, timestamp } => {
                    println!("🔔 User {} logged out at {}", user_id, timestamp);
                }
            }
        }
    });

    // Wait a moment for the welcome event
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Demonstrate service calls
    println!("\n📝 Creating users...");
    
    let (user1_id, user1_name) = user_service
        .create_user("alice".to_string(), "alice@example.com".to_string())
        .await??;
    println!("✅ Created user: {} (ID: {})", user1_name, user1_id);

    let (user2_id, user2_name) = user_service
        .create_user("bob".to_string(), "bob@example.com".to_string())
        .await??;
    println!("✅ Created user: {} (ID: {})", user2_name, user2_id);

    println!("\n🔍 Retrieving user information...");
    let (id, name, email) = user_service.get_user(user1_id).await??;
    println!("✅ User {}: {} <{}>", id, name, email);

    println!("\n🗑️  Deleting user...");
    let deleted_id = user_service.delete_user(user2_id).await??;
    println!("✅ Deleted user ID: {}", deleted_id);

    println!("\n🔍 Trying to get deleted user...");
    match user_service.get_user(user2_id).await {
        Ok(Ok(_)) => println!("❌ Unexpected: User still exists"),
        Ok(Err(e)) => println!("✅ Expected error: {}", e),
        Err(e) => println!("❌ Channel error: {}", e),
    }

    println!("\n✅ Demo complete!");
    
    // Cleanup
    endpoint.shutdown().await?;
    
    Ok(())
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔════════════════════════════════════════════════════════════╗");
    println!("║     Complete Server-Client Demo with EndpointBuilder      ║");
    println!("║                                                            ║");
    println!("║  Demonstrates:                                             ║");
    println!("║  • EndpointBuilder for easy setup                          ║");
    println!("║  • Service macro for RPC-style communication               ║");
    println!("║  • Message protocol for event notifications                ║");
    println!("║  • Server accept() pattern                                 ║");
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();

    // Check command line arguments
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() < 2 {
        eprintln!("Usage: {} <server|client>", args[0]);
        eprintln!();
        eprintln!("Run the server in one terminal:");
        eprintln!("  cargo run --example complete_server_client_demo server");
        eprintln!();
        eprintln!("Then run the client in another terminal:");
        eprintln!("  cargo run --example complete_server_client_demo client");
        std::process::exit(1);
    }

    match args[1].as_str() {
        "server" => {
            run_server().await?;
        }
        "client" => {
            // Give server time to start if running both quickly
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            run_client().await?;
        }
        _ => {
            eprintln!("Invalid argument. Use 'server' or 'client'");
            std::process::exit(1);
        }
    }

    Ok(())
}

// Made with Bob
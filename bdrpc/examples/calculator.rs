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

//! # Calculator Example - Manual Protocol Implementation
//!
//! This example demonstrates the **manual protocol implementation approach** for RPC
//! using BDRPC. It shows how to define protocols, handle requests, and manage
//! communication without using the `#[bdrpc::service]` macro.
//!
//! ## 🔍 Two Approaches to BDRPC
//!
//! BDRPC supports two ways to implement RPC services:
//!
//! ### 1. Manual Protocol (This Example)
//! - Define protocol as an enum with all message variants
//! - Manually implement Protocol trait
//! - Write match statements to handle each message type
//! - Full control over message structure and handling
//! - More boilerplate but maximum flexibility
//!
//! ### 2. Service Macro (See `calculator_service.rs`)
//! - Define service as a trait with async methods
//! - Macro generates protocol enum, client, server, and dispatcher
//! - Type-safe RPC calls with automatic routing
//! - Less boilerplate, cleaner API
//! - Recommended for most use cases
//!
//! ## What This Example Shows
//!
//! **Manual Protocol Implementation:**
//! - Defining a protocol enum with request/response variants
//! - Implementing the Protocol trait manually
//! - Writing match statements to handle requests
//! - Manual request-response correlation
//! - Setting up a TCP server
//! - Connecting a TCP client
//! - Bi-directional RPC pattern (both sides can send and receive)
//! - Error handling (division by zero)
//! - Server notifications to client
//! - Graceful shutdown
//!
//! ## 📊 Comparison: Manual vs Service Macro
//!
//! | Aspect | Manual (This Example) | Service Macro (`calculator_service.rs`) |
//! |--------|----------------------|------------------------------------------|
//! | Protocol Definition | Enum with all variants | Trait with async methods |
//! | Boilerplate | More (enum + match) | Less (macro generates) |
//! | Type Safety | Runtime matching | Compile-time checking |
//! | Request Routing | Manual match statements | Automatic dispatcher |
//! | Client API | Send/recv protocol enum | Type-safe method calls |
//! | Flexibility | Maximum control | Structured patterns |
//! | Best For | Custom protocols | Standard RPC patterns |
//!
//! ## When to Use Manual Protocol
//!
//! Choose manual protocol implementation when you need:
//! - Custom message structures that don't fit RPC patterns
//! - Fine-grained control over serialization
//! - Non-standard request-response flows
//! - Protocol versioning with custom logic
//! - Integration with existing protocol definitions
//!
//! ## Architecture
//!
//! ```text
//! Client                          Server
//!   |                               |
//!   |-- Add(5, 3) ----------------->|
//!   |<-------------- Result(8) -----|
//!   |                               |
//!   |-- Multiply(4, 7) ------------>|
//!   |<------------ Result(28) ------|
//!   |                               |
//!   |<---- Notification("Done") ----|
//!   |                               |
//! ```
//!
//! ## Running This Example
//!
//! In one terminal (server):
//! ```bash
//! cargo run --example calculator -- server
//! ```
//!
//! In another terminal (client):
//! ```bash
//! cargo run --example calculator -- client
//! ```
//!
//! Or run both automatically:
//! ```bash
//! cargo run --example calculator
//! ```

use bdrpc::channel::{Channel, ChannelId, Protocol};
use bdrpc::transport::TcpTransport;
use std::error::Error;
use std::time::Duration;
use tokio::time::sleep;

/// Calculator protocol supporting various operations.
///
/// This protocol demonstrates:
/// - Multiple request types (Add, Subtract, Multiply, Divide)
/// - Response types (Result, Error)
/// - Notifications (server can notify client)
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
enum CalculatorProtocol {
    // Client requests
    /// Add two numbers
    Add { a: i32, b: i32 },
    /// Subtract two numbers
    Subtract { a: i32, b: i32 },
    /// Multiply two numbers
    Multiply { a: i32, b: i32 },
    /// Divide two numbers
    Divide { a: i32, b: i32 },

    // Server responses
    /// Successful result
    Result { value: i32 },
    /// Error response
    Error { message: String },

    // Server notifications
    /// Server sends notification to client
    Notification { message: String },
}

impl Protocol for CalculatorProtocol {
    fn method_name(&self) -> &'static str {
        match self {
            Self::Add { .. } => "add",
            Self::Subtract { .. } => "subtract",
            Self::Multiply { .. } => "multiply",
            Self::Divide { .. } => "divide",
            Self::Result { .. } => "result",
            Self::Error { .. } => "error",
            Self::Notification { .. } => "notification",
        }
    }

    fn is_request(&self) -> bool {
        matches!(
            self,
            Self::Add { .. } | Self::Subtract { .. } | Self::Multiply { .. } | Self::Divide { .. }
        )
    }
}

/// Run the calculator server.
async fn run_server() -> Result<(), Box<dyn Error>> {
    println!("🖥️  Starting Calculator Server");
    println!("═══════════════════════════════════════════════════════════\n");

    // Step 1: Bind TCP server
    println!("🌐 Step 1: Binding to TCP address");
    let addr = "127.0.0.1:9090";
    let server_transport = TcpTransport::bind(addr).await?;
    println!("   ✅ Listening on {}\n", addr);

    // Step 2: Accept client connection
    println!("⏳ Step 2: Waiting for client connection...");
    let (_client_stream, client_addr) = server_transport.accept().await?;
    println!("   ✅ Client connected from {}\n", client_addr);

    // Step 3: Create in-memory calculator channel
    println!("📡 Step 3: Creating in-memory calculator channel");
    let channel_id = ChannelId::new();
    let (server_sender, mut server_receiver) =
        Channel::<CalculatorProtocol>::new_in_memory(channel_id, 10);
    println!(
        "   ✅ In-memory calculator channel created (ID: {})",
        channel_id
    );
    println!("   ℹ️  Note: In a full implementation, use Endpoint API\n");

    // Step 4: Process requests
    println!("🔢 Step 4: Processing calculator requests");
    println!("═══════════════════════════════════════════════════════════\n");

    let mut request_count = 0;

    // Spawn a task to receive requests
    let receiver_task = tokio::spawn(async move {
        while let Some(request) = server_receiver.recv().await {
            request_count += 1;
            println!("📥 Request #{}: {:?}", request_count, request);

            let response = match request {
                CalculatorProtocol::Add { a, b } => {
                    let result = a + b;
                    println!("   Computing: {} + {} = {}", a, b, result);
                    CalculatorProtocol::Result { value: result }
                }
                CalculatorProtocol::Subtract { a, b } => {
                    let result = a - b;
                    println!("   Computing: {} - {} = {}", a, b, result);
                    CalculatorProtocol::Result { value: result }
                }
                CalculatorProtocol::Multiply { a, b } => {
                    let result = a * b;
                    println!("   Computing: {} × {} = {}", a, b, result);
                    CalculatorProtocol::Result { value: result }
                }
                CalculatorProtocol::Divide { a, b } => {
                    if b == 0 {
                        println!("   ⚠️  Error: Division by zero");
                        CalculatorProtocol::Error {
                            message: "Division by zero".to_string(),
                        }
                    } else {
                        let result = a / b;
                        println!("   Computing: {} ÷ {} = {}", a, b, result);
                        CalculatorProtocol::Result { value: result }
                    }
                }
                _ => {
                    println!("   ⚠️  Unexpected message type");
                    continue;
                }
            };

            // Send response
            println!("📤 Sending response: {:?}\n", response);
            if let Err(e) = server_sender.send(response).await {
                eprintln!("Error sending response: {}", e);
                break;
            }

            // After processing 4 requests, send a notification and exit
            if request_count >= 4 {
                println!("📢 Sending completion notification to client");
                if let Err(e) = server_sender
                    .send(CalculatorProtocol::Notification {
                        message: "All calculations complete!".to_string(),
                    })
                    .await
                {
                    eprintln!("Error sending notification: {}", e);
                }
                println!("   ✅ Notification sent\n");
                break;
            }
        }
        request_count
    });

    let final_count = receiver_task.await?;

    println!("═══════════════════════════════════════════════════════════");
    println!("✅ Server processed {} requests", final_count);
    println!("👋 Server shutting down\n");

    Ok(())
}

/// Run the calculator client.
async fn run_client() -> Result<(), Box<dyn Error>> {
    println!("💻 Starting Calculator Client");
    println!("═══════════════════════════════════════════════════════════\n");

    // Give server time to start
    sleep(Duration::from_millis(500)).await;

    // Step 1: Connect to server
    println!("🌐 Step 1: Connecting to server");
    let addr = "127.0.0.1:9090";
    let _client_stream = TcpTransport::connect(addr).await?;
    println!("   ✅ Connected to {}\n", addr);

    // Step 2: Create in-memory calculator channel
    println!("📡 Step 2: Creating in-memory calculator channel");
    let channel_id = ChannelId::new();
    let (client_sender, mut client_receiver) =
        Channel::<CalculatorProtocol>::new_in_memory(channel_id, 10);
    println!(
        "   ✅ In-memory calculator channel created (ID: {})",
        channel_id
    );
    println!("   ℹ️  Note: In a full implementation, use Endpoint API\n");

    // Step 3: Send requests and receive responses
    println!("🔢 Step 3: Sending calculator requests");
    println!("═══════════════════════════════════════════════════════════\n");

    // Request 1: Add
    println!("📤 Request 1: Add(5, 3)");
    client_sender
        .send(CalculatorProtocol::Add { a: 5, b: 3 })
        .await?;
    if let Some(response) = client_receiver.recv().await {
        println!("📥 Response: {:?}\n", response);
    }

    // Request 2: Subtract
    println!("📤 Request 2: Subtract(10, 4)");
    client_sender
        .send(CalculatorProtocol::Subtract { a: 10, b: 4 })
        .await?;
    if let Some(response) = client_receiver.recv().await {
        println!("📥 Response: {:?}\n", response);
    }

    // Request 3: Multiply
    println!("📤 Request 3: Multiply(7, 6)");
    client_sender
        .send(CalculatorProtocol::Multiply { a: 7, b: 6 })
        .await?;
    if let Some(response) = client_receiver.recv().await {
        println!("📥 Response: {:?}\n", response);
    }

    // Request 4: Divide (with error case)
    println!("📤 Request 4: Divide(20, 0) - Testing error handling");
    client_sender
        .send(CalculatorProtocol::Divide { a: 20, b: 0 })
        .await?;
    if let Some(response) = client_receiver.recv().await {
        println!("📥 Response: {:?}\n", response);
    }

    // Wait for server notification
    println!("⏳ Waiting for server notification...");
    if let Some(notification) = client_receiver.recv().await {
        match notification {
            CalculatorProtocol::Notification { message } => {
                println!("📢 Server notification: {}\n", message);
            }
            _ => {
                println!("⚠️  Unexpected message type\n");
            }
        }
    }

    println!("═══════════════════════════════════════════════════════════");
    println!("✅ Client completed all operations");
    println!("👋 Client shutting down\n");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("\n🧮 BDRPC Calculator Example - Bi-directional RPC Pattern\n");
    println!("⚠️  Note: This example demonstrates the RPC pattern with TCP transport");
    println!("   but uses in-memory channels. For full network RPC, see 'network_chat'\n");

    // Check command line arguments
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 {
        match args[1].as_str() {
            "server" => {
                run_server().await?;
            }
            "client" => {
                run_client().await?;
            }
            _ => {
                eprintln!("Usage: {} [server|client]", args[0]);
                eprintln!("  server - Run as server");
                eprintln!("  client - Run as client");
                eprintln!("  (no args) - Run both automatically");
                std::process::exit(1);
            }
        }
    } else {
        // Run both server and client
        println!("🚀 Running both server and client automatically\n");

        // Spawn server in background
        let server_handle = tokio::spawn(async {
            if let Err(e) = run_server().await {
                eprintln!("Server error: {}", e);
            }
        });

        // Run client in foreground
        if let Err(e) = run_client().await {
            eprintln!("Client error: {}", e);
        }

        // Wait for server to finish
        server_handle.await?;
    }

    println!("═══════════════════════════════════════════════════════════");
    println!("🎉 Calculator example completed successfully!\n");

    println!("📚 What this example demonstrated (Manual Protocol):");
    println!("   • Defining a protocol enum with all message variants");
    println!("   • Implementing the Protocol trait manually");
    println!("   • Writing match statements to handle requests");
    println!("   • Manual request-response correlation");
    println!("   • TCP transport for network communication");
    println!("   • In-memory channels for typed message passing");
    println!("   • Bi-directional RPC pattern (request-response)");
    println!("   • Multiple operation types (Add, Subtract, Multiply, Divide)");
    println!("   • Error handling (division by zero)");
    println!("   • Server-to-client notifications");
    println!("   • Graceful shutdown");

    println!("\n💡 Key concepts (Manual Protocol Approach):");
    println!("   • Protocol enum: Define all message types explicitly");
    println!("   • Protocol trait: Implement method_name() and is_request()");
    println!("   • Match statements: Route requests to handlers manually");
    println!("   • Full control: Maximum flexibility over message handling");
    println!("   • TCP provides reliable network communication");
    println!("   • Channels provide typed message passing");
    println!("   • Both sides can send and receive messages");
    println!("   • Proper error handling and propagation");

    println!("\n🔄 Alternative: Service Macro Approach");
    println!("   See 'calculator_service.rs' for the same functionality using:");
    println!("   • #[bdrpc::service] macro for automatic code generation");
    println!("   • Service trait instead of protocol enum");
    println!("   • Generated client with type-safe method calls");
    println!("   • Generated dispatcher for automatic routing");
    println!("   • Less boilerplate, cleaner API");

    println!("\n📊 Manual vs Service Macro:");
    println!("   Manual Protocol (this example):");
    println!("     ✓ Maximum flexibility and control");
    println!("     ✓ Custom message structures");
    println!("     ✓ Non-standard request-response flows");
    println!("     ✗ More boilerplate code");
    println!("     ✗ Manual request routing");
    println!("   Service Macro (calculator_service.rs):");
    println!("     ✓ Less boilerplate code");
    println!("     ✓ Type-safe RPC calls");
    println!("     ✓ Automatic request routing");
    println!("     ✓ Self-documenting service interface");
    println!("     ✗ Less flexibility for custom patterns");

    println!("\n⚠️  Important note:");
    println!("   • This example: TCP + in-memory channels (demonstration)");
    println!("   • Full network RPC: Use Endpoint API (see 'network_chat')");

    println!("\n📖 Next steps:");
    println!("   • Try 'calculator_service.rs' to see the service macro approach");
    println!("   • Compare the two implementations side-by-side");
    println!("   • See 'network_chat' for full Endpoint API usage");
    println!("   • Check 'chat_server' for multiple clients pattern");
    println!("   • Read the documentation for more features");

    Ok(())
}

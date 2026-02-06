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

//! # Service Macro Demo - Using #[bdrpc::service]
//!
//! This example demonstrates the complete usage of the `#[bdrpc::service]` macro
//! for bidirectional RPC communication. It shows:
//!
//! - Defining a service trait with the macro
//! - Implementing the generated server trait
//! - Using the generated client stub
//! - Using the generated dispatcher
//! - Bidirectional communication patterns
//!
//! ## What Gets Generated
//!
//! The `#[bdrpc::service]` macro generates:
//! 1. **Protocol Enum** - Implements `bdrpc::channel::Protocol`
//! 2. **Client Stub** - For making RPC calls
//! 3. **Server Trait** - To implement your service logic
//! 4. **Dispatcher** - Routes requests to your implementation
//!
//! ## Running This Example
//!
//! ```bash
//! cargo run --example service_macro_demo --features serde
//! ```

use bdrpc::channel::{Channel, ChannelId};
use bdrpc::service;
use std::error::Error;
use std::time::Duration;
use tokio::time::sleep;

// ============================================================================
// STEP 1: Define the service trait with the macro
// ============================================================================

/// Calculator service demonstrating the #[bdrpc::service] macro.
///
/// This macro generates:
/// - CalculatorProtocol enum (with AddRequest, AddResponse, etc.)
/// - CalculatorClient struct (for making RPC calls)
/// - CalculatorServer trait (to implement)
/// - CalculatorDispatcher struct (for routing requests)
#[service(direction = "bidirectional")]
#[allow(dead_code)]
trait Calculator {
    /// Add two numbers
    async fn add(&self, a: i32, b: i32) -> Result<i32, String>;

    /// Subtract two numbers
    async fn subtract(&self, a: i32, b: i32) -> Result<i32, String>;

    /// Multiply two numbers
    async fn multiply(&self, a: i32, b: i32) -> Result<i32, String>;

    /// Divide two numbers (can fail with division by zero)
    async fn divide(&self, a: i32, b: i32) -> Result<i32, String>;
}

// ============================================================================
// STEP 2: Implement the server trait
// ============================================================================

/// Our calculator implementation.
struct MyCalculator {
    name: String,
}

impl MyCalculator {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

/// Implement the generated CalculatorServer trait.
#[async_trait::async_trait]
impl CalculatorServer for MyCalculator {
    async fn add(&self, a: i32, b: i32) -> Result<i32, String> {
        println!("[{}] Computing: {} + {} = {}", self.name, a, b, a + b);
        Ok(a + b)
    }

    async fn subtract(&self, a: i32, b: i32) -> Result<i32, String> {
        println!("[{}] Computing: {} - {} = {}", self.name, a, b, a - b);
        Ok(a - b)
    }

    async fn multiply(&self, a: i32, b: i32) -> Result<i32, String> {
        println!("[{}] Computing: {} × {} = {}", self.name, a, b, a * b);
        Ok(a * b)
    }

    async fn divide(&self, a: i32, b: i32) -> Result<i32, String> {
        if b == 0 {
            println!("[{}] Error: Division by zero", self.name);
            Err("Division by zero".to_string())
        } else {
            println!("[{}] Computing: {} ÷ {} = {}", self.name, a, b, a / b);
            Ok(a / b)
        }
    }
}

// ============================================================================
// STEP 3: Use the generated client and server
// ============================================================================

/// Run the server side using the generated dispatcher.
async fn run_server(
    server_sender: bdrpc::channel::ChannelSender<CalculatorProtocol>,
    mut server_receiver: bdrpc::channel::ChannelReceiver<CalculatorProtocol>,
) -> Result<(), Box<dyn Error>> {
    println!("\n🖥️  SERVER: Starting calculator server");
    println!("═══════════════════════════════════════════════════════════\n");

    // Create our calculator implementation
    let calculator = MyCalculator::new("Server");

    // Create the generated dispatcher
    let dispatcher = CalculatorDispatcher::new(calculator);

    println!("✅ Server ready, waiting for requests...\n");

    // Process incoming requests using the dispatcher
    let mut request_count = 0;
    while let Some(request) = server_receiver.recv().await {
        request_count += 1;
        println!("📥 Request #{}: {:?}", request_count, request);

        // Dispatch the request to the appropriate handler
        let response = dispatcher.dispatch(request).await;

        println!("📤 Sending response: {:?}\n", response);
        server_sender.send(response).await?;

        // Stop after 4 requests
        if request_count >= 4 {
            break;
        }
    }

    println!("═══════════════════════════════════════════════════════════");
    println!("✅ Server processed {} requests", request_count);
    println!("👋 Server shutting down\n");

    Ok(())
}

/// Run the client side using the generated client stub.
async fn run_client(
    client_sender: bdrpc::channel::ChannelSender<CalculatorProtocol>,
    client_receiver: bdrpc::channel::ChannelReceiver<CalculatorProtocol>,
) -> Result<(), Box<dyn Error>> {
    println!("\n💻 CLIENT: Starting calculator client");
    println!("═══════════════════════════════════════════════════════════\n");

    // Give server time to start
    sleep(Duration::from_millis(100)).await;

    // Create the generated client stub
    let client = CalculatorClient::new(client_sender, client_receiver);

    println!("✅ Client ready, making RPC calls...\n");

    // Make RPC calls using the generated client methods
    // Note: The client methods return Result<Result<T, String>, ChannelError>
    // The outer Result is for channel errors, inner Result is the service result

    // Call 1: Add
    println!("📤 Calling: add(5, 3)");
    match client.add(5, 3).await {
        Ok(Ok(value)) => println!("📥 Result: {}\n", value),
        Ok(Err(e)) => println!("📥 Service Error: {}\n", e),
        Err(e) => println!("❌ Channel Error: {}\n", e),
    }

    sleep(Duration::from_millis(200)).await;

    // Call 2: Subtract
    println!("📤 Calling: subtract(10, 4)");
    match client.subtract(10, 4).await {
        Ok(Ok(value)) => println!("📥 Result: {}\n", value),
        Ok(Err(e)) => println!("📥 Service Error: {}\n", e),
        Err(e) => println!("❌ Channel Error: {}\n", e),
    }

    sleep(Duration::from_millis(200)).await;

    // Call 3: Multiply
    println!("📤 Calling: multiply(7, 6)");
    match client.multiply(7, 6).await {
        Ok(Ok(value)) => println!("📥 Result: {}\n", value),
        Ok(Err(e)) => println!("📥 Service Error: {}\n", e),
        Err(e) => println!("❌ Channel Error: {}\n", e),
    }

    sleep(Duration::from_millis(200)).await;

    // Call 4: Divide (with error case)
    println!("📤 Calling: divide(20, 0) - Testing error handling");
    match client.divide(20, 0).await {
        Ok(Ok(value)) => println!("📥 Result: {}\n", value),
        Ok(Err(e)) => println!("📥 Service Error (expected): {}\n", e),
        Err(e) => println!("❌ Channel Error: {}\n", e),
    }

    println!("═══════════════════════════════════════════════════════════");
    println!("✅ Client completed all calls");
    println!("👋 Client shutting down\n");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("\n🧮 BDRPC Service Macro Demo");
    println!("═══════════════════════════════════════════════════════════");
    println!("This example demonstrates the #[bdrpc::service] macro\n");

    println!("📋 What the macro generates:");
    println!("   1. CalculatorProtocol enum (implements Protocol trait)");
    println!("   2. CalculatorClient struct (for making RPC calls)");
    println!("   3. CalculatorServer trait (to implement)");
    println!("   4. CalculatorDispatcher struct (routes requests)\n");

    // Create bidirectional in-memory channels
    let channel_id = ChannelId::new();
    println!("📡 Creating bidirectional channels (ID: {})", channel_id);

    // Client -> Server channel
    let (client_to_server_sender, client_to_server_receiver) =
        Channel::<CalculatorProtocol>::new_in_memory(channel_id, 10);

    // Server -> Client channel
    let (server_to_client_sender, server_to_client_receiver) =
        Channel::<CalculatorProtocol>::new_in_memory(channel_id, 10);

    println!("✅ Channels created\n");

    // Spawn server task
    let server_handle = tokio::spawn(async move {
        if let Err(e) = run_server(server_to_client_sender, client_to_server_receiver).await {
            eprintln!("Server error: {}", e);
        }
    });

    // Run client in foreground
    if let Err(e) = run_client(client_to_server_sender, server_to_client_receiver).await {
        eprintln!("Client error: {}", e);
    }

    // Wait for server to finish
    server_handle.await?;

    println!("═══════════════════════════════════════════════════════════");
    println!("🎉 Service macro demo completed successfully!\n");

    println!("📚 What this example demonstrated:");
    println!("   ✅ Defining a service with #[bdrpc::service]");
    println!("   ✅ Implementing the generated server trait");
    println!("   ✅ Using the generated client stub");
    println!("   ✅ Using the generated dispatcher");
    println!("   ✅ Bidirectional RPC communication");
    println!("   ✅ Error handling (division by zero)");

    println!("\n💡 Key benefits of the macro:");
    println!("   • Type-safe RPC calls at compile time");
    println!("   • Automatic request/response matching");
    println!("   • Clean separation of client and server code");
    println!("   • Automatic protocol enum generation");
    println!("   • Built-in error handling");

    println!("\n🔧 How to use in your code:");
    println!("   1. Define trait with #[bdrpc::service]");
    println!("   2. Implement the generated *Server trait");
    println!("   3. Create *Dispatcher with your implementation");
    println!("   4. Use *Client to make RPC calls");
    println!("   5. Dispatcher routes requests automatically");

    println!("\n📖 Next steps:");
    println!("   • Add more methods to the service trait");
    println!("   • Try different direction modes (call, respond)");
    println!("   • Use with Endpoint API for network communication");
    println!("   • Add version and feature attributes");

    Ok(())
}

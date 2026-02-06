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

//! # Hello World Service Example - Using #[bdrpc::service]
//!
//! This example demonstrates the simplest possible RPC service using the `#[bdrpc::service]` macro.
//! Compare this with `hello_world_manual.rs` to see the difference between manual protocol
//! implementation and using the service macro.
//!
//! ## What This Example Shows
//!
//! - Defining a simple greeting service with the `#[bdrpc::service]` macro
//! - Implementing the generated server trait
//! - Using the generated client stub for type-safe RPC calls
//! - Using the generated dispatcher for request routing
//! - Basic request-response pattern
//!
//! ## Benefits Over Manual Implementation
//!
//! - **Simplicity**: Define service as a trait, not an enum
//! - **Type Safety**: Compile-time checking of RPC calls
//! - **Less Code**: No manual protocol enum or matching logic
//! - **Clean API**: Client methods match service trait methods
//!
//! ## Running This Example
//!
//! ```bash
//! cargo run --example hello_world_service --features serde
//! ```

use bdrpc::channel::{Channel, ChannelId};
use bdrpc::service;
use std::error::Error;

// ============================================================================
// STEP 1: Define the service trait with the macro
// ============================================================================

/// A simple greeting service.
///
/// This macro generates:
/// - GreetingProtocol enum (with GreetRequest, GreetResponse)
/// - GreetingClient struct (for making RPC calls)
/// - GreetingServer trait (to implement)
/// - GreetingDispatcher struct (for routing requests)
#[service(direction = "bidirectional")]
#[allow(dead_code)]
trait Greeting {
    /// Greet someone by name
    async fn greet(&self, name: String) -> Result<String, String>;
}

// ============================================================================
// STEP 2: Implement the server trait
// ============================================================================

/// Our greeting service implementation.
struct MyGreetingService;

/// Implement the generated GreetingServer trait.
#[async_trait::async_trait]
impl GreetingServer for MyGreetingService {
    async fn greet(&self, name: String) -> Result<String, String> {
        println!("   Server: Received greeting request for '{}'", name);
        let message = format!("Hello, {}! Welcome to BDRPC! 👋", name);
        println!("   Server: Sending response: '{}'", message);
        Ok(message)
    }
}

// ============================================================================
// STEP 3: Use the generated client and server
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("🚀 BDRPC Hello World Service Example");
    println!("   Using the #[bdrpc::service] macro\n");

    // Step 1: Create bidirectional in-memory channels
    println!("📡 Step 1: Creating in-memory channels");
    let channel_id = ChannelId::new();
    println!("   Channel ID: {}", channel_id);

    let (client_to_server_sender, mut client_to_server_receiver) =
        Channel::<GreetingProtocol>::new_in_memory(channel_id, 10);
    let (server_to_client_sender, server_to_client_receiver) =
        Channel::<GreetingProtocol>::new_in_memory(channel_id, 10);

    println!("   ✅ Created in-memory channels\n");

    // Step 2: Create server with dispatcher
    println!("🖥️  Step 2: Starting server task");
    let service = MyGreetingService;
    let dispatcher = GreetingDispatcher::new(service);

    let server_task = tokio::spawn(async move {
        println!("   Server: Waiting for greeting request...");

        if let Some(request) = client_to_server_receiver.recv().await {
            println!("   Server: Processing request: {:?}", request);

            // Dispatch the request to the appropriate handler
            let response = dispatcher.dispatch(request).await;

            println!("   Server: Sending response: {:?}", response);
            if let Err(e) = server_to_client_sender.send(response).await {
                eprintln!("   Server: Failed to send response: {}", e);
            }
        }
    });

    // Give server a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Step 3: Create client and make RPC call
    println!("💬 Step 3: Client making RPC call");
    let client = GreetingClient::new(client_to_server_sender, server_to_client_receiver);

    println!("   Client: Calling greet('World')...");
    match client.greet("World".to_string()).await {
        Ok(Ok(message)) => {
            println!("   ✅ Client: Received response: '{}'", message);
        }
        Ok(Err(e)) => {
            println!("   ❌ Client: Service error: {}", e);
        }
        Err(e) => {
            println!("   ❌ Client: Channel error: {}", e);
        }
    }

    // Wait for server to complete
    server_task.await?;

    println!("\n🎉 Example completed successfully!\n");

    println!("📚 What we demonstrated:");
    println!("   1. Defined a service trait with #[bdrpc::service]");
    println!("   2. Implemented the generated GreetingServer trait");
    println!("   3. Used GreetingDispatcher to route requests");
    println!("   4. Used GreetingClient for type-safe RPC calls");
    println!("   5. Basic request-response pattern");

    println!("\n💡 Key benefits:");
    println!("   • Simple trait definition instead of enum");
    println!("   • Type-safe RPC calls at compile time");
    println!("   • Automatic request/response matching");
    println!("   • Clean separation of client and server code");
    println!("   • No manual protocol enum or matching logic");

    println!("\n🔍 Compare with hello_world_manual:");
    println!("   • hello_world_manual: Manual GreetingProtocol enum");
    println!("   • hello_world_service.rs: Generated via macro");
    println!("   • Service macro is simpler and more maintainable");

    println!("\n📖 Next steps:");
    println!("   • See calculator_service.rs for more complex example");
    println!("   • Try service_macro_demo.rs for detailed explanation");
    println!("   • Check chat_server_service.rs for multi-client pattern");
    println!("   • Read the macro documentation for advanced features");

    Ok(())
}

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

//! # Hello World Example - Manual Protocol Implementation
//!
//! This example demonstrates the **manual protocol implementation approach** for
//! defining and using protocols in BDRPC. It shows the foundational concepts without
//! using the `#[bdrpc::service]` macro.
//!
//! ## 🔍 Two Approaches to BDRPC
//!
//! ### 1. Manual Protocol (This Example)
//! - Define protocol as an enum with message variants
//! - Manually implement Protocol trait
//! - Direct control over message structure
//! - More verbose but maximum flexibility
//!
//! ### 2. Service Macro (See `hello_world_service.rs`)
//! - Define service as a trait with async methods
//! - Macro generates protocol, client, server, dispatcher
//! - Type-safe RPC calls
//! - Less boilerplate, recommended for RPC patterns
//!
//! ## What This Example Shows
//!
//! **Manual Protocol Implementation:**
//! - Defining a protocol enum with request/response variants
//! - Implementing the Protocol trait (method_name, is_request)
//! - Creating a protocol with the Protocol trait
//! - Setting up in-memory transports for testing
//! - Using channels for bi-directional communication
//! - Sending requests and receiving responses
//! - Proper async task management
//! - Manual message handling with match statements
//!
//! ## Running This Example
//!
//! ```bash
//! cargo run --example hello_world --features serde
//! ```

use bdrpc::channel::{Channel, ChannelId, Protocol};
use bdrpc::transport::{MemoryTransport, Transport};
use std::error::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// A simple greeting protocol with request and response messages.
///
/// This demonstrates the minimal protocol implementation required for BDRPC.
/// The protocol must implement Clone and the Protocol trait.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
enum GreetingProtocol {
    /// Request to greet someone by name
    Greet { name: String },
    /// Response with a greeting message
    Response { message: String },
}

impl Protocol for GreetingProtocol {
    fn method_name(&self) -> &'static str {
        match self {
            Self::Greet { .. } => "greet",
            Self::Response { .. } => "response",
        }
    }

    fn is_request(&self) -> bool {
        matches!(self, Self::Greet { .. })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("🚀 BDRPC Hello World Example");
    println!("   Demonstrating the full BDRPC stack\n");

    // Step 1: Create a pair of connected memory transports
    // This simulates a network connection but runs in-process for simplicity
    println!("📡 Step 1: Creating transport layer");
    let (mut client_transport, mut server_transport) = MemoryTransport::pair(1024);
    println!("   ✅ Created pair of MemoryTransports");
    println!(
        "   ✅ Client transport ID: {}",
        client_transport.metadata().id
    );
    println!(
        "   ✅ Server transport ID: {}\n",
        server_transport.metadata().id
    );

    // Step 2: Create channels for typed communication
    // Channels provide FIFO ordering and type safety
    //
    // ⚠️ NOTE: This example uses in-memory channels for demonstration purposes.
    // These channels are NOT connected to the transport layer and only work within
    // the same process. For real network communication, use Endpoint::create_channel()
    // which properly wires channels to transports.
    println!("🔗 Step 2: Creating channel layer");
    let channel_id = ChannelId::new();
    println!("   Channel ID: {}", channel_id);

    let (client_sender, mut client_receiver) =
        Channel::<GreetingProtocol>::new_in_memory(channel_id, 10);
    let (server_sender, mut server_receiver) =
        Channel::<GreetingProtocol>::new_in_memory(channel_id, 10);
    println!("   ✅ Created in-memory client and server channels");
    println!("   ℹ️  These are for demonstration only - not connected to transport\n");

    // Step 3: Spawn server task to handle requests
    println!("🖥️  Step 3: Starting server task");
    let server_task = tokio::spawn(async move {
        println!("   Server: Waiting for greeting request...");

        // In a real application, you would read from the transport,
        // deserialize, and route to the appropriate channel.
        // For this example, we'll simulate receiving directly.

        if let Some(message) = server_receiver.recv().await {
            match message {
                GreetingProtocol::Greet { name } => {
                    println!("   Server: Received request to greet '{}'", name);

                    // Process the request and send response
                    let response = GreetingProtocol::Response {
                        message: format!("Hello, {}! Welcome to BDRPC! 👋", name),
                    };

                    println!("   Server: Sending response...");
                    if let Err(e) = server_sender.send(response).await {
                        eprintln!("   Server: Failed to send response: {}", e);
                        return Err(e);
                    }
                    println!("   Server: Response sent successfully");
                }
                GreetingProtocol::Response { .. } => {
                    eprintln!("   Server: Unexpected response message");
                }
            }
        }
        Ok::<_, bdrpc::ChannelError>(())
    });

    // Give server a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Step 4: Client sends request
    println!("\n💬 Step 4: Client sending request");
    let request = GreetingProtocol::Greet {
        name: "World".to_string(),
    };

    println!("   Client: Sending greeting request...");
    client_sender.send(request).await?;
    println!("   Client: Request sent\n");

    // Step 5: Demonstrate transport layer (optional - shows the underlying bytes)
    println!("📦 Step 5: Transport layer demonstration");
    println!("   In a real application, messages would be:");
    println!("   1. Serialized to bytes (e.g., JSON, Postcard, rkyv)");
    println!("   2. Framed with length prefix");
    println!("   3. Sent over the transport");
    println!("   4. Received and deserialized on the other side");

    // Demonstrate writing/reading raw bytes
    let demo_message = b"Hello from transport layer!";
    client_transport.write_all(demo_message).await?;

    let mut buffer = vec![0u8; 1024];
    let n = server_transport.read(&mut buffer).await?;
    println!("   ✅ Sent {} bytes over transport", demo_message.len());
    println!(
        "   ✅ Received {} bytes: {:?}\n",
        n,
        String::from_utf8_lossy(&buffer[..n])
    );

    // Wait for server to complete
    server_task.await??;

    // Step 6: Client receives response
    println!("⏳ Step 6: Client waiting for response");

    // Simulate the response coming back
    // In a real app, this would come through the transport and be deserialized
    let demo_response = GreetingProtocol::Response {
        message: "Hello, World! Welcome to BDRPC! 👋".to_string(),
    };
    client_sender.send(demo_response).await?;

    if let Some(message) = client_receiver.recv().await {
        match message {
            GreetingProtocol::Response { message } => {
                println!("   ✅ Client received: '{}'", message);
            }
            GreetingProtocol::Greet { .. } => {
                eprintln!("   ❌ Client: Unexpected request message");
            }
        }
    }

    println!("\n🎉 Example completed successfully!\n");

    println!("📚 What we demonstrated (Manual Protocol):");
    println!("   1. Protocol Enum - Defined GreetingProtocol with Greet and Response variants");
    println!("   2. Protocol Trait - Implemented method_name() and is_request()");
    println!("   3. Transport Layer - MemoryTransport for in-process communication");
    println!("   4. Channel Layer - Typed, FIFO-ordered message passing");
    println!("   5. Manual Handling - Match statements to process messages");
    println!("   6. Async Communication - Tokio-based async/await");
    println!("   7. Bi-directional Flow - Both sides can send and receive");

    println!("\n💡 Key BDRPC concepts (Manual Protocol):");
    println!("   • Protocol enum - Explicit definition of all message types");
    println!("   • Protocol trait - method_name() and is_request() methods");
    println!("   • Manual routing - Match statements to handle messages");
    println!("   • Transport abstraction - swap MemoryTransport for TcpTransport");
    println!("   • Channel multiplexing - multiple protocols over one transport");
    println!("   • Type safety - Protocol trait ensures correct message types");
    println!("   • FIFO ordering - messages delivered in order within a channel");
    println!("   • Zero unsafe code - all safe Rust");

    println!("\n🔄 Alternative: Service Macro Approach");
    println!("   See 'hello_world_service.rs' for the same functionality using:");
    println!("   • #[bdrpc::service] macro - automatic code generation");
    println!("   • Service trait - define methods instead of enum variants");
    println!("   • Generated client - type-safe method calls");
    println!("   • Generated dispatcher - automatic request routing");
    println!("   • Simpler, cleaner code with less boilerplate");

    println!("\n📊 When to use each approach:");
    println!("   Manual Protocol (this example):");
    println!("     • Need custom message structures");
    println!("     • Non-standard communication patterns");
    println!("     • Maximum control over protocol details");
    println!("   Service Macro (hello_world_service.rs):");
    println!("     • Standard RPC request-response patterns");
    println!("     • Want type-safe client API");
    println!("     • Prefer less boilerplate code");

    println!("\n📖 Next steps:");
    println!("   • Compare with 'hello_world_service.rs' (service macro version)");
    println!("   • See 'channel_basics' for channel-only example");
    println!("   • Try 'calculator' for more complex manual protocol");
    println!("   • Try 'calculator_service' for service macro version");
    println!("   • Explore TCP transport for network communication");

    Ok(())
}

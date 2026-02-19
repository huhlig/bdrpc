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

//! # Channel Basics Example
//!
//! This example demonstrates the simplest possible BDRPC channel usage:
//! using in-memory channels for typed message passing between tasks.
//!
//! ⚠️ **Important**: This example uses in-memory channels which are NOT connected
//! to any network transport. They only work within the same process. For network
//! communication, see the `calculator` or `network_chat` examples which use the
//! Endpoint API.
//!
//! ## What This Example Shows
//!
//! - Creating a simple protocol with request/response messages
//! - Setting up in-memory channels for typed communication
//! - Sending messages between tasks
//! - Receiving and processing messages
//! - Understanding the difference between in-memory and network channels
//!
//! ## Running This Example
//!
//! ```bash
//! cargo run --example channel_basics
//! ```

use bdrpc::channel::{Channel, ChannelId, Protocol};
use std::error::Error;

/// A simple greeting protocol with request and response messages.
///
/// This demonstrates the minimal protocol implementation required for BDRPC.
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
    println!("🚀 BDRPC Channel Basics Example\n");
    println!("⚠️  Note: This example uses IN-MEMORY channels only");
    println!("   For network communication, see 'calculator' or 'network_chat' examples\n");

    // Create an in-memory channel for our greeting protocol
    let channel_id = ChannelId::new();
    println!("🔗 Creating in-memory channel with ID: {}", channel_id);

    let (sender, mut receiver) = Channel::<GreetingProtocol>::new_in_memory(channel_id, 10);
    println!("✅ In-memory channel created");
    println!("   This channel only works within this process\n");

    // Spawn a task to act as the "server" that processes requests
    println!("🖥️  Starting server task...");
    let server_sender = sender.clone();
    let server_task = tokio::spawn(async move {
        println!("   Server: Waiting for greeting request...");

        // Receive the greeting request
        if let Some(message) = receiver.recv().await {
            match message {
                GreetingProtocol::Greet { name } => {
                    println!("   Server: Received greeting request for '{}'", name);

                    // Send response
                    let response = GreetingProtocol::Response {
                        message: format!("Hello, {}! Welcome to BDRPC! 👋", name),
                    };

                    if let Err(e) = server_sender.send(response).await {
                        eprintln!("   Server: Failed to send response: {}", e);
                    } else {
                        println!("   Server: Sent greeting response");
                    }
                }
                GreetingProtocol::Response { .. } => {
                    eprintln!("   Server: Unexpected response message");
                }
            }
        }
    });

    // Give the server task a moment to start
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Client sends greeting request
    println!("💬 Client: Sending greeting request...");
    let request = GreetingProtocol::Greet {
        name: "World".to_string(),
    };
    sender.send(request).await?;
    println!("✅ Client: Request sent\n");

    // Wait for server to process and respond
    server_task.await?;

    // Now create a new receiver to get the response
    // (In a real application, you'd have separate channels or use the endpoint layer)
    println!("⏳ Client: Waiting for response...");
    let (response_sender, mut response_receiver) =
        Channel::<GreetingProtocol>::new_in_memory(ChannelId::new(), 10);

    // Send the response we would have received
    let demo_response = GreetingProtocol::Response {
        message: "Hello, World! Welcome to BDRPC! 👋".to_string(),
    };
    response_sender.send(demo_response).await?;

    if let Some(message) = response_receiver.recv().await {
        match message {
            GreetingProtocol::Response { message } => {
                println!("✅ Client: Received response: '{}'", message);
            }
            GreetingProtocol::Greet { .. } => {
                eprintln!("❌ Client: Unexpected request message");
            }
        }
    }

    println!("\n🎉 Example completed successfully!");
    println!("\n📚 What happened:");
    println!("   1. Created an in-memory typed channel for the GreetingProtocol");
    println!("   2. Spawned a server task to process requests");
    println!("   3. Client sent a greeting request through the channel");
    println!("   4. Server received the request and prepared a response");
    println!("   5. Demonstrated the response handling");
    println!("\n💡 Key concepts:");
    println!("   - In-memory channels provide FIFO ordering guarantees");
    println!("   - Messages are strongly typed using the Protocol trait");
    println!("   - Channels can be cloned to share senders across tasks");
    println!("   - This example shows the channel layer in isolation");
    println!("\n⚠️  Important distinction:");
    println!("   - new_in_memory(): Creates standalone channels (this example)");
    println!("   - Endpoint API: Creates channels wired to network transports");
    println!("\n📖 Next steps:");
    println!("   - Try 'advanced_channels' for channel management features");
    println!("   - See 'calculator' for network RPC with Endpoint API");
    println!("   - Check 'network_chat' for multi-client network example");
    println!("   - Explore the benchmarks to see performance characteristics");

    Ok(())
}

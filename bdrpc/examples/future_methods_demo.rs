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

//! # Future Methods Demo
//!
//! This example demonstrates that the `#[bdrpc::service]` macro supports both:
//! 1. Traditional `async fn` methods
//! 2. Methods that manually return `impl Future<Output = ...>`
//!
//! Both styles can be mixed within the same service trait, giving you flexibility
//! in how you implement your service methods.
//!
//! ## When to Use Each Style
//!
//! **Use `async fn` (recommended for most cases):**
//! - Simpler, more readable syntax
//! - Standard Rust async/await pattern
//! - Easier to work with
//!
//! **Use `impl Future` when:**
//! - You need more control over the Future type
//! - You're wrapping existing Future-returning functions
//! - You want to avoid the async fn transformation
//! - You need specific lifetime or trait bounds
//!
//! ## Running This Example
//!
//! ```bash
//! cargo run --example future_methods_demo --features serde
//! ```

use bdrpc::channel::{Channel, ChannelId};
use bdrpc::service;
use std::error::Error;
use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;

// ============================================================================
// Define a service with mixed method styles
// ============================================================================

/// A service demonstrating both async fn and impl Future methods.
#[service(direction = "bidirectional")]
#[allow(dead_code)]
trait MixedStyleService {
    /// Traditional async method - most common and recommended
    async fn async_add(&self, a: i32, b: i32) -> Result<i32, String>;

    /// Method returning impl Future - gives more control
    fn future_multiply(&self, a: i32, b: i32) -> impl Future<Output = Result<i32, String>> + Send;

    /// Another async method
    async fn async_greet(&self, name: String) -> Result<String, String>;

    /// Another impl Future method
    fn future_uppercase(&self, text: String) -> impl Future<Output = Result<String, String>> + Send;
}

// ============================================================================
// Implement the service
// ============================================================================

struct MyService;

#[async_trait::async_trait]
impl MixedStyleServiceServer for MyService {
    async fn async_add(&self, a: i32, b: i32) -> Result<i32, String> {
        println!("  [Server] async_add({}, {}) = {}", a, b, a + b);
        Ok(a + b)
    }

    fn future_multiply(&self, a: i32, b: i32) -> impl Future<Output = Result<i32, String>> + Send {
        println!("  [Server] future_multiply({}, {}) called", a, b);
        // Return an async block that computes the result
        async move {
            let result = a * b;
            println!("  [Server] future_multiply result = {}", result);
            Ok(result)
        }
    }

    async fn async_greet(&self, name: String) -> Result<String, String> {
        let greeting = format!("Hello, {}!", name);
        println!("  [Server] async_greet(\"{}\") = \"{}\"", name, greeting);
        Ok(greeting)
    }

    fn future_uppercase(&self, text: String) -> impl Future<Output = Result<String, String>> + Send {
        println!("  [Server] future_uppercase(\"{}\") called", text);
        async move {
            let result = text.to_uppercase();
            println!("  [Server] future_uppercase result = \"{}\"", result);
            Ok(result)
        }
    }
}

// ============================================================================
// Run the example
// ============================================================================

async fn run_server(
    server_sender: bdrpc::channel::ChannelSender<MixedStyleServiceProtocol>,
    mut server_receiver: bdrpc::channel::ChannelReceiver<MixedStyleServiceProtocol>,
) -> Result<(), Box<dyn Error>> {
    println!("\n🖥️  SERVER: Starting");
    println!("═══════════════════════════════════════════════════════════\n");

    let service = MyService;
    let dispatcher = MixedStyleServiceDispatcher::new(service);

    println!("✅ Server ready, waiting for requests...\n");

    let mut request_count = 0;
    while let Some(request) = server_receiver.recv().await {
        request_count += 1;
        println!("📥 Request #{}", request_count);

        let response = dispatcher.dispatch(request).await;
        server_sender.send(response).await?;

        if request_count >= 4 {
            break;
        }
    }

    println!("\n═══════════════════════════════════════════════════════════");
    println!("✅ Server processed {} requests", request_count);
    println!("👋 Server shutting down\n");

    Ok(())
}

async fn run_client(
    client_sender: bdrpc::channel::ChannelSender<MixedStyleServiceProtocol>,
    client_receiver: bdrpc::channel::ChannelReceiver<MixedStyleServiceProtocol>,
) -> Result<(), Box<dyn Error>> {
    println!("\n💻 CLIENT: Starting");
    println!("═══════════════════════════════════════════════════════════\n");

    sleep(Duration::from_millis(100)).await;

    let client = MixedStyleServiceClient::new(client_sender, client_receiver);

    println!("✅ Client ready, making RPC calls...\n");

    // Call async method
    println!("📤 Calling async_add(10, 20)");
    match client.async_add(10, 20).await {
        Ok(Ok(result)) => println!("📥 Result: {}\n", result),
        Ok(Err(e)) => println!("📥 Service Error: {}\n", e),
        Err(e) => println!("❌ Channel Error: {}\n", e),
    }

    sleep(Duration::from_millis(200)).await;

    // Call impl Future method
    println!("📤 Calling future_multiply(7, 6)");
    match client.future_multiply(7, 6).await {
        Ok(Ok(result)) => println!("📥 Result: {}\n", result),
        Ok(Err(e)) => println!("📥 Service Error: {}\n", e),
        Err(e) => println!("❌ Channel Error: {}\n", e),
    }

    sleep(Duration::from_millis(200)).await;

    // Call another async method
    println!("📤 Calling async_greet(\"World\")");
    match client.async_greet("World".to_string()).await {
        Ok(Ok(result)) => println!("📥 Result: \"{}\"\n", result),
        Ok(Err(e)) => println!("📥 Service Error: {}\n", e),
        Err(e) => println!("❌ Channel Error: {}\n", e),
    }

    sleep(Duration::from_millis(200)).await;

    // Call another impl Future method
    println!("📤 Calling future_uppercase(\"rust\")");
    match client.future_uppercase("rust".to_string()).await {
        Ok(Ok(result)) => println!("📥 Result: \"{}\"\n", result),
        Ok(Err(e)) => println!("📥 Service Error: {}\n", e),
        Err(e) => println!("❌ Channel Error: {}\n", e),
    }

    println!("═══════════════════════════════════════════════════════════");
    println!("✅ Client completed all calls");
    println!("👋 Client shutting down\n");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("\n🎯 BDRPC Future Methods Demo");
    println!("═══════════════════════════════════════════════════════════");
    println!("Demonstrating async fn and impl Future methods\n");

    println!("📋 Service trait has:");
    println!("   • async_add - async fn method");
    println!("   • future_multiply - impl Future method");
    println!("   • async_greet - async fn method");
    println!("   • future_uppercase - impl Future method\n");

    // Create bidirectional channels
    let channel_id = ChannelId::new();
    println!("📡 Creating bidirectional channels (ID: {})", channel_id);

    let (client_to_server_sender, client_to_server_receiver) =
        Channel::<MixedStyleServiceProtocol>::new_in_memory(channel_id, 10);
    let (server_to_client_sender, server_to_client_receiver) =
        Channel::<MixedStyleServiceProtocol>::new_in_memory(channel_id, 10);

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
    println!("🎉 Future methods demo completed successfully!\n");

    println!("📚 What this example demonstrated:");
    println!("   ✅ Mixing async fn and impl Future methods");
    println!("   ✅ Both styles work seamlessly together");
    println!("   ✅ Generated code handles both correctly");
    println!("   ✅ Client calls work identically for both styles");

    println!("\n💡 Key takeaways:");
    println!("   • Use async fn for most cases (simpler, clearer)");
    println!("   • Use impl Future when you need more control");
    println!("   • Both styles can be mixed in the same service");
    println!("   • The macro extracts the Output type automatically");
    println!("   • No runtime overhead difference between the two");

    println!("\n📖 Related examples:");
    println!("   • calculator_service.rs - Basic service with async methods");
    println!("   • file_transfer_service_async.rs - Advanced async patterns");

    Ok(())
}

// Made with Bob

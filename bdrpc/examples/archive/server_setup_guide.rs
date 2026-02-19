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

//! # Server Setup Guide - EndpointBuilder Integration
//!
//! This example explains how to properly set up a server using EndpointBuilder
//! and how it relates to the lower-level TCP transport.
//!
//! ## Two Approaches:
//!
//! 1. **High-Level (Recommended)**: Use EndpointBuilder with automatic transport management
//! 2. **Low-Level**: Use TcpTransport directly with manual channel management
//!
//! ## Running This Example:
//!
//! ```bash
//! cargo run --example server_setup_guide --features serde
//! ```

use bdrpc::channel::{Channel, ChannelId, Protocol};
use bdrpc::endpoint::{EndpointBuilder, EndpointError};
use bdrpc::serialization::PostcardSerializer;
use bdrpc::service;
use bdrpc::transport::TcpTransport;
use std::collections::HashMap;
use std::error::Error;

// ============================================================================
// Protocol Definitions
// ============================================================================

#[service(direction = "bidirectional")]
#[allow(dead_code)]
trait GatewayService {
    async fn fetch_properties(&self, keys: Vec<String>) -> Result<HashMap<String, String>, String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[allow(dead_code)]
pub enum MessageProtocol {
    Message { content: String },
    Ack,
}

impl Protocol for MessageProtocol {
    fn method_name(&self) -> &'static str {
        match self {
            Self::Message { .. } => "message",
            Self::Ack => "ack",
        }
    }

    fn is_request(&self) -> bool {
        matches!(self, Self::Message { .. })
    }
}

// ============================================================================
// APPROACH 1: High-Level with EndpointBuilder (RECOMMENDED)
// ============================================================================

async fn approach_1_endpoint_builder() -> Result<(), EndpointError> {
    println!("═══════════════════════════════════════════════════════════");
    println!("APPROACH 1: Using EndpointBuilder (Recommended)");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("📋 Step 1: Build endpoint with protocols and transport");
    println!("```rust");
    println!("let endpoint = EndpointBuilder::server(PostcardSerializer::default())");
    println!("    .with_responder(\"GatewayRPCProtocol\", 1)");
    println!("    .with_bidirectional(\"SessionMessageProtocol\", 1)");
    println!("    .with_tcp_listener(\"127.0.0.1:9090\")");
    println!("    .build()");
    println!("    .await?;");
    println!("```\n");

    let _endpoint = EndpointBuilder::server(PostcardSerializer::default())
        .with_responder("GatewayRPCProtocol", 1)
        .with_bidirectional("SessionMessageProtocol", 1)
        .with_tcp_listener("127.0.0.1:9090")
        .build()
        .await?;

    println!("✅ Endpoint created with:");
    println!("   • Protocols registered");
    println!("   • TCP listener configured");
    println!("   • Transport manager initialized\n");

    println!("📋 Step 2: Accept connections (CONCEPTUAL - not yet implemented)");
    println!("```rust");
    println!("loop {{");
    println!("    // This is the INTENDED API (not yet fully implemented)");
    println!("    let connection = endpoint.accept().await?;");
    println!("    ");
    println!("    // Get typed channels for this connection");
    println!("    let (rpc_sender, rpc_receiver) = endpoint");
    println!("        .get_channels::<GatewayServiceProtocol>(");
    println!("            connection.id(),");
    println!("            \"GatewayRPCProtocol\"");
    println!("        )");
    println!("        .await?;");
    println!("    ");
    println!("    let (msg_sender, msg_receiver) = endpoint");
    println!("        .get_channels::<SessionMessageProtocol>(");
    println!("            connection.id(),");
    println!("            \"SessionMessageProtocol\"");
    println!("        )");
    println!("        .await?;");
    println!("    ");
    println!("    // Spawn handlers");
    println!("    tokio::spawn(handle_rpc(rpc_sender, rpc_receiver));");
    println!("    tokio::spawn(handle_messages(msg_sender, msg_receiver));");
    println!("}}");
    println!("```\n");

    println!("💡 Benefits:");
    println!("   • Automatic protocol negotiation");
    println!("   • Built-in handshake handling");
    println!("   • Transport abstraction");
    println!("   • Reconnection support");
    println!("   • Type-safe channel creation\n");

    println!("⚠️  Current Status:");
    println!("   The EndpointBuilder API is designed and partially implemented.");
    println!("   The connection acceptance loop (endpoint.accept()) is the");
    println!("   missing piece that would complete the high-level API.\n");

    Ok(())
}

// ============================================================================
// APPROACH 2: Low-Level with TcpTransport (CURRENT WORKAROUND)
// ============================================================================

async fn approach_2_tcp_transport() -> Result<(), Box<dyn Error>> {
    println!("═══════════════════════════════════════════════════════════");
    println!("APPROACH 2: Using TcpTransport Directly (Current Workaround)");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("📋 Step 1: Bind TCP listener directly");
    println!("```rust");
    println!("let listener = TcpTransport::bind(\"127.0.0.1:9090\").await?;");
    println!("```\n");

    let listener = TcpTransport::bind("127.0.0.1:9091").await?;
    println!("✅ TCP listener bound to 127.0.0.1:9091\n");

    println!("📋 Step 2: Accept connections manually");
    println!("```rust");
    println!("loop {{");
    println!("    let (_transport, peer_addr) = listener.accept().await?;");
    println!("    ");
    println!("    // Create channels manually");
    println!("    let channel_id = ChannelId::new();");
    println!("    let (sender, receiver) = Channel::<Protocol>::new_in_memory(channel_id, 10);");
    println!("    ");
    println!("    // Spawn handler");
    println!("    tokio::spawn(handle_client(sender, receiver));");
    println!("}}");
    println!("```\n");

    // Accept one connection for demo
    println!("⏳ Waiting for one connection (with timeout)...");

    tokio::select! {
        result = listener.accept() => {
            match result {
                Ok((_transport, peer_addr)) => {
                    println!("✅ Connection accepted from {}\n", peer_addr);

                    // Create channels manually
                    let channel_id = ChannelId::new();
                    let (_sender, _receiver) = Channel::<MessageProtocol>::new_in_memory(channel_id, 10);

                    println!("✅ Channels created manually for client\n");
                }
                Err(e) => {
                    println!("❌ Accept failed: {}\n", e);
                }
            }
        }
        _ = tokio::time::sleep(tokio::time::Duration::from_secs(2)) => {
            println!("⏱️  Timeout - no connection received\n");
        }
    }

    println!("💡 Characteristics:");
    println!("   • Direct control over transport");
    println!("   • Manual channel management");
    println!("   • No automatic protocol negotiation");
    println!("   • Requires manual handshake");
    println!("   • More boilerplate code\n");

    println!("⚠️  Limitations:");
    println!("   • No integration with Endpoint features");
    println!("   • Manual protocol version handling");
    println!("   • No automatic strategy");
    println!("   • More error-prone\n");

    Ok(())
}

// ============================================================================
// APPROACH 3: Hybrid (Bridge Pattern)
// ============================================================================

async fn approach_3_hybrid() -> Result<(), EndpointError> {
    println!("═══════════════════════════════════════════════════════════");
    println!("APPROACH 3: Hybrid Approach (Bridge Pattern)");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("This approach uses EndpointBuilder for configuration but");
    println!("manually manages the accept loop until the API is complete.\n");

    println!("📋 Step 1: Create endpoint with EndpointBuilder");
    let _endpoint = EndpointBuilder::server(PostcardSerializer::default())
        .with_responder("GatewayRPCProtocol", 1)
        .with_bidirectional("SessionMessageProtocol", 1)
        .with_tcp_listener("127.0.0.1:9092")
        .build()
        .await?;

    println!("✅ Endpoint configured\n");

    println!("📋 Step 2: Access transport manager for manual accept");
    println!("```rust");
    println!("// Get the transport manager from endpoint");
    println!("let transport_manager = endpoint.transport_manager();");
    println!("");
    println!("// Manually accept connections");
    println!("// (This would require additional transport manager API)");
    println!("```\n");

    println!("💡 This approach would:");
    println!("   • Use EndpointBuilder for configuration");
    println!("   • Leverage protocol registration");
    println!("   • Manually handle accept loop");
    println!("   • Bridge to full Endpoint API when ready\n");

    Ok(())
}

// ============================================================================
// Main Entry Point
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("\n🎯 BDRPC Server Setup Guide");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("This guide explains three approaches to setting up a server:\n");
    println!("1. High-Level: EndpointBuilder (recommended, partially implemented)");
    println!("2. Low-Level: TcpTransport (current workaround)");
    println!("3. Hybrid: Bridge pattern (transitional)\n");

    // Show Approach 1
    if let Err(e) = approach_1_endpoint_builder().await {
        eprintln!("Approach 1 error: {}", e);
    }

    // Show Approach 2
    if let Err(e) = approach_2_tcp_transport().await {
        eprintln!("Approach 2 error: {}", e);
    }

    // Show Approach 3
    if let Err(e) = approach_3_hybrid().await {
        eprintln!("Approach 3 error: {}", e);
    }

    println!("═══════════════════════════════════════════════════════════");
    println!("📊 Summary");
    println!("═══════════════════════════════════════════════════════════\n");

    println!("✅ What Works Today:");
    println!("   • EndpointBuilder configuration");
    println!("   • Protocol registration");
    println!("   • Transport configuration");
    println!("   • TcpTransport.bind() and accept()");
    println!("   • Manual channel creation\n");

    println!("🚧 What's Missing:");
    println!("   • endpoint.accept() implementation");
    println!("   • Automatic channel creation from endpoint");
    println!("   • Integration between transport manager and channels");
    println!("   • Automatic protocol negotiation on accept\n");

    println!("📖 Recommended Pattern (Today):");
    println!("   1. Use EndpointBuilder to configure protocols");
    println!("   2. Use TcpTransport.bind() for listener");
    println!("   3. Manually accept() connections");
    println!("   4. Create channels with Channel::new_in_memory()");
    println!("   5. Spawn handlers per protocol type\n");

    println!("🔮 Future Pattern (When Complete):");
    println!("   1. Use EndpointBuilder with .with_tcp_listener()");
    println!("   2. Call endpoint.accept() in loop");
    println!("   3. Use endpoint.get_channels() for typed channels");
    println!("   4. Spawn handlers - everything else automatic\n");

    println!("📚 See Also:");
    println!("   • tcp_gateway_server.rs - Working TCP server example");
    println!("   • gateway_server_example.rs - Conceptual endpoint pattern");
    println!("   • calculator_manual.rs - Simple TCP example\n");

    Ok(())
}

// Made with Bob

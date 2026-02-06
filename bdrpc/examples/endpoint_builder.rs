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

//! Example demonstrating the EndpointBuilder pattern.
//!
//! This example shows how to use the builder pattern to create endpoints
//! with protocols pre-registered, making the code more ergonomic and readable.

use bdrpc::endpoint::{EndpointBuilder, EndpointConfig};
use bdrpc::serialization::PostcardSerializer;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== EndpointBuilder Examples ===\n");

    // Example 1: Basic client with default configuration
    println!("1. Basic Client:");
    let client = EndpointBuilder::new(PostcardSerializer::default())
        .with_caller("UserService", 1)
        .with_caller("OrderService", 1)
        .build()
        .await?;
    println!("   Created client endpoint: {}", client.id());
    println!(
        "   Registered protocols: {:?}",
        client.capabilities().await.len()
    );
    println!();

    // Example 2: Server with preset configuration
    println!("2. Server with Preset:");
    let server = EndpointBuilder::server(PostcardSerializer::default())
        .with_responder("UserService", 1)
        .with_responder("OrderService", 1)
        .build()
        .await?;
    println!("   Created server endpoint: {}", server.id());
    println!("   Max connections: {:?}", "1000 (from preset)");
    println!();

    // Example 3: Peer-to-peer with preset
    println!("3. Peer-to-Peer with Preset:");
    let peer = EndpointBuilder::peer(PostcardSerializer::default())
        .with_bidirectional("ChatProtocol", 1)
        .with_bidirectional("FileTransfer", 1)
        .build()
        .await?;
    println!("   Created peer endpoint: {}", peer.id());
    println!("   Channel buffer size: 200 (from preset)");
    println!();

    // Example 4: Custom configuration with builder
    println!("4. Custom Configuration:");
    let custom = EndpointBuilder::new(PostcardSerializer::default())
        .configure(|config| {
            config
                .with_channel_buffer_size(500)
                .with_handshake_timeout(Duration::from_secs(15))
                .with_max_connections(Some(50))
                .with_endpoint_id("my-custom-endpoint".to_string())
        })
        .with_caller("UserService", 1)
        .with_responder("NotificationService", 1)
        .build()
        .await?;
    println!("   Created custom endpoint: {}", custom.id());
    println!("   Custom buffer size: 500");
    println!("   Custom timeout: 15s");
    println!();

    // Example 5: Using with_config for advanced configuration
    println!("5. Advanced Configuration:");
    let config = EndpointConfig::new()
        .with_channel_buffer_size(1000)
        .with_handshake_timeout(Duration::from_secs(10))
        .with_max_frame_size(32 * 1024 * 1024) // 32 MB
        .with_frame_integrity(true)
        .with_endpoint_id("advanced-endpoint".to_string());

    let advanced = EndpointBuilder::with_config(PostcardSerializer::default(), config)
        .with_caller("UserService", 1)
        .with_responder("NotificationService", 1)
        .with_bidirectional("ChatProtocol", 1)
        .build()
        .await?;
    println!("   Created advanced endpoint: {}", advanced.id());
    println!("   Protocols: {}", advanced.capabilities().await.len());
    println!();

    // Example 6: Hybrid endpoint (different directions for different protocols)
    println!("6. Hybrid Endpoint:");
    let hybrid = EndpointBuilder::new(PostcardSerializer::default())
        .with_caller("UserService", 1) // Can call UserService
        .with_responder("NotificationService", 1) // Can respond to NotificationService
        .with_bidirectional("ChatProtocol", 1) // Can both call and respond to ChatProtocol
        .build()
        .await?;
    println!("   Created hybrid endpoint: {}", hybrid.id());
    println!("   - UserService: CallOnly");
    println!("   - NotificationService: RespondOnly");
    println!("   - ChatProtocol: Bidirectional");
    println!();

    // Example 7: Client preset with additional configuration
    println!("7. Client Preset + Custom Config:");
    let client_custom = EndpointBuilder::client(PostcardSerializer::default())
        .configure(|config| config.with_channel_buffer_size(2000))
        .with_caller("UserService", 1)
        .with_caller("OrderService", 1)
        .build()
        .await?;
    println!(
        "   Created client with custom buffer: {}",
        client_custom.id()
    );
    println!();

    // Example 8: Comparison with manual approach
    println!("8. Comparison - Manual vs Builder:");
    println!("   Manual approach (old way):");
    println!("   ```rust");
    println!("   let mut endpoint = Endpoint::new(serializer, config);");
    println!("   endpoint.register_caller(\"UserService\", 1).await?;");
    println!("   endpoint.register_caller(\"OrderService\", 1).await?;");
    println!("   ```");
    println!();
    println!("   Builder approach (new way):");
    println!("   ```rust");
    println!("   let endpoint = EndpointBuilder::new(serializer)");
    println!("       .with_caller(\"UserService\", 1)");
    println!("       .with_caller(\"OrderService\", 1)");
    println!("       .build().await?;");
    println!("   ```");
    println!();

    println!("=== Benefits of EndpointBuilder ===");
    println!("✓ More ergonomic and readable");
    println!("✓ Protocols registered during construction");
    println!("✓ Preset configurations for common use cases");
    println!("✓ Method chaining for fluent API");
    println!("✓ Compile-time protocol registration");
    println!("✓ Less boilerplate code");

    Ok(())
}

// Made with Bob

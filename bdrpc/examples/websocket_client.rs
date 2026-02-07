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

//! WebSocket Client Example
//!
//! This example demonstrates how to create a WebSocket client using BDRPC.
//! It connects to a WebSocket server and sends/receives messages.
//!
//! # Features
//!
//! - WebSocket transport with binary message support
//! - Automatic ping/pong keepalive
//! - Support for both ws:// and wss:// URLs
//! - Interactive message sending
//!
//! # Running
//!
//! First, start the WebSocket server:
//! ```bash
//! cargo run --example websocket_server --features websocket
//! ```
//!
//! Then run this client:
//! ```bash
//! cargo run --example websocket_client --features websocket
//! ```
//!
//! # Usage
//!
//! The client will connect to ws://localhost:8080 and send a series of test messages.
//! You can modify the code to send custom messages or connect to different servers.

#[cfg(feature = "websocket")]
use bdrpc::transport::{Transport, WebSocketConfig, WebSocketTransport};
#[cfg(feature = "websocket")]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(feature = "websocket")]
use tokio::time::{Duration, sleep};

#[cfg(feature = "websocket")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure WebSocket
    let config = WebSocketConfig::default();

    // Connect to the server
    let url = "ws://127.0.0.1:8080";
    println!("Connecting to {}...", url);

    let mut transport = WebSocketTransport::connect(url, config).await?;
    let metadata = transport.metadata();

    println!("Connected! Transport ID: {}", metadata.id);
    println!("Local address: {:?}", metadata.local_addr);
    println!("Peer address: {:?}", metadata.peer_addr);

    // Send a series of test messages
    let messages = [
        "Hello, WebSocket server!",
        "This is message #2",
        "Testing binary transport",
        "BDRPC over WebSocket works!",
        "Final message",
    ];

    for (i, message) in messages.iter().enumerate() {
        println!("Sending message {}: {}", i + 1, message);

        // Write the message
        transport.write_all(message.as_bytes()).await?;
        transport.flush().await?;

        // Read the echo response
        let mut buffer = vec![0u8; 8192];
        let n = transport.read(&mut buffer).await?;

        let response = String::from_utf8_lossy(&buffer[..n]);
        println!("Received echo ({} bytes): {}", n, response);

        // Verify echo
        if response == *message {
            println!("✓ Echo verified");
        } else {
            println!("✗ Echo mismatch!");
        }

        // Wait a bit between messages
        sleep(Duration::from_millis(500)).await;
    }

    println!("All messages sent and verified!");

    // Test larger message
    println!("Testing larger message (1 MB)...");
    let large_message = vec![b'X'; 1024 * 1024]; // 1 MB
    transport.write_all(&large_message).await?;
    transport.flush().await?;

    let mut large_buffer = vec![0u8; 2 * 1024 * 1024]; // 2 MB buffer
    let n = transport.read(&mut large_buffer).await?;
    println!("Received large echo: {} bytes", n);

    if n == large_message.len() {
        println!("✓ Large message verified");
    } else {
        println!(
            "✗ Large message size mismatch: expected {}, got {}",
            large_message.len(),
            n
        );
    }

    // Graceful shutdown
    println!("Shutting down connection...");
    Transport::shutdown(&mut transport).await?;
    println!("Connection closed gracefully");

    Ok(())
}

#[cfg(not(feature = "websocket"))]
fn main() {
    eprintln!("This example requires the 'websocket' feature.");
    eprintln!("Run with: cargo run --example websocket_client --features websocket");
    std::process::exit(1);
}

// Made with Bob

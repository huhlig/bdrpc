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

//! WebSocket Server Example
//!
//! This example demonstrates how to create a WebSocket server using BDRPC.
//! It accepts WebSocket connections and echoes back any messages received.
//!
//! # Features
//!
//! - WebSocket transport with binary message support
//! - Automatic ping/pong keepalive
//! - Graceful connection handling
//! - Compatible with browser WebSocket clients
//!
//! # Running
//!
//! ```bash
//! cargo run --example websocket_server --features websocket
//! ```
//!
//! # Testing with a Browser
//!
//! Open your browser's developer console and run:
//!
//! ```javascript
//! const ws = new WebSocket('ws://localhost:8080');
//! ws.binaryType = 'arraybuffer';
//! ws.onopen = () => {
//!     console.log('Connected');
//!     // Send a simple message
//!     const data = new TextEncoder().encode('Hello from browser!');
//!     ws.send(data);
//! };
//! ws.onmessage = (event) => {
//!     const data = new Uint8Array(event.data);
//!     const text = new TextDecoder().decode(data);
//!     console.log('Received:', text);
//! };
//! ```

#[cfg(feature = "websocket")]
use bdrpc::transport::{Transport, WebSocketConfig, WebSocketListener};
#[cfg(feature = "websocket")]
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[cfg(feature = "websocket")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure WebSocket with reasonable defaults
    let config = WebSocketConfig {
        max_frame_size: 16 * 1024 * 1024,      // 16 MB
        max_message_size: 64 * 1024 * 1024,    // 64 MB
        compression: false,                     // Disabled for performance
        ping_interval: std::time::Duration::from_secs(30),
        pong_timeout: std::time::Duration::from_secs(10),
        accept_unmasked_frames: false,
    };

    // Bind to localhost
    let listener = WebSocketListener::bind("127.0.0.1:8080", config).await?;
    let addr = listener.local_addr()?;

    println!("WebSocket server listening on ws://{}", addr);
    println!("Press Ctrl+C to stop");
    println!();
    println!("Test with browser console:");
    println!("  const ws = new WebSocket('ws://localhost:8080');");
    println!("  ws.binaryType = 'arraybuffer';");
    println!("  ws.onopen = () => ws.send(new TextEncoder().encode('Hello!'));");
    println!("  ws.onmessage = (e) => console.log(new TextDecoder().decode(e.data));");

    // Accept connections in a loop
    loop {
        match listener.accept().await {
            Ok(transport) => {
                let metadata = transport.metadata();
                println!(
                    "Accepted WebSocket connection from {:?}",
                    metadata.peer_addr
                );

                // Spawn a task to handle this connection
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(transport).await {
                        eprintln!("Connection error: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("Failed to accept connection: {}", e);
            }
        }
    }
}

#[cfg(feature = "websocket")]
/// Handle a single WebSocket connection.
///
/// This function reads messages from the client and echoes them back.
async fn handle_connection(mut transport: impl Transport) -> Result<(), Box<dyn std::error::Error>> {
    let connection_id = transport.metadata().id;
    println!("Handling connection {}", connection_id);

    let mut buffer = vec![0u8; 8192];
    let mut message_count = 0u64;

    loop {
        // Read data from the WebSocket
        match transport.read(&mut buffer).await {
            Ok(0) => {
                // Connection closed
                println!(
                    "Connection {} closed after {} messages",
                    connection_id,
                    message_count
                );
                break;
            }
            Ok(n) => {
                message_count += 1;

                // Log the received message
                let message = String::from_utf8_lossy(&buffer[..n]);
                println!(
                    "Connection {} received ({} bytes): {}",
                    connection_id,
                    n,
                    message
                );

                // Echo the message back
                transport.write_all(&buffer[..n]).await?;
                transport.flush().await?;

                println!("Connection {} echoed {} bytes", connection_id, n);
            }
            Err(e) => {
                eprintln!("Connection {} read error: {}", connection_id, e);
                break;
            }
        }
    }

    // Graceful shutdown
    Transport::shutdown(&mut transport).await?;
    println!("Connection {} shutdown complete", connection_id);

    Ok(())
}

#[cfg(not(feature = "websocket"))]
fn main() {
    eprintln!("This example requires the 'websocket' feature.");
    eprintln!("Run with: cargo run --example websocket_server --features websocket");
    std::process::exit(1);
}

// Made with Bob

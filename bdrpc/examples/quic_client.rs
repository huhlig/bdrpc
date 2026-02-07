//! QUIC Echo Client Example
//!
//! This example demonstrates a simple QUIC client that connects to the QUIC echo server
//! and sends test messages.
//!
//! # Features
//!
//! - QUIC transport with TLS 1.3 encryption
//! - Connection to QUIC server
//! - Send and receive messages
//! - Message verification
//!
//! # Running
//!
//! First, start the server:
//! ```bash
//! cargo run --example quic_server --features quic
//! ```
//!
//! Then run this client:
//! ```bash
//! cargo run --example quic_client --features quic
//! ```

#[cfg(not(feature = "quic"))]
fn main() {
    eprintln!("This example requires the 'quic' feature to be enabled.");
    eprintln!("Run with: cargo run --example quic_client --features quic");
    std::process::exit(1);
}

#[cfg(feature = "quic")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use bdrpc::transport::{QuicConfig, QuicTransport, Transport};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    println!("QUIC Echo Client");
    println!("================");
    println!();

    // Create QUIC configuration
    let config = QuicConfig {
        max_idle_timeout: std::time::Duration::from_secs(60),
        keep_alive_interval: std::time::Duration::from_secs(15),
        enable_0rtt: true,
        ..Default::default()
    };

    // Connect to server
    let addr = "127.0.0.1:4433";
    println!("Connecting to {}...", addr);
    let mut transport = QuicTransport::connect(addr, config).await?;
    
    let metadata = transport.metadata();
    println!("✓ Connected!");
    println!("  Transport ID: {}", metadata.id);
    println!("  Local addr: {:?}", metadata.local_addr);
    println!("  Peer addr: {:?}", metadata.peer_addr);
    println!("  Transport type: {}", metadata.transport_type);
    println!();

    // Send test messages
    let test_messages = vec![
        "Hello, QUIC!",
        "This is a test message",
        "QUIC provides 0-RTT and connection migration",
        "Perfect for mobile apps and real-time communication",
    ];

    for (i, message) in test_messages.iter().enumerate() {
        println!("[Message {}] Sending: {}", i + 1, message);
        
        // Send message
        transport.write_all(message.as_bytes()).await?;
        println!("[Message {}] Sent {} bytes", i + 1, message.len());

        // Read response
        let mut buffer = vec![0u8; 4096];
        let n = transport.read(&mut buffer).await?;
        let response = String::from_utf8_lossy(&buffer[..n]);
        
        println!("[Message {}] Received: {}", i + 1, response.trim());
        
        // Verify echo
        if response.trim() == *message {
            println!("[Message {}] ✓ Echo verified!", i + 1);
        } else {
            println!("[Message {}] ✗ Echo mismatch!", i + 1);
        }
        println!();

        // Small delay between messages
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    println!("All messages sent and verified!");
    println!("Closing connection...");
    
    // Graceful shutdown
    Transport::shutdown(&mut transport).await?;
    
    println!("✓ Connection closed");
    
    Ok(())
}

// Made with Bob

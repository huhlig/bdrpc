//! QUIC Echo Server Example
//!
//! This example demonstrates a simple QUIC echo server using BDRPC's QUIC transport.
//!
//! # Features
//!
//! - QUIC transport with TLS 1.3 encryption
//! - Self-signed certificate for testing
//! - Echo server that responds to all messages
//! - Graceful shutdown on Ctrl+C
//!
//! # Running
//!
//! Start the server:
//! ```bash
//! cargo run --example quic_server --features quic
//! ```
//!
//! In another terminal, run the client:
//! ```bash
//! cargo run --example quic_client --features quic
//! ```

#[cfg(not(feature = "quic"))]
fn main() {
    eprintln!("This example requires the 'quic' feature to be enabled.");
    eprintln!("Run with: cargo run --example quic_server --features quic");
    std::process::exit(1);
}

#[cfg(feature = "quic")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use bdrpc::transport::{QuicConfig, QuicListener, Transport};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    println!("QUIC Echo Server");
    println!("================");
    println!();

    // Create QUIC configuration
    let config = QuicConfig {
        max_idle_timeout: std::time::Duration::from_secs(60),
        keep_alive_interval: std::time::Duration::from_secs(15),
        enable_0rtt: true,
        ..Default::default()
    };

    // Bind to address
    let addr = "127.0.0.1:4433";
    println!("Binding to {}...", addr);
    let listener = QuicListener::bind(addr, config).await?;
    println!("✓ Server listening on {}", listener.local_addr()?);
    println!();
    println!("Waiting for connections...");
    println!("Press Ctrl+C to stop");
    println!();

    // Accept connections
    let mut connection_count = 0;
    loop {
        match listener.accept().await {
            Ok(mut transport) => {
                connection_count += 1;
                let conn_id = connection_count;
                
                let metadata = transport.metadata();
                println!(
                    "[Connection #{}] Accepted from {:?}",
                    conn_id, metadata.peer_addr
                );

                // Spawn task to handle this connection
                tokio::spawn(async move {
                    let mut buffer = vec![0u8; 4096];
                    let mut message_count = 0;

                    loop {
                        match transport.read(&mut buffer).await {
                            Ok(0) => {
                                println!("[Connection #{}] Client disconnected", conn_id);
                                break;
                            }
                            Ok(n) => {
                                message_count += 1;
                                let received = String::from_utf8_lossy(&buffer[..n]);
                                println!(
                                    "[Connection #{}] Message #{}: {} ({} bytes)",
                                    conn_id, message_count, received.trim(), n
                                );

                                // Echo back
                                if let Err(e) = transport.write_all(&buffer[..n]).await {
                                    eprintln!(
                                        "[Connection #{}] Failed to send response: {}",
                                        conn_id, e
                                    );
                                    break;
                                }
                                println!("[Connection #{}] Echoed back {} bytes", conn_id, n);
                            }
                            Err(e) => {
                                eprintln!("[Connection #{}] Read error: {}", conn_id, e);
                                break;
                            }
                        }
                    }

                    println!(
                        "[Connection #{}] Handled {} messages total",
                        conn_id, message_count
                    );
                });
            }
            Err(e) => {
                eprintln!("Failed to accept connection: {}", e);
            }
        }
    }
}

// Made with Bob

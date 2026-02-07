//! Echo server using the listener pattern with `add_listener()`.
//!
//! This example demonstrates the modern API for servers using the transport manager.
//! Connections are handled automatically via the TransportEventHandler.
//!
//! Run the server:
//! ```bash
//! cargo run --example echo_server_manual --features=tracing
//! ```
//!
//! Then connect with a client (in another terminal):
//! ```bash
//! cargo run --example echo_client
//! ```

use bdrpc::channel::Protocol;
use bdrpc::endpoint::EndpointBuilder;
use bdrpc::serialization::JsonSerializer;
use bdrpc::transport::{TransportConfig, TransportType};

/// Simple echo protocol - messages are echoed back to the sender.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[allow(dead_code)]
enum EchoProtocol {
    /// A message to be echoed
    Echo(String),
}

impl Protocol for EchoProtocol {
    fn method_name(&self) -> &'static str {
        "echo"
    }

    fn is_request(&self) -> bool {
        true
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create endpoint using EndpointBuilder with protocol pre-registered
    let mut endpoint = EndpointBuilder::server(JsonSerializer::default())
        .with_bidirectional("Echo", 1)
        .build()
        .await?;

    // Add a TCP listener using the new transport manager API
    let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080");
    endpoint
        .add_listener("tcp-listener".to_string(), config)
        .await?;

    println!("Echo server listening on 127.0.0.1:8080");
    println!("Waiting for connections...");
    println!("Note: Connections are handled automatically by the transport manager");
    println!("Press Ctrl+C to stop the server");

    // Keep the server running
    // In a real application, you would handle incoming messages through
    // the channel manager or use a service-based approach
    tokio::signal::ctrl_c().await?;
    println!("\nShutting down server...");

    Ok(())
}

// Made with Bob

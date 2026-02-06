//! Echo server using the manual acceptance pattern with `listen_manual()` and `accept_channels()`.
//!
//! This example demonstrates the simpler API for servers that want to handle
//! connections one at a time, similar to `TcpListener::accept()`.
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
use bdrpc::endpoint::{Endpoint, EndpointConfig};
use bdrpc::serialization::JsonSerializer;

/// Simple echo protocol - messages are echoed back to the sender.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
    // Create endpoint and register the echo protocol
    let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    endpoint.register_bidirectional("Echo", 1).await?;

    // Start listening in manual mode
    let mut listener = endpoint.listen_manual("127.0.0.1:8080").await?;
    println!("Echo server listening on {}", listener.local_addr());
    println!("Waiting for connections...");

    // Accept connections one at a time
    loop {
        // This blocks until a connection arrives, performs handshake,
        // and returns typed channels ready to use
        let (sender, mut receiver) = listener.accept_channels::<EchoProtocol>().await?;
        
        println!("New connection accepted!");

        // Spawn a task to handle this connection
        tokio::spawn(async move {
            let mut message_count = 0;
            
            // Echo messages back to the client
            while let Some(msg) = receiver.recv().await {
                message_count += 1;
                
                match &msg {
                    EchoProtocol::Echo(text) => {
                        println!("Received message #{}: {}", message_count, text);
                        
                        // Echo it back
                        if let Err(e) = sender.send(msg).await {
                            eprintln!("Failed to send echo: {}", e);
                            break;
                        }
                    }
                }
            }
            
            println!("Connection closed after {} messages", message_count);
        });
    }
}

// Made with Bob

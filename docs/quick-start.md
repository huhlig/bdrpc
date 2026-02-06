# BDRPC Quick Start Guide

Welcome to BDRPC! This guide will help you get started with building bi-directional RPC applications in Rust.

## Installation

Add BDRPC to your `Cargo.toml`:

```toml
[dependencies]
bdrpc = "0.1"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
```

## Your First BDRPC Application

Let's build a simple calculator service that demonstrates the core concepts of BDRPC.

### Step 1: Define Your Protocol

First, define the messages your service will exchange:

```rust
use serde::{Deserialize, Serialize};
use bdrpc::channel::Protocol;

// Request message
#[derive(Serialize, Deserialize, Debug)]
pub struct CalculateRequest {
    pub operation: String,
    pub a: i32,
    pub b: i32,
}

// Response message
#[derive(Serialize, Deserialize, Debug)]
pub struct CalculateResponse {
    pub result: i32,
}

// Protocol definition
pub struct CalculatorProtocol;

impl Protocol for CalculatorProtocol {
    type Request = CalculateRequest;
    type Response = CalculateResponse;
    
    fn name() -> &'static str {
        "Calculator"
    }
}
```

### Step 2: Create a Server

Build a server that responds to calculation requests:

```rust
use bdrpc::endpoint::{Endpoint, EndpointConfig, ProtocolDirection};
use bdrpc::serialization::PostcardSerializer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create server endpoint
    let config = EndpointConfig::default();
    let mut server = Endpoint::new(PostcardSerializer::default(), config);
    
    // Register as responder
    server.register_protocol::<CalculatorProtocol>(
        ProtocolDirection::RespondOnly
    )?;
    
    // Listen for connections
    let mut listener = server.listen("127.0.0.1:8080").await?;
    println!("Calculator server listening on 127.0.0.1:8080");
    
    // Accept connections
    while let Ok(connection) = listener.accept().await {
        tokio::spawn(async move {
            // Accept channel for this protocol
            let channel = connection
                .accept_channel::<CalculatorProtocol>()
                .await?;
            
            // Handle requests
            while let Ok(request) = channel.recv().await {
                let result = match request.operation.as_str() {
                    "add" => request.a + request.b,
                    "subtract" => request.a - request.b,
                    "multiply" => request.a * request.b,
                    "divide" => request.a / request.b,
                    _ => 0,
                };
                
                let response = CalculateResponse { result };
                channel.send(response).await?;
            }
            
            Ok::<_, Box<dyn std::error::Error>>(())
        });
    }
    
    Ok(())
}
```

### Step 3: Create a Client

Build a client that calls the calculator service:

```rust
use bdrpc::endpoint::{Endpoint, EndpointConfig, ProtocolDirection};
use bdrpc::serialization::PostcardSerializer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create client endpoint
    let config = EndpointConfig::default();
    let mut client = Endpoint::new(PostcardSerializer::default(), config);
    
    // Register as caller
    client.register_protocol::<CalculatorProtocol>(
        ProtocolDirection::CallOnly
    )?;
    
    // Connect to server
    let connection = client.connect("127.0.0.1:8080").await?;
    println!("Connected to calculator server");
    
    // Create channel
    let channel = connection.create_channel::<CalculatorProtocol>().await?;
    
    // Make some calculations
    let request = CalculateRequest {
        operation: "add".to_string(),
        a: 10,
        b: 5,
    };
    
    let response = channel.send(request).await?;
    println!("10 + 5 = {}", response.result);
    
    let request = CalculateRequest {
        operation: "multiply".to_string(),
        a: 7,
        b: 6,
    };
    
    let response = channel.send(request).await?;
    println!("7 * 6 = {}", response.result);
    
    Ok(())
}
```

## Key Concepts

### 1. Protocol

A protocol defines the message types exchanged between endpoints:

```rust
impl Protocol for MyProtocol {
    type Request = MyRequest;
    type Response = MyResponse;
    fn name() -> &'static str { "MyProtocol" }
}
```

### 2. Endpoint

An endpoint manages connections and channels:

```rust
let endpoint = Endpoint::new(serializer, config);
```

### 3. Protocol Direction

Specify whether an endpoint can call, respond, or both:

- `CallOnly`: Can send requests, receive responses
- `RespondOnly`: Can receive requests, send responses  
- `Bidirectional`: Can both call and respond

### 4. Connection

A connection represents a link to another endpoint:

```rust
// Client side
let connection = endpoint.connect("127.0.0.1:8080").await?;

// Server side
let connection = listener.accept().await?;
```

### 5. Channel

A channel provides typed message passing for a specific protocol:

```rust
// Create channel (client)
let channel = connection.create_channel::<MyProtocol>().await?;

// Accept channel (server)
let channel = connection.accept_channel::<MyProtocol>().await?;

// Send and receive
channel.send(request).await?;
let response = channel.recv().await?;
```

## Common Patterns

### Request-Response Pattern

The most common pattern where client sends requests and server responds:

```rust
// Client
let response = channel.send(request).await?;

// Server
let request = channel.recv().await?;
channel.send(response).await?;
```

### Server Push Pattern

Server initiates communication to client:

```rust
// Server registers as CallOnly
server.register_protocol::<NotificationProtocol>(
    ProtocolDirection::CallOnly
)?;

// Client registers as RespondOnly
client.register_protocol::<NotificationProtocol>(
    ProtocolDirection::RespondOnly
)?;
```

### Peer-to-Peer Pattern

Both sides can initiate communication:

```rust
// Both register as Bidirectional
endpoint.register_protocol::<ChatProtocol>(
    ProtocolDirection::Bidirectional
)?;
```

## Error Handling

BDRPC provides a comprehensive error hierarchy:

```rust
use bdrpc::BdrpcError;

match channel.send(request).await {
    Ok(response) => println!("Success: {:?}", response),
    Err(BdrpcError::Transport(e)) => {
        eprintln!("Transport error: {}", e);
        // Connection lost, may need to reconnect
    }
    Err(BdrpcError::Channel(e)) => {
        eprintln!("Channel error: {}", e);
        // Channel closed, but connection may still be alive
    }
    Err(e) => eprintln!("Other error: {}", e),
}
```

## Configuration

Customize endpoint behavior with `EndpointConfig`:

```rust
use std::time::Duration;

let config = EndpointConfig::new()
    .with_channel_buffer_size(1000)
    .with_handshake_timeout(Duration::from_secs(10))
    .with_max_connections(Some(100))
    .with_max_frame_size(32 * 1024 * 1024);
```

## Serialization

BDRPC supports multiple serialization formats:

```rust
// Postcard (recommended, compact binary)
use bdrpc::serialization::PostcardSerializer;
let serializer = PostcardSerializer::default();

// JSON (human-readable, debugging)
use bdrpc::serialization::JsonSerializer;
let serializer = JsonSerializer::default();
```

## Reconnection

Enable automatic reconnection for resilient clients:

```rust
use bdrpc::reconnection::ExponentialBackoff;
use std::sync::Arc;
use std::time::Duration;

let config = EndpointConfig::new()
    .with_reconnection_strategy(Arc::new(
        ExponentialBackoff::new(
            Duration::from_secs(1),   // Initial delay
            Duration::from_secs(30),  // Max delay
            2.0,                      // Multiplier
            Some(5),                  // Max attempts
        )
    ));

let connection = endpoint
    .connect_with_reconnection("127.0.0.1:8080")
    .await?;
```

## Next Steps

Now that you understand the basics, explore:

- **[Examples](../bdrpc/examples/)**: Working code examples
- **[Architecture Guide](architecture-guide.md)**: Deep dive into BDRPC design
- **[Performance Guide](performance-guide.md)**: Optimization tips
- **[API Documentation](https://docs.rs/bdrpc)**: Complete API reference

## Getting Help

- **GitHub Issues**: Report bugs or request features
- **Discussions**: Ask questions and share ideas
- **Examples**: Check the examples directory for more patterns

## Common Issues

### Connection Refused

Make sure the server is running and listening on the correct address:

```rust
// Server
listener = server.listen("127.0.0.1:8080").await?;

// Client
connection = client.connect("127.0.0.1:8080").await?;
```

### Protocol Not Found

Ensure both sides register the same protocol:

```rust
// Both client and server must register
endpoint.register_protocol::<MyProtocol>(direction)?;
```

### Serialization Errors

Ensure your types implement `Serialize` and `Deserialize`:

```rust
#[derive(Serialize, Deserialize)]
struct MyMessage {
    field: String,
}
```

### Timeout Errors

Use timeout methods to prevent deadlocks:

```rust
use std::time::Duration;

// Send with timeout
channel.send_timeout(request, Duration::from_secs(5)).await?;

// Receive with timeout
let response = channel.recv_timeout(Duration::from_secs(5)).await?;
```

## Summary

You've learned:

- ✅ How to define protocols
- ✅ How to create servers and clients
- ✅ How to send and receive messages
- ✅ Common communication patterns
- ✅ Error handling strategies
- ✅ Configuration options

Ready to build production applications with BDRPC!
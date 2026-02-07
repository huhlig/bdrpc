# BDRPC Quick Start Guide

Welcome to BDRPC! This guide will help you get started with building bi-directional RPC applications in Rust using modern patterns and best practices.

## Installation

Add BDRPC to your `Cargo.toml`:

```toml
[dependencies]
bdrpc = "0.1"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
async-trait = "0.1"
```

## Your First BDRPC Application

Let's build a simple calculator service that demonstrates the modern BDRPC patterns using the `#[bdrpc::service]` macro and `EndpointBuilder`.

### Step 1: Define Your Service

Use the `#[bdrpc::service]` macro to define your service as a trait:

```rust
use bdrpc::service;

/// Calculator service demonstrating the #[bdrpc::service] macro.
///
/// This macro generates:
/// - CalculatorProtocol enum (with request/response variants)
/// - CalculatorClient struct (for making RPC calls)
/// - CalculatorServer trait (to implement)
/// - CalculatorDispatcher struct (for routing requests)
#[service(direction = "bidirectional")]
trait Calculator {
    /// Add two numbers
    async fn add(&self, a: i32, b: i32) -> Result<i32, String>;
    
    /// Subtract two numbers
    async fn subtract(&self, a: i32, b: i32) -> Result<i32, String>;
    
    /// Multiply two numbers
    async fn multiply(&self, a: i32, b: i32) -> Result<i32, String>;
    
    /// Divide two numbers (can fail with division by zero)
    async fn divide(&self, a: i32, b: i32) -> Result<i32, String>;
}
```

### Step 2: Implement the Server

Implement the generated `CalculatorServer` trait:

```rust
use async_trait::async_trait;

/// Our calculator implementation
struct MyCalculator;

#[async_trait]
impl CalculatorServer for MyCalculator {
    async fn add(&self, a: i32, b: i32) -> Result<i32, String> {
        println!("Computing: {} + {} = {}", a, b, a + b);
        Ok(a + b)
    }
    
    async fn subtract(&self, a: i32, b: i32) -> Result<i32, String> {
        println!("Computing: {} - {} = {}", a, b, a - b);
        Ok(a - b)
    }
    
    async fn multiply(&self, a: i32, b: i32) -> Result<i32, String> {
        println!("Computing: {} × {} = {}", a, b, a * b);
        Ok(a * b)
    }
    
    async fn divide(&self, a: i32, b: i32) -> Result<i32, String> {
        if b == 0 {
            Err("Division by zero".to_string())
        } else {
            println!("Computing: {} ÷ {} = {}", a, b, a / b);
            Ok(a / b)
        }
    }
}
```

### Step 3: Create a Server with EndpointBuilder

Use the `EndpointBuilder` to create a server endpoint with protocols pre-registered:

```rust
use bdrpc::endpoint::EndpointBuilder;
use bdrpc::serialization::PostcardSerializer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create server endpoint with builder pattern
    let server = EndpointBuilder::server(PostcardSerializer::default())
        .with_responder("Calculator", 1)
        .build()
        .await?;
    
    // Listen for connections
    let mut listener = server.listen("127.0.0.1:8080").await?;
    println!("Calculator server listening on 127.0.0.1:8080");
    
    // Accept connections and handle requests
    while let Ok(connection) = listener.accept().await {
        tokio::spawn(async move {
            // Accept channel for the Calculator protocol
            let (sender, mut receiver) = connection
                .accept_channel_pair::<CalculatorProtocol>()
                .await?;
            
            // Create service implementation and dispatcher
            let calculator = MyCalculator;
            let dispatcher = CalculatorDispatcher::new(calculator);
            
            // Message loop: process requests using the dispatcher
            while let Some(request) = receiver.recv().await {
                let response = dispatcher.dispatch(request).await;
                sender.send(response).await?;
            }
            
            Ok::<_, Box<dyn std::error::Error>>(())
        });
    }
    
    Ok(())
}
```

### Step 4: Create a Client

Build a client that calls the calculator service using the generated client stub:

```rust
use bdrpc::endpoint::EndpointBuilder;
use bdrpc::serialization::PostcardSerializer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create client endpoint with builder pattern
    let client = EndpointBuilder::client(PostcardSerializer::default())
        .with_caller("Calculator", 1)
        .build()
        .await?;
    
    // Connect to server
    let connection = client.connect("127.0.0.1:8080").await?;
    println!("Connected to calculator server");
    
    // Create channel and get the generated client stub
    let (sender, receiver) = connection
        .create_channel_pair::<CalculatorProtocol>()
        .await?;
    
    let calc_client = CalculatorClient::new(sender, receiver);
    
    // Make RPC calls using the type-safe client methods
    match calc_client.add(10, 5).await {
        Ok(Ok(result)) => println!("10 + 5 = {}", result),
        Ok(Err(e)) => println!("Service error: {}", e),
        Err(e) => println!("Channel error: {}", e),
    }
    
    match calc_client.multiply(7, 6).await {
        Ok(Ok(result)) => println!("7 × 6 = {}", result),
        Ok(Err(e)) => println!("Service error: {}", e),
        Err(e) => println!("Channel error: {}", e),
    }
    
    // Test error handling
    match calc_client.divide(20, 0).await {
        Ok(Ok(result)) => println!("20 ÷ 0 = {}", result),
        Ok(Err(e)) => println!("Expected error: {}", e),
        Err(e) => println!("Channel error: {}", e),
    }
    
    Ok(())
}
```

## Key Concepts

### 1. Service Macro

The `#[bdrpc::service]` macro generates all the boilerplate for you:

```rust
#[service(direction = "bidirectional")]
trait MyService {
    async fn my_method(&self, arg: String) -> Result<String, String>;
}
```

**Generates:**
- `MyServiceProtocol` enum (implements `Protocol` trait)
- `MyServiceClient` struct (type-safe RPC calls)
- `MyServiceServer` trait (implement this)
- `MyServiceDispatcher` struct (routes requests to handlers)

**Direction options:**
- `"call_only"`: Can only call methods (client-side)
- `"respond_only"`: Can only respond to methods (server-side)
- `"bidirectional"`: Can both call and respond (peer-to-peer)

### 2. EndpointBuilder

The builder pattern provides an ergonomic way to create endpoints:

```rust
// Client endpoint
let client = EndpointBuilder::client(PostcardSerializer::default())
    .with_caller("UserService", 1)
    .with_caller("OrderService", 1)
    .build()
    .await?;

// Server endpoint
let server = EndpointBuilder::server(PostcardSerializer::default())
    .with_responder("UserService", 1)
    .with_responder("OrderService", 1)
    .build()
    .await?;

// Peer endpoint (bidirectional)
let peer = EndpointBuilder::peer(PostcardSerializer::default())
    .with_bidirectional("ChatProtocol", 1)
    .build()
    .await?;
```

**Preset configurations:**
- `client()`: Optimized for outgoing calls
- `server()`: Optimized for incoming requests (with connection limits)
- `peer()`: Optimized for bidirectional communication (larger buffers)

### 3. Message Loop Pattern

The message loop is the core pattern for handling requests:

```rust
// Server-side message loop with dispatcher
let dispatcher = MyServiceDispatcher::new(service_impl);

while let Some(request) = receiver.recv().await {
    let response = dispatcher.dispatch(request).await;
    sender.send(response).await?;
}
```

**Key points:**
- Use `recv()` to receive requests
- Use `dispatcher.dispatch()` to route to the correct handler
- Use `send()` to send responses
- Loop continues until channel closes

### 4. Connection and Channels

Connections represent links between endpoints, channels provide typed message passing:

```rust
// Client side
let connection = endpoint.connect("127.0.0.1:8080").await?;
let (sender, receiver) = connection
    .create_channel_pair::<MyProtocol>()
    .await?;

// Server side
let connection = listener.accept().await?;
let (sender, receiver) = connection
    .accept_channel_pair::<MyProtocol>()
    .await?;
```

### 5. Type-Safe Client Stubs

The generated client provides type-safe method calls:

```rust
let client = MyServiceClient::new(sender, receiver);

// Type-safe RPC call
let result = client.my_method("arg".to_string()).await?;

// Returns: Result<Result<ReturnType, ServiceError>, ChannelError>
// - Outer Result: Channel/transport errors
// - Inner Result: Service-level errors
```

## Common Patterns

### Request-Response Pattern

The most common pattern where client sends requests and server responds:

```rust
// Server with message loop
let dispatcher = CalculatorDispatcher::new(MyCalculator);

while let Some(request) = receiver.recv().await {
    let response = dispatcher.dispatch(request).await;
    sender.send(response).await?;
}

// Client with type-safe calls
let client = CalculatorClient::new(sender, receiver);
let result = client.add(5, 3).await?;
```

### Server Push Pattern

Server initiates communication to client (reverse direction):

```rust
// Server registers as caller
let server = EndpointBuilder::server(serializer)
    .with_caller("Notification", 1)  // Server can call
    .build()
    .await?;

// Client registers as responder
let client = EndpointBuilder::client(serializer)
    .with_responder("Notification", 1)  // Client can respond
    .build()
    .await?;
```

### Peer-to-Peer Pattern

Both sides can initiate communication:

```rust
// Both register as bidirectional
let peer = EndpointBuilder::peer(serializer)
    .with_bidirectional("Chat", 1)
    .build()
    .await?;
```

### Multi-Client Server Pattern

Handle multiple concurrent clients:

```rust
while let Ok(connection) = listener.accept().await {
    tokio::spawn(async move {
        let (sender, mut receiver) = connection
            .accept_channel_pair::<MyProtocol>()
            .await?;
        
        let service = MyService::new();
        let dispatcher = MyServiceDispatcher::new(service);
        
        // Each client gets its own message loop
        while let Some(request) = receiver.recv().await {
            let response = dispatcher.dispatch(request).await;
            sender.send(response).await?;
        }
        
        Ok::<_, Box<dyn std::error::Error>>(())
    });
}
```

## Error Handling

BDRPC provides a comprehensive error hierarchy:

```rust
use bdrpc::BdrpcError;

match client.my_method(arg).await {
    Ok(Ok(value)) => {
        // Success: got the service result
        println!("Result: {:?}", value);
    }
    Ok(Err(service_error)) => {
        // Service-level error (e.g., validation failed)
        eprintln!("Service error: {}", service_error);
    }
    Err(BdrpcError::Transport(e)) => {
        // Transport error: connection lost
        eprintln!("Transport error: {}", e);
        // May need to reconnect
    }
    Err(BdrpcError::Channel(e)) => {
        // Channel error: channel closed
        eprintln!("Channel error: {}", e);
        // Channel closed, but connection may still be alive
    }
    Err(e) => {
        eprintln!("Other error: {}", e);
    }
}

## Dynamic Channel Management

BDRPC supports creating and removing channels dynamically at runtime, enabling sophisticated multiplexing patterns like API gateways and service meshes.

### System Protocol

Every connection automatically includes a **System Protocol** (Channel ID 0) that handles:
- Channel creation requests
- Channel closure notifications
- Connection health checks (ping/pong)

### Creating Channels Dynamically

Create channels on-demand without pre-configuration:

```rust
// Server side: Accept dynamically created channels
while let Ok(connection) = listener.accept().await {
    tokio::spawn(async move {
        // Wait for client to request channels
        loop {
            // The system protocol handles channel creation automatically
            // When a client requests a channel, accept it here
            match connection.accept_next_channel().await {
                Ok(channel_info) => {
                    match channel_info.protocol_name.as_str() {
                        "Calculator" => {
                            let (sender, mut receiver) = connection
                                .accept_channel_pair::<CalculatorProtocol>()
                                .await?;
                            
                            // Spawn handler for this channel
                            tokio::spawn(async move {
                                let service = MyCalculator;
                                let dispatcher = CalculatorDispatcher::new(service);
                                
                                while let Some(request) = receiver.recv().await {
                                    let response = dispatcher.dispatch(request).await;
                                    sender.send(response).await?;
                                }
                                Ok::<_, Box<dyn std::error::Error>>(())
                            });
                        }
                        "Logger" => {
                            // Handle logger channel
                            let (sender, mut receiver) = connection
                                .accept_channel_pair::<LoggerProtocol>()
                                .await?;
                            // ... handle logger requests
                        }
                        _ => {
                            eprintln!("Unknown protocol requested");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error accepting channel: {}", e);
                    break;
                }
            }
        }
        Ok::<_, Box<dyn std::error::Error>>(())
    });
}
```

```rust
// Client side: Create channels on-demand
let connection = client.connect("127.0.0.1:8080").await?;

// Create first channel for calculator
let (calc_sender, calc_receiver) = connection
    .create_channel_pair::<CalculatorProtocol>()
    .await?;
let calc_client = CalculatorClient::new(calc_sender, calc_receiver);

// Use calculator
let result = calc_client.add(5, 3).await?;

// Later, create another channel for logging
let (log_sender, log_receiver) = connection
    .create_channel_pair::<LoggerProtocol>()
    .await?;
let log_client = LoggerClient::new(log_sender, log_receiver);

// Use logger
log_client.log("INFO".to_string(), "Calculation complete".to_string()).await?;
```

### Multiplexing Gateway Pattern

Handle multiple services over a single connection:

```rust
// Define multiple services
#[service(direction = "bidirectional")]
trait Auth {
    async fn login(&self, username: String, password: String) -> Result<String, String>;
}

#[service(direction = "bidirectional")]
trait Data {
    async fn query(&self, sql: String) -> Result<Vec<String>, String>;
}

#[service(direction = "bidirectional")]
trait Logger {
    async fn log(&self, level: String, message: String) -> Result<(), String>;
}

// Gateway server handling multiple services
let gateway = EndpointBuilder::server(PostcardSerializer::default())
    .with_responder("Auth", 1)
    .with_responder("Data", 1)
    .with_responder("Logger", 1)
    .build()
    .await?;

let mut listener = gateway.listen("127.0.0.1:8080").await?;

while let Ok(connection) = listener.accept().await {
    tokio::spawn(async move {
        // Track active channels
        let mut channels = HashMap::new();
        
        loop {
            match connection.accept_next_channel().await {
                Ok(channel_info) => {
                    let channel_id = channel_info.channel_id;
                    
                    match channel_info.protocol_name.as_str() {
                        "Auth" => {
                            let (sender, mut receiver) = connection
                                .accept_channel_pair::<AuthProtocol>()
                                .await?;
                            
                            channels.insert(channel_id, "Auth");
                            
                            tokio::spawn(async move {
                                let service = AuthService::new();
                                let dispatcher = AuthDispatcher::new(service);
                                
                                while let Some(request) = receiver.recv().await {
                                    let response = dispatcher.dispatch(request).await;
                                    sender.send(response).await?;
                                }
                                Ok::<_, Box<dyn std::error::Error>>(())
                            });
                        }
                        "Data" => {
                            let (sender, mut receiver) = connection
                                .accept_channel_pair::<DataProtocol>()
                                .await?;
                            
                            channels.insert(channel_id, "Data");
                            
                            tokio::spawn(async move {
                                let service = DataService::new();
                                let dispatcher = DataDispatcher::new(service);
                                
                                while let Some(request) = receiver.recv().await {
                                    let response = dispatcher.dispatch(request).await;
                                    sender.send(response).await?;
                                }
                                Ok::<_, Box<dyn std::error::Error>>(())
                            });
                        }
                        "Logger" => {
                            let (sender, mut receiver) = connection
                                .accept_channel_pair::<LoggerProtocol>()
                                .await?;
                            
                            channels.insert(channel_id, "Logger");
                            
                            tokio::spawn(async move {
                                let service = LoggerService::new();
                                let dispatcher = LoggerDispatcher::new(service);
                                
                                while let Some(request) = receiver.recv().await {
                                    let response = dispatcher.dispatch(request).await;
                                    sender.send(response).await?;
                                }
                                Ok::<_, Box<dyn std::error::Error>>(())
                            });
                        }
                        _ => {
                            eprintln!("Unknown protocol: {}", channel_info.protocol_name);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error accepting channel: {}", e);
                    break;
                }
            }
        }
        Ok::<_, Box<dyn std::error::Error>>(())
    });
}
```

### Closing Channels

Channels can be closed explicitly or will close automatically when dropped:

```rust
// Explicit closure
drop(sender);  // Closes the sending side
drop(receiver);  // Closes the receiving side

// The channel will be cleaned up automatically
// The system protocol notifies the remote peer
```

### Benefits of Dynamic Channels

1. **Flexibility**: Create channels only when needed
2. **Efficiency**: No pre-allocation of unused channels
3. **Scalability**: Support arbitrary number of services
4. **Isolation**: Each channel has independent flow control
5. **Multiplexing**: Multiple protocols over single TCP connection

### Use Cases

- **API Gateway**: Route requests to different microservices
- **Database Proxy**: Multiplex queries to different databases
- **Message Broker**: Route messages to different topics
- **Service Mesh**: Dynamic service discovery and routing
- **Multi-tenant Systems**: Isolate traffic per tenant

```

## Configuration

### Custom Endpoint Configuration

```rust
use std::time::Duration;

let endpoint = EndpointBuilder::new(PostcardSerializer::default())
    .configure(|config| {
        config
            .with_channel_buffer_size(1000)
            .with_handshake_timeout(Duration::from_secs(10))
            .with_max_connections(Some(100))
            .with_max_frame_size(32 * 1024 * 1024)
            .with_frame_integrity(true)
    })
    .with_caller("UserService", 1)
    .build()
    .await?;
```

### Advanced Configuration

```rust
use bdrpc::endpoint::EndpointConfig;

let config = EndpointConfig::new()
    .with_channel_buffer_size(1000)
    .with_handshake_timeout(Duration::from_secs(10))
    .with_max_frame_size(32 * 1024 * 1024)
    .with_frame_integrity(true)
    .with_endpoint_id("my-service".to_string());

let endpoint = EndpointBuilder::with_config(serializer, config)
    .with_caller("UserService", 1)
    .build()
    .await?;
```

## Serialization

BDRPC supports multiple serialization formats:

```rust
// Postcard (recommended: compact binary)
use bdrpc::serialization::PostcardSerializer;
let serializer = PostcardSerializer::default();

// JSON (human-readable, debugging)
use bdrpc::serialization::JsonSerializer;
let serializer = JsonSerializer::default();

// rkyv (zero-copy deserialization)
use bdrpc::serialization::RkyvSerializer;
let serializer = RkyvSerializer::default();
```

## Reconnection

Enable automatic reconnection for resilient clients:

```rust
use bdrpc::reconnection::ExponentialBackoff;
use std::sync::Arc;
use std::time::Duration;

let endpoint = EndpointBuilder::client(PostcardSerializer::default())
    .configure(|config| {
        config.with_reconnection_strategy(Arc::new(
            ExponentialBackoff::new(
                Duration::from_secs(1),   // Initial delay
                Duration::from_secs(30),  // Max delay
                2.0,                      // Multiplier
                Some(5),                  // Max attempts
            )
        ))
    })
    .with_caller("UserService", 1)
    .build()
    .await?;

let connection = endpoint
    .connect_with_reconnection("127.0.0.1:8080")
    .await?;
```

## Next Steps

Now that you understand the basics, explore:

- **[Examples](../bdrpc/examples/)**: Working code examples
  - `hello_world_service.rs`: Simple greeting service
  - `calculator_service.rs`: Calculator with error handling
  - `chat_server_service.rs`: Multi-client chat server
  - `dynamic_channels_service.rs`: Dynamic channel creation and multiplexing
  - `endpoint_builder.rs`: Builder pattern examples
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
let listener = server.listen("127.0.0.1:8080").await?;

// Client
let connection = client.connect("127.0.0.1:8080").await?;
```

### Protocol Not Found

Ensure both sides register the same protocol with matching names:

```rust
// Server
.with_responder("Calculator", 1)

// Client
.with_caller("Calculator", 1)
```

### Serialization Errors

Ensure your types implement `Serialize` and `Deserialize`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct MyMessage {
    field: String,
}
```

### Message Loop Not Processing

Make sure your message loop continues until the channel closes:

```rust
// Correct: loop continues
while let Some(request) = receiver.recv().await {
    // process request
}

// Incorrect: only processes one message
if let Some(request) = receiver.recv().await {
    // process request
}
```

## Summary

You've learned:

- ✅ How to define services with `#[bdrpc::service]`
- ✅ How to use `EndpointBuilder` for ergonomic endpoint creation
- ✅ How to implement the message loop pattern
- ✅ How to use generated client stubs for type-safe RPC
- ✅ How to use dispatchers for request routing
- ✅ How to create and manage channels dynamically
- ✅ How to build multiplexing gateways
- ✅ Common communication patterns
- ✅ Error handling strategies
- ✅ Configuration options

**Key Takeaways:**

1. **Use `#[bdrpc::service]`** for type-safe, boilerplate-free service definitions
2. **Use `EndpointBuilder`** for ergonomic endpoint creation with presets
3. **Use the message loop pattern** with dispatchers for request handling
4. **Use generated client stubs** for type-safe RPC calls
5. **Handle both layers of errors**: channel errors and service errors
6. **Use dynamic channels** for flexible, on-demand service multiplexing

Ready to build production applications with BDRPC!
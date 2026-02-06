# Bi-Directional Remote Procedure Call (BDRPC)

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![Status](https://img.shields.io/badge/status-v0.1.0%20Released-green.svg)](docs/dev/post-v0.1.0-roadmap.md)
[![Build](https://github.com/huhlig/bdrpc/actions/workflows/ci.yml/badge.svg)](https://github.com/huhlig/bdrpc/actions/workflows/ci.yml)
[![Documentation](https://img.shields.io/badge/docs-GitHub%20Pages-blue.svg)](https://huhlig.github.io/bdrpc/)

BDRPC is a Rust library for implementing bi-directional remote procedure calls (RPC) over TCP/IP connections. It 
provides a simple and efficient way to communicate between different processes or machines, enabling remote procedure 
calls and data exchange. It is designed to be straightforward to use, flexible, and highly performant, making it 
suitable for a wide range of applications, from simple client-server interactions to complex distributed systems. It 
supports various protocols and data formats, allowing seamless integration with existing systems and frameworks.

## Features

- Easy-to-use API for defining and calling remote procedures
- Support for multiple data formats, including JSON, MessagePack, and custom binary formats
- Built-in support for error handling and exception propagation
- Flexible configuration options for customizing network behavior
- Extensible architecture for integrating with existing systems and frameworks

## Why BDRPC and not Other Libraries (gRPC, Cap'n'Proto, tarpc)?

BDRPC stands out from other RPC libraries due to its focus on bi-directional communication, robust error handling, and 
extensibility. While other libraries may offer similar features, BDRPC provides a more comprehensive solution that 
addresses the needs of modern peer to peer distributed systems. Its support for custom transports and serialization 
formats allows for seamless integration with existing systems, making it a versatile choice for a wide range of 

## Channel Types: In-Memory vs Network

BDRPC provides two types of channels for different use cases:

### In-Memory Channels

Created with `Channel::new_in_memory()`, these channels:
- ✅ Work within a single process only
- ✅ Perfect for testing and examples
- ✅ No network overhead
- ❌ Cannot communicate across network boundaries

**Use for:**
- Unit tests
- In-process communication
- Learning the channel API
- Prototyping

**Example:**
```rust
use bdrpc::channel::{Channel, ChannelId, Protocol};

#[derive(Debug, Clone)]
enum MyProtocol { Ping }
impl Protocol for MyProtocol {
    fn method_name(&self) -> &'static str { "ping" }
    fn is_request(&self) -> bool { true }
}

let (sender, receiver) = Channel::<MyProtocol>::new_in_memory(ChannelId::new(), 100);
```

### Network Channels

Created via the Endpoint API, these channels:
- ✅ Work across network boundaries
- ✅ Automatically wired to transports (TCP, etc.)
- ✅ Support protocol negotiation
- ✅ Handle serialization/deserialization

**Use for:**
- Production applications
- Client-server communication
- Distributed systems
- Real network RPC

**Example (using #[bdrpc::service] macro - recommended):**
```rust,no_run
use bdrpc::service;
use bdrpc::endpoint::{Endpoint, EndpointConfig};
use bdrpc::serialization::JsonSerializer;

// Define your service with the macro
#[service(direction = "bidirectional")]
trait Calculator {
    async fn add(&self, a: i32, b: i32) -> Result<i32, String>;
    async fn subtract(&self, a: i32, b: i32) -> Result<i32, String>;
}

// Implement the generated server trait
struct MyCalculator;

#[async_trait::async_trait]
impl CalculatorServer for MyCalculator {
    async fn add(&self, a: i32, b: i32) -> Result<i32, String> {
        Ok(a + b)
    }
    
    async fn subtract(&self, a: i32, b: i32) -> Result<i32, String> {
        Ok(a - b)
    }
}

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
// Server side
let mut server_endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
server_endpoint.register_bidirectional("Calculator", 1).await?;
server_endpoint.listen("127.0.0.1:8080").await?;

// Accept connection and get channels
let (sender, receiver) = server_endpoint.accept_channels::<CalculatorProtocol>().await?;

// Create dispatcher and handle requests
let calculator = MyCalculator;
let dispatcher = CalculatorDispatcher::new(calculator);

tokio::spawn(async move {
    while let Some(request) = receiver.recv().await {
        let response = dispatcher.dispatch(request).await;
        sender.send(response).await.ok();
    }
});

// Client side
let mut client_endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
client_endpoint.register_bidirectional("Calculator", 1).await?;
client_endpoint.connect("127.0.0.1:8080").await?;

// Get channels and create client
let (sender, receiver) = client_endpoint.get_channels::<CalculatorProtocol>().await?;
let client = CalculatorClient::new(sender, receiver);

// Make RPC calls
let result = client.add(5, 3).await??;
println!("5 + 3 = {}", result);
# Ok(())
# }
```

**Manual example (without macro):**
```rust,no_run
use bdrpc::endpoint::{Endpoint, EndpointConfig};
use bdrpc::serialization::JsonSerializer;
use bdrpc::channel::{ChannelId, Protocol};

// Define protocol enum manually
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
enum CalculatorProtocol {
    AddRequest { a: i32, b: i32 },
    AddResponse { result: i32 },
    SubtractRequest { a: i32, b: i32 },
    SubtractResponse { result: i32 },
}

impl Protocol for CalculatorProtocol {
    fn method_name(&self) -> &'static str {
        match self {
            Self::AddRequest { .. } => "add",
            Self::AddResponse { .. } => "add",
            Self::SubtractRequest { .. } => "subtract",
            Self::SubtractResponse { .. } => "subtract",
        }
    }
    
    fn is_request(&self) -> bool {
        matches!(self, Self::AddRequest { .. } | Self::SubtractRequest { .. })
    }
}

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
// Server side
let mut server_endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
server_endpoint.register_bidirectional("Calculator", 1).await?;
server_endpoint.listen("127.0.0.1:8080").await?;

let (sender, mut receiver) = server_endpoint.accept_channels::<CalculatorProtocol>().await?;

// Handle requests manually
tokio::spawn(async move {
    while let Some(request) = receiver.recv().await {
        let response = match request {
            CalculatorProtocol::AddRequest { a, b } => {
                CalculatorProtocol::AddResponse { result: a + b }
            }
            CalculatorProtocol::SubtractRequest { a, b } => {
                CalculatorProtocol::SubtractResponse { result: a - b }
            }
            _ => continue, // Ignore responses
        };
        sender.send(response).await.ok();
    }
});

// Client side
let mut client_endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
client_endpoint.register_bidirectional("Calculator", 1).await?;
client_endpoint.connect("127.0.0.1:8080").await?;

let (sender, mut receiver) = client_endpoint.get_channels::<CalculatorProtocol>().await?;

// Make request manually
sender.send(CalculatorProtocol::AddRequest { a: 5, b: 3 }).await?;

// Wait for response
if let Some(CalculatorProtocol::AddResponse { result }) = receiver.recv().await {
    println!("5 + 3 = {}", result);
}
# Ok(())
# }
```

### Example Guide

- `hello_world.rs` - Basic BDRPC concepts and full stack overview
- `channel_basics.rs` - In-memory channels only
- `advanced_channels.rs` - In-memory channel management
- `dynamic_channels.rs` - **System protocol and dynamic channel creation** ⭐
- `chat_server.rs` - In-memory multi-client pattern
- `calculator.rs` - TCP transport + in-memory channels (demo)
- `network_chat.rs` - **Full Endpoint API with network channels** ⭐

For production use, always use the Endpoint API as shown in `network_chat.rs`.

## Project Structure

```text
bdrpc/
+-- bdrpc/                      # Main library crate
    +-- src/
        +-- backpressure/      # Flow control and backpressure management
        +-- channel/           # Channel abstraction and management
        +-- endpoint/          # Connection and protocol negotiation
        +-- observability/     # Metrics, health checks, and correlation
        +-- reconnection/      # Reconnection strategies and circuit breakers
        +-- serialization/     # Serialization formats (JSON, Postcard, rkyv)
        +-- transport/         # Transport layer (TCP, TLS, Memory, Compression)
    +-- examples/              # Comprehensive examples
    +-- benches/               # Performance benchmarks
    +-- tests/                 # Integration tests
+-- bdrpc-macros/              # Procedural macros for code generation
+-- docs/                      # Documentation
    +-- adr/                   # Architecture Decision Records
    +-- dev/                   # Development documentation
```

## Documentation

- **[Quick Start Guide](docs/quick-start.md)** - Get started with BDRPC
- **[Architecture Guide](docs/architecture-guide.md)** - System architecture and design
- **[Performance Guide](docs/performance-guide.md)** - Optimization and benchmarking
- **[Migration Guide](docs/migration-guide.md)** - Upgrading between versions
- **[Protocol Evolution](docs/protocol-evolution.md)** - Versioning and compatibility
- **[Error Recovery](docs/error-recovery.md)** - Error handling patterns
- **[ADRs](docs/adr/)** - Architecture Decision Records
- **[Post-v0.1.0 Roadmap](docs/dev/post-v0.1.0-roadmap.md)** - Future features and enhancements
- **[v0.1.0 Implementation Plan](docs/dev/archive/implementation-plan-v0.1.0.md)** - Archived v0.1.0 planning (90% complete)
- **[API Documentation](https://huhlig.github.io/bdrpc/)** - Full API reference

## Project Status

BDRPC v0.1.0 has been released! See the [post-v0.1.0 roadmap](docs/dev/post-v0.1.0-roadmap.md) for future features and enhancements.

**Completed Features:**
- ✅ Core channel and transport abstractions
- ✅ Multiple serialization formats (JSON, Postcard, rkyv)
- ✅ TCP and TLS transports
- ✅ Compression support (Gzip, Zstd, LZ4)
- ✅ Reconnection strategies and circuit breakers
- ✅ Backpressure and flow control
- ✅ Endpoint API with protocol negotiation
- ✅ System protocol for dynamic channel creation
- ✅ Comprehensive examples and documentation
- ✅ Observability (metrics, tracing, health checks)
- ✅ Performance optimization (4.02M msg/s throughput)
- ✅ 367 tests passing with 83% code coverage

**Planned for v0.2.0:**
- 🔜 WebSocket transport
- 🔜 QUIC transport
- 🔜 Enhanced fuzz and stress testing

### Transport Layer Enhancements

#### 1. WebSocket Transport
- [ ] **Add WebSocket transport implementation**
  - Location: New file `bdrpc/src/transport/websocket.rs`
  - Description: Implement WebSocket transport for browser compatibility
  - Priority: Medium
  - Estimated effort: Large
  - Dependencies: Add `tokio-tungstenite` crate to Cargo.toml
  - Notes: Essential for web-based applications

#### 2. WebTransport over QUIC
- [ ] **Add WebTransport over QUIC support**
  - Location: New file `bdrpc/src/transport/webtransport.rs`
  - Description: Implement WebTransport protocol for modern web applications
  - Priority: Low
  - Estimated effort: Large
  - Dependencies: Add appropriate QUIC/WebTransport crate
  - Notes: Cutting-edge transport with multiplexing and low latency
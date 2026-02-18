# Bi-Directional Remote Procedure Call (BDRPC)

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![Status](https://img.shields.io/badge/status-v0.2.0%20In%20Development-yellow.svg)](docs/dev/transport-manager-enhancement-plan.md)
[![Build](https://github.com/huhlig/bdrpc/actions/workflows/ci.yml/badge.svg)](https://github.com/huhlig/bdrpc/actions/workflows/ci.yml)
[![Documentation](https://img.shields.io/badge/docs-GitHub%20Pages-blue.svg)](https://huhlig.github.io/bdrpc/)

BDRPC is a Rust library for implementing bi-directional remote procedure calls (RPC) over TCP/IP connections. It
provides a simple and efficient way to communicate between different processes or machines, enabling remote procedure
calls and data exchange. It is designed to be straightforward to use, flexible, and highly performant, making it
suitable for a wide range of applications, from simple client-server interactions to complex distributed systems. It
supports various protocols and data formats, allowing seamless integration with existing systems and frameworks.

## Features

### Core Features
- **Bi-directional RPC**: Both sides can call and respond
- **Type-safe Communication**: Compile-time guarantees with `#[bdrpc::service]` macro
- **Multiple Serialization Formats**: JSON, Postcard, rkyv support
- **High Performance**: 4.02M messages/second throughput
- **Flexible Transports**: TCP, TLS, Memory, with compression support

### v0.2.0 Transport Management (In Development)
- **Multiple Transports**: Run servers on multiple ports/protocols simultaneously
- **Named Connections**: Reference transports by name for clarity and failover
- **Automatic Reconnection**: Built-in reconnection with configurable strategies (exponential backoff, circuit breakers)
- **Transport Failover**: Switch between transports automatically for high availability
- **Dynamic Management**: Add/remove transports at runtime
- **Event-Driven**: React to connection lifecycle events via `TransportEventHandler`
- **EndpointBuilder**: Ergonomic API for configuring endpoints with transports

### Additional Features
- Built-in error handling and exception propagation
- Comprehensive observability (metrics, tracing, health checks)
- Backpressure and flow control
- Dynamic channel creation and multiplexing
- Protocol versioning and negotiation

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
// See examples/calculator_service.rs for a complete working example
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

impl CalculatorServer for MyCalculator {
    async fn add(&self, a: i32, b: i32) -> Result<i32, String> {
        Ok(a + b)
    }
    
    async fn subtract(&self, a: i32, b: i32) -> Result<i32, String> {
        Ok(a - b)
    }
}

async fn example() -> Result<(), Box<dyn std::error::Error>> {
    // Server side
    let mut server_endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    server_endpoint.register_bidirectional("Calculator", 1).await?;
    server_endpoint.listen("127.0.0.1:8080").await?;

    // Client side
    let mut client_endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    client_endpoint.register_bidirectional("Calculator", 1).await?;
    let connection = client_endpoint.connect("127.0.0.1:8080").await?;

    // Get typed channels for the protocol
    let (sender, receiver) = client_endpoint.get_channels::<CalculatorProtocol>(
        connection.id(),
        "Calculator"
    ).await?;

    // Create client and make RPC calls
    let client = CalculatorClient::new(sender, receiver);
    let result = client.add(5, 3).await??;
    println!("5 + 3 = {}", result);
    Ok(())
}
```

**Manual example (without macro):**

```rust,no_run
// See examples/calculator_manual.rs for a complete working example
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

async fn example() -> Result<(), Box<dyn std::error::Error>> {
    // Server side
    let mut server_endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    server_endpoint.register_bidirectional("Calculator", 1).await?;
    server_endpoint.listen("127.0.0.1:8080").await?;

    // Client side
    let mut client_endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    client_endpoint.register_bidirectional("Calculator", 1).await?;
    let connection = client_endpoint.connect("127.0.0.1:8080").await?;

    // Get typed channels for the protocol
    let (sender, mut receiver) = client_endpoint.get_channels::<CalculatorProtocol>(
        connection.id(),
        "Calculator"
    ).await?;

    // Make request manually
    sender.send(CalculatorProtocol::AddRequest { a: 5, b: 3 }).await?;

    // Wait for response
    if let Some(CalculatorProtocol::AddResponse { result }) = receiver.recv().await {
        println!("5 + 3 = {}", result);
    }
    Ok(())
}
```

### Example Guide

BDRPC provides examples in two forms to demonstrate different approaches:

#### 🎯 Getting Started Examples

| Example                    | Description                                  | Key Concepts                                           |
|----------------------------|----------------------------------------------|--------------------------------------------------------|
| **hello_world_service.rs** | Simplest RPC using `#[bdrpc::service]` macro | Service traits, generated client/server, type-safe RPC |
| **hello_world_manual.rs**  | Same functionality with manual protocol      | Protocol enum, manual routing, full control            |
| **channel_basics.rs**      | In-memory channel fundamentals               | Channel creation, send/recv, FIFO ordering             |

#### 🧮 RPC Pattern Examples

| Example           | Service Macro Version                                                                | Manual Protocol Version                                                     |
|-------------------|--------------------------------------------------------------------------------------|-----------------------------------------------------------------------------|
| **Calculator**    | `calculator_service.rs` - Type-safe math operations with generated client/dispatcher | `calculator_manual.rs` - Manual protocol enum with match-based routing      |
| **Chat Server**   | `chat_server_service.rs` - Multi-client chat with generated service API              | `chat_server_manual.rs` - Manual message broadcasting and client management |
| **File Transfer** | `file_transfer_service.rs` - Streaming large files with progress tracking            | `file_transfer_manual.rs` - Manual chunked transfer with CRC32 verification |

#### 🌐 Advanced Patterns

| Example                           | Description                                | Key Features                                                               |
|-----------------------------------|--------------------------------------------|----------------------------------------------------------------------------|
| **dynamic_channels_service.rs** ⭐ | Multiplexing gateway with service macro    | System protocol, dynamic channel creation, type-safe multi-service gateway |
| **dynamic_channels_manual.rs**    | Same gateway pattern with manual protocols | Full control over channel lifecycle, custom routing logic                  |
| **network_chat.rs** ⭐             | Real network communication pattern         | Endpoint API, TCP transport, protocol negotiation                          |
| **mtls_demo.rs** 🔐                | Mutual TLS configuration guide             | mTLS setup, client certificates, CA trust stores, reconnection strategies  |
| **advanced_channels.rs**          | Channel management techniques              | Channel lifecycle, error handling, cleanup                                 |
| **service_macro_demo.rs**         | Detailed macro explanation                 | Code generation, dispatcher internals, best practices                      |

#### 🚀 Transport Management (v0.2.0+)

| Example                        | Description                                  | Key Features                                                                    |
|--------------------------------|----------------------------------------------|---------------------------------------------------------------------------------|
| **multi_transport_server.rs** ⭐ | Server with multiple listeners               | Multiple TCP listeners, automatic connection acceptance, EndpointBuilder        |
| **auto_reconnect_client.rs** ⭐  | Client with automatic reconnection           | Named transports, exponential backoff, connection resilience                    |
| **transport_failover.rs** ⭐     | Client with transport failover               | Multiple transports, automatic failover, high availability patterns             |
| **endpoint_builder.rs**        | EndpointBuilder API demonstration            | Transport configuration, reconnection strategies, preset configurations         |

####  Choosing Your Approach

**Use Service Macro (`#[bdrpc::service]`) when:**

- ✅ Building standard RPC services
- ✅ Want type-safe client APIs
- ✅ Prefer less boilerplate code
- ✅ Need automatic request routing
- ✅ Value compile-time checking

**Use Manual Protocol when:**

- ✅ Need custom message structures
- ✅ Require non-standard communication patterns
- ✅ Want maximum control over protocol details
- ✅ Integrating with existing protocol definitions
- ✅ Building specialized streaming patterns

#### 🚀 Running Examples

```bash
# Service macro examples (recommended for most use cases)
cargo run --example hello_world_service --features serde
cargo run --example calculator_service --features serde
cargo run --example chat_server_service --features serde
cargo run --example file_transfer_service --features serde
cargo run --example dynamic_channels_service --features serde

# Manual protocol examples (for learning or custom patterns)
cargo run --example hello_world_manual --features serde
cargo run --example calculator_manual --features serde
cargo run --example chat_server_manual --features serde
cargo run --example file_transfer_manual --features serde
cargo run --example dynamic_channels_manual --features serde

# Network communication (production pattern)
cargo run --example network_chat --features serde

# Security and TLS examples
cargo run --example mtls_demo --features tls,serde

# Transport management examples (v0.2.0+)
cargo run --example multi_transport_server --features serde
cargo run --example auto_reconnect_client --features serde
cargo run --example transport_failover --features serde
cargo run --example endpoint_builder --features serde
```

#### 💡 Example Comparison

Each paired example (service vs manual) demonstrates the **same functionality** using different approaches:

| Aspect             | Service Macro        | Manual Protocol  |
|--------------------|----------------------|------------------|
| **Code Volume**    | ~200 lines           | ~400 lines       |
| **Type Safety**    | Compile-time checked | Runtime matching |
| **Boilerplate**    | Minimal (generated)  | More (explicit)  |
| **Flexibility**    | Structured patterns  | Maximum control  |
| **Learning Curve** | Easier to start      | Shows internals  |
| **Best For**       | Production services  | Custom protocols |

**For production use**, always use the Endpoint API as shown in `network_chat.rs` for proper network communication.

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
```

## Documentation

- **[Quick Start Guide](docs/quick-start.md)** - Get started with BDRPC (updated for v0.2.0)
- **[Transport Configuration Guide](docs/transport-configuration.md)** - Complete transport management guide (v0.2.0+)
- **[Migration Guide v0.2.0](docs/migration-guide-v0.2.0.md)** - Upgrade from v0.1.0 to v0.2.0
- **[Architecture Guide](docs/architecture-guide.md)** - System architecture and design (updated for v0.2.0)
- **[Performance Guide](docs/performance-guide.md)** - Optimization and benchmarking
- **[Migration Guide](docs/migration-guide.md)** - General upgrade guide
- **[Protocol Evolution](docs/protocol-evolution.md)** - Versioning and compatibility
- **[Error Recovery](docs/error-recovery.md)** - Error handling patterns
- **[ADRs](docs/ADR/)** - Architecture Decision Records
  - **[ADR-013](docs/ADR/ADR-013-enhanced-transport-manager.md)** - Enhanced Transport Manager (v0.2.0)
- **[API Documentation](https://huhlig.github.io/bdrpc/)** - Full API reference

## Project Status

**Current Version:** v0.2.0 (In Development - 68.75% Complete)

BDRPC v0.1.0 has been released! v0.2.0 is currently in development with enhanced transport management features. See the [transport manager enhancement plan](docs/dev/transport-manager-enhancement-plan.md) for detailed progress.

**v0.1.0 Features (Released):**

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
- ✅ 462 tests passing with high code coverage

**v0.2.0 Features (In Development - 68.75% Complete):**

- ✅ Enhanced TransportManager with multiple transport support
- ✅ Named transport connections for failover
- ✅ Automatic reconnection per transport
- ✅ Transport lifecycle event callbacks
- ✅ EndpointBuilder with transport configuration
- ✅ Dynamic transport management (add/remove at runtime)
- ⏳ Comprehensive documentation updates (87.5% complete)
- ⏳ Testing and hardening (pending)
- ⏳ Migration tools and final polish (pending)

**Planned for Future Releases:**

- 🔜 WebSocket transport
- 🔜 QUIC transport
- 🔜 Enhanced fuzz and stress testing
- 🔜 Service mesh integration

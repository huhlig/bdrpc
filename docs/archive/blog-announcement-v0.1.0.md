# Announcing BDRPC v0.1.0: A Modern Bi-Directional RPC Framework for Rust

**February 6, 2026** — We're excited to announce the initial release of BDRPC (Bi-Directional Remote Procedure Call), a high-performance, flexible RPC framework for Rust that brings true peer-to-peer communication to distributed systems.

## What is BDRPC?

BDRPC is a Rust library that enables bi-directional remote procedure calls over various transport layers. Unlike traditional RPC frameworks that enforce strict client-server roles, BDRPC allows both endpoints to initiate communication, making it ideal for modern distributed architectures, real-time applications, and peer-to-peer systems.

## Why We Built BDRPC

While excellent RPC frameworks like gRPC, Cap'n Proto, and tarpc exist, we found a gap in the ecosystem for a framework that:

- **Truly supports bi-directional communication** — Either side can initiate requests
- **Provides pluggable transports** — TCP, TLS, in-memory, with WebSocket and QUIC planned
- **Offers multiple serialization formats** — JSON, Postcard, with rkyv support for zero-copy planned
- **Delivers exceptional performance** — 4M+ messages/second throughput, sub-10µs latency
- **Maintains type safety** — Compile-time guarantees for protocol correctness
- **Handles production concerns** — Reconnection, backpressure, observability built-in

BDRPC addresses these needs while maintaining the safety and ergonomics Rust developers expect.

## Key Features

### 🔄 True Bi-Directional Communication

BDRPC supports three communication patterns through protocol directionality:

- **CallOnly**: Traditional client behavior (send requests, receive responses)
- **RespondOnly**: Traditional server behavior (receive requests, send responses)
- **Bidirectional**: Peer-to-peer behavior (both call and respond)

This flexibility enables patterns like server push notifications, peer-to-peer messaging, and collaborative applications without architectural gymnastics.

### 🚀 Exceptional Performance

BDRPC achieves impressive performance metrics:

- **4.02M messages/second** throughput with batching
- **481K messages/second** for individual messages
- **2-4 microseconds** one-way latency
- **4-8 microseconds** round-trip latency
- **173 nanoseconds** channel creation overhead

These numbers make BDRPC suitable for high-frequency trading, real-time gaming, IoT systems, and other latency-sensitive applications.

### 🔌 Pluggable Architecture

**Transport Layer:**
- TCP with automatic `TCP_NODELAY` optimization
- TLS for secure communication
- In-memory for testing and same-process communication
- Compression (Gzip, Zstd, LZ4) for bandwidth-limited scenarios
- WebSocket and QUIC planned for v0.2.0

**Serialization Formats:**
- **Postcard** — Compact binary format (recommended for production)
- **JSON** — Human-readable for debugging
- **rkyv** — Zero-copy deserialization for maximum performance planned for v0.2.0

### 🛡️ Production-Ready Features

**Reconnection Strategies:**
- Exponential backoff with jitter
- Fixed interval reconnection
- Circuit breaker pattern
- Configurable retry limits

**Flow Control:**
- Automatic backpressure management
- Bounded queues prevent memory exhaustion
- Configurable buffer sizes per channel

**Observability:**
- Metrics integration via the `metrics` crate
- Distributed tracing with correlation IDs
- Health check endpoints
- Comprehensive error hierarchy

### 🔒 Type Safety

BDRPC leverages Rust's type system to ensure protocol correctness at compile time:

```rust
use bdrpc::channel::Protocol;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct CalculateRequest {
    pub operation: String,
    pub a: i32,
    pub b: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CalculateResponse {
    pub result: i32,
}

pub struct CalculatorProtocol;

impl Protocol for CalculatorProtocol {
    type Request = CalculateRequest;
    type Response = CalculateResponse;
    
    fn name() -> &'static str {
        "Calculator"
    }
}
```

The compiler ensures you can't accidentally send the wrong message type or mix up request and response handling.

## Getting Started

Add BDRPC to your `Cargo.toml`:

```toml
[dependencies]
bdrpc = "0.1"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
```

### Simple Server Example

```rust
use bdrpc::endpoint::{Endpoint, EndpointConfig, ProtocolDirection};
use bdrpc::serialization::PostcardSerializer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create server endpoint
    let mut server = Endpoint::new(
        PostcardSerializer::default(),
        EndpointConfig::default()
    );
    
    // Register protocol as responder
    server.register_protocol::<CalculatorProtocol>(
        ProtocolDirection::RespondOnly
    )?;
    
    // Listen for connections
    let mut listener = server.listen("127.0.0.1:8080").await?;
    println!("Server listening on 127.0.0.1:8080");
    
    // Accept and handle connections
    while let Ok(connection) = listener.accept().await {
        tokio::spawn(async move {
            let channel = connection
                .accept_channel::<CalculatorProtocol>()
                .await?;
            
            while let Ok(request) = channel.recv().await {
                let result = match request.operation.as_str() {
                    "add" => request.a + request.b,
                    "subtract" => request.a - request.b,
                    "multiply" => request.a * request.b,
                    "divide" => request.a / request.b,
                    _ => 0,
                };
                
                channel.send(CalculateResponse { result }).await?;
            }
            
            Ok::<_, Box<dyn std::error::Error>>(())
        });
    }
    
    Ok(())
}
```

### Simple Client Example

```rust
use bdrpc::endpoint::{Endpoint, EndpointConfig, ProtocolDirection};
use bdrpc::serialization::PostcardSerializer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create client endpoint
    let mut client = Endpoint::new(
        PostcardSerializer::default(),
        EndpointConfig::default()
    );
    
    // Register protocol as caller
    client.register_protocol::<CalculatorProtocol>(
        ProtocolDirection::CallOnly
    )?;
    
    // Connect to server
    let connection = client.connect("127.0.0.1:8080").await?;
    let channel = connection.create_channel::<CalculatorProtocol>().await?;
    
    // Make calculation
    let request = CalculateRequest {
        operation: "add".to_string(),
        a: 10,
        b: 5,
    };
    
    let response = channel.send(request).await?;
    println!("10 + 5 = {}", response.result);
    
    Ok(())
}
```

## Real-World Use Cases

BDRPC is designed for:

- **Microservices Communication** — High-performance inter-service RPC
- **Real-Time Applications** — Gaming, chat, collaborative editing
- **IoT Systems** — Device-to-device and device-to-cloud communication
- **Financial Systems** — Low-latency trading and market data distribution
- **Distributed Databases** — Replication and consensus protocols
- **Plugin Architectures** — In-process communication with isolation

## Architecture Highlights

### Channel Multiplexing

BDRPC supports multiple independent channels over a single transport connection. Each channel has its own message queue, flow control, and ordering guarantees, enabling efficient resource utilization.

### Protocol Negotiation

During connection establishment, endpoints negotiate supported protocols and their directionality. This ensures both sides agree on communication patterns before exchanging messages, preventing runtime errors.

### Automatic Reconnection

Built-in reconnection strategies handle transient network failures gracefully. Configure exponential backoff, circuit breakers, and retry limits to match your reliability requirements.

### Zero-Copy Optimization

When using rkyv serialization, BDRPC can deserialize messages without copying data, significantly reducing CPU usage and memory allocations for large messages.

## Project Status

BDRPC v0.1.0 is **production-ready** with:

- ✅ 367 tests passing with 83% code coverage
- ✅ Comprehensive documentation and examples
- ✅ 12 Architecture Decision Records documenting design choices
- ✅ Performance benchmarks and optimization guide
- ✅ Multiple real-world examples (chat server, file transfer, calculator)

## Documentation

Comprehensive documentation is available:

- **[Quick Start Guide](https://github.com/huhlig/bdrpc/blob/main/docs/quick-start.md)** — Get up and running in minutes
- **[Architecture Guide](https://github.com/huhlig/bdrpc/blob/main/docs/architecture-guide.md)** — Deep dive into design decisions
- **[Performance Guide](https://github.com/huhlig/bdrpc/blob/main/docs/performance-guide.md)** — Optimization tips and benchmarks
- **[API Documentation](https://docs.rs/bdrpc)** — Complete API reference
- **[Examples](https://github.com/huhlig/bdrpc/tree/main/bdrpc/examples)** — Working code examples

## Community and Support

- **GitHub**: [github.com/huhlig/bdrpc](https://github.com/huhlig/bdrpc)
- **Issues**: Report bugs and request features
- **Discussions**: Ask questions and share ideas
- **License**: Apache 2.0

## Acknowledgments

BDRPC builds on the excellent work of the Rust async ecosystem, particularly Tokio, and draws inspiration from established RPC frameworks while addressing gaps in bi-directional communication patterns.

## Try It Today

Install BDRPC and start building distributed systems with true peer-to-peer communication:

```bash
cargo add bdrpc
```

We're excited to see what you build with BDRPC. Whether you're creating microservices, real-time applications, or distributed systems, BDRPC provides the performance, flexibility, and safety you need.

Happy coding! 🦀

---

*BDRPC is developed by Hans W. Uhlig and released under the Apache 2.0 license.*
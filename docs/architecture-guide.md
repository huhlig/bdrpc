# BDRPC Architecture Guide

This guide provides a comprehensive overview of BDRPC's architecture, design decisions, and internal workings.

## Table of Contents

- [Overview](#overview)
- [Layered Architecture](#layered-architecture)
- [Core Components](#core-components)
- [Data Flow](#data-flow)
- [Design Principles](#design-principles)
- [Key Features](#key-features)
- [Architecture Decision Records](#architecture-decision-records)

## Overview

BDRPC (Bi-Directional RPC) is a modern RPC framework for Rust that enables efficient, type-safe communication between distributed components. Unlike traditional RPC frameworks that enforce a strict client-server model, BDRPC supports true bi-directional communication where both sides can initiate calls.

### Design Goals

1. **Bi-directional**: Both sides can call and respond
2. **Type-safe**: Compile-time guarantees for message types
3. **Efficient**: High throughput, low latency
4. **Flexible**: Multiple transports and serialization formats
5. **Resilient**: Automatic reconnection and error recovery
6. **Observable**: Comprehensive metrics and tracing

## Layered Architecture

BDRPC uses a clean layered architecture where each layer has specific responsibilities:

```text
┌─────────────────────────────────────────┐
│         Application Layer               │  User code
│  (Protocol implementations, handlers)   │
├─────────────────────────────────────────┤
│          Endpoint Layer                 │  Connection management
│  (Handshake, negotiation, lifecycle)   │  Protocol registration
├─────────────────────────────────────────┤
│          Channel Layer                  │  Multiplexing
│  (Message routing, FIFO ordering)       │  Type-safe messaging
├─────────────────────────────────────────┤
│       Serialization Layer               │  Message encoding
│  (Framing, buffer pooling)              │  Format abstraction
├─────────────────────────────────────────┤
│         Transport Layer                 │  Network I/O
│  (TCP, TLS, Memory, Compression)        │  Connection handling
└─────────────────────────────────────────┘
```

### Layer Responsibilities

#### 1. Transport Layer

**Purpose**: Reliable byte stream communication

**Components**:
- `Transport` trait: Abstraction for different transport types
- `TcpTransport`: TCP/IP networking
- `TlsTransport`: Encrypted TCP with TLS (optional feature)
- `MemoryTransport`: In-process communication
- `CompressionTransport`: Transparent compression
- `TransportManager`: Enhanced connection lifecycle management (v0.2.0+)
  - Manages multiple listener transports (servers)
  - Manages multiple caller transports (clients)
  - Tracks active connections
  - Handles automatic reconnection per transport
  - Supports dynamic transport enable/disable
  - Provides lifecycle event callbacks

**Key Features**:
- Async I/O with Tokio
- Multiple concurrent transports
- Named transport connections
- Automatic reconnection with configurable strategies
- Transport failover support
- Connection pooling
- Graceful shutdown
- Error propagation
- Dynamic transport management (add/remove at runtime)

**v0.2.0 Enhancements**:
- **Multiple Listeners**: Servers can listen on multiple addresses/ports simultaneously
- **Named Callers**: Clients can configure multiple named connections for failover
- **Reconnection Strategies**: Per-transport reconnection with exponential backoff, circuit breakers, etc.
- **Event Callbacks**: `TransportEventHandler` trait for connection lifecycle events
- **Transport Metadata**: Attach custom metadata to transports for routing/monitoring

#### 2. Serialization Layer

**Purpose**: Convert between Rust types and bytes

**Components**:
- `Serializer` trait: Pluggable serialization
- `PostcardSerializer`: Compact binary (default)
- `JsonSerializer`: Human-readable JSON
- `RkyvSerializer`: Zero-copy deserialization (planned)
- `Framing`: Length-prefixed message boundaries
- `BufferPool`: Efficient buffer reuse

**Key Features**:
- Multiple format support
- Frame integrity checking (CRC32)
- Buffer pooling for performance
- Configurable size limits

#### 3. Channel Layer

**Purpose**: Multiplexed, type-safe message passing

**Components**:
- `Channel<P>`: Typed channel for protocol P
- `ChannelManager`: Channel lifecycle and routing
- `Protocol` trait: Message type definitions
- `Envelope`: Internal message wrapper
- `ChannelId`: Unique channel identifier

**Key Features**:
- FIFO ordering per channel
- Type safety via generics
- Backpressure support
- Channel splitting (sender/receiver)

#### 3.1 Request-Response Correlation

**Purpose**: Enable concurrent RPC calls with proper request-response matching

**Overview**: 
BDRPC supports concurrent RPC operations through an optional correlation ID system. This allows multiple requests to be in-flight simultaneously without race conditions, dramatically improving throughput for RPC-style communication patterns.

**Components**:
- `CorrelationIdGenerator`: Thread-safe unique ID generation
- `PendingRequests<T>`: Tracks in-flight requests awaiting responses
- `RpcChannel<P>`: High-level API for concurrent RPC calls
- `Envelope.correlation_id`: Optional field for request-response matching

**Key Features**:
- Concurrent RPC calls without blocking
- Out-of-order response handling
- Type-safe correlation tracking
- Zero overhead for non-RPC channels
- Automatic timeout handling
- Request cancellation support

**Architecture**:

```text
Client Side:
┌─────────────────────────────────────────────────────┐
│                  Application                        │
│  tokio::join!(call1(), call2(), call3())           │
└────────────┬────────────────────────────────────────┘
             │ Multiple concurrent calls
             ↓
┌─────────────────────────────────────────────────────┐
│              RpcChannel / Client Stub               │
│  ┌──────────────────────────────────────────────┐  │
│  │  send_request() → correlation_id = 1         │  │
│  │  send_request() → correlation_id = 2         │  │
│  │  send_request() → correlation_id = 3         │  │
│  └──────────────────────────────────────────────┘  │
│                                                     │
│  ┌──────────────────────────────────────────────┐  │
│  │         PendingRequests<Protocol>            │  │
│  │  1 → oneshot::Sender<Response>               │  │
│  │  2 → oneshot::Sender<Response>               │  │
│  │  3 → oneshot::Sender<Response>               │  │
│  └──────────────────────────────────────────────┘  │
│                                                     │
│  ┌──────────────────────────────────────────────┐  │
│  │         Response Router Task                 │  │
│  │  Receives responses, matches correlation_id, │  │
│  │  completes corresponding pending request     │  │
│  └──────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────┘
             │ Requests with correlation IDs
             ↓
┌─────────────────────────────────────────────────────┐
│                  Channel Layer                      │
│  Envelope { correlation_id: Some(1), ... }         │
│  Envelope { correlation_id: Some(2), ... }         │
│  Envelope { correlation_id: Some(3), ... }         │
└─────────────────────────────────────────────────────┘
             │
             ↓ Transport
             
Server Side:
┌─────────────────────────────────────────────────────┐
│                  Channel Layer                      │
│  Receives envelopes with correlation IDs            │
└────────────┬────────────────────────────────────────┘
             │
             ↓
┌─────────────────────────────────────────────────────┐
│              Server Dispatcher                      │
│  dispatch_envelope(envelope) {                      │
│    let correlation_id = envelope.correlation_id;    │
│    let response = handle_request(envelope.payload); │
│    send_response(response, correlation_id);         │
│  }                                                  │
└─────────────────────────────────────────────────────┘
             │ Responses with same correlation IDs
             ↓
┌─────────────────────────────────────────────────────┐
│                  Channel Layer                      │
│  Envelope { correlation_id: Some(2), ... }         │
│  Envelope { correlation_id: Some(1), ... }         │
│  Envelope { correlation_id: Some(3), ... }         │
└─────────────────────────────────────────────────────┘
             │ (Note: responses may arrive out-of-order)
             ↓ Transport back to client
```

**Correlation ID Lifecycle**:

```text
1. Client generates unique correlation ID (atomic counter)
2. Client registers pending request with ID
3. Client sends request with correlation ID in envelope
4. Server receives request, preserves correlation ID
5. Server processes request (may take variable time)
6. Server sends response with same correlation ID
7. Client response router receives response
8. Router matches correlation ID to pending request
9. Router completes oneshot channel with response
10. Original caller receives response
```

**Concurrency Model**:

```rust
// Traditional (sequential) - slow requests block fast ones
let r1 = client.slow_operation().await?;  // Takes 1s
let r2 = client.fast_operation().await?;  // Takes 10ms, but waits 1s
// Total time: 1.01s

// With correlation (concurrent) - operations run in parallel
let (r1, r2) = tokio::join!(
    client.slow_operation(),  // Takes 1s
    client.fast_operation(),  // Takes 10ms
);
// Total time: 1s (limited by slowest operation)
```

**Design Decisions**:

1. **Optional Correlation**: The `correlation_id` field is `Option<u64>`, making it:
   - Zero-cost for non-RPC channels (streaming, notifications)
   - Backward compatible with existing code
   - Opt-in for RPC patterns

2. **Atomic ID Generation**: Uses `AtomicU64` for lock-free ID generation:
   - Thread-safe without mutex overhead
   - Monotonically increasing IDs
   - Starts at 1 (0 reserved for non-correlated messages)

3. **Oneshot Channels**: Each pending request gets a `oneshot::Sender`:
   - Efficient single-value communication
   - Automatic cleanup on drop
   - Type-safe response delivery

4. **Response Router Task**: Dedicated task routes responses:
   - Runs concurrently with application code
   - Matches correlation IDs efficiently
   - Handles out-of-order responses
   - Cleans up completed requests

**Performance Characteristics**:

- **ID Generation**: ~5 ns (atomic increment)
- **Request Registration**: ~100 ns (async mutex + HashMap insert)
- **Response Routing**: ~150 ns (async mutex + HashMap lookup + oneshot send)
- **Memory per pending request**: ~200 bytes (HashMap entry + oneshot channel)
- **Scalability**: Tested with 100+ concurrent requests

**Error Handling**:

```rust
// Timeout handling
match tokio::time::timeout(Duration::from_secs(30), response_rx).await {
    Ok(Ok(response)) => Ok(response),
    Ok(Err(_)) => Err(ChannelError::Closed),
    Err(_) => {
        // Cancel pending request on timeout
        pending.cancel(correlation_id).await;
        Err(ChannelError::Timeout)
    }
}

// Connection loss handling
// Response router task exits when channel closes
// All pending requests receive Err(RecvError)
```

**Backward Compatibility**:

- Non-RPC channels continue to work without correlation IDs
- `correlation_id: None` for streaming and notification patterns
- Existing code requires no changes
- Opt-in via `send_request()` / `send_response()` methods
- Generated client stubs automatically use correlation

**Use Cases**:

1. **Concurrent RPC**: Multiple simultaneous requests to same service
2. **Request Pipelining**: Send multiple requests without waiting
3. **Parallel Operations**: Fan-out requests to multiple services
4. **Mixed Latency**: Fast and slow operations don't block each other
5. **Load Testing**: Stress test with many concurrent requests

**Example Usage**:

```rust
// Manual API
let correlation_id = sender.send_request(request).await?;
let response_rx = pending.register(correlation_id).await;
let response = response_rx.await?;

// High-level RpcChannel API
let rpc = RpcChannel::new(sender, receiver);
let response = rpc.call(request).await?;

// Generated client stub (automatic)
let client = CalculatorClient::new(sender, receiver);
let (r1, r2, r3) = tokio::join!(
    client.add(5, 3),
    client.multiply(7, 6),
    client.divide(20, 4)
);
```

**Limitations**:

- Correlation IDs are per-connection (not globally unique)
- Maximum 2^64 requests per connection (practically unlimited)
- Pending requests consume memory until completed or timed out
- Response router task must be running to receive responses

**Future Enhancements**:

- Request cancellation API
- Streaming RPC with correlation
- Priority-based request handling
- Request batching with correlation
- Distributed tracing integration using correlation IDs


#### 4. Endpoint Layer

**Purpose**: High-level connection orchestration

**Components**:
- `Endpoint`: Main API entry point
- `Connection`: Handle to active connection
- `Listener`: Server-side acceptor
- `EndpointConfig`: Configuration options
- `Handshake`: Protocol negotiation
- `ProtocolDirection`: Call/respond capabilities

**Key Features**:
- Protocol registration
- Capability negotiation
- Version compatibility
- Automatic reconnection

## Core Components

### Protocol Definition

Protocols define the contract between endpoints:

```rust
pub trait Protocol: Send + Sync + 'static {
    type Request: Serialize + DeserializeOwned + Send + 'static;
    type Response: Serialize + DeserializeOwned + Send + 'static;
    fn name() -> &'static str;
}
```

**Design Rationale**:
- Type safety: Compile-time verification of message types
- Flexibility: Any serializable types can be used
- Simplicity: Minimal boilerplate required

### Channel Multiplexing

Multiple logical channels share a single transport connection:

```text
Transport Connection
│
├─ Channel 0 (System)
├─ Channel 1 (UserService)
├─ Channel 2 (Notifications)
└─ Channel 3 (FileTransfer)
```

**Benefits**:
- Efficient resource usage
- Independent flow control per channel
- Isolated error handling
- Concurrent operations

### Message Framing

Messages are framed with length prefix and optional integrity check:

```text
┌──────────┬──────────┬─────────────┬──────────┐
│  Length  │   CRC32  │   Payload   │  Padding │
│ (4 bytes)│ (4 bytes)│  (N bytes)  │ (0-3 B)  │
└──────────┴──────────┴─────────────┴──────────┘
```

**Features**:
- Reliable message boundaries
- Corruption detection
- Configurable size limits
- Efficient parsing

### Protocol Directionality

BDRPC supports three directional modes (ADR-008):

1. **CallOnly**: Can send requests, receive responses
2. **RespondOnly**: Can receive requests, send responses
3. **Bidirectional**: Can both call and respond

**Use Cases**:
- **Client-Server**: Client is CallOnly, Server is RespondOnly
- **Server Push**: Server is CallOnly, Client is RespondOnly
- **Peer-to-Peer**: Both are Bidirectional
- **Hybrid**: Different protocols with different directions

## Data Flow

### Client Request Flow

```text
1. Application calls channel.send(request)
2. Channel wraps in Envelope with sequence number
3. Serializer converts to bytes
4. Framing adds length prefix and CRC
5. Transport sends over network
6. Server transport receives bytes
7. Framing validates and extracts payload
8. Serializer deserializes to type
9. Channel routes to correct handler
10. Application processes request
11. Response follows reverse path
```

### Connection Establishment

```text
Client                          Server
  │                               │
  │─── TCP Connect ──────────────>│
  │                               │
  │─── Hello (capabilities) ─────>│
  │                               │
  │<─── Hello (capabilities) ─────│
  │                               │
  │─── Ack (negotiated) ─────────>│
  │                               │
  │<─── Ack (negotiated) ─────────│
  │                               │
  │  Connection Ready             │
  │                               │
  │─── Create Channel ───────────>│
  │                               │
  │<─── Channel Ready ────────────│
  │                               │
  │  Application Messages         │

### Enhanced Transport Manager (v0.2.0+)

The Enhanced Transport Manager introduced in v0.2.0 provides sophisticated transport lifecycle management:

```text
TransportManager
│
├─ Listener Transports (Servers)
│  ├─ TCP Listener #1 (0.0.0.0:8080)
│  ├─ TCP Listener #2 (0.0.0.0:8081)
│  └─ TLS Listener (0.0.0.0:8443)
│
├─ Caller Transports (Clients)
│  ├─ "primary" → 127.0.0.1:8080
│  │  ├─ Reconnection Strategy: ExponentialBackoff
│  │  ├─ State: Connected
│  │  └─ Active Connection: TransportId(42)
│  │
│  └─ "backup" → 127.0.0.1:8081
│     ├─ Reconnection Strategy: ExponentialBackoff
│     ├─ State: Disconnected
│     └─ Reconnection Task: Running
│
└─ Active Connections
   ├─ TransportId(42) → TCP connection
   ├─ TransportId(43) → TLS connection
   └─ TransportId(44) → TCP connection
```

**Key Components**:

1. **TransportListener**: Trait for server-side transport acceptance
   - Accepts incoming connections
   - Provides local address information
   - Supports graceful shutdown

2. **CallerTransport**: Client-side transport with reconnection
   - Named transport configuration
   - Automatic reconnection loop
   - State machine (Disconnected, Connecting, Connected, Reconnecting, Disabled)
   - Per-transport reconnection strategy

3. **TransportConnection**: Active connection tracking
   - Transport ID for identification
   - Connection metadata
   - Lifecycle management

4. **TransportEventHandler**: Lifecycle event callbacks
   - `on_transport_connected()`: New connection established
   - `on_transport_disconnected()`: Connection lost
   - `on_new_channel_request()`: Channel creation request

**Reconnection State Machine**:

```text
┌─────────────┐
│ Disconnected│
└──────┬──────┘
       │ connect()
       ↓
┌─────────────┐
│ Connecting  │
└──────┬──────┘
       │ success
       ↓
┌─────────────┐     connection lost
│  Connected  │────────────────────┐
└─────────────┘                    │
                                   ↓
                            ┌──────────────┐
                            │ Reconnecting │
                            └──────┬───────┘
                                   │
                                   │ retry with backoff
                                   └────────────┐
                                                │
                                   ┌────────────↓
                                   │
                            max attempts reached
                                   │
                                   ↓
                            ┌─────────────┐
                            │ Disconnected│
                            └─────────────┘
```

**Transport Lifecycle**:

```text
Server Startup:
1. Create TransportManager
2. Add listener transports (TCP, TLS, etc.)
3. Listeners automatically accept connections
4. Each connection triggers on_transport_connected()
5. System channels created automatically
6. Application channels negotiated on-demand

Client Startup:
1. Create TransportManager
2. Add caller transports with names
3. Configure reconnection strategies
4. Call connect_transport("name")
5. Automatic reconnection on failure
6. Channels restored after reconnection

Dynamic Management:
1. Add new transports at runtime
2. Remove unused transports
3. Enable/disable transports temporarily
4. Query transport status
5. Update reconnection strategies
```

**Benefits**:

- **Multiple Transports**: Run servers on multiple ports/protocols simultaneously
- **Named Connections**: Reference transports by name for clarity and failover
- **Automatic Reconnection**: Built-in reconnection with configurable strategies
- **Failover Support**: Switch between transports automatically
- **Dynamic Management**: Add/remove transports at runtime
- **Event-Driven**: React to connection lifecycle events
- **Metadata Support**: Attach custom metadata for routing/monitoring

For detailed configuration examples, see the [Transport Configuration Guide](transport-configuration.md).

```

### Error Propagation

Errors flow upward through layers with appropriate handling:

```text
Transport Error (connection lost)
    ↓
Close all channels
    ↓
Trigger reconnection (if configured)
    ↓
Notify application

Channel Error (protocol violation)
    ↓
Close affected channel
    ↓
Keep transport alive
    ↓
Notify application

Application Error (business logic)
    ↓
Return to caller
    ↓
No framework action
```

## Design Principles

### 1. Zero-Copy Where Possible

- Buffer pooling reduces allocations
- Direct serialization to transport buffers
- Minimal data copying in hot paths

### 2. Type Safety

- Compile-time protocol verification
- Generic channels prevent type confusion
- Strongly-typed error hierarchy

### 3. Async-First

- Built on Tokio for efficient concurrency
- Non-blocking I/O throughout
- Structured concurrency patterns

### 4. Composability

- Transports can be layered (TCP → TLS → Compression)
- Pluggable serialization
- Configurable reconnection strategies
- Custom backpressure policies

### 5. Observability

- Comprehensive metrics at every layer
- Distributed tracing support
- Health checks for orchestration
- Structured logging

### 6. Fail-Safe Defaults

- Bounded buffers prevent memory exhaustion
- Timeouts prevent deadlocks
- Frame size limits prevent DoS
- Automatic reconnection for resilience

## Key Features

### Automatic Reconnection

Configurable strategies handle connection failures:

```rust
ExponentialBackoff::new(
    Duration::from_secs(1),   // Start with 1s delay
    Duration::from_secs(60),  // Max 60s delay
    2.0,                      // Double each time
    Some(10),                 // Max 10 attempts
)
```

**Strategies**:
- Exponential backoff with jitter
- Fixed delay
- Circuit breaker pattern
- No reconnection

### Backpressure

Flow control prevents overwhelming receivers:

```rust
BoundedQueue::new(100)  // Buffer up to 100 messages
```

**Mechanisms**:
- Bounded channels with blocking
- Async waiting for capacity
- Per-channel flow control
- Metrics for monitoring

### Protocol Evolution

Version negotiation enables backward compatibility:

```text
Client: [v1, v2, v3]
Server: [v2, v3, v4]
Result: v3 (highest common)
```

**Features**:
- Semantic versioning
- Feature negotiation
- Graceful degradation
- Migration support

### Security

Multiple layers of security:

- **TLS Transport**: Encrypted connections
- **Frame Integrity**: CRC32 checksums
- **Size Limits**: Prevent resource exhaustion
- **Timeouts**: Prevent hanging connections

## Architecture Decision Records

Detailed design decisions are documented in ADRs:

- [ADR-001: Core Architecture](adr/ADR-001-core-architecture.md)
- [ADR-002: Reconnection Strategy](adr/ADR-002-reconnection-strategy.md)
- [ADR-003: Backpressure & Flow Control](adr/ADR-003-backpressure-flow-control.md)
- [ADR-004: Error Handling Hierarchy](adr/ADR-004-error-handling-hierarchy.md)
- [ADR-005: Serialization Strategy](adr/ADR-005-serialization-strategy.md)
- [ADR-006: Protocol Evolution & Versioning](adr/ADR-006-protocol-evolution-versioning.md)
- [ADR-007: Message Ordering Guarantees](adr/ADR-007-message-ordering-guarantees.md)
- [ADR-008: Protocol Directionality](adr/ADR-008-protocol-directionality.md)
- [ADR-009: Frame Integrity Checking](adr/ADR-009-frame-integrity-checking.md)
- [ADR-010: Dynamic Channel Negotiation](adr/ADR-010-dynamic-channel-negotiation.md)
- [ADR-011: Large Transfer Streaming](adr/ADR-011-large-transfer-streaming.md)
- [ADR-012: Channel-Transport Coupling](adr/ADR-012-channel-transport-coupling.md)

## Performance Characteristics

### Throughput

- **Single message**: 481K messages/second
- **Batch (1000)**: 4.02M messages/second
- **Scaling**: Linear with batch size

### Latency

- **Send operation**: 2-4 µs
- **Round-trip**: 4-8 µs
- **Channel creation**: 173 ns

### Memory

- **Per channel**: ~1 KB overhead
- **Buffer pooling**: Reduces allocations by 80%+
- **Zero-copy**: Minimal data copying

### Scalability

- **Channels**: Thousands per connection
- **Connections**: Limited by OS resources
- **Messages**: Millions per second

## Thread Safety

All components are thread-safe:

- **Endpoint**: Clone and share across tasks
- **Connection**: Concurrent channel creation
- **Channel**: Concurrent send/receive
- **Metrics**: Lock-free atomic counters

## Future Enhancements

Planned improvements:

1. **WebSocket Transport**: Browser compatibility
2. **QUIC Transport**: UDP-based with built-in encryption
3. **gRPC Compatibility**: Interop with gRPC services
4. **Schema Registry**: Centralized protocol management
5. **Service Mesh**: Integration with Istio/Linkerd
6. **Load Balancing**: Client-side load distribution

## Summary

BDRPC's architecture provides:

- ✅ Clean separation of concerns
- ✅ Type-safe communication
- ✅ High performance
- ✅ Production-ready reliability
- ✅ Comprehensive observability
- ✅ Flexible configuration

The layered design enables easy testing, maintenance, and extension while maintaining excellent performance characteristics.
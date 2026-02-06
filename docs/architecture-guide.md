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
- `TlsTransport`: Encrypted TCP with TLS
- `MemoryTransport`: In-process communication
- `CompressionTransport`: Transparent compression
- `TransportManager`: Connection lifecycle management

**Key Features**:
- Async I/O with Tokio
- Connection pooling
- Graceful shutdown
- Error propagation

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
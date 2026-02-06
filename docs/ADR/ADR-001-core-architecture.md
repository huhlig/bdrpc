# ADR-001: Core Architecture - Three-Layer Model

## Status
Accepted

## Context
BDRPC needs a clear architectural foundation that supports bi-directional communication, multiple protocols, and various transport mechanisms while remaining simple to understand and use.

## Decision
We will implement a three-layer architecture:

### Layer 1: Transport
- **Responsibility**: Raw byte stream communication
- **Interface**: Splittable `AsyncRead` + `AsyncWrite`
- **Variants**: 
  - TCP/TLS connections
  - In-memory queues (for testing)
  - Encrypted transports
  - Compressed transports
- **Key Trait**:
```rust
trait Transport: AsyncRead + AsyncWrite + Send + Sync {
    fn split(self) -> (impl AsyncRead, impl AsyncWrite);
}
```

### Layer 2: Protocol
- **Responsibility**: Message type definitions and serialization
- **Generation**: Derived from `#[bdrpc::service]` annotated traits
- **Structure**: Enum of all possible messages for a service
- **Key Concept**: Each protocol is a strongly-typed message catalog

```rust
#[bdrpc::service]
trait MyService {
    async fn method_a(&self, arg: String) -> Result<i32>;
    async fn method_b(&self, x: i32, y: i32) -> Result<String>;
}

// Generated:
enum MyServiceProtocol {
    MethodARequest { arg: String },
    MethodAResponse { result: Result<i32> },
    MethodBRequest { x: i32, y: i32 },
    MethodBResponse { result: Result<String> },
}
```

### Layer 3: Channel
- **Responsibility**: Routing and addressing
- **Composition**: Protocol + Identifier
- **Behavior**: Async stream of protocol messages
- **Key Property**: Each channel is independent and isolated

```rust
struct Channel<P: Protocol> {
    id: ChannelId,
    protocol: PhantomData<P>,
    sender: mpsc::Sender<P>,
    receiver: mpsc::Receiver<P>,
}
```

### Endpoint: The Orchestrator
- **Responsibility**: Manages transports and channels
- **Capabilities**: 
  - Acts as both client and server
  - Handles lifecycle (connect, disconnect, reconnect)
  - Routes messages to appropriate channels
  - Multiplexes multiple channels over transports

```rust
struct Endpoint<S: Serializer> {
    transports: HashMap<TransportId, Transport>,
    channels: HashMap<ChannelId, Box<dyn ChannelHandler>>,
    serializer: S,
    config: EndpointConfig,
}
```

## Consequences

### Positive
- **Clear separation of concerns**: Each layer has a single responsibility
- **Composability**: Layers can be mixed and matched
- **Testability**: Each layer can be tested independently
- **Type safety**: Protocols are strongly typed at compile time
- **Flexibility**: Easy to add new transports or protocols

### Negative
- **Complexity**: Three layers add conceptual overhead
- **Performance**: Layer boundaries may introduce overhead (mitigated by zero-cost abstractions)
- **Learning curve**: Developers must understand all three layers

### Neutral
- **Multiplexing**: Multiple channels share a transport, requiring careful design
- **Routing**: Channel IDs must be unique within an endpoint

## Alternatives Considered

### Single-Layer Approach
Combine transport and protocol into one abstraction. Rejected because it would make testing difficult and limit transport flexibility.

### Four-Layer with Session
Add a session layer between transport and protocol. Rejected as premature; can be added later if needed.

## Notes
- This architecture mirrors successful patterns from HTTP/2 (streams over connections) and gRPC
- The three-layer model maps cleanly to OSI layers: Transport (L4), Protocol (L6), Channel (L7)
- Future extensions (encryption, compression) fit naturally at the transport layer
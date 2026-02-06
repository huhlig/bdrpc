# Bi-Directional RPC (Bee-Dee-Arr-Pee-Sea)

BDRPC is a bi-directional RPC framework that allows for efficient and reliable communication between microservices. It 
is designed to provide a seamless experience for developers by abstracting away the complexities of network 
communication and providing a simple and intuitive API for building distributed systems.

## Fundamentals

BDRPC should be built on top of tokio and use serde or rykv for serialization and deserialization. API surfaces are 
traits annotated with bdrpc::service and written like normal traits. 

* Each connection is wrapped in a Transport, transports are splittable AsyncReader/AsyncWriters. Transports can be encrypted
or compressed, or simple in memory queues.
* Each interface trait decorated with `bdrpc::service` is turned into a protocol. A Protocol represents an enumeration of 
messages back and forth.
* Each protocol can be combined with an identifier to create a channel. A Channel is basically a stream of messages that can be 
read and written to asynchronously. The combination of protocol and identifier leads to easy routing and addressing.

Thus each Endpoint represents a set of transports, using channels to move messages described by protocols back and forth.

Each Endpoint is responsible for managing the lifecycle of transports and channels including reconnection, ensuring that 
messages are processed and delivered reliably. Each Endpoint can be configured both as a listener and/or a client.




## Bob's Comments

### Conceptual Review

The BDRPC architecture presents a well-thought-out layered approach to bi-directional RPC communication. Here are my 
observations:

**Strengths:**

1. **Clean Abstraction Layers**: The three-tier model (Transport → Protocol → Channel) provides excellent separation 
of concerns. This makes the system highly composable and testable.

2. **Transport Flexibility**: The splittable AsyncReader/AsyncWriter design is elegant. Supporting encryption, 
compression, and in-memory queues at the transport layer means these concerns don't leak into higher abstractions.

3. **Protocol Generation via Traits**: Using Rust traits with procedural macros (`bdrpc::service`) is idiomatic and 
will provide compile-time safety. This approach mirrors successful patterns from tonic/gRPC but adapted for 
bi-directional communication.

4. **Channel Addressing**: The protocol plus identifier combination for routing is simple yet powerful. This should 
scale well and make debugging easier since each channel has a clear identity.

5. **Unified Endpoint Model**: Having a single Endpoint type that can act as both client and listener eliminates 
artificial distinctions and simplifies peer-to-peer scenarios.

**Considerations:**

1. **Lifecycle Management Complexity**: The Endpoint's responsibility for reconnection and reliability is ambitious. 
Consider whether this should be pluggable (e.g., different strategies for different use cases: at-most-once, 
at-least-once, exactly-once semantics).

2. **Backpressure & Flow Control**: The design should explicitly address how backpressure propagates through the layers. 
With bi-directional communication, both sides can be producers and consumers simultaneously.

3. **Message Ordering Guarantees**: Within a channel, what ordering guarantees exist? Across channels on the same 
transport? This should be documented clearly.

4. **Error Handling Strategy**: How do errors at different layers (transport failure, deserialization error, protocol 
mismatch) propagate? Consider whether channels should have error types as part of their protocol definition.

5. **Serialization Choice**: Mentioning both serde and rykv is interesting. Rykv's zero-copy deserialization could be a 
significant performance win, but it's less mature. Consider making this pluggable at the protocol level.

**Architectural Questions:**

- How will multiplexing work when multiple channels share a transport?
- What's the strategy for protocol versioning and evolution?
- Should there be a discovery mechanism for available protocols on an endpoint?
- How will streaming/chunked messages be handled for large payloads?

Overall, this is a solid foundation that leverages Rust's strengths while learning from existing RPC frameworks. The 
bi-directional nature sets it apart from traditional request-response patterns and could enable interesting use cases 
like server-initiated updates, peer-to-peer communication, and reactive data flows.


### Design Clarifications & Refinements

Based on further discussion, here are refined thoughts on the key design decisions:

**1. Reconnection Strategy (Pluggable)**

The client-initiated connection model with pluggable reconnection strategies is the right approach. Here's how this could be structured:

```rust
trait ReconnectionStrategy: Send + Sync {
    async fn should_reconnect(&self, attempt: u32, last_error: &TransportError) -> bool;
    async fn next_delay(&self, attempt: u32) -> Duration;
    fn on_connected(&self);
    fn on_disconnected(&self, error: &TransportError);
}
```

Common implementations could include:
- **ExponentialBackoff**: Standard exponential backoff with jitter and max delay
- **FixedDelay**: Simple fixed interval retries
- **CircuitBreaker**: Stops attempting after threshold, requires manual reset
- **NoReconnect**: Fail immediately on disconnect (useful for testing)
- **Custom**: User-defined logic based on error types, time of day, etc.

The Endpoint would accept a `Box<dyn ReconnectionStrategy>` during client configuration, making it easy to swap strategies without changing core logic.

**2. Backpressure Strategies**

Backpressure in bi-directional systems is indeed complex. Potential pluggable strategies:

```rust
trait BackpressureStrategy: Send + Sync {
    async fn should_send(&self, channel_id: &ChannelId, queue_depth: usize) -> bool;
    fn on_message_sent(&self, channel_id: &ChannelId);
    fn on_message_received(&self, channel_id: &ChannelId);
    async fn wait_for_capacity(&self, channel_id: &ChannelId);
}
```

Strategies could include:
- **BoundedQueue**: Simple bounded channel with blocking sends
- **TokenBucket**: Rate limiting with burst capacity
- **SlidingWindow**: Credit-based flow control (similar to HTTP/2)
- **AdaptiveWindow**: Dynamically adjusts based on RTT and throughput
- **Priority**: Different limits for different message priorities

The key insight is that backpressure should be per-channel, not per-transport, since different protocols may have different throughput requirements.

**3. Ordering Guarantees - Clarified**

The ordering model is now clear and sensible:
- **Within a Channel**: FIFO ordering guaranteed (deterministic)
- **Across Channels**: No ordering guarantees (non-deterministic)
- **Across Transports**: No ordering guarantees (non-deterministic)

This is the right trade-off. It allows for:
- Parallel processing of independent message streams
- No head-of-line blocking between unrelated protocols
- Simple mental model for developers

Documentation should emphasize that if cross-channel ordering is needed, the application layer must handle it (e.g., via sequence numbers or causal ordering).

**4. Error Hierarchy - Three Layers**

The three-layer error model is elegant and maps well to the architecture:

```rust
// Layer 1: Transport Errors (connection-level)
enum TransportError {
    ConnectionLost,
    ConnectionRefused,
    Timeout,
    IoError(io::Error),
    EncryptionError,
    // etc.
}

// Layer 2: Channel Errors (protocol-level)
enum ChannelError {
    ProtocolMismatch,
    DeserializationError,
    ChannelClosed,
    InvalidChannelId,
    // etc.
}

// Layer 3: Application Errors (user-defined per protocol)
// These are defined in the trait and part of the protocol
```

The elegant handling could use Rust's error composition:

```rust
enum BdrpcError {
    Transport(TransportError),
    Channel(ChannelError),
    Application(Box<dyn Error + Send + Sync>),
}
```

Each layer can decide how to handle errors from lower layers:
- Transport errors → trigger reconnection strategy
- Channel errors → close specific channel, keep transport alive
- Application errors → propagate to user code, channel stays open

**5. Serialization Strategy - Per-Endpoint Choice**

Given that serde and rykv are incompatible in practice, the serialization choice should be made at the Endpoint level, not per-protocol. This means:

```rust
trait Serializer: Send + Sync {
    fn serialize<T: Serialize>(&self, value: &T) -> Result<Vec<u8>>;
    fn deserialize<T: Deserialize>(&self, bytes: &[u8]) -> Result<T>;
}

struct SerdeSerializer;
struct RkyvSerializer;

// Endpoint is generic over serializer
struct Endpoint<S: Serializer> {
    serializer: S,
    // ...
}
```

This approach:
- Keeps the choice explicit and visible
- Allows compile-time optimization for the chosen serializer
- Prevents accidental mixing of incompatible serialization formats
- Makes it clear that both endpoints must agree on serialization

The trade-off is that you can't mix serializers on a single endpoint, but this is actually a feature—it prevents subtle bugs and makes the system more predictable.

**Additional Thoughts:**

- Consider adding a **health check protocol** as a built-in channel that all endpoints support
- **Metrics and observability** should be first-class: expose channel queue depths, transport stats, error rates
- **Graceful shutdown** needs careful design: drain in-flight messages, close channels cleanly, notify peers
- **Protocol negotiation** during connection establishment could allow version compatibility checks

The refined design maintains simplicity while providing the flexibility needed for real-world distributed systems.


### Protocol Evolution & Versioning

Protocol evolution is indeed critical for long-lived distributed systems. Here's a comprehensive approach:

**Version Negotiation at Connection Time**

When a transport is established, endpoints should exchange capability information:

```rust
struct ProtocolCapability {
    protocol_name: String,
    supported_versions: Vec<u32>,
    features: HashSet<String>,
}

// During handshake
struct Handshake {
    endpoint_id: String,
    protocols: Vec<ProtocolCapability>,
}
```

This allows both sides to:
- Discover what protocols the peer supports
- Negotiate the highest mutually supported version
- Enable/disable optional features based on peer capabilities

**Evolution Strategies**

1. **Additive Changes (Backward Compatible)**
   - New optional fields in messages (use `Option<T>`)
   - New message types in the protocol enum
   - New methods in the service trait with default implementations
   
   ```rust
   #[bdrpc::service]
   trait MyService {
       async fn old_method(&self, arg: String) -> Result<i32>;
       
       #[since(version = 2)]
       async fn new_method(&self, arg: String) -> Result<String> {
           // Default implementation for v1 clients
           Err(ChannelError::UnsupportedMethod)
       }
   }
   ```

2. **Field Evolution**
   - Use `#[serde(default)]` for new optional fields
   - Use `#[serde(skip_serializing_if = "Option::is_none")]` to avoid sending unknowns
   - Consider using `Unknown` variants in enums for forward compatibility
   
   ```rust
   #[derive(Serialize, Deserialize)]
   enum Message {
       TypeA { data: String },
       TypeB { count: i32 },
       #[serde(other)]
       Unknown,  // Allows newer messages to be received without error
   }
   ```

3. **Breaking Changes (Version Bump)**
   - When changes aren't backward compatible, increment major version
   - Endpoint can support multiple versions simultaneously
   - Each channel negotiates its version independently
   
   ```rust
   enum MyServiceV1 { /* old protocol */ }
   enum MyServiceV2 { /* new protocol */ }
   
   // Adapter pattern to bridge versions
   struct MyServiceAdapter {
       v1_handler: Option<Box<dyn MyServiceV1>>,
       v2_handler: Box<dyn MyServiceV2>,
   }
   ```

**Feature Flags**

Beyond versions, feature flags allow fine-grained evolution:

```rust
#[bdrpc::service]
#[features("compression", "streaming", "priority")]
trait AdvancedService {
    #[requires_feature("streaming")]
    async fn stream_data(&self) -> impl Stream<Item = Data>;
    
    #[requires_feature("priority")]
    async fn urgent_request(&self, priority: u8) -> Result<()>;
}
```

Features can be:
- Negotiated during handshake
- Enabled/disabled per channel
- Used to gate functionality without version bumps

**Deprecation Path**

For graceful evolution, support a deprecation workflow:

```rust
#[bdrpc::service]
trait MyService {
    #[deprecated(since = "2.0", note = "Use new_method instead")]
    async fn old_method(&self) -> Result<i32>;
    
    #[since(version = 2)]
    async fn new_method(&self) -> Result<i32>;
}
```

The framework could:
- Log warnings when deprecated methods are called
- Track usage metrics for deprecation planning
- Eventually remove support in a future major version

**Schema Registry Pattern**

For complex systems, consider a schema registry approach:

```rust
trait SchemaRegistry {
    fn register_protocol(&self, name: &str, version: u32, schema: ProtocolSchema);
    fn get_compatible_version(&self, name: &str, versions: &[u32]) -> Option<u32>;
    fn can_upgrade(&self, from: u32, to: u32) -> bool;
}
```

This allows:
- Centralized protocol version management
- Runtime compatibility checks
- Automated migration paths between versions

**Practical Evolution Example**

```rust
// Version 1: Initial protocol
#[bdrpc::service(version = 1)]
trait UserService {
    async fn get_user(&self, id: u64) -> Result<User>;
}

// Version 2: Add optional email field
#[derive(Serialize, Deserialize)]
struct User {
    id: u64,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    email: Option<String>,  // New in v2
}

// Version 3: Add new method, keep backward compatibility
#[bdrpc::service(version = 3)]
trait UserService {
    async fn get_user(&self, id: u64) -> Result<User>;
    
    #[since(version = 3)]
    async fn search_users(&self, query: String) -> Result<Vec<User>> {
        // Default: not supported in older versions
        Err(ChannelError::UnsupportedMethod)
    }
}
```

**Key Principles**

1. **Explicit is better than implicit**: Version numbers and feature flags should be visible in the API
2. **Fail gracefully**: Unknown messages/methods should not crash, but return clear errors
3. **Support N-1 versions**: Maintain compatibility with at least the previous major version
4. **Metrics matter**: Track version usage to inform deprecation decisions
5. **Test compatibility**: Automated tests should verify cross-version communication

**Implementation Notes**

The `#[bdrpc::service]` macro could generate:
- Version metadata embedded in the protocol enum
- Compatibility checking code
- Automatic serialization of version info in message headers
- Runtime version negotiation logic

This approach balances the need for evolution with the complexity of maintaining multiple versions, giving developers clear tools for managing protocol lifecycles.

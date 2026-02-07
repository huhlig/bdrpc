# ADR-013: Enhanced Transport Manager

**Status:** Accepted  
**Date:** 2026-02-07  
**Deciders:** Core Team  
**Related:** [ADR-012: Channel-Transport Coupling](ADR-012-channel-transport-coupling.md)

## Context

The current BDRPC v0.1.0 architecture has a `TransportManager` that only tracks transport metadata. The actual transport lifecycle management (connecting, listening, reconnection) is handled directly by the `Endpoint`. This creates several limitations:

1. **Single Transport Type**: Each endpoint can only use one transport type at a time
2. **Manual Reconnection**: Reconnection logic must be implemented by users
3. **No Dynamic Management**: Cannot enable/disable transports at runtime
4. **Limited Observability**: No centralized events for transport lifecycle
5. **Tight Coupling**: Endpoint is tightly coupled to transport implementation details

For v0.2.0, we need an enhanced `TransportManager` that:
- Supports multiple transport types simultaneously (TCP, TLS, WebSocket, QUIC, etc.)
- Manages both listener (server) and caller (client) transports
- Provides automatic reconnection for caller transports
- Enables dynamic transport enable/disable
- Offers lifecycle event callbacks
- Maintains backward compatibility where possible

## Decision

We will implement an enhanced `TransportManager` with the following architecture:

### Core Types

#### 1. TransportEventHandler Trait

```rust
pub trait TransportEventHandler: Send + Sync {
    fn on_transport_connected(&self, transport_id: TransportId);
    fn on_transport_disconnected(&self, transport_id: TransportId, error: Option<TransportError>);
    fn on_new_channel_request(&self, channel_id: ChannelId, protocol: &str, transport_id: TransportId) -> Result<bool, String>;
}
```

**Purpose:** Allows the endpoint to respond to transport lifecycle events.

**Design Rationale:**
- Synchronous callbacks for simplicity (async can be spawned inside)
- Returns `Result<bool, String>` for channel requests to allow accept/reject decisions
- Separate methods for different event types for clarity

#### 2. TransportListener Trait

```rust
#[async_trait::async_trait]
pub trait TransportListener: Send + Sync {
    type Transport: Transport;
    
    async fn accept(&self) -> Result<Self::Transport, TransportError>;
    fn local_addr(&self) -> Result<String, TransportError>;
    async fn shutdown(&self) -> Result<(), TransportError>;
}
```

**Purpose:** Abstracts server-side transport acceptance.

**Design Rationale:**
- Associated type `Transport` for type safety (not `Box<dyn Transport>` due to object safety)
- Async `accept()` for non-blocking operation
- Synchronous `local_addr()` as it's typically a simple getter
- Async `shutdown()` for graceful cleanup

#### 3. CallerTransport Struct

```rust
pub struct CallerTransport {
    name: String,
    config: TransportConfig,
    state: Arc<RwLock<CallerState>>,
    reconnection_task: Arc<RwLock<Option<JoinHandle<()>>>>,
}

pub enum CallerState {
    Disconnected,
    Connecting,
    Connected(TransportId),
    Reconnecting { attempt: u32, last_error: String },
    Disabled,
}
```

**Purpose:** Manages client-side transport with automatic reconnection.

**Design Rationale:**
- Separate state enum for clear state machine
- `Arc<RwLock<>>` for shared mutable state across tasks
- Optional reconnection task handle for lifecycle management
- Name-based identification for user-friendly API

#### 4. TransportConnection Struct

```rust
pub struct TransportConnection {
    transport_id: TransportId,
    transport_type: TransportType,
    caller_name: Option<String>,
    connected_at: std::time::Instant,
}
```

**Purpose:** Tracks metadata about active connections.

**Design Rationale:**
- Distinguishes client (has `caller_name`) from server (no `caller_name`) connections
- Tracks connection time for metrics and debugging
- Immutable after creation for simplicity

### Enhanced TransportManager

The enhanced `TransportManager` will have:

```rust
pub struct TransportManager {
    // Listener transports (servers)
    listeners: Arc<RwLock<HashMap<String, Box<dyn TransportListener>>>>,
    
    // Caller transports (clients)
    callers: Arc<RwLock<HashMap<String, CallerTransport>>>,
    
    // Active connections
    connections: Arc<RwLock<HashMap<TransportId, TransportConnection>>>,
    
    // Event handler
    event_handler: Option<Arc<dyn TransportEventHandler>>,
    
    // Existing fields...
    next_id: AtomicU64,
}
```

**Key Methods:**
- `add_listener(name, config)` - Add a server transport
- `add_caller(name, config)` - Add a client transport with reconnection
- `remove_listener(name)` - Remove a server transport
- `remove_caller(name)` - Remove a client transport
- `enable_transport(name)` - Enable a transport
- `disable_transport(name)` - Disable a transport
- `connect_caller(name)` - Manually trigger connection
- `set_event_handler(handler)` - Set lifecycle event handler

### Integration with Endpoint

The `Endpoint` will implement `TransportEventHandler`:

```rust
impl<S: Serializer> TransportEventHandler for Endpoint<S> {
    fn on_transport_connected(&self, transport_id: TransportId) {
        // Spawn system message handler
        // Initialize channels for this transport
    }
    
    fn on_transport_disconnected(&self, transport_id: TransportId, error: Option<TransportError>) {
        // Clean up channels
        // Log disconnection
    }
    
    fn on_new_channel_request(&self, channel_id: ChannelId, protocol: &str, transport_id: TransportId) -> Result<bool, String> {
        // Delegate to channel_negotiator
    }
}
```

## Consequences

### Positive

1. **Multiple Transports**: Endpoints can use TCP, TLS, WebSocket, etc. simultaneously
2. **Automatic Reconnection**: Built-in reconnection for client transports
3. **Dynamic Management**: Enable/disable transports at runtime
4. **Better Observability**: Centralized lifecycle events
5. **Cleaner Separation**: Transport management separated from endpoint logic
6. **Extensibility**: Easy to add new transport types

### Negative

1. **Breaking Changes**: API changes from v0.1.0 (mitigated with shims)
2. **Increased Complexity**: More types and abstractions to understand
3. **Migration Effort**: Users must update their code
4. **Performance Overhead**: Additional indirection and state tracking (minimal)

### Neutral

1. **Learning Curve**: New concepts for users to learn
2. **Documentation Burden**: More comprehensive docs needed
3. **Testing Complexity**: More scenarios to test

## Implementation Plan

See [Transport Manager Enhancement Plan](../dev/transport-manager-enhancement-plan.md) for detailed phased implementation.

**Phase 1 (Complete):** Foundation types and traits
**Phase 2:** Enhanced TransportManager core
**Phase 3:** Reconnection integration
**Phase 4:** Endpoint integration
**Phase 5:** EndpointBuilder enhancement
**Phase 6:** Examples and documentation
**Phase 7:** Testing and hardening
**Phase 8:** Migration tools and release

## Alternatives Considered

### Alternative 1: Keep Current Architecture

**Pros:**
- No breaking changes
- Simpler to maintain

**Cons:**
- Doesn't address limitations
- Users must implement reconnection themselves
- No multi-transport support

**Decision:** Rejected - doesn't meet v0.2.0 goals

### Alternative 2: Separate TransportManager Library

**Pros:**
- Could be used by other projects
- Clear separation of concerns

**Cons:**
- Additional maintenance burden
- Overkill for current needs
- Harder to integrate tightly with BDRPC

**Decision:** Rejected - premature abstraction

### Alternative 3: Event-Driven Architecture with Channels

**Pros:**
- More Rust-idiomatic
- Better decoupling

**Cons:**
- More complex implementation
- Harder to reason about
- Performance overhead of channels

**Decision:** Rejected - callbacks are simpler and sufficient

## Migration Strategy

### Backward Compatibility Shims

Provide deprecated methods that map to new API:

```rust
#[deprecated(since = "0.2.0", note = "Use add_caller and connect_transport instead")]
pub async fn connect(&mut self, addr: impl ToString) -> Result<Connection, EndpointError> {
    // Create temporary caller transport
    // Connect and return
}
```

### Migration Guide

Provide comprehensive guide with examples:
- Before/after code samples
- Common patterns
- Troubleshooting

### Deprecation Timeline

- v0.2.0: Deprecation warnings, shims provided
- v0.3.0: Shims still available but marked for removal
- v1.0.0: Shims removed, new API only

## References

- [Transport Manager Enhancement Plan](../dev/transport-manager-enhancement-plan.md)
- [ADR-012: Channel-Transport Coupling](ADR-012-channel-transport-coupling.md)
- [Post-v0.1.0 Roadmap](../dev/post-v0.1.0-roadmap.md)

## Notes

- Phase 1 completed 2026-02-07
- All foundation types implemented and tested
- 7 unit tests passing for new types
- Ready to proceed to Phase 2

---

**Last Updated:** 2026-02-07  
**Next Review:** Start of Phase 2
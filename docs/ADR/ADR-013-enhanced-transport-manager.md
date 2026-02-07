# ADR-013: Enhanced Transport Manager

**Status**: Accepted  
**Date**: 2026-02-07  
**Deciders**: Development Team  
**Related**: ADR-001 (Core Architecture), ADR-002 (Reconnection Strategy), ADR-012 (Channel-Transport Coupling)

## Context

The current BDRPC architecture has several limitations in transport management:

1. **Single Transport Focus**: The Endpoint directly manages connect/listen operations, making it difficult to support multiple transport types simultaneously
2. **No Reconnection Management**: Reconnection logic is ad-hoc and not centrally managed
3. **Limited Flexibility**: Cannot dynamically enable/disable transports or switch between them
4. **Tight Coupling**: Endpoint is tightly coupled to transport lifecycle, making it harder to test and extend
5. **No Multi-Transport Support**: Cannot run multiple listeners (e.g., TCP + TLS + WebSocket) simultaneously

### Current Architecture Issues

```rust
// Current: Endpoint directly manages transports
let mut endpoint = Endpoint::new(serializer, config);
let conn = endpoint.connect("127.0.0.1:8080").await?;  // Only one transport type
let listener = endpoint.listen("0.0.0.0:8080").await?;  // Only one listener
```

Problems:
- No way to have multiple listeners on different ports/protocols
- No automatic reconnection management
- Transport lifecycle tied to Endpoint lifecycle
- Difficult to add new transport types
- No transport-level observability

## Decision

We will enhance the `TransportManager` to become a full-featured transport lifecycle manager that:

1. **Manages Multiple Transports**: Support multiple listener and caller transports simultaneously
2. **Handles Reconnection**: Automatically reconnect caller transports using pluggable strategies
3. **Provides Lifecycle Management**: Enable/disable transports dynamically
4. **Offers Event Callbacks**: Notify on transport lifecycle events (connect, disconnect, new channels)
5. **Maintains Separation**: Keep transport concerns separate from endpoint protocol logic

### New Architecture

```rust
// Enhanced: TransportManager handles all transport concerns
let endpoint = EndpointBuilder::server(serializer)
    .with_tcp_listener("0.0.0.0:8080")
    .with_tls_listener("0.0.0.0:8443", tls_config)
    .with_tcp_caller("backup", "backup.example.com:8080")
    .with_reconnection_strategy("backup", ExponentialBackoff::default())
    .with_responder("UserService", 1)
    .build()
    .await?;

// Server automatically accepts on all configured listeners
// Callers automatically reconnect on failure
```

### Key Components

#### 1. TransportConfig
Configuration for a single transport (listener or caller):

```rust
pub struct TransportConfig {
    pub transport_type: TransportType,
    pub address: String,
    pub reconnection_strategy: Option<Arc<dyn ReconnectionStrategy>>,
    pub enabled: bool,
    pub metadata: HashMap<String, String>,
}
```

#### 2. TransportEventHandler
Trait for receiving transport lifecycle events:

```rust
pub trait TransportEventHandler: Send + Sync {
    fn on_transport_connected(&self, transport_id: TransportId);
    fn on_transport_disconnected(&self, transport_id: TransportId, error: Option<TransportError>);
    fn on_new_channel_request(&self, channel_id: ChannelId, protocol: &str, transport_id: TransportId) -> Result<bool, String>;
}
```

#### 3. TransportListener
Trait for transport listeners (servers):

```rust
pub trait TransportListener: Send + Sync {
    async fn accept(&self) -> Result<Box<dyn Transport>, TransportError>;
    fn local_addr(&self) -> Result<String, TransportError>;
    async fn shutdown(&self) -> Result<(), TransportError>;
}
```

#### 4. Enhanced TransportManager
Manages all transports:

```rust
pub struct TransportManager {
    listeners: Arc<RwLock<HashMap<String, Box<dyn TransportListener>>>>,
    callers: Arc<RwLock<HashMap<String, CallerTransport>>>,
    connections: Arc<RwLock<HashMap<TransportId, TransportConnection>>>,
    event_handler: Option<Arc<dyn TransportEventHandler>>,
    reconnection_strategies: HashMap<String, Arc<dyn ReconnectionStrategy>>,
}
```

## Consequences

### Positive

1. **Multi-Transport Support**: Can run multiple listeners and callers simultaneously
2. **Automatic Reconnection**: Centralized reconnection management with pluggable strategies
3. **Better Separation**: Transport concerns separated from protocol/channel logic
4. **Extensibility**: Easy to add new transport types (WebSocket, QUIC, etc.)
5. **Observability**: Event callbacks enable monitoring and metrics
6. **Flexibility**: Dynamic enable/disable of transports
7. **Testability**: Easier to test transport logic in isolation

### Negative

1. **Breaking Changes**: Requires API migration from v0.1.0
2. **Complexity**: More moving parts to understand and maintain
3. **Migration Effort**: All examples and documentation need updates
4. **Learning Curve**: Users need to understand new configuration model

### Neutral

1. **Performance**: Should be similar or better (more efficient connection management)
2. **Memory**: Slightly higher due to additional tracking structures
3. **Code Size**: Larger codebase but better organized

## Implementation Strategy

### Phase 1: Foundation (Non-Breaking)
- Add new types and traits alongside existing code
- No changes to existing Endpoint API
- Comprehensive unit tests

### Phase 2: Integration
- Refactor TransportManager internals
- Update Endpoint to use enhanced TransportManager
- Maintain backward compatibility with shims

### Phase 3: Migration
- Add deprecation warnings to old methods
- Provide migration guide and examples
- Update all documentation

### Phase 4: Cleanup (v1.0.0)
- Remove deprecated methods
- Finalize API

## Alternatives Considered

### Alternative 1: Keep Current Architecture
**Rejected**: Doesn't solve multi-transport or reconnection problems. Would require ad-hoc solutions for each use case.

### Alternative 2: Separate Transport Service
**Rejected**: Too much indirection. Transport management is core to RPC functionality and should be integrated.

### Alternative 3: Plugin-Based Transport System
**Rejected**: Over-engineered for current needs. Can be added later if needed.

### Alternative 4: Configuration-Only Approach
**Rejected**: Not flexible enough for dynamic transport management. Need programmatic control.

## Migration Path

### v0.1.0 → v0.2.0

**Old API (deprecated but working):**
```rust
let mut endpoint = Endpoint::new(serializer, config);
let conn = endpoint.connect("127.0.0.1:8080").await?;
```

**New API:**
```rust
let endpoint = EndpointBuilder::client(serializer)
    .with_tcp_caller("main", "127.0.0.1:8080")
    .with_caller("UserService", 1)
    .build()
    .await?;
let conn = endpoint.connect_transport("main").await?;
```

### Compatibility Shims

Provide shims for one release cycle:
```rust
#[deprecated(since = "0.2.0", note = "Use add_caller and connect_transport")]
pub async fn connect(&mut self, addr: impl ToString) -> Result<Connection, EndpointError> {
    // Create temporary caller transport
    // Connect and return
}
```

## Success Metrics

1. **Functionality**: All existing tests pass with new implementation
2. **Performance**: No regression in throughput or latency
3. **Adoption**: Migration guide enables smooth transition
4. **Extensibility**: New transport types can be added easily
5. **Reliability**: Reconnection works reliably under various failure scenarios

## References

- [Transport Manager Enhancement Plan](../dev/transport-manager-enhancement-plan.md)
- [ADR-001: Core Architecture](ADR-001-core-architecture.md)
- [ADR-002: Reconnection Strategy](ADR-002-reconnection-strategy.md)
- [ADR-012: Channel-Transport Coupling](ADR-012-channel-transport-coupling.md)

## Notes

This ADR represents a significant architectural evolution of BDRPC. The enhanced Transport Manager will enable:
- Production-ready multi-transport deployments
- Reliable automatic reconnection
- Better separation of concerns
- Foundation for future transport types (WebSocket, QUIC, etc.)

The phased implementation approach ensures we can develop incrementally while maintaining a single coordinated release to minimize disruption.

---

**Approved By**: Development Team  
**Implementation**: See [Transport Manager Enhancement Plan](../dev/transport-manager-enhancement-plan.md)
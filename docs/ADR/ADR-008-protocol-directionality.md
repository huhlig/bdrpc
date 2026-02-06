# ADR-008: Protocol Directionality and Endpoint Capabilities

## Status
Proposed

## Context
In a bi-directional RPC system, not all endpoints support all directions of a protocol. Consider a session control protocol:
- **Endpoint A** (client) can **call** session methods (initiate requests)
- **Endpoint B** (server) can **respond** to session methods (handle requests)
- But Endpoint B may not be able to **call** session methods itself

Currently, BDRPC assumes that if an endpoint supports a protocol, it supports both calling and responding. This is too coarse-grained and doesn't reflect real-world asymmetric communication patterns.

### Real-World Examples
1. **Client-Server RPC**: Client calls, server responds (traditional RPC)
2. **Server Push**: Server calls, client responds (notifications, updates)
3. **Peer-to-Peer**: Both sides can call and respond (symmetric)
4. **Hybrid**: Some protocols are symmetric, others asymmetric on the same endpoint

### Current Limitations
- No way to express that an endpoint only responds to a protocol
- No way to prevent invalid calls at compile time
- Protocol negotiation doesn't include directionality information
- Error messages are unclear when calling unsupported directions

## Decision

### Protocol Direction Enum
We will introduce a `ProtocolDirection` enum to specify which directions an endpoint supports:

```rust
/// Specifies which directions of a protocol an endpoint supports
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProtocolDirection {
    /// Can only call methods (send requests, receive responses)
    CallOnly,
    /// Can only respond to methods (receive requests, send responses)
    RespondOnly,
    /// Can both call and respond (full bi-directional)
    Bidirectional,
}

impl ProtocolDirection {
    /// Check if this direction allows calling methods
    pub fn can_call(&self) -> bool {
        matches!(self, Self::CallOnly | Self::Bidirectional)
    }

    /// Check if this direction allows responding to methods
    pub fn can_respond(&self) -> bool {
        matches!(self, Self::RespondOnly | Self::Bidirectional)
    }

    /// Check if this direction is compatible with another
    /// (i.e., can they communicate?)
    pub fn is_compatible_with(&self, other: &Self) -> bool {
        self.can_call() && other.can_respond() || 
        self.can_respond() && other.can_call()
    }
}
```

### Protocol Capability Structure
Extend `ProtocolCapability` to include direction information:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolCapability {
    /// Protocol name (e.g., "UserService")
    pub protocol_name: String,
    /// Supported versions (e.g., [1, 2, 3])
    pub supported_versions: Vec<u32>,
    /// Optional features (e.g., ["streaming", "compression"])
    pub features: HashSet<String>,
    /// Direction support for this protocol
    pub direction: ProtocolDirection,
}
```

### Endpoint Registration
Endpoints register protocols with their supported direction:

```rust
impl<S: Serializer> Endpoint<S> {
    /// Register a protocol with call-only support
    pub fn register_caller<P: Protocol>(&mut self) -> Result<()> {
        self.register_protocol::<P>(ProtocolDirection::CallOnly)
    }

    /// Register a protocol with respond-only support
    pub fn register_responder<P: Protocol>(&mut self, handler: Arc<dyn P::Handler>) -> Result<()> {
        self.register_protocol::<P>(ProtocolDirection::RespondOnly)?;
        self.register_handler::<P>(handler)
    }

    /// Register a protocol with bidirectional support
    pub fn register_bidirectional<P: Protocol>(&mut self, handler: Arc<dyn P::Handler>) -> Result<()> {
        self.register_protocol::<P>(ProtocolDirection::Bidirectional)?;
        self.register_handler::<P>(handler)
    }

    /// Internal registration with direction
    fn register_protocol<P: Protocol>(&mut self, direction: ProtocolDirection) -> Result<()> {
        let capability = ProtocolCapability {
            protocol_name: P::NAME.to_string(),
            supported_versions: vec![P::VERSION],
            features: P::FEATURES.iter().map(|s| s.to_string()).collect(),
            direction,
        };
        self.capabilities.insert(P::NAME.to_string(), capability);
        Ok(())
    }
}
```

### Handshake Negotiation
During handshake, endpoints exchange capabilities including direction:

```rust
impl<S: Serializer> Endpoint<S> {
    async fn negotiate_protocol(
        &self,
        protocol_name: &str,
        peer_capability: &ProtocolCapability,
    ) -> Result<NegotiatedProtocol> {
        let our_capability = self.capabilities.get(protocol_name)
            .ok_or(ChannelError::ProtocolNotSupported)?;

        // Check direction compatibility
        if !our_capability.direction.is_compatible_with(&peer_capability.direction) {
            return Err(ChannelError::IncompatibleDirection {
                protocol: protocol_name.to_string(),
                our_direction: our_capability.direction,
                peer_direction: peer_capability.direction,
            });
        }

        // Determine negotiated directions
        let negotiated = NegotiatedProtocol {
            name: protocol_name.to_string(),
            version: negotiate_version(our_capability, peer_capability)?,
            features: negotiate_features(our_capability, peer_capability),
            we_can_call: our_capability.direction.can_call() && peer_capability.direction.can_respond(),
            we_can_respond: our_capability.direction.can_respond() && peer_capability.direction.can_call(),
        };

        Ok(negotiated)
    }
}
```

### Runtime Validation
Validate direction at call time:

```rust
impl<P: Protocol> Channel<P> {
    /// Send a request (call a method)
    pub async fn call(&self, request: P::Request) -> Result<P::Response> {
        // Check if we can call this protocol
        if !self.negotiated.we_can_call {
            return Err(ChannelError::DirectionNotSupported {
                protocol: P::NAME,
                direction: "call",
                our_direction: self.our_direction,
            });
        }

        // Proceed with call
        self.send_request(request).await
    }

    /// This is called internally when receiving a request
    async fn handle_request(&self, request: P::Request) -> Result<P::Response> {
        // Check if we can respond to this protocol
        if !self.negotiated.we_can_respond {
            return Err(ChannelError::DirectionNotSupported {
                protocol: P::NAME,
                direction: "respond",
                our_direction: self.our_direction,
            });
        }

        // Dispatch to handler
        self.handler.handle(request).await
    }
}
```

### Macro Support
The `#[bdrpc::service]` macro can include direction hints:

```rust
// Call-only (client)
#[bdrpc::service(direction = "call")]
trait UserService {
    async fn get_user(&self, id: u64) -> Result<User>;
}

// Respond-only (server)
#[bdrpc::service(direction = "respond")]
trait UserService {
    async fn get_user(&self, id: u64) -> Result<User>;
}

// Bidirectional (default)
#[bdrpc::service(direction = "bidirectional")]
trait UserService {
    async fn get_user(&self, id: u64) -> Result<User>;
}

// Or omit for default bidirectional
#[bdrpc::service]
trait UserService {
    async fn get_user(&self, id: u64) -> Result<User>;
}
```

## Examples

### Example 1: Traditional Client-Server
```rust
// Server endpoint
let mut server = Endpoint::new(BincodeSerializer::default(), config);
server.register_responder::<UserService>(Arc::new(UserServiceImpl))?;
server.listen("127.0.0.1:8080").await?;

// Client endpoint
let mut client = Endpoint::new(BincodeSerializer::default(), config);
client.register_caller::<UserService>()?;
let channel = client.connect_and_create_channel::<UserService>("127.0.0.1:8080").await?;

// Client can call
let user = channel.call(GetUserRequest { id: 42 }).await?;

// Server cannot call (would fail at registration or runtime)
```

### Example 2: Server Push Notifications
```rust
// Server can push notifications
let mut server = Endpoint::new(BincodeSerializer::default(), config);
server.register_caller::<NotificationService>()?;
server.register_responder::<UserService>(Arc::new(UserServiceImpl))?;

// Client can receive notifications
let mut client = Endpoint::new(BincodeSerializer::default(), config);
client.register_responder::<NotificationService>(Arc::new(NotificationHandler))?;
client.register_caller::<UserService>()?;

// After connection, server can push
let notification_channel = server.get_channel::<NotificationService>(client_id).await?;
notification_channel.call(NotifyRequest { message: "Update available" }).await?;
```

### Example 3: Peer-to-Peer
```rust
// Both peers support bidirectional
let mut peer_a = Endpoint::new(BincodeSerializer::default(), config);
peer_a.register_bidirectional::<ChatService>(Arc::new(ChatHandler))?;

let mut peer_b = Endpoint::new(BincodeSerializer::default(), config);
peer_b.register_bidirectional::<ChatService>(Arc::new(ChatHandler))?;

// Either peer can initiate messages
```

## Consequences

### Positive
- **Explicit Intent**: Clear declaration of endpoint capabilities
- **Type Safety**: Compile-time and runtime validation of direction support
- **Better Errors**: Clear error messages when direction is unsupported
- **Flexibility**: Supports asymmetric, symmetric, and hybrid patterns
- **Protocol Negotiation**: Peers can verify compatibility before communication
- **Documentation**: Self-documenting code (clear what each endpoint does)

### Negative
- **API Complexity**: More methods and concepts to learn
- **Registration Overhead**: Must explicitly register direction
- **Migration**: Existing code assumes bidirectional (breaking change)
- **Verbosity**: More boilerplate for simple cases

### Neutral
- **Default Behavior**: Bidirectional is the default (backward compatible)
- **Opt-in Restrictions**: Can use bidirectional everywhere if desired
- **Gradual Adoption**: Can add direction restrictions incrementally

## Implementation Notes

### Error Types
Add new error variants:

```rust
pub enum ChannelError {
    // ... existing variants ...
    
    /// Protocol direction not supported
    DirectionNotSupported {
        protocol: String,
        direction: String,  // "call" or "respond"
        our_direction: ProtocolDirection,
    },
    
    /// Incompatible protocol directions
    IncompatibleDirection {
        protocol: String,
        our_direction: ProtocolDirection,
        peer_direction: ProtocolDirection,
    },
}
```

### Testing Strategy
1. **Unit Tests**: Test direction validation logic
2. **Integration Tests**: Test all direction combinations
3. **Error Tests**: Verify clear error messages
4. **Compatibility Tests**: Test negotiation between different directions

### Migration Path
1. **Phase 1**: Add direction support with bidirectional default
2. **Phase 2**: Update examples to use explicit directions
3. **Phase 3**: Add deprecation warnings for implicit bidirectional
4. **Phase 4**: (v2.0) Make direction explicit (breaking change)

## Alternatives Considered

### Alternative 1: Separate Caller and Responder Types
Create separate `Caller<P>` and `Responder<P>` types. Rejected because:
- More complex type system
- Harder to support bidirectional
- Doesn't integrate well with channel abstraction

### Alternative 2: Capability Bits
Use bitflags for fine-grained capabilities. Rejected because:
- Overkill for current needs
- Call/Respond is sufficient for now
- Can add more granular capabilities later if needed

### Alternative 3: No Direction Support
Keep current bidirectional-only model. Rejected because:
- Doesn't reflect real-world patterns
- Prevents compile-time validation
- Makes errors less clear
- Limits architectural flexibility

## References
- ADR-001: Core Architecture
- ADR-006: Protocol Evolution & Versioning
- [gRPC Streaming Patterns](https://grpc.io/docs/what-is-grpc/core-concepts/#rpc-life-cycle)
- [Cap'n Proto RPC](https://capnproto.org/rpc.html)

## Decision Date
2026-02-05

## Participants
- Hans W. Uhlig (Author)
- Bob (AI Assistant)
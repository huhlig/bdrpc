# ADR-006: Protocol Evolution and Versioning

## Status
Accepted

## Context
Distributed systems evolve over time. Services add features, change APIs, and deprecate old functionality. BDRPC needs a comprehensive strategy for protocol evolution that:
- Allows backward-compatible changes
- Supports breaking changes when necessary
- Enables gradual rollouts
- Maintains type safety
- Provides clear migration paths

## Decision

### Multi-Faceted Evolution Strategy
We will support protocol evolution through three complementary mechanisms:

1. **Version Numbers**: Major.Minor versioning for protocols
2. **Feature Flags**: Optional capabilities negotiated at runtime
3. **Additive Changes**: Backward-compatible extensions

### Version Negotiation at Handshake

```rust
struct ProtocolCapability {
    protocol_name: String,
    supported_versions: Vec<u32>,  // [1, 2, 3]
    features: HashSet<String>,     // ["compression", "streaming"]
}

struct Handshake {
    endpoint_id: String,
    serializer: String,
    protocols: Vec<ProtocolCapability>,
}

impl Endpoint {
    async fn negotiate_protocol(
        &self,
        protocol_name: &str,
        peer_capability: &ProtocolCapability,
    ) -> Result<NegotiatedProtocol> {
        // Find highest mutually supported version
        let our_versions = self.supported_versions(protocol_name);
        let common_versions: Vec<_> = our_versions
            .iter()
            .filter(|v| peer_capability.supported_versions.contains(v))
            .collect();
        
        let version = common_versions.into_iter().max()
            .ok_or(ChannelError::NoCommonVersion)?;
        
        // Negotiate features
        let our_features = self.supported_features(protocol_name);
        let common_features: HashSet<_> = our_features
            .intersection(&peer_capability.features)
            .cloned()
            .collect();
        
        Ok(NegotiatedProtocol {
            name: protocol_name.to_string(),
            version: *version,
            features: common_features,
        })
    }
}
```

### Protocol Versioning

#### Macro Syntax
```rust
#[bdrpc::service(version = 2)]
trait UserService {
    // Available in all versions
    async fn get_user(&self, id: u64) -> Result<User>;
    
    // Added in version 2
    #[since(version = 2)]
    async fn search_users(&self, query: String) -> Result<Vec<User>>;
    
    // Deprecated in version 2, removed in version 3
    #[deprecated(since = "2.0", note = "Use search_users instead")]
    async fn find_user(&self, name: String) -> Result<Option<User>>;
}
```

#### Generated Code
```rust
// Version 1
enum UserServiceV1 {
    GetUserRequest { id: u64 },
    GetUserResponse { result: Result<User> },
    FindUserRequest { name: String },
    FindUserResponse { result: Result<Option<User>> },
}

// Version 2 (superset of V1)
enum UserServiceV2 {
    GetUserRequest { id: u64 },
    GetUserResponse { result: Result<User> },
    SearchUsersRequest { query: String },
    SearchUsersResponse { result: Result<Vec<User>> },
    // FindUser still supported but deprecated
    FindUserRequest { name: String },
    FindUserResponse { result: Result<Option<User>> },
}
```

### Additive Changes (Backward Compatible)

#### Optional Fields
```rust
#[derive(Serialize, Deserialize)]
struct User {
    id: u64,
    name: String,
    
    // Added in v2, optional for v1 compatibility
    #[serde(default, skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    
    // Added in v3
    #[serde(default, skip_serializing_if = "Option::is_none")]
    phone: Option<String>,
}
```

#### Default Implementations
```rust
#[bdrpc::service(version = 2)]
trait UserService {
    async fn get_user(&self, id: u64) -> Result<User>;
    
    #[since(version = 2)]
    async fn search_users(&self, query: String) -> Result<Vec<User>> {
        // Default implementation for v1 servers
        Err(ChannelError::UnsupportedMethod("search_users".into()))
    }
}
```

#### Unknown Variants
```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum Message {
    TypeA { data: String },
    TypeB { count: i32 },
    
    // Allows forward compatibility
    #[serde(other)]
    Unknown,
}

impl Handler {
    async fn handle(&self, msg: Message) -> Result<()> {
        match msg {
            Message::TypeA { data } => self.handle_a(data).await,
            Message::TypeB { count } => self.handle_b(count).await,
            Message::Unknown => {
                // Log and ignore unknown message types
                tracing::warn!("Received unknown message type");
                Ok(())
            }
        }
    }
}
```

### Feature Flags

#### Declaration
```rust
#[bdrpc::service(version = 2)]
#[features("streaming", "compression", "priority")]
trait AdvancedService {
    async fn basic_method(&self) -> Result<String>;
    
    #[requires_feature("streaming")]
    async fn stream_data(&self) -> impl Stream<Item = Data>;
    
    #[requires_feature("priority")]
    async fn urgent_request(&self, priority: u8) -> Result<()>;
    
    #[requires_feature("compression")]
    async fn large_payload(&self, data: Vec<u8>) -> Result<()>;
}
```

#### Runtime Checking
```rust
impl Channel {
    async fn call_method(&self, method: &str, args: Args) -> Result<Response> {
        // Check if method requires features
        let required_features = self.protocol.required_features(method);
        
        for feature in required_features {
            if !self.negotiated_features.contains(feature) {
                return Err(ChannelError::FeatureNotSupported {
                    feature: feature.clone(),
                    method: method.to_string(),
                });
            }
        }
        
        // Proceed with call
        self.send_request(method, args).await
    }
}
```

### Breaking Changes (Version Bumps)

#### Multiple Version Support
```rust
// Server can support multiple versions simultaneously
struct UserServiceImpl {
    // Internal implementation
}

impl UserServiceV1 for UserServiceImpl {
    async fn get_user(&self, id: u64) -> Result<User> {
        // V1 implementation
    }
}

impl UserServiceV2 for UserServiceImpl {
    async fn get_user(&self, id: u64) -> Result<User> {
        // V2 implementation (may differ from V1)
    }
    
    async fn search_users(&self, query: String) -> Result<Vec<User>> {
        // New method in V2
    }
}

// Endpoint registers both versions
endpoint.register_service_v1(Arc::new(UserServiceImpl::new()));
endpoint.register_service_v2(Arc::new(UserServiceImpl::new()));
```

#### Adapter Pattern
```rust
// Adapt V2 implementation to V1 interface
struct V1Adapter<T: UserServiceV2> {
    inner: T,
}

impl<T: UserServiceV2> UserServiceV1 for V1Adapter<T> {
    async fn get_user(&self, id: u64) -> Result<User> {
        // Forward to V2, strip new fields
        let mut user = self.inner.get_user(id).await?;
        user.email = None;  // Remove V2 fields
        user.phone = None;
        Ok(user)
    }
    
    async fn find_user(&self, name: String) -> Result<Option<User>> {
        // Translate to V2 search_users
        let results = self.inner.search_users(name).await?;
        Ok(results.into_iter().next())
    }
}
```

### Deprecation Workflow

#### Deprecation Metadata
```rust
#[bdrpc::service(version = 3)]
trait UserService {
    #[deprecated(
        since = "2.0",
        note = "Use search_users instead",
        remove_in = "4.0"
    )]
    async fn find_user(&self, name: String) -> Result<Option<User>>;
    
    #[since(version = 2)]
    async fn search_users(&self, query: String) -> Result<Vec<User>>;
}
```

#### Runtime Warnings
```rust
impl Channel {
    async fn call_method(&self, method: &str, args: Args) -> Result<Response> {
        // Check deprecation status
        if let Some(deprecation) = self.protocol.deprecation_info(method) {
            tracing::warn!(
                method = method,
                since = deprecation.since,
                note = deprecation.note,
                "Called deprecated method"
            );
            
            // Emit metric
            metrics::counter!("deprecated_method_calls", 1, "method" => method);
        }
        
        self.send_request(method, args).await
    }
}
```

### Schema Registry (Optional)

For complex systems with many services:

```rust
trait SchemaRegistry: Send + Sync {
    /// Register a protocol version
    fn register_protocol(
        &self,
        name: &str,
        version: u32,
        schema: ProtocolSchema,
    ) -> Result<()>;
    
    /// Get compatible version for given constraints
    fn get_compatible_version(
        &self,
        name: &str,
        client_versions: &[u32],
        server_versions: &[u32],
    ) -> Option<u32>;
    
    /// Check if upgrade path exists
    fn can_upgrade(&self, name: &str, from: u32, to: u32) -> bool;
    
    /// Get migration guide
    fn migration_guide(&self, name: &str, from: u32, to: u32) -> Option<String>;
}

struct ProtocolSchema {
    version: u32,
    methods: Vec<MethodSchema>,
    types: Vec<TypeSchema>,
    features: HashSet<String>,
}
```

## Consequences

### Positive
- **Gradual evolution**: Services can evolve without breaking clients
- **Type safety**: Compile-time checking of version compatibility
- **Flexibility**: Multiple mechanisms for different evolution needs
- **Observability**: Deprecation warnings and metrics
- **Clear migration**: Explicit version numbers and deprecation timelines

### Negative
- **Complexity**: More concepts to understand
- **Maintenance burden**: Supporting multiple versions
- **Code duplication**: Adapter implementations
- **Testing overhead**: Must test all version combinations

### Neutral
- **Version proliferation**: Need discipline to avoid too many versions
- **Feature flag explosion**: Need to limit feature combinations

## Alternatives Considered

### Protobuf-style Field Numbers
Use field numbers instead of names. Rejected because:
- Less idiomatic in Rust
- Harder to read and maintain
- Doesn't solve method evolution

### GraphQL-style Introspection
Runtime schema introspection. Rejected because:
- Adds runtime overhead
- Doesn't provide compile-time safety
- More complex to implement

### No Versioning
Always maintain backward compatibility. Rejected because:
- Sometimes breaking changes are necessary
- Accumulates technical debt
- Makes evolution harder over time

## Implementation Notes

### Version Metadata
Store version info in generated code:

```rust
impl UserServiceProtocol {
    pub const VERSION: u32 = 2;
    pub const MIN_SUPPORTED_VERSION: u32 = 1;
    pub const FEATURES: &'static [&'static str] = &["streaming", "compression"];
    
    pub fn method_version(method: &str) -> Option<u32> {
        match method {
            "get_user" => Some(1),
            "search_users" => Some(2),
            _ => None,
        }
    }
}
```

### Testing Version Compatibility
```rust
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_v1_client_v2_server() {
        let server = create_v2_server();
        let client = create_v1_client();
        
        // V1 methods should work
        let user = client.get_user(1).await.unwrap();
        assert_eq!(user.id, 1);
        
        // V2 methods should fail gracefully
        let result = client.search_users("test").await;
        assert!(matches!(result, Err(ChannelError::UnsupportedMethod(_))));
    }
    
    #[tokio::test]
    async fn test_v2_client_v1_server() {
        let server = create_v1_server();
        let client = create_v2_client();
        
        // V1 methods should work
        let user = client.get_user(1).await.unwrap();
        
        // V2 methods should fail gracefully
        let result = client.search_users("test").await;
        assert!(matches!(result, Err(ChannelError::UnsupportedMethod(_))));
    }
}
```

### Documentation Generation
Generate documentation showing version history:

```markdown
# UserService Protocol

## Version 2 (Current)
- `get_user(id: u64) -> User` - Get user by ID (since v1)
- `search_users(query: String) -> Vec<User>` - Search users (since v2)
- `find_user(name: String) -> Option<User>` - **DEPRECATED** (since v2, use search_users)

## Version 1
- `get_user(id: u64) -> User` - Get user by ID
- `find_user(name: String) -> Option<User>` - Find user by name
```

## Open Questions

1. **Version negotiation failure**: What happens if no common version exists?
2. **Feature dependencies**: Can features depend on other features?
3. **Automatic migration**: Should we support automatic data migration between versions?
4. **Version discovery**: Should clients be able to query supported versions before connecting?

## Related ADRs
- ADR-001: Core Architecture (defines Protocol)
- ADR-004: Error Handling Hierarchy (VersionMismatch, UnsupportedMethod errors)
- ADR-005: Serialization Strategy (serialization affects schema evolution)

## References
- [Semantic Versioning](https://semver.org/)
- [Protobuf Language Guide](https://developers.google.com/protocol-buffers/docs/proto3)
- [gRPC Versioning](https://grpc.io/docs/guides/versioning/)
- [API Evolution Best Practices](https://cloud.google.com/apis/design/versioning)
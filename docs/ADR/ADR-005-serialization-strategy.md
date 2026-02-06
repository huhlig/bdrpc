# ADR-005: Per-Endpoint Serialization Strategy

## Status
Accepted

## Context
BDRPC needs to serialize messages for transmission over the wire. Two main options exist:
- **serde**: Mature, widely-used, supports many formats (JSON, bincode, etc.)
- **rkyv**: Zero-copy deserialization, potentially much faster

However, serde and rkyv are fundamentally incompatible in practice:
- Different trait systems (`Serialize`/`Deserialize` vs `Archive`/`Serialize`/`Deserialize`)
- Different wire formats
- Cannot mix on the same connection

We need a strategy that:
- Makes the choice explicit
- Prevents accidental mixing
- Allows compile-time optimization
- Keeps the API clean

## Decision

### Per-Endpoint Serialization Choice
The serialization format is chosen at the **Endpoint level**, not per-protocol. This means all protocols on an endpoint use the same serializer.

```rust
trait Serializer: Send + Sync + 'static {
    /// Serialize a value to bytes
    fn serialize<T>(&self, value: &T) -> Result<Vec<u8>, SerializationError>
    where
        T: ?Sized;
    
    /// Deserialize bytes to a value
    fn deserialize<T>(&self, bytes: &[u8]) -> Result<T, DeserializationError>;
    
    /// Get serializer name for protocol negotiation
    fn name(&self) -> &'static str;
}
```

### Endpoint is Generic Over Serializer
```rust
struct Endpoint<S: Serializer> {
    serializer: S,
    transports: HashMap<TransportId, Transport>,
    channels: HashMap<ChannelId, Box<dyn ChannelHandler>>,
    config: EndpointConfig,
}

impl<S: Serializer> Endpoint<S> {
    pub fn new(serializer: S, config: EndpointConfig) -> Self {
        Self {
            serializer,
            transports: HashMap::new(),
            channels: HashMap::new(),
            config,
        }
    }
}
```

### Built-in Serializers

#### 1. Serde-based Serializers
```rust
// Bincode (compact binary)
struct BincodeSerializer {
    config: bincode::Config,
}

impl Serializer for BincodeSerializer {
    fn serialize<T: serde::Serialize>(&self, value: &T) -> Result<Vec<u8>, SerializationError> {
        bincode::serialize(value).map_err(Into::into)
    }
    
    fn deserialize<T: serde::de::DeserializeOwned>(&self, bytes: &[u8]) -> Result<T, DeserializationError> {
        bincode::deserialize(bytes).map_err(Into::into)
    }
    
    fn name(&self) -> &'static str {
        "bincode"
    }
}

// JSON (human-readable, debugging)
struct JsonSerializer;

impl Serializer for JsonSerializer {
    fn serialize<T: serde::Serialize>(&self, value: &T) -> Result<Vec<u8>, SerializationError> {
        serde_json::to_vec(value).map_err(Into::into)
    }
    
    fn deserialize<T: serde::de::DeserializeOwned>(&self, bytes: &[u8]) -> Result<T, DeserializationError> {
        serde_json::from_slice(bytes).map_err(Into::into)
    }
    
    fn name(&self) -> &'static str {
        "json"
    }
}

// MessagePack (compact, cross-language)
struct MessagePackSerializer;
```

#### 2. Rkyv-based Serializer
```rust
struct RkyvSerializer {
    // Configuration for alignment, validation, etc.
    config: RkyvConfig,
}

impl Serializer for RkyvSerializer {
    fn serialize<T: rkyv::Serialize<AllocSerializer<256>>>(&self, value: &T) -> Result<Vec<u8>, SerializationError> {
        rkyv::to_bytes(value).map(|v| v.to_vec()).map_err(Into::into)
    }
    
    fn deserialize<T: rkyv::Archive>(&self, bytes: &[u8]) -> Result<&T::Archived, DeserializationError> {
        // Zero-copy deserialization
        rkyv::check_archived_root::<T>(bytes).map_err(Into::into)
    }
    
    fn name(&self) -> &'static str {
        "rkyv"
    }
}
```

### Protocol Trait Bounds
Protocols must be compatible with the chosen serializer:

```rust
// For serde-based endpoints
#[bdrpc::service]
trait MyService 
where
    Self::Request: serde::Serialize + serde::de::DeserializeOwned,
    Self::Response: serde::Serialize + serde::de::DeserializeOwned,
{
    async fn method(&self, req: Self::Request) -> Result<Self::Response>;
}

// For rkyv-based endpoints
#[bdrpc::service]
trait MyService
where
    Self::Request: rkyv::Archive + rkyv::Serialize<AllocSerializer<256>>,
    Self::Response: rkyv::Archive + rkyv::Serialize<AllocSerializer<256>>,
{
    async fn method(&self, req: Self::Request) -> Result<Self::Response>;
}
```

### Serializer Negotiation
During connection handshake, endpoints exchange serializer information:

```rust
struct Handshake {
    endpoint_id: String,
    serializer: String,  // "bincode", "json", "rkyv", etc.
    protocols: Vec<ProtocolCapability>,
}

impl Endpoint<S> {
    async fn handshake(&self, transport: &mut Transport) -> Result<Handshake> {
        // Send our serializer name
        let our_handshake = Handshake {
            endpoint_id: self.id.clone(),
            serializer: self.serializer.name().to_string(),
            protocols: self.list_protocols(),
        };
        
        // Receive peer's handshake
        let peer_handshake = self.receive_handshake(transport).await?;
        
        // Verify serializer compatibility
        if our_handshake.serializer != peer_handshake.serializer {
            return Err(ChannelError::SerializerMismatch {
                ours: our_handshake.serializer,
                theirs: peer_handshake.serializer,
            });
        }
        
        Ok(peer_handshake)
    }
}
```

## Consequences

### Positive
- **Explicit choice**: Serializer is visible in type signature
- **Compile-time optimization**: Monomorphization allows inlining
- **No mixing**: Impossible to accidentally use different serializers
- **Type safety**: Trait bounds ensure compatibility
- **Performance**: Zero-cost abstraction, no dynamic dispatch for serialization

### Negative
- **Inflexibility**: Can't mix serializers on one endpoint
- **Type complexity**: Generic parameter propagates through code
- **Migration difficulty**: Changing serializers requires new endpoint
- **Code duplication**: May need separate endpoints for different serializers

### Neutral
- **Per-endpoint choice**: Different endpoints can use different serializers
- **Testing**: Can use JSON for debugging, bincode for production

## Alternatives Considered

### Per-Protocol Serialization
Allow each protocol to choose its serializer. Rejected because:
- Serde and rkyv are incompatible
- Would require trait objects (dynamic dispatch)
- Complex to implement
- Confusing for users

### Dynamic Serializer Selection
Use `Box<dyn Serializer>` for runtime selection. Rejected because:
- Loses compile-time optimization
- Dynamic dispatch overhead
- Still can't mix serde and rkyv

### Hardcode Bincode
Just use bincode everywhere. Rejected because:
- No flexibility for debugging (JSON)
- Can't leverage rkyv's performance
- Limits future options

## Implementation Notes

### Message Framing
Messages need length prefixes for framing:

```rust
struct FramedMessage {
    length: u32,
    payload: Vec<u8>,
}

impl<S: Serializer> Endpoint<S> {
    async fn send_message<T>(&self, transport: &mut Transport, message: &T) -> Result<()> {
        // Serialize
        let payload = self.serializer.serialize(message)?;
        
        // Frame
        let length = payload.len() as u32;
        transport.write_u32(length).await?;
        transport.write_all(&payload).await?;
        
        Ok(())
    }
    
    async fn recv_message<T>(&self, transport: &mut Transport) -> Result<T> {
        // Read frame
        let length = transport.read_u32().await?;
        let mut payload = vec![0u8; length as usize];
        transport.read_exact(&mut payload).await?;
        
        // Deserialize
        self.serializer.deserialize(&payload)
    }
}
```

### Compression Integration
Compression happens at transport layer, before serialization:

```rust
struct CompressedTransport<T: Transport> {
    inner: T,
    compressor: Box<dyn Compressor>,
}

// Serialization happens after compression
// Transport -> Compress -> Serialize -> Protocol
```

### Zero-Copy with Rkyv
Rkyv's zero-copy deserialization requires careful lifetime management:

```rust
impl RkyvSerializer {
    fn deserialize_borrowed<'a, T: rkyv::Archive>(
        &self,
        bytes: &'a [u8],
    ) -> Result<&'a T::Archived, DeserializationError> {
        // Returns reference into bytes, no allocation
        rkyv::check_archived_root::<T>(bytes).map_err(Into::into)
    }
}
```

### Testing Different Serializers
```rust
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_bincode_serializer() {
        let endpoint = Endpoint::new(BincodeSerializer::default(), config);
        // Test with bincode
    }
    
    #[tokio::test]
    async fn test_json_serializer() {
        let endpoint = Endpoint::new(JsonSerializer, config);
        // Test with JSON (easier to debug)
    }
    
    #[tokio::test]
    async fn test_rkyv_serializer() {
        let endpoint = Endpoint::new(RkyvSerializer::default(), config);
        // Test with rkyv (performance)
    }
}
```

### Migration Strategy
To migrate between serializers:

1. **Dual Endpoints**: Run both old and new serializers simultaneously
2. **Gradual Migration**: Migrate services one at a time
3. **Version Flag**: Use protocol version to indicate serializer
4. **Proxy**: Use a proxy to translate between serializers

```rust
// Migration example
struct MigrationProxy {
    old_endpoint: Endpoint<BincodeSerializer>,
    new_endpoint: Endpoint<RkyvSerializer>,
}

impl MigrationProxy {
    async fn forward_request(&self, req: Request) -> Response {
        // Deserialize with old, re-serialize with new
    }
}
```

## Performance Considerations

### Benchmarks
Expected performance characteristics:

| Serializer | Serialize | Deserialize | Size | Use Case |
|------------|-----------|-------------|------|----------|
| Bincode | Fast | Fast | Small | Production default |
| JSON | Slow | Slow | Large | Debugging, interop |
| MessagePack | Fast | Fast | Small | Cross-language |
| Rkyv | Fastest | Instant | Small | High-performance |

### When to Use Each

- **Bincode**: Default choice, good balance
- **JSON**: Development, debugging, human-readable logs
- **MessagePack**: Cross-language compatibility
- **Rkyv**: Maximum performance, Rust-to-Rust only

## Open Questions

1. **Schema evolution**: How do serializers handle schema changes?
2. **Custom serializers**: Should we support user-defined serializers?
3. **Serializer versioning**: How to handle serializer format changes?

## Related ADRs
- ADR-001: Core Architecture (defines Protocol)
- ADR-004: Error Handling Hierarchy (SerializationError, DeserializationError)
- ADR-006: Protocol Evolution and Versioning (interacts with serialization)

## References
- [serde documentation](https://serde.rs/)
- [rkyv documentation](https://rkyv.org/)
- [bincode documentation](https://docs.rs/bincode/)
- [Zero-Copy Deserialization](https://davidkoloski.me/blog/rkyv-is-faster-than/)
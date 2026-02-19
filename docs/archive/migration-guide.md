# BDRPC Migration Guide

This guide helps you migrate to newer BDRPC APIs and patterns.

## Table of Contents

- [Migrating to Convenience Methods](#migrating-to-convenience-methods)
- [Migrating to Automatic Protocol Registration](#migrating-to-automatic-protocol-registration)
- [Migrating to New Error Types](#migrating-to-new-error-types)
- [Migrating Serializers](#migrating-serializers)
- [Breaking Changes by Version](#breaking-changes-by-version)

## Migrating to Convenience Methods

### Overview

BDRPC v0.1.0 introduced convenience methods that simplify channel creation:
- `Endpoint::get_channels<P>()` - Create bidirectional channels in one call
- Automatic protocol registration and allowlisting

### Before: Manual Channel Creation

```rust
use bdrpc::channel::{ChannelId, Protocol};
use bdrpc::endpoint::{Endpoint, EndpointConfig};

// Define protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessage {
    user: String,
    text: String,
}

impl Protocol for ChatMessage {
    fn method_name(&self) -> &'static str {
        "chat"
    }
    
    fn is_request(&self) -> bool {
        true
    }
}

// Client side - old way
async fn old_client_pattern() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = Endpoint::connect("127.0.0.1:8080").await?;
    
    // Register protocol
    endpoint.register_caller::<ChatMessage>()?;
    
    // Manually allow protocol in negotiator
    if let Some(negotiator) = endpoint.default_negotiator() {
        negotiator.allow_protocol("chat");
    }
    
    // Create channel ID
    let channel_id = ChannelId::new();
    
    // Request channel from peer
    let (sender, receiver) = endpoint
        .request_channel::<ChatMessage>(channel_id)
        .await?;
    
    // Use channels
    sender.send(ChatMessage {
        user: "Alice".to_string(),
        text: "Hello!".to_string(),
    }).await?;
    
    Ok(())
}

// Server side - old way
async fn old_server_pattern() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = Endpoint::new();
    
    // Register protocol
    endpoint.register_responder::<ChatMessage>()?;
    
    // Manually allow protocol
    if let Some(negotiator) = endpoint.default_negotiator() {
        negotiator.allow_protocol("chat");
    }
    
    // Listen for connections
    let listener = endpoint.listen("127.0.0.1:8080").await?;
    
    // Accept connection
    let connection = listener.accept().await?;
    
    // Wait for channel request from client
    // (handled by endpoint internally)
    
    // Get channels from channel manager
    let channels = endpoint.channels();
    // ... complex channel retrieval logic
    
    Ok(())
}
```

### After: Using Convenience Methods

```rust
use bdrpc::channel::Protocol;
use bdrpc::endpoint::Endpoint;

// Same protocol definition
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessage {
    user: String,
    text: String,
}

impl Protocol for ChatMessage {
    fn method_name(&self) -> &'static str {
        "chat"
    }
    
    fn is_request(&self) -> bool {
        true
    }
}

// Client side - new way
async fn new_client_pattern() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = Endpoint::connect("127.0.0.1:8080").await?;
    
    // Register protocol (automatically allows it)
    endpoint.register_caller::<ChatMessage>()?;
    
    // Get channels in one call - much simpler!
    let (sender, receiver) = endpoint.get_channels::<ChatMessage>().await?;
    
    // Use channels
    sender.send(ChatMessage {
        user: "Alice".to_string(),
        text: "Hello!".to_string(),
    }).await?;
    
    Ok(())
}

// Server side - new way
async fn new_server_pattern() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = Endpoint::new();
    
    // Register protocol (automatically allows it)
    endpoint.register_responder::<ChatMessage>()?;
    
    // Listen for connections
    let listener = endpoint.listen("127.0.0.1:8080").await?;
    
    // Accept connection
    let connection = listener.accept().await?;
    
    // Get channels - same simple API on server side
    let (sender, receiver) = endpoint.get_channels::<ChatMessage>().await?;
    
    // Use channels
    while let Some(msg) = receiver.recv().await {
        println!("{}: {}", msg.user, msg.text);
        
        // Echo back
        sender.send(msg).await?;
    }
    
    Ok(())
}
```

### Migration Steps

1. **Replace manual channel creation** with `get_channels()`:
   ```rust
   // Old
   let channel_id = ChannelId::new();
   let (sender, receiver) = endpoint.request_channel::<P>(channel_id).await?;
   
   // New
   let (sender, receiver) = endpoint.get_channels::<P>().await?;
   ```

2. **Remove manual protocol allowlisting** (now automatic):
   ```rust
   // Old - remove this
   if let Some(negotiator) = endpoint.default_negotiator() {
       negotiator.allow_protocol("my_protocol");
   }
   
   // New - just register the protocol
   endpoint.register_bidirectional::<MyProtocol>()?;
   // Protocol is automatically allowed
   ```

3. **Simplify error handling** with new error types:
   ```rust
   // Old
   match endpoint.request_channel::<P>(id).await {
       Ok(channels) => { /* use channels */ },
       Err(e) => {
           // Generic error handling
           eprintln!("Channel creation failed: {}", e);
       }
   }
   
   // New
   match endpoint.get_channels::<P>().await {
       Ok(channels) => { /* use channels */ },
       Err(e) => {
           // Specific error types with helpful hints
           eprintln!("Channel creation failed: {}", e);
           // Error message includes troubleshooting hints
       }
   }
   ```

### Benefits of New API

- **Less boilerplate**: No manual channel ID creation
- **Automatic allowlisting**: Protocols are allowed when registered
- **Better error messages**: Includes troubleshooting hints
- **Consistent API**: Same pattern for client and server
- **Type safety**: Compile-time protocol checking

### When to Use Manual API

The manual API (`request_channel()`) is still available for advanced use cases:

- Custom channel ID management
- Fine-grained control over channel creation
- Integration with existing channel management systems
- Custom negotiation logic

```rust
// Advanced: Manual channel creation with custom ID
let custom_id = ChannelId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
let (sender, receiver) = endpoint.request_channel::<MyProtocol>(custom_id).await?;
```

## Migrating to Automatic Protocol Registration

### Overview

Starting in v0.1.0, protocols are automatically allowed when registered, eliminating the need for manual `allow_protocol()` calls.

### Before: Manual Allowlisting

```rust
// Register protocol
endpoint.register_bidirectional::<MyProtocol>()?;

// Manually allow protocol
if let Some(negotiator) = endpoint.default_negotiator() {
    negotiator.allow_protocol("my_protocol");
}
```

### After: Automatic Allowlisting

```rust
// Just register - automatically allowed
endpoint.register_bidirectional::<MyProtocol>()?;
```

### Custom Negotiators

If you have a custom negotiator, you can opt into automatic allowlisting:

```rust
use bdrpc::endpoint::ChannelNegotiator;

struct MyNegotiator {
    allowed: HashSet<String>,
}

impl ChannelNegotiator for MyNegotiator {
    // Implement required methods...
    
    // Opt into automatic allowlisting
    fn on_protocol_registered(&mut self, protocol_name: &str) {
        self.allowed.insert(protocol_name.to_string());
    }
}
```

Or keep manual control by not implementing `on_protocol_registered()` (default is no-op).

## Migrating to New Error Types

### Overview

BDRPC v0.1.0 introduced specific error types for channel creation failures with helpful troubleshooting hints.

### Before: Generic Errors

```rust
// Old error handling
match endpoint.request_channel::<P>(id).await {
    Err(e) => {
        eprintln!("Error: {}", e);
        // Generic error message, hard to diagnose
    }
    Ok(channels) => { /* ... */ }
}
```

### After: Specific Error Types

```rust
use bdrpc::endpoint::EndpointError;

// New error handling with specific types
match endpoint.get_channels::<P>().await {
    Err(EndpointError::ChannelRequestTimeout { 
        connection_id, 
        protocol_name, 
        channel_id, 
        timeout 
    }) => {
        eprintln!("Channel request timed out after {:?}", timeout);
        eprintln!("Connection: {}, Protocol: {}", connection_id, protocol_name);
        // Error includes hint: "Ensure the peer is responding..."
    }
    Err(EndpointError::ChannelRequestRejected { 
        connection_id, 
        protocol_name, 
        channel_id, 
        reason 
    }) => {
        eprintln!("Channel request rejected: {}", reason);
        // Error includes hint: "Ensure both endpoints use the same serializer..."
    }
    Err(EndpointError::ChannelCreationFailed { 
        connection_id, 
        protocol_name, 
        channel_id, 
        source 
    }) => {
        eprintln!("Channel creation failed: {}", source);
        // Error includes hint: "Check that the protocol is registered..."
    }
    Err(e) => {
        eprintln!("Other error: {}", e);
    }
    Ok((sender, receiver)) => {
        // Use channels
    }
}
```

### Error Message Improvements

New error messages include:
- Connection ID for debugging
- Protocol name for context
- Channel ID for tracking
- Specific failure reason
- Troubleshooting hints

Example error output:
```
Error: Channel request timed out after 5s for protocol 'ChatMessage' on connection abc123-def456
Channel ID: 12345678-90ab-cdef-1234-567890abcdef
Hint: Ensure the peer is responding to channel requests and not overloaded. 
      Check network connectivity and consider increasing the timeout.
```

## Migrating Serializers

### Overview

BDRPC supports multiple serialization formats. You may want to migrate to a more efficient serializer.

### JSON to Postcard

Postcard is more efficient than JSON for binary protocols:

```rust
// Before: JSON serializer (default)
use bdrpc::serialization::JsonSerializer;
let endpoint = Endpoint::with_serializer(JsonSerializer);

// After: Postcard serializer (more efficient)
use bdrpc::serialization::PostcardSerializer;
let endpoint = Endpoint::with_serializer(PostcardSerializer);
```

**Benefits:**
- Smaller message size
- Faster serialization/deserialization
- Better performance

**Considerations:**
- Not human-readable
- Both endpoints must use same serializer
- May require schema evolution planning

### JSON to rkyv

For maximum performance, use rkyv (zero-copy deserialization):

```rust
// Before: JSON serializer
use bdrpc::serialization::JsonSerializer;
let endpoint = Endpoint::with_serializer(JsonSerializer);

// After: rkyv serializer (fastest)
use bdrpc::serialization::RkyvSerializer;
let endpoint = Endpoint::with_serializer(RkyvSerializer);
```

**Benefits:**
- Zero-copy deserialization
- Fastest performance
- Lowest latency

**Considerations:**
- Messages must derive `rkyv::Archive`
- More complex setup
- Larger binary size
- Both endpoints must use rkyv

### Migration Steps

1. **Update message types** to support new serializer:
   ```rust
   // For Postcard - no changes needed if using serde
   #[derive(Serialize, Deserialize)]
   struct MyMessage { /* ... */ }
   
   // For rkyv - add Archive derive
   #[derive(Archive, Serialize, Deserialize)]
   struct MyMessage { /* ... */ }
   ```

2. **Update both endpoints** to use same serializer:
   ```rust
   // Client
   let client = Endpoint::with_serializer(PostcardSerializer)
       .connect("127.0.0.1:8080")
       .await?;
   
   // Server
   let server = Endpoint::with_serializer(PostcardSerializer);
   ```

3. **Test thoroughly** - serialization changes can cause subtle bugs

4. **Consider versioning** for gradual rollout:
   ```rust
   // Support both serializers during migration
   match endpoint.negotiated_serializer() {
       "json" => { /* use JSON */ }
       "postcard" => { /* use Postcard */ }
       _ => { /* error */ }
   }
   ```

## Breaking Changes by Version

### v0.1.0

**New Features:**
- `Endpoint::get_channels<P>()` convenience method
- Automatic protocol allowlisting on registration
- `Endpoint::default_negotiator()` for safe downcasting
- Specific error types for channel creation failures

**Breaking Changes:**
- None - all changes are backward compatible

**Deprecations:**
- None - manual APIs still supported

**Migration Required:**
- No - optional migration to new convenience methods

### Future Versions

Check the [CHANGELOG.md](../CHANGELOG.md) for breaking changes in newer versions.

## Best Practices

### Protocol Design

1. **Use semantic versioning** for protocols:
   ```rust
   impl Protocol for MyProtocol {
       fn version(&self) -> u32 {
           1  // Increment for breaking changes
       }
   }
   ```

2. **Design for evolution**:
   ```rust
   #[derive(Serialize, Deserialize)]
   struct MyMessage {
       // Required fields
       id: u64,
       
       // Optional fields for backward compatibility
       #[serde(default)]
       new_field: Option<String>,
   }
   ```

3. **Document protocol changes** in your code

### Error Handling

1. **Use specific error types** for better diagnostics
2. **Log errors with context** (connection ID, protocol name)
3. **Implement retry logic** for transient failures
4. **Provide user-friendly error messages**

### Performance

1. **Choose appropriate serializer** for your use case
2. **Use bounded channels** to apply backpressure
3. **Batch small messages** when possible
4. **Profile with benchmarks** before optimizing

### Testing

1. **Test protocol evolution** with multiple versions
2. **Test error scenarios** (timeouts, rejections, failures)
3. **Test under load** with stress tests
4. **Test network failures** with integration tests

## Getting Help

If you need help migrating:

1. Check the [documentation](README.md)
2. Review [examples](../bdrpc/examples/)
3. Read the [troubleshooting guide](troubleshooting-guide.md)
4. Ask on [discussions](https://github.com/yourusername/bdrpc/discussions)
5. File an [issue](https://github.com/yourusername/bdrpc/issues)

## Contributing

Found an issue with this migration guide? Please:
1. [Open an issue](https://github.com/yourusername/bdrpc/issues/new)
2. [Submit a PR](https://github.com/yourusername/bdrpc/pulls)
3. Help others on [discussions](https://github.com/yourusername/bdrpc/discussions)
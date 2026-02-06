# ADR-004: Three-Layer Error Handling Hierarchy

## Status
Accepted

## Context
Distributed systems have errors at multiple levels. BDRPC needs a clear error model that:
- Distinguishes between different error sources
- Allows appropriate handling at each layer
- Provides good error messages for debugging
- Enables proper recovery strategies
- Maintains type safety

## Decision

### Three-Layer Error Model
We will implement three distinct error types corresponding to our architectural layers:

```rust
// Top-level error type
enum BdrpcError {
    Transport(TransportError),
    Channel(ChannelError),
    Application(Box<dyn Error + Send + Sync>),
}
```

### Layer 1: Transport Errors
Connection-level failures that affect the entire transport.

```rust
#[derive(Debug, thiserror::Error)]
enum TransportError {
    #[error("Connection lost: {0}")]
    ConnectionLost(String),
    
    #[error("Connection refused: {0}")]
    ConnectionRefused(String),
    
    #[error("Connection timeout after {0:?}")]
    Timeout(Duration),
    
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),
    
    #[error("TLS error: {0}")]
    TlsError(String),
    
    #[error("Encryption error: {0}")]
    EncryptionError(String),
    
    #[error("Compression error: {0}")]
    CompressionError(String),
    
    #[error("Transport closed")]
    Closed,
}
```

**Handling Strategy:**
- Trigger reconnection strategy (see ADR-002)
- Close all channels on this transport
- Notify application of transport failure
- Log at ERROR level

### Layer 2: Channel Errors
Protocol-level failures that affect a specific channel.

```rust
#[derive(Debug, thiserror::Error)]
enum ChannelError {
    #[error("Protocol mismatch: expected {expected}, got {actual}")]
    ProtocolMismatch {
        expected: String,
        actual: String,
    },
    
    #[error("Deserialization error: {0}")]
    DeserializationError(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Channel closed")]
    ChannelClosed,
    
    #[error("Invalid channel ID: {0}")]
    InvalidChannelId(String),
    
    #[error("Channel not found: {0}")]
    ChannelNotFound(String),
    
    #[error("Unsupported method: {0}")]
    UnsupportedMethod(String),
    
    #[error("Version mismatch: client {client_version}, server {server_version}")]
    VersionMismatch {
        client_version: u32,
        server_version: u32,
    },
    
    #[error("Message too large: {size} bytes (max: {max})")]
    MessageTooLarge {
        size: usize,
        max: usize,
    },
    
    #[error("Backpressure timeout")]
    BackpressureTimeout,
}
```

**Handling Strategy:**
- Close the affected channel
- Keep transport alive (other channels unaffected)
- Return error to application
- Log at WARN level
- Optionally attempt channel recovery

### Layer 3: Application Errors
User-defined errors specific to each protocol.

```rust
// Defined by user in their service trait
#[bdrpc::service]
trait UserService {
    async fn get_user(&self, id: u64) -> Result<User, UserServiceError>;
}

#[derive(Debug, thiserror::Error)]
enum UserServiceError {
    #[error("User not found: {0}")]
    NotFound(u64),
    
    #[error("Permission denied")]
    PermissionDenied,
    
    #[error("Database error: {0}")]
    DatabaseError(String),
}
```

**Handling Strategy:**
- Propagate to caller
- Channel remains open
- No framework-level handling
- Log at application's discretion

## Error Composition and Conversion

### Automatic Conversion
```rust
impl From<TransportError> for BdrpcError {
    fn from(e: TransportError) -> Self {
        BdrpcError::Transport(e)
    }
}

impl From<ChannelError> for BdrpcError {
    fn from(e: ChannelError) -> Self {
        BdrpcError::Channel(e)
    }
}
```

### Error Context
Use `thiserror` for rich error context:
```rust
#[derive(Debug, thiserror::Error)]
#[error("Failed to send message on channel {channel_id}")]
struct SendError {
    channel_id: String,
    #[source]
    source: BdrpcError,
}
```

## Error Recovery Strategies

### Transport Errors → Reconnection
```rust
impl Endpoint {
    async fn handle_transport_error(&self, transport_id: TransportId, error: TransportError) {
        // Close transport
        self.close_transport(transport_id).await;
        
        // Close all channels on this transport
        for channel_id in self.channels_on_transport(transport_id) {
            self.close_channel(channel_id).await;
        }
        
        // Trigger reconnection if client-initiated
        if let Some(strategy) = self.reconnection_strategy(transport_id) {
            strategy.on_disconnected(&error);
            self.spawn_reconnection_task(transport_id, strategy).await;
        }
    }
}
```

### Channel Errors → Channel Recovery
```rust
impl Endpoint {
    async fn handle_channel_error(&self, channel_id: ChannelId, error: ChannelError) {
        match error {
            ChannelError::ProtocolMismatch { .. } => {
                // Fatal: close channel permanently
                self.close_channel(channel_id).await;
            }
            ChannelError::DeserializationError(_) => {
                // Potentially recoverable: skip message, keep channel open
                self.log_and_continue(channel_id, error).await;
            }
            ChannelError::BackpressureTimeout => {
                // Recoverable: retry or drop message
                self.handle_backpressure_timeout(channel_id).await;
            }
            _ => {
                // Default: close channel
                self.close_channel(channel_id).await;
            }
        }
    }
}
```

### Application Errors → Propagate
```rust
// Application errors are returned to caller
async fn call_service<T, E>(&self, request: T) -> Result<Response, E>
where
    E: Error + Send + Sync + 'static,
{
    // Framework doesn't handle application errors
    // Just propagates them
}
```

## Error Observability

### Metrics
Each error type should be counted:
```rust
struct ErrorMetrics {
    transport_errors: Counter,
    channel_errors: Counter,
    application_errors: Counter,
    
    // Detailed breakdowns
    transport_errors_by_type: HashMap<String, Counter>,
    channel_errors_by_type: HashMap<String, Counter>,
}
```

### Logging
Structured logging with context:
```rust
tracing::error!(
    error = ?transport_error,
    transport_id = %transport_id,
    "Transport error occurred"
);

tracing::warn!(
    error = ?channel_error,
    channel_id = %channel_id,
    protocol = %protocol_name,
    "Channel error occurred"
);
```

### Error Callbacks
Allow users to hook into error handling:
```rust
trait ErrorHandler: Send + Sync {
    fn on_transport_error(&self, transport_id: TransportId, error: &TransportError);
    fn on_channel_error(&self, channel_id: ChannelId, error: &ChannelError);
}

struct EndpointConfig {
    error_handler: Option<Box<dyn ErrorHandler>>,
    // ...
}
```

## Consequences

### Positive
- **Clear responsibility**: Each layer handles its own errors
- **Isolation**: Channel errors don't affect transport
- **Type safety**: Compile-time error type checking
- **Debuggability**: Rich error context and structured logging
- **Recovery**: Appropriate recovery strategy per error type

### Negative
- **Complexity**: Three error types to understand
- **Boilerplate**: More error handling code
- **Conversion overhead**: Wrapping/unwrapping errors

### Neutral
- **Error propagation**: Errors bubble up through layers
- **Partial failures**: Some channels can fail while others succeed

## Alternatives Considered

### Single Error Type
One error enum for everything. Rejected because:
- Loses semantic meaning
- Makes recovery logic complex
- Harder to handle appropriately

### Result<T, E> with Generic E
Let each function define its error type. Rejected because:
- Inconsistent error handling
- Harder to compose
- No framework-level recovery

### Panic on Errors
Crash on any error. Rejected because:
- Not resilient
- Can't recover from transient failures
- Poor user experience

## Implementation Notes

### Error Serialization
Channel errors may need to be sent over the wire:
```rust
#[derive(Serialize, Deserialize)]
struct WireError {
    kind: String,
    message: String,
    context: HashMap<String, String>,
}

impl From<ChannelError> for WireError {
    fn from(e: ChannelError) -> Self {
        // Convert to wire format
    }
}
```

### Timeout Errors
Timeouts can occur at multiple layers:
- Transport timeout (connection establishment)
- Channel timeout (waiting for response)
- Application timeout (business logic)

Each should be represented at the appropriate layer.

### Error Testing
Comprehensive error testing:
```rust
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_transport_error_closes_channels() {
        // Simulate transport failure
        // Verify all channels closed
    }
    
    #[tokio::test]
    async fn test_channel_error_keeps_transport_alive() {
        // Simulate channel error
        // Verify transport still works
    }
}
```

## Related ADRs
- ADR-001: Core Architecture (defines layers)
- ADR-002: Reconnection Strategy (handles transport errors)
- ADR-003: Backpressure and Flow Control (BackpressureTimeout error)

## References
- [thiserror crate](https://docs.rs/thiserror/)
- [Error Handling in Rust](https://doc.rust-lang.org/book/ch09-00-error-handling.html)
- [Failure Modes in Distributed Systems](https://www.microsoft.com/en-us/research/publication/failure-modes-in-distributed-systems/)
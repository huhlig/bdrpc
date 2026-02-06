# BDRPC Best Practices

This guide provides recommendations for building robust, performant, and maintainable applications with BDRPC.

## Table of Contents

- [Protocol Design](#protocol-design)
- [Channel Management](#channel-management)
- [Error Handling](#error-handling)
- [Performance Optimization](#performance-optimization)
- [Security Considerations](#security-considerations)
- [Testing Strategies](#testing-strategies)
- [Production Deployment](#production-deployment)

## Protocol Design

### Use Semantic Versioning

Always version your protocols to support evolution:

```rust
use bdrpc::channel::Protocol;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserService {
    // fields
}

impl Protocol for UserService {
    fn method_name(&self) -> &'static str {
        "user_service"
    }
    
    fn is_request(&self) -> bool {
        true
    }
    
    fn version(&self) -> u32 {
        2  // Increment for breaking changes
    }
}
```

**Guidelines:**
- Increment version for breaking changes
- Support multiple versions during transitions
- Document version changes in code comments
- Use feature flags for gradual rollout

### Design for Backward Compatibility

Use optional fields for non-breaking changes:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserMessage {
    // Required fields - always present
    pub user_id: u64,
    pub username: String,
    
    // Optional fields - added in v2
    #[serde(default)]
    pub email: Option<String>,
    
    #[serde(default)]
    pub created_at: Option<u64>,
}
```

**Guidelines:**
- Use `#[serde(default)]` for new fields
- Use `Option<T>` for nullable fields
- Provide sensible defaults
- Never remove or rename existing fields without version bump

### Keep Messages Small

Design messages to be compact and efficient:

```rust
// Good: Compact message
#[derive(Serialize, Deserialize)]
struct UserUpdate {
    user_id: u64,
    field: UpdateField,
    value: String,
}

enum UpdateField {
    Username,
    Email,
    Status,
}

// Avoid: Large message with all fields
#[derive(Serialize, Deserialize)]
struct UserUpdateBad {
    user_id: u64,
    username: Option<String>,
    email: Option<String>,
    status: Option<String>,
    // ... many optional fields
}
```

**Guidelines:**
- Send only necessary data
- Use enums for variants
- Consider separate messages for different operations
- Use streaming for large data transfers

### Use Strong Types

Leverage Rust's type system for safety:

```rust
// Good: Strong types
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct UserId(u64);

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct SessionId(u64);

#[derive(Serialize, Deserialize)]
struct LoginRequest {
    user_id: UserId,  // Can't accidentally use SessionId
    session_id: SessionId,
}

// Avoid: Primitive types
#[derive(Serialize, Deserialize)]
struct LoginRequestBad {
    user_id: u64,  // Easy to mix up with session_id
    session_id: u64,
}
```

**Guidelines:**
- Use newtype pattern for IDs
- Use enums for variants
- Avoid stringly-typed data
- Leverage compile-time checks

### Document Protocols

Provide clear documentation for your protocols:

```rust
/// User authentication protocol.
///
/// # Version History
/// - v1: Initial version with username/password
/// - v2: Added optional email field
/// - v3: Added session management
///
/// # Examples
/// ```
/// use my_app::AuthProtocol;
/// 
/// let auth = AuthProtocol {
///     username: "alice".to_string(),
///     password: "secret".to_string(),
///     email: Some("alice@example.com".to_string()),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthProtocol {
    /// Username for authentication
    pub username: String,
    
    /// Password (should be hashed in production)
    pub password: String,
    
    /// Optional email address (added in v2)
    #[serde(default)]
    pub email: Option<String>,
}
```

**Guidelines:**
- Document version history
- Provide usage examples
- Explain field purposes
- Note security considerations

## Channel Management

### Register Protocols Early

Register all protocols during initialization:

```rust
async fn initialize_endpoint() -> Result<Endpoint, Box<dyn std::error::Error>> {
    let endpoint = Endpoint::new();
    
    // Register all protocols upfront
    endpoint.register_bidirectional::<UserService>()?;
    endpoint.register_bidirectional::<ChatService>()?;
    endpoint.register_bidirectional::<FileService>()?;
    
    Ok(endpoint)
}
```

**Guidelines:**
- Register protocols before accepting connections
- Use `register_bidirectional()` for most cases
- Use `register_caller()` / `register_responder()` for one-way protocols
- Avoid registering protocols dynamically during operation

### Use Appropriate Buffer Sizes

Choose buffer sizes based on your use case:

```rust
use bdrpc::endpoint::EndpointConfig;
use std::time::Duration;

// High-throughput service
let high_throughput_config = EndpointConfig::default()
    .with_channel_buffer_size(1000);  // Large buffer

// Low-latency service
let low_latency_config = EndpointConfig::default()
    .with_channel_buffer_size(10);  // Small buffer for quick backpressure

// Balanced
let balanced_config = EndpointConfig::default()
    .with_channel_buffer_size(100);  // Default
```

**Guidelines:**
- Larger buffers: Higher throughput, more memory
- Smaller buffers: Lower latency, faster backpressure
- Default (100) works well for most cases
- Monitor queue depths in production

### Clean Up Channels

Properly close channels when done:

```rust
async fn process_messages(
    sender: Sender<MyMessage>,
    mut receiver: Receiver<MyMessage>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Process messages
    while let Some(msg) = receiver.recv().await {
        process(msg).await?;
    }
    
    // Channels are automatically closed when dropped
    // But you can explicitly drop them:
    drop(sender);
    drop(receiver);
    
    Ok(())
}
```

**Guidelines:**
- Channels close automatically when dropped
- Explicitly drop for clarity
- Don't hold channels longer than needed
- Monitor active channel count

### Handle Channel Errors

Implement robust error handling:

```rust
use bdrpc::channel::ChannelError;

async fn send_with_retry(
    sender: &Sender<MyMessage>,
    msg: MyMessage,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut retries = 3;
    
    loop {
        match sender.send(msg.clone()).await {
            Ok(_) => return Ok(()),
            Err(ChannelError::Closed) => {
                // Channel closed, can't retry
                return Err("Channel closed".into());
            }
            Err(e) if retries > 0 => {
                // Transient error, retry
                retries -= 1;
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(e) => {
                // Out of retries
                return Err(e.into());
            }
        }
    }
}
```

**Guidelines:**
- Distinguish between transient and permanent errors
- Implement retry logic for transient errors
- Use exponential backoff for retries
- Log errors with context

## Error Handling

### Use Specific Error Types

Match on specific error types for better handling:

```rust
use bdrpc::endpoint::EndpointError;

async fn create_channel_with_fallback(
    endpoint: &Endpoint,
) -> Result<(Sender<MyProtocol>, Receiver<MyProtocol>), Box<dyn std::error::Error>> {
    match endpoint.get_channels::<MyProtocol>().await {
        Ok(channels) => Ok(channels),
        
        Err(EndpointError::ChannelRequestTimeout { .. }) => {
            // Timeout - retry with longer timeout
            tokio::time::sleep(Duration::from_secs(1)).await;
            endpoint.get_channels::<MyProtocol>().await
                .map_err(Into::into)
        }
        
        Err(EndpointError::ChannelRequestRejected { reason, .. }) => {
            // Rejected - log and fail
            eprintln!("Channel rejected: {}", reason);
            Err("Channel creation rejected".into())
        }
        
        Err(e) => Err(e.into()),
    }
}
```

**Guidelines:**
- Match on specific error variants
- Implement appropriate recovery strategies
- Log errors with full context
- Provide user-friendly error messages

### Implement Timeouts

Always use timeouts for operations:

```rust
use tokio::time::{timeout, Duration};

async fn send_with_timeout(
    sender: &Sender<MyMessage>,
    msg: MyMessage,
) -> Result<(), Box<dyn std::error::Error>> {
    timeout(
        Duration::from_secs(5),
        sender.send(msg)
    ).await??;
    
    Ok(())
}

async fn recv_with_timeout(
    receiver: &mut Receiver<MyMessage>,
) -> Result<Option<MyMessage>, Box<dyn std::error::Error>> {
    match timeout(Duration::from_secs(10), receiver.recv()).await {
        Ok(msg) => Ok(msg),
        Err(_) => {
            // Timeout
            Ok(None)
        }
    }
}
```

**Guidelines:**
- Use timeouts on all network operations
- Choose appropriate timeout values
- Handle timeout errors gracefully
- Consider adaptive timeouts for varying network conditions

### Log Errors Effectively

Provide context in error logs:

```rust
use tracing::{error, warn, info};

async fn handle_connection(endpoint: &Endpoint) {
    match endpoint.get_channels::<MyProtocol>().await {
        Ok((sender, receiver)) => {
            info!("Channel created successfully");
            // Use channels
        }
        Err(e) => {
            error!(
                error = %e,
                protocol = "MyProtocol",
                connection_id = %endpoint.connection_id(),
                "Failed to create channel"
            );
        }
    }
}
```

**Guidelines:**
- Use structured logging (tracing)
- Include relevant context (connection ID, protocol name)
- Use appropriate log levels
- Don't log sensitive data

## Performance Optimization

### Choose the Right Serializer

Select serializer based on requirements:

```rust
use bdrpc::serialization::{JsonSerializer, PostcardSerializer, RkyvSerializer};

// Human-readable, debugging
let debug_endpoint = Endpoint::with_serializer(JsonSerializer);

// Balanced performance
let balanced_endpoint = Endpoint::with_serializer(PostcardSerializer);

// Maximum performance
let fast_endpoint = Endpoint::with_serializer(RkyvSerializer);
```

**Guidelines:**
- JSON: Human-readable, slower, larger
- Postcard: Binary, fast, compact
- rkyv: Zero-copy, fastest, complex
- Use JSON for development, binary for production

### Batch Small Messages

Combine small messages for better throughput:

```rust
async fn send_batch(
    sender: &Sender<MyMessage>,
    messages: Vec<MyMessage>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Send all messages without waiting
    for msg in messages {
        sender.send(msg).await?;
    }
    Ok(())
}

// Or use a batching wrapper
struct BatchSender<T> {
    sender: Sender<T>,
    batch: Vec<T>,
    batch_size: usize,
}

impl<T: Clone> BatchSender<T> {
    async fn send(&mut self, msg: T) -> Result<(), Box<dyn std::error::Error>> {
        self.batch.push(msg);
        
        if self.batch.len() >= self.batch_size {
            self.flush().await?;
        }
        
        Ok(())
    }
    
    async fn flush(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        for msg in self.batch.drain(..) {
            self.sender.send(msg).await?;
        }
        Ok(())
    }
}
```

**Guidelines:**
- Batch messages when latency is not critical
- Flush batches periodically
- Balance batch size vs latency
- Monitor throughput metrics

### Use Compression for Large Messages

Enable compression for large payloads:

```rust
use bdrpc::transport::CompressionConfig;

let config = EndpointConfig::default()
    .with_compression(CompressionConfig {
        enabled: true,
        min_size: 1024,  // Only compress messages > 1KB
        level: 6,        // Compression level (1-9)
    });
```

**Guidelines:**
- Only compress large messages (> 1KB)
- Balance compression ratio vs CPU usage
- Test with your actual data
- Monitor CPU and bandwidth usage

### Profile and Benchmark

Measure before optimizing:

```bash
# Run benchmarks
cargo bench

# Profile with flamegraph
cargo install flamegraph
cargo flamegraph --bench throughput

# Memory profiling
cargo install cargo-instruments
cargo instruments -t alloc --bench throughput
```

**Guidelines:**
- Establish baseline performance
- Profile to find bottlenecks
- Optimize hot paths first
- Verify improvements with benchmarks

## Security Considerations

### Validate Input

Always validate messages from untrusted sources:

```rust
#[derive(Serialize, Deserialize)]
struct UserInput {
    username: String,
    age: u32,
}

fn validate_user_input(input: &UserInput) -> Result<(), String> {
    // Validate username
    if input.username.is_empty() || input.username.len() > 50 {
        return Err("Invalid username length".to_string());
    }
    
    if !input.username.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err("Invalid username characters".to_string());
    }
    
    // Validate age
    if input.age > 150 {
        return Err("Invalid age".to_string());
    }
    
    Ok(())
}

async fn handle_user_input(
    mut receiver: Receiver<UserInput>,
) -> Result<(), Box<dyn std::error::Error>> {
    while let Some(input) = receiver.recv().await {
        // Validate before processing
        if let Err(e) = validate_user_input(&input) {
            eprintln!("Invalid input: {}", e);
            continue;
        }
        
        // Process validated input
        process_user(input).await?;
    }
    Ok(())
}
```

**Guidelines:**
- Validate all input from network
- Check bounds and constraints
- Sanitize strings
- Reject invalid input early

### Use TLS for Production

Enable TLS for encrypted communication:

```rust
use bdrpc::transport::TlsConfig;

let tls_config = TlsConfig {
    cert_path: "server.crt".into(),
    key_path: "server.key".into(),
    ca_path: Some("ca.crt".into()),
};

let endpoint = Endpoint::with_tls(tls_config);
```

**Guidelines:**
- Always use TLS in production
- Use valid certificates
- Keep certificates up to date
- Verify peer certificates

### Implement Authentication

Authenticate connections before use:

```rust
#[derive(Serialize, Deserialize)]
struct AuthRequest {
    username: String,
    token: String,
}

async fn authenticate_connection(
    endpoint: &Endpoint,
) -> Result<bool, Box<dyn std::error::Error>> {
    let (sender, mut receiver) = endpoint.get_channels::<AuthRequest>().await?;
    
    // Receive auth request
    let auth = receiver.recv().await
        .ok_or("No auth request received")?;
    
    // Verify token
    let valid = verify_token(&auth.username, &auth.token).await?;
    
    if !valid {
        return Ok(false);
    }
    
    Ok(true)
}
```

**Guidelines:**
- Authenticate before processing requests
- Use secure token generation
- Implement rate limiting
- Log authentication attempts

### Rate Limiting

Protect against abuse:

```rust
use std::collections::HashMap;
use std::time::{Duration, Instant};

struct RateLimiter {
    requests: HashMap<String, Vec<Instant>>,
    max_requests: usize,
    window: Duration,
}

impl RateLimiter {
    fn check(&mut self, client_id: &str) -> bool {
        let now = Instant::now();
        let requests = self.requests.entry(client_id.to_string())
            .or_insert_with(Vec::new);
        
        // Remove old requests
        requests.retain(|&time| now.duration_since(time) < self.window);
        
        // Check limit
        if requests.len() >= self.max_requests {
            return false;
        }
        
        requests.push(now);
        true
    }
}
```

**Guidelines:**
- Implement per-client rate limiting
- Use sliding window algorithm
- Return appropriate error codes
- Log rate limit violations

## Testing Strategies

### Unit Test Protocols

Test protocol serialization:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_protocol_serialization() {
        let msg = MyMessage {
            id: 42,
            data: "test".to_string(),
        };
        
        // Test JSON serialization
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: MyMessage = serde_json::from_str(&json).unwrap();
        
        assert_eq!(msg.id, deserialized.id);
        assert_eq!(msg.data, deserialized.data);
    }
    
    #[test]
    fn test_protocol_version() {
        let msg = MyMessage::default();
        assert_eq!(msg.version(), 2);
    }
}
```

### Integration Test Channels

Test end-to-end channel communication:

```rust
#[tokio::test]
async fn test_channel_communication() {
    let endpoint1 = Endpoint::new();
    let endpoint2 = Endpoint::new();
    
    endpoint1.register_bidirectional::<MyProtocol>().unwrap();
    endpoint2.register_bidirectional::<MyProtocol>().unwrap();
    
    // Connect endpoints
    let listener = endpoint1.listen("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    let client_handle = tokio::spawn(async move {
        endpoint2.connect(&addr.to_string()).await.unwrap();
        let (sender, _) = endpoint2.get_channels::<MyProtocol>().await.unwrap();
        sender.send(MyProtocol::default()).await.unwrap();
    });
    
    let server_handle = tokio::spawn(async move {
        listener.accept().await.unwrap();
        let (_, mut receiver) = endpoint1.get_channels::<MyProtocol>().await.unwrap();
        receiver.recv().await.unwrap();
    });
    
    client_handle.await.unwrap();
    server_handle.await.unwrap();
}
```

### Stress Test

Test under load:

```rust
#[tokio::test]
async fn test_high_load() {
    let endpoint = Endpoint::new();
    endpoint.register_bidirectional::<MyProtocol>().unwrap();
    
    // Create many channels
    let mut handles = vec![];
    for _ in 0..1000 {
        let ep = endpoint.clone();
        let handle = tokio::spawn(async move {
            let (sender, mut receiver) = ep.get_channels::<MyProtocol>().await.unwrap();
            
            // Send and receive messages
            for i in 0..100 {
                sender.send(MyProtocol { id: i }).await.unwrap();
                receiver.recv().await.unwrap();
            }
        });
        handles.push(handle);
    }
    
    // Wait for all tasks
    for handle in handles {
        handle.await.unwrap();
    }
}
```

## Production Deployment

### Enable Observability

Use metrics and health checks:

```rust
use bdrpc::observability::{Metrics, HealthCheck};

async fn monitor_endpoint(endpoint: &Endpoint) {
    // Collect metrics
    let metrics = endpoint.metrics();
    println!("Messages sent: {}", metrics.messages_sent());
    println!("Messages received: {}", metrics.messages_received());
    println!("Active channels: {}", metrics.active_channels());
    
    // Health check
    let health = endpoint.health_check().await;
    if !health.is_healthy() {
        eprintln!("Unhealthy: {:?}", health.issues());
    }
}
```

### Implement Graceful Shutdown

Handle shutdown cleanly:

```rust
use tokio::signal;

async fn run_server(endpoint: Endpoint) -> Result<(), Box<dyn std::error::Error>> {
    let listener = endpoint.listen("0.0.0.0:8080").await?;
    
    // Spawn server task
    let server_handle = tokio::spawn(async move {
        while let Ok(conn) = listener.accept().await {
            tokio::spawn(handle_connection(conn));
        }
    });
    
    // Wait for shutdown signal
    signal::ctrl_c().await?;
    println!("Shutting down...");
    
    // Stop accepting new connections
    drop(listener);
    
    // Wait for existing connections to finish (with timeout)
    tokio::time::timeout(
        Duration::from_secs(30),
        server_handle
    ).await??;
    
    println!("Shutdown complete");
    Ok(())
}
```

### Configure for Production

Use production-ready configuration:

```rust
let config = EndpointConfig::default()
    .with_connection_timeout(Duration::from_secs(30))
    .with_channel_timeout(Duration::from_secs(10))
    .with_channel_buffer_size(100)
    .with_reconnection_strategy(ExponentialBackoff::default())
    .with_compression(CompressionConfig {
        enabled: true,
        min_size: 1024,
        level: 6,
    });

let endpoint = Endpoint::with_config(config)
    .with_serializer(PostcardSerializer)
    .with_tls(tls_config);
```

## Summary

Following these best practices will help you build:
- **Robust** applications with proper error handling
- **Performant** systems with optimized serialization and batching
- **Secure** services with validation and authentication
- **Maintainable** code with good testing and documentation
- **Production-ready** deployments with monitoring and graceful shutdown

For more information, see:
- [Architecture Guide](architecture-guide.md)
- [Performance Guide](performance-guide.md)
- [Troubleshooting Guide](troubleshooting-guide.md)
- [Migration Guide](migration-guide.md)
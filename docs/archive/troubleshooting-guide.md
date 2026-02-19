# BDRPC Troubleshooting Guide

This guide helps you diagnose and resolve common issues when using BDRPC.

## Table of Contents

- [Connection Issues](#connection-issues)
- [Channel Creation Failures](#channel-creation-failures)
- [Serialization Errors](#serialization-errors)
- [Performance Issues](#performance-issues)
- [Deadlock Prevention](#deadlock-prevention)
- [Network Failures](#network-failures)
- [Protocol Negotiation](#protocol-negotiation)

## Connection Issues

### Problem: Connection Refused

**Symptoms:**
```
Error: Transport error: Connection refused (os error 10061)
```

**Causes:**
1. Server is not running
2. Wrong host/port
3. Firewall blocking connection
4. Server not listening on the correct interface

**Solutions:**
1. Verify server is running: Check that `Endpoint::listen()` was called and is active
2. Check connection details:
   ```rust
   // Ensure host and port match
   let endpoint = Endpoint::connect("127.0.0.1:8080").await?;
   ```
3. Check firewall settings: Allow the port through Windows Firewall
4. Verify server binding:
   ```rust
   // Bind to all interfaces
   let listener = endpoint.listen("0.0.0.0:8080").await?;
   
   // Or specific interface
   let listener = endpoint.listen("127.0.0.1:8080").await?;
   ```

### Problem: Connection Timeout

**Symptoms:**
```
Error: Transport error: Connection timed out
```

**Causes:**
1. Network latency too high
2. Server overloaded
3. Incorrect timeout configuration

**Solutions:**
1. Increase connection timeout:
   ```rust
   use std::time::Duration;
   
   let config = EndpointConfig::default()
       .with_connection_timeout(Duration::from_secs(30));
   let endpoint = Endpoint::with_config(config);
   ```
2. Check network connectivity
3. Verify server is responsive

### Problem: Address Already in Use

**Symptoms:**
```
Error: Transport error: Address already in use (os error 10048)
```

**Causes:**
1. Another process is using the port
2. Previous server instance didn't shut down cleanly
3. Multiple listeners on same port

**Solutions:**
1. Use a different port
2. Find and stop the process using the port:
   ```powershell
   # Windows
   netstat -ano | findstr :8080
   taskkill /PID <process_id> /F
   ```
3. Wait for OS to release the port (typically 30-120 seconds)
4. Enable SO_REUSEADDR if appropriate for your use case

## Channel Creation Failures

### Problem: Channel Request Timeout

**Symptoms:**
```
Error: Channel request timed out after 5s for protocol 'MyProtocol' on connection abc123
Hint: Ensure the peer is responding to channel requests and not overloaded
```

**Causes:**
1. Peer not responding to channel requests
2. Peer overloaded or blocked
3. Network latency too high
4. Timeout too short

**Solutions:**
1. Verify peer is running and responsive
2. Check peer's channel negotiator is configured:
   ```rust
   // Server side - ensure negotiator allows the protocol
   endpoint.register_bidirectional::<MyProtocol>()?;
   ```
3. Increase channel creation timeout:
   ```rust
   let config = EndpointConfig::default()
       .with_channel_timeout(Duration::from_secs(10));
   ```
4. Check network latency with ping/traceroute

### Problem: Channel Request Rejected

**Symptoms:**
```
Error: Channel request rejected by peer for protocol 'MyProtocol' on connection abc123
Reason: Protocol not allowed
Hint: Ensure both endpoints have registered the protocol and the negotiator allows it
```

**Causes:**
1. Protocol not registered on peer
2. Protocol not allowed by negotiator
3. Protocol version mismatch
4. Custom negotiator rejecting requests

**Solutions:**
1. Register protocol on both endpoints:
   ```rust
   // Client
   endpoint.register_caller::<MyProtocol>()?;
   
   // Server
   endpoint.register_responder::<MyProtocol>()?;
   
   // Or bidirectional
   endpoint.register_bidirectional::<MyProtocol>()?;
   ```
2. Verify protocol is allowed (automatic with default negotiator):
   ```rust
   // With default negotiator, registration auto-allows
   // For custom negotiator, explicitly allow:
   if let Some(negotiator) = endpoint.default_negotiator() {
       negotiator.allow_protocol("MyProtocol");
   }
   ```
3. Check protocol versions match on both sides

### Problem: Channel Creation Failed

**Symptoms:**
```
Error: Failed to create channel for protocol 'MyProtocol' on connection abc123
Hint: Check that the protocol is registered and the connection is active
```

**Causes:**
1. Protocol not registered
2. Connection closed
3. Internal channel manager error
4. Resource exhaustion

**Solutions:**
1. Register protocol before creating channels:
   ```rust
   endpoint.register_bidirectional::<MyProtocol>()?;
   let (sender, receiver) = endpoint.get_channels::<MyProtocol>().await?;
   ```
2. Verify connection is active:
   ```rust
   if !endpoint.is_connected() {
       // Reconnect or handle error
   }
   ```
3. Check system resources (memory, file descriptors)

## Serialization Errors

### Problem: Serialization Failed

**Symptoms:**
```
Error: Serialization error: Failed to serialize message
```

**Causes:**
1. Message type doesn't implement required traits
2. Circular references in data structure
3. Unsupported data types
4. Buffer too small

**Solutions:**
1. Ensure message implements required traits:
   ```rust
   use serde::{Serialize, Deserialize};
   
   #[derive(Debug, Clone, Serialize, Deserialize)]
   struct MyMessage {
       // fields
   }
   ```
2. Avoid circular references or use `Rc`/`Arc` with care
3. Use supported serialization formats (JSON, Postcard, rkyv)
4. Check for large messages that may need streaming

### Problem: Deserialization Failed

**Symptoms:**
```
Error: Deserialization error: Invalid data format
```

**Causes:**
1. Protocol version mismatch
2. Corrupted data
3. Wrong serializer on receiving end
4. Incompatible schema changes

**Solutions:**
1. Ensure both endpoints use same serializer:
   ```rust
   // Both sides must use same serializer
   use bdrpc::serialization::JsonSerializer;
   
   let endpoint = Endpoint::with_serializer(JsonSerializer);
   ```
2. Use protocol versioning for schema evolution
3. Validate data integrity with checksums
4. Follow backward-compatible schema changes

## Performance Issues

### Problem: Low Throughput

**Symptoms:**
- Messages per second below expectations
- High latency
- CPU usage high

**Causes:**
1. Small buffer sizes
2. Inefficient serialization
3. Network bottleneck
4. Too many small messages

**Solutions:**
1. Increase buffer sizes:
   ```rust
   let config = EndpointConfig::default()
       .with_channel_buffer_size(1000);
   ```
2. Use efficient serializers (Postcard or rkyv instead of JSON):
   ```rust
   use bdrpc::serialization::PostcardSerializer;
   let endpoint = Endpoint::with_serializer(PostcardSerializer);
   ```
3. Batch small messages:
   ```rust
   // Send multiple messages in quick succession
   for msg in messages {
       sender.send(msg).await?;
   }
   ```
4. Use compression for large messages
5. Profile with benchmarks: `cargo bench`

### Problem: High Memory Usage

**Symptoms:**
- Memory usage growing over time
- Out of memory errors
- Slow performance

**Causes:**
1. Channel buffers too large
2. Messages not being consumed
3. Memory leaks in application code
4. Too many channels

**Solutions:**
1. Reduce buffer sizes:
   ```rust
   let config = EndpointConfig::default()
       .with_channel_buffer_size(100);
   ```
2. Ensure receivers are consuming messages:
   ```rust
   // Don't let messages accumulate
   while let Some(msg) = receiver.recv().await {
       process(msg).await?;
   }
   ```
3. Close unused channels:
   ```rust
   drop(sender);
   drop(receiver);
   ```
4. Monitor with profiling tools

### Problem: Backpressure Not Working

**Symptoms:**
- Sender not blocking when receiver is slow
- Messages being dropped
- Memory usage growing

**Causes:**
1. Unbounded channels
2. Receiver not applying backpressure
3. Configuration issue

**Solutions:**
1. Use bounded channels (default):
   ```rust
   // Bounded channels apply backpressure automatically
   let (sender, receiver) = endpoint.get_channels::<MyProtocol>().await?;
   ```
2. Ensure receiver processes messages:
   ```rust
   // Slow receiver will cause sender to block
   tokio::spawn(async move {
       while let Some(msg) = receiver.recv().await {
           // Process message (may be slow)
           process(msg).await;
       }
   });
   ```
3. Monitor queue depths with metrics

## Deadlock Prevention

### Problem: Deadlock Detected

**Symptoms:**
- Application hangs
- No progress being made
- Timeout errors

**Causes:**
1. Circular channel dependencies
2. Blocking in async context
3. Lock ordering issues
4. Request-response cycles

**Solutions:**
1. Follow deadlock prevention guidelines in [deadlock-prevention.md](deadlock-prevention.md)
2. Use timeouts on all operations:
   ```rust
   use tokio::time::timeout;
   
   let result = timeout(
       Duration::from_secs(5),
       sender.send(msg)
   ).await??;
   ```
3. Avoid circular dependencies:
   ```rust
   // BAD: A waits for B, B waits for A
   // GOOD: Use separate channels or one-way communication
   ```
4. Never block in async code:
   ```rust
   // BAD
   std::thread::sleep(Duration::from_secs(1));
   
   // GOOD
   tokio::time::sleep(Duration::from_secs(1)).await;
   ```

## Network Failures

### Problem: Connection Dropped

**Symptoms:**
```
Error: Transport error: Connection reset by peer
```

**Causes:**
1. Network interruption
2. Peer crashed
3. Firewall closed connection
4. Idle timeout

**Solutions:**
1. Implement reconnection strategy:
   ```rust
   use bdrpc::reconnection::ExponentialBackoff;
   
   let config = EndpointConfig::default()
       .with_reconnection_strategy(ExponentialBackoff::default());
   ```
2. Handle connection errors gracefully:
   ```rust
   match sender.send(msg).await {
       Ok(_) => {},
       Err(e) if e.is_connection_error() => {
           // Reconnect and retry
           endpoint.reconnect().await?;
       }
       Err(e) => return Err(e),
   }
   ```
3. Implement heartbeat/keepalive
4. Monitor connection health

### Problem: Slow Network

**Symptoms:**
- High latency
- Timeouts
- Poor throughput

**Causes:**
1. Network congestion
2. High packet loss
3. Bandwidth limitations
4. Routing issues

**Solutions:**
1. Increase timeouts:
   ```rust
   let config = EndpointConfig::default()
       .with_connection_timeout(Duration::from_secs(30))
       .with_channel_timeout(Duration::from_secs(10));
   ```
2. Use compression for large messages
3. Implement adaptive timeouts
4. Monitor network metrics
5. Consider QoS settings

## Protocol Negotiation

### Problem: Protocol Version Mismatch

**Symptoms:**
```
Error: Protocol version mismatch: client=2, server=1
```

**Causes:**
1. Client and server using different versions
2. Incompatible protocol changes
3. Missing version negotiation

**Solutions:**
1. Implement version negotiation:
   ```rust
   impl Protocol for MyProtocol {
       fn version(&self) -> u32 {
           2  // Current version
       }
   }
   ```
2. Support multiple versions:
   ```rust
   // Server supports versions 1 and 2
   endpoint.register_protocol_version::<MyProtocolV1>(1)?;
   endpoint.register_protocol_version::<MyProtocolV2>(2)?;
   ```
3. Follow protocol evolution guidelines in [protocol-evolution.md](protocol-evolution.md)

### Problem: Feature Negotiation Failed

**Symptoms:**
```
Error: Required feature not supported by peer
```

**Causes:**
1. Peer doesn't support required feature
2. Feature not enabled
3. Configuration mismatch

**Solutions:**
1. Check feature support before using:
   ```rust
   if endpoint.supports_feature("compression") {
       // Use compression
   } else {
       // Fallback to uncompressed
   }
   ```
2. Make features optional when possible
3. Provide clear error messages about requirements

## Debugging Tips

### Enable Detailed Logging

```rust
use tracing_subscriber;

tracing_subscriber::fmt()
    .with_max_level(tracing::Level::DEBUG)
    .init();
```

### Use Metrics

```rust
use bdrpc::observability::Metrics;

let metrics = endpoint.metrics();
println!("Messages sent: {}", metrics.messages_sent());
println!("Messages received: {}", metrics.messages_received());
println!("Active channels: {}", metrics.active_channels());
```

### Enable Health Checks

```rust
use bdrpc::observability::HealthCheck;

let health = endpoint.health_check().await;
if !health.is_healthy() {
    println!("Unhealthy: {:?}", health.issues());
}
```

### Run Tests

```bash
# Run all tests
cargo nextest run

# Run specific test
cargo nextest run test_name

# Run with logging
RUST_LOG=debug cargo nextest run
```

### Run Benchmarks

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench --bench throughput

# Compare against baseline
cargo bench -- --baseline main
```

## Getting Help

If you're still experiencing issues:

1. Check the [documentation](README.md)
2. Review [examples](../bdrpc/examples/)
3. Search [GitHub issues](https://github.com/yourusername/bdrpc/issues)
4. Ask on [discussions](https://github.com/yourusername/bdrpc/discussions)
5. File a [bug report](https://github.com/yourusername/bdrpc/issues/new)

When reporting issues, include:
- BDRPC version
- Rust version
- Operating system
- Minimal reproducible example
- Error messages and logs
- Expected vs actual behavior
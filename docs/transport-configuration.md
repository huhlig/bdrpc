# Transport Configuration Guide

**Version:** 0.2.0  
**Last Updated:** 2026-02-07

## Overview

The BDRPC transport system provides a flexible and powerful way to manage network connections. Starting with v0.2.0, the enhanced Transport Manager supports:

- **Multiple transport types** (TCP, TLS, Memory, and more)
- **Multiple listeners** (server-side) on a single endpoint
- **Multiple callers** (client-side) with automatic reconnection
- **Dynamic transport management** (enable/disable at runtime)
- **Transport lifecycle events** for monitoring and control

This guide covers everything you need to know about configuring and using transports in BDRPC.

## Table of Contents

1. [Quick Start](#quick-start)
2. [Transport Types](#transport-types)
3. [Server Configuration](#server-configuration)
4. [Client Configuration](#client-configuration)
5. [Reconnection Strategies](#reconnection-strategies)
6. [Advanced Topics](#advanced-topics)
7. [Best Practices](#best-practices)
8. [Troubleshooting](#troubleshooting)

## Quick Start

### Simple Server

```rust
use bdrpc::endpoint::EndpointBuilder;
use bdrpc::serialization::JsonSerializer;

let endpoint = EndpointBuilder::server(JsonSerializer::default())
    .with_tcp_listener("0.0.0.0:8080")
    .with_responder("MyService", 1)
    .build()
    .await?;
```

### Simple Client with Auto-Reconnect

```rust
use bdrpc::endpoint::EndpointBuilder;
use bdrpc::reconnection::ExponentialBackoff;
use bdrpc::serialization::JsonSerializer;
use std::sync::Arc;

let reconnection = Arc::new(ExponentialBackoff::default());

let mut endpoint = EndpointBuilder::client(JsonSerializer::default())
    .with_tcp_caller("server", "127.0.0.1:8080")
    .with_reconnection_strategy("server", reconnection)
    .with_caller("MyService", 1)
    .build()
    .await?;

// Connect to the server
let connection = endpoint.connect_transport("server").await?;
```

## Transport Types

### TCP Transport

The most common transport type for network communication.

```rust
// Server: Listen on TCP
let endpoint = EndpointBuilder::server(serializer)
    .with_tcp_listener("0.0.0.0:8080")
    .build()
    .await?;

// Client: Connect via TCP
let endpoint = EndpointBuilder::client(serializer)
    .with_tcp_caller("server", "127.0.0.1:8080")
    .build()
    .await?;
```

**Features:**
- Standard TCP/IP networking
- Works across networks
- Firewall-friendly (single port)
- Good performance

**Use Cases:**
- General-purpose RPC
- Cross-machine communication
- Production deployments

### TLS Transport

Secure, encrypted communication using TLS.

```rust
use bdrpc::transport::TlsConfig;

// Server: Listen with TLS
#[cfg(feature = "tls")]
{
    let tls_config = TlsConfig::server()
        .with_cert_path("server.crt")
        .with_key_path("server.key")
        .build()?;

    let endpoint = EndpointBuilder::server(serializer)
        .with_tls_listener("0.0.0.0:8443", tls_config)
        .build()
        .await?;
}

// Client: Connect with TLS
#[cfg(feature = "tls")]
{
    let tls_config = TlsConfig::client()
        .with_ca_path("ca.crt")
        .build()?;

    let endpoint = EndpointBuilder::client(serializer)
        .with_tls_caller("server", "example.com:8443", tls_config)
        .build()
        .await?;
}
```

**Features:**
- Encrypted communication
- Certificate-based authentication
- Mutual TLS (mTLS) support
- Industry-standard security

**Use Cases:**
- Secure communication over untrusted networks
- Authentication requirements
- Compliance requirements (HIPAA, PCI-DSS, etc.)

### Memory Transport

In-process communication for testing and local IPC.

```rust
use bdrpc::transport::MemoryTransport;

// Create a memory transport pair
let (client_transport, server_transport) = MemoryTransport::pair();

// Use in endpoints...
```

**Features:**
- Zero-copy in-process communication
- No network overhead
- Perfect for testing
- Deterministic behavior

**Use Cases:**
- Unit testing
- Integration testing
- In-process IPC
- Development and debugging

### Custom Transports

You can implement custom transports for specialized needs:

```rust
use bdrpc::transport::{Transport, TransportConfig, TransportType};

let custom_config = TransportConfig::new(
    TransportType::Custom("my-protocol".to_string()),
    "custom://address",
);

let endpoint = EndpointBuilder::new(serializer)
    .with_transport(custom_config)
    .build()
    .await?;
```

## Server Configuration

### Single Listener

The simplest server configuration with one listener:

```rust
let endpoint = EndpointBuilder::server(serializer)
    .with_tcp_listener("0.0.0.0:8080")
    .with_responder("UserService", 1)
    .with_responder("OrderService", 1)
    .build()
    .await?;
```

### Multiple Listeners

Serve on multiple ports or protocols simultaneously:

```rust
let endpoint = EndpointBuilder::server(serializer)
    .with_tcp_listener("0.0.0.0:8080")      // HTTP-style port
    .with_tcp_listener("0.0.0.0:8443")      // HTTPS-style port
    .with_responder("UserService", 1)
    .build()
    .await?;
```

### Mixed Protocol Server

Combine TCP and TLS listeners:

```rust
#[cfg(feature = "tls")]
let endpoint = EndpointBuilder::server(serializer)
    .with_tcp_listener("0.0.0.0:8080")           // Unencrypted
    .with_tls_listener("0.0.0.0:8443", tls_config) // Encrypted
    .with_responder("UserService", 1)
    .build()
    .await?;
```

### Server with Custom Configuration

Fine-tune server behavior:

```rust
use std::time::Duration;

let endpoint = EndpointBuilder::server(serializer)
    .configure(|config| {
        config
            .with_channel_buffer_size(1000)
            .with_handshake_timeout(Duration::from_secs(30))
            .with_max_connections(Some(500))
    })
    .with_tcp_listener("0.0.0.0:8080")
    .with_responder("UserService", 1)
    .build()
    .await?;
```

## Client Configuration

### Simple Client

Connect to a single server:

```rust
let mut endpoint = EndpointBuilder::client(serializer)
    .with_tcp_caller("server", "127.0.0.1:8080")
    .with_caller("UserService", 1)
    .build()
    .await?;

let connection = endpoint.connect_transport("server").await?;
```

### Client with Reconnection

Automatically reconnect on connection loss:

```rust
use bdrpc::reconnection::ExponentialBackoff;
use std::time::Duration;

let reconnection = Arc::new(
    ExponentialBackoff::builder()
        .initial_delay(Duration::from_millis(100))
        .max_delay(Duration::from_secs(30))
        .multiplier(2.0)
        .max_attempts(None) // Unlimited
        .build()
);

let mut endpoint = EndpointBuilder::client(serializer)
    .with_tcp_caller("server", "127.0.0.1:8080")
    .with_reconnection_strategy("server", reconnection)
    .with_caller("UserService", 1)
    .build()
    .await?;
```

### Multi-Server Client

Connect to multiple servers:

```rust
let mut endpoint = EndpointBuilder::client(serializer)
    .with_tcp_caller("primary", "server1.example.com:8080")
    .with_tcp_caller("backup", "server2.example.com:8080")
    .with_caller("UserService", 1)
    .build()
    .await?;

// Connect to primary
let conn1 = endpoint.connect_transport("primary").await?;

// Connect to backup
let conn2 = endpoint.connect_transport("backup").await?;
```

### Client with Failover

Implement failover logic:

```rust
let transports = vec!["primary", "backup", "tertiary"];

for transport_name in &transports {
    match endpoint.connect_transport(transport_name).await {
        Ok(connection) => {
            println!("Connected to {}", transport_name);
            // Use this connection
            break;
        }
        Err(e) => {
            eprintln!("Failed to connect to {}: {}", transport_name, e);
            // Try next transport
        }
    }
}
```

## Reconnection Strategies

### Exponential Backoff

Increases delay exponentially between reconnection attempts:

```rust
use bdrpc::reconnection::ExponentialBackoff;
use std::time::Duration;

let strategy = ExponentialBackoff::builder()
    .initial_delay(Duration::from_millis(100))  // Start with 100ms
    .max_delay(Duration::from_secs(60))         // Cap at 60s
    .multiplier(2.0)                            // Double each time
    .jitter(true)                               // Add randomness
    .max_attempts(Some(10))                     // Try 10 times
    .build();
```

**Delay Sequence:** 100ms → 200ms → 400ms → 800ms → 1.6s → 3.2s → 6.4s → 12.8s → 25.6s → 51.2s

**Best For:**
- General-purpose reconnection
- Preventing thundering herd
- Network instability

### Fixed Delay

Constant delay between attempts:

```rust
use bdrpc::reconnection::FixedDelay;
use std::time::Duration;

let strategy = FixedDelay::builder()
    .delay(Duration::from_secs(5))
    .max_attempts(Some(20))
    .build();
```

**Best For:**
- Predictable behavior
- Testing
- Simple scenarios

### Circuit Breaker

Stops attempting after too many failures:

```rust
use bdrpc::reconnection::CircuitBreaker;
use std::time::Duration;

let strategy = CircuitBreaker::builder()
    .failure_threshold(5)                       // Open after 5 failures
    .success_threshold(2)                       // Close after 2 successes
    .timeout(Duration::from_secs(60))           // Stay open for 60s
    .build();
```

**States:**
- **Closed:** Normal operation, attempts allowed
- **Open:** Too many failures, attempts blocked
- **Half-Open:** Testing if service recovered

**Best For:**
- Protecting against cascading failures
- Microservices architectures
- High-availability systems

### No Reconnection

Disable automatic reconnection:

```rust
use bdrpc::reconnection::NoReconnect;

let strategy = Arc::new(NoReconnect);
```

**Best For:**
- Manual connection management
- One-shot connections
- Testing

### Custom Strategy

Implement your own reconnection logic:

```rust
use bdrpc::reconnection::ReconnectionStrategy;
use async_trait::async_trait;

struct CustomStrategy;

#[async_trait]
impl ReconnectionStrategy for CustomStrategy {
    async fn should_reconnect(&self, attempt: u32, error: &TransportError) -> bool {
        // Your logic here
        attempt < 5
    }

    async fn next_delay(&self, attempt: u32) -> Duration {
        // Your delay calculation
        Duration::from_secs(attempt as u64)
    }

    fn name(&self) -> &str {
        "CustomStrategy"
    }
}
```

## Advanced Topics

### Dynamic Transport Management

Enable and disable transports at runtime:

```rust
// Disable a transport
endpoint.disable_transport("backup").await?;

// Enable it again
endpoint.enable_transport("backup").await?;

// Remove a transport completely
endpoint.remove_caller("backup").await?;
```

### Transport Events

Monitor transport lifecycle:

```rust
use bdrpc::transport::TransportEventHandler;

struct MyEventHandler;

impl TransportEventHandler for MyEventHandler {
    fn on_transport_connected(&self, transport_id: TransportId) {
        println!("Transport {} connected", transport_id);
    }

    fn on_transport_disconnected(
        &self,
        transport_id: TransportId,
        error: Option<TransportError>
    ) {
        println!("Transport {} disconnected: {:?}", transport_id, error);
    }

    fn on_new_channel_request(
        &self,
        channel_id: ChannelId,
        protocol: &str,
        transport_id: TransportId
    ) -> Result<bool, String> {
        // Accept or reject channel creation
        Ok(true)
    }
}
```

### Custom Transport Configuration

Fine-tune transport behavior:

```rust
use bdrpc::transport::TransportConfig;
use std::collections::HashMap;

let mut metadata = HashMap::new();
metadata.insert("region".to_string(), "us-west".to_string());
metadata.insert("priority".to_string(), "high".to_string());

let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080")
    .with_enabled(true)
    .with_metadata(metadata);

let endpoint = EndpointBuilder::new(serializer)
    .with_transport(config)
    .build()
    .await?;
```

## Best Practices

### 1. Use Named Transports

Always name your transports for easy management:

```rust
// Good
.with_tcp_caller("primary-db", "db1.example.com:8080")
.with_tcp_caller("backup-db", "db2.example.com:8080")

// Avoid
.with_tcp_caller("tcp-caller-0", "...")
```

### 2. Configure Reconnection

Always configure reconnection for production clients:

```rust
let reconnection = Arc::new(ExponentialBackoff::default());

endpoint_builder
    .with_tcp_caller("server", address)
    .with_reconnection_strategy("server", reconnection)
```

### 3. Handle Connection Failures

Always handle connection failures gracefully:

```rust
match endpoint.connect_transport("server").await {
    Ok(connection) => {
        // Use connection
    }
    Err(e) => {
        eprintln!("Connection failed: {}", e);
        // Implement fallback logic
    }
}
```

### 4. Use TLS in Production

Always use TLS for production deployments:

```rust
#[cfg(feature = "tls")]
let endpoint = EndpointBuilder::server(serializer)
    .with_tls_listener("0.0.0.0:8443", tls_config)
    .build()
    .await?;
```

### 5. Monitor Transport Health

Implement health checks and monitoring:

```rust
// Periodically check connection health
tokio::spawn(async move {
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;
        // Check connection status
        // Reconnect if needed
    }
});
```

### 6. Limit Connection Attempts

Don't retry forever in production:

```rust
let reconnection = ExponentialBackoff::builder()
    .max_attempts(Some(10))  // Limit attempts
    .build();
```

### 7. Use Appropriate Buffer Sizes

Configure buffer sizes based on your workload:

```rust
endpoint_builder.configure(|config| {
    config
        .with_channel_buffer_size(1000)  // High throughput
        .with_max_frame_size(16 * 1024 * 1024)  // 16MB frames
})
```

## Troubleshooting

### Connection Refused

**Problem:** `connect_transport()` fails with "connection refused"

**Solutions:**
1. Verify server is running
2. Check firewall rules
3. Verify correct address and port
4. Check network connectivity

```rust
// Add detailed error logging
match endpoint.connect_transport("server").await {
    Err(e) => {
        eprintln!("Connection failed: {}", e);
        eprintln!("Check: Server running? Firewall? Network?");
    }
    Ok(conn) => { /* ... */ }
}
```

### Reconnection Not Working

**Problem:** Client doesn't reconnect after disconnection

**Solutions:**
1. Verify reconnection strategy is configured
2. Check max_attempts limit
3. Review error logs

```rust
// Enable detailed logging
#[cfg(feature = "tracing")]
tracing_subscriber::fmt()
    .with_max_level(tracing::Level::DEBUG)
    .init();
```

### High Memory Usage

**Problem:** Memory usage grows over time

**Solutions:**
1. Reduce channel buffer sizes
2. Limit max connections
3. Implement connection pooling

```rust
endpoint_builder.configure(|config| {
    config
        .with_channel_buffer_size(100)  // Smaller buffers
        .with_max_connections(Some(50))  // Limit connections
})
```

### Slow Performance

**Problem:** RPC calls are slow

**Solutions:**
1. Use TCP instead of TLS for local connections
2. Increase buffer sizes
3. Enable compression for large messages
4. Check network latency

```rust
// Optimize for performance
endpoint_builder.configure(|config| {
    config
        .with_channel_buffer_size(2000)  // Larger buffers
        .with_max_frame_size(32 * 1024 * 1024)  // 32MB frames
})
```

## Migration from v0.1.0

See the [Migration Guide](migration-guide-v0.2.0.md) for detailed instructions on upgrading from v0.1.0 to v0.2.0.

## See Also

- [Quick Start Guide](quick-start.md)
- [Architecture Guide](architecture-guide.md)
- [Migration Guide v0.2.0](migration-guide-v0.2.0.md)
- [API Documentation](https://docs.rs/bdrpc)

---

**Made with Bob**
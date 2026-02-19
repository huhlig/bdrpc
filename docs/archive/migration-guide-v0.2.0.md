# Migration Guide: v0.1.0 → v0.2.0

**Version:** 0.2.0  
**Last Updated:** 2026-02-07  
**Breaking Changes:** Yes

## Overview

Version 0.2.0 introduces a major enhancement to the transport management system. While this is a breaking change, we've provided backward compatibility shims and clear migration paths to make the transition smooth.

**Key Changes:**
- Enhanced Transport Manager with multi-transport support
- New `EndpointBuilder` transport configuration methods
- Deprecated old connection/listener methods
- Improved reconnection integration
- Better transport lifecycle management

**Migration Timeline:**
- **v0.2.0:** Old API deprecated with warnings
- **v0.3.0:** Old API will be removed

## Quick Migration Checklist

- [ ] Update `Cargo.toml` to version 0.2.0
- [ ] Replace `connect()` with `connect_transport()`
- [ ] Replace `listen()` with `add_listener()` + builder
- [ ] Replace `connect_with_reconnection()` with builder pattern
- [ ] Update `EndpointBuilder` usage for transports
- [ ] Test all connection/listener code
- [ ] Remove deprecation warnings

## Breaking Changes

### 1. Connection Methods

#### Old API (v0.1.0)

```rust
let mut endpoint = Endpoint::new(serializer, config);
endpoint.register_caller("UserService", 1).await?;

// Direct connection
let connection = endpoint.connect("127.0.0.1:8080").await?;
```

#### New API (v0.2.0)

```rust
let mut endpoint = EndpointBuilder::client(serializer)
    .with_tcp_caller("server", "127.0.0.1:8080")
    .with_caller("UserService", 1)
    .build()
    .await?;

// Named transport connection
let connection = endpoint.connect_transport("server").await?;
```

**Why Changed:**
- Named transports enable better management
- Supports multiple simultaneous connections
- Clearer separation of configuration and connection

### 2. Listener Methods

#### Old API (v0.1.0)

```rust
let mut endpoint = Endpoint::new(serializer, config);
endpoint.register_responder("UserService", 1).await?;

let listener = endpoint.listen("0.0.0.0:8080").await?;
```

#### New API (v0.2.0)

```rust
let endpoint = EndpointBuilder::server(serializer)
    .with_tcp_listener("0.0.0.0:8080")
    .with_responder("UserService", 1)
    .build()
    .await?;

// Listener is automatically managed
```

**Why Changed:**
- Automatic listener management
- Support for multiple listeners
- Cleaner builder pattern

### 3. Reconnection Configuration

#### Old API (v0.1.0)

```rust
let strategy = Arc::new(ExponentialBackoff::default());
let connection = endpoint
    .connect_with_reconnection("127.0.0.1:8080", strategy)
    .await?;
```

#### New API (v0.2.0)

```rust
let strategy = Arc::new(ExponentialBackoff::default());

let mut endpoint = EndpointBuilder::client(serializer)
    .with_tcp_caller("server", "127.0.0.1:8080")
    .with_reconnection_strategy("server", strategy)
    .with_caller("UserService", 1)
    .build()
    .await?;

let connection = endpoint.connect_transport("server").await?;
```

**Why Changed:**
- Reconnection configured at transport level
- Supports different strategies per transport
- More flexible and maintainable

## Step-by-Step Migration

### Step 1: Update Dependencies

Update your `Cargo.toml`:

```toml
[dependencies]
bdrpc = "0.2.0"
```

Run:
```bash
cargo update
```

### Step 2: Migrate Client Code

#### Before (v0.1.0)

```rust
use bdrpc::endpoint::{Endpoint, EndpointConfig};
use bdrpc::serialization::JsonSerializer;

async fn create_client() -> Result<Endpoint<JsonSerializer>, Box<dyn std::error::Error>> {
    let config = EndpointConfig::default();
    let mut endpoint = Endpoint::new(JsonSerializer::default(), config);
    
    endpoint.register_caller("UserService", 1).await?;
    endpoint.register_caller("OrderService", 1).await?;
    
    let connection = endpoint.connect("127.0.0.1:8080").await?;
    
    Ok(endpoint)
}
```

#### After (v0.2.0)

```rust
use bdrpc::endpoint::EndpointBuilder;
use bdrpc::serialization::JsonSerializer;

async fn create_client() -> Result<Endpoint<JsonSerializer>, Box<dyn std::error::Error>> {
    let mut endpoint = EndpointBuilder::client(JsonSerializer::default())
        .with_tcp_caller("server", "127.0.0.1:8080")
        .with_caller("UserService", 1)
        .with_caller("OrderService", 1)
        .build()
        .await?;
    
    let connection = endpoint.connect_transport("server").await?;
    
    Ok(endpoint)
}
```

### Step 3: Migrate Server Code

#### Before (v0.1.0)

```rust
use bdrpc::endpoint::{Endpoint, EndpointConfig};
use bdrpc::serialization::JsonSerializer;

async fn create_server() -> Result<(), Box<dyn std::error::Error>> {
    let config = EndpointConfig::default();
    let mut endpoint = Endpoint::new(JsonSerializer::default(), config);
    
    endpoint.register_responder("UserService", 1).await?;
    endpoint.register_responder("OrderService", 1).await?;
    
    let listener = endpoint.listen("0.0.0.0:8080").await?;
    
    // Accept connections...
    
    Ok(())
}
```

#### After (v0.2.0)

```rust
use bdrpc::endpoint::EndpointBuilder;
use bdrpc::serialization::JsonSerializer;

async fn create_server() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = EndpointBuilder::server(JsonSerializer::default())
        .with_tcp_listener("0.0.0.0:8080")
        .with_responder("UserService", 1)
        .with_responder("OrderService", 1)
        .build()
        .await?;
    
    // Listener is automatically managed
    
    Ok(())
}
```

### Step 4: Migrate Reconnection Code

#### Before (v0.1.0)

```rust
use bdrpc::reconnection::ExponentialBackoff;
use std::sync::Arc;

let strategy = Arc::new(ExponentialBackoff::default());
let connection = endpoint
    .connect_with_reconnection("127.0.0.1:8080", strategy)
    .await?;
```

#### After (v0.2.0)

```rust
use bdrpc::reconnection::ExponentialBackoff;
use std::sync::Arc;
use std::time::Duration;

let strategy = Arc::new(
    ExponentialBackoff::builder()
        .initial_delay(Duration::from_millis(100))
        .max_delay(Duration::from_secs(60))
        .multiplier(2.0)
        .build()
);

let mut endpoint = EndpointBuilder::client(serializer)
    .with_tcp_caller("server", "127.0.0.1:8080")
    .with_reconnection_strategy("server", strategy)
    .with_caller("UserService", 1)
    .build()
    .await?;

let connection = endpoint.connect_transport("server").await?;
```

### Step 5: Update Manual Listener Code

#### Before (v0.1.0)

```rust
let mut listener = endpoint.listen_manual("127.0.0.1:8080").await?;

loop {
    let (sender, receiver) = listener.accept_channels::<MyProtocol>().await?;
    // Handle connection...
}
```

#### After (v0.2.0)

The manual listener pattern is still supported but deprecated. Consider using the automatic listener management in the new API. If you need manual control, the old API still works with deprecation warnings.

### Step 6: Test Your Changes

Run your test suite:

```bash
cargo test
```

Check for deprecation warnings:

```bash
cargo build 2>&1 | grep "deprecated"
```

## Common Migration Patterns

### Pattern 1: Simple Client

```rust
// OLD
let mut endpoint = Endpoint::new(serializer, config);
endpoint.register_caller("Service", 1).await?;
let conn = endpoint.connect("127.0.0.1:8080").await?;

// NEW
let mut endpoint = EndpointBuilder::client(serializer)
    .with_tcp_caller("server", "127.0.0.1:8080")
    .with_caller("Service", 1)
    .build()
    .await?;
let conn = endpoint.connect_transport("server").await?;
```

### Pattern 2: Simple Server

```rust
// OLD
let mut endpoint = Endpoint::new(serializer, config);
endpoint.register_responder("Service", 1).await?;
let listener = endpoint.listen("0.0.0.0:8080").await?;

// NEW
let endpoint = EndpointBuilder::server(serializer)
    .with_tcp_listener("0.0.0.0:8080")
    .with_responder("Service", 1)
    .build()
    .await?;
```

### Pattern 3: Client with Reconnection

```rust
// OLD
let strategy = Arc::new(ExponentialBackoff::default());
let conn = endpoint.connect_with_reconnection(addr, strategy).await?;

// NEW
let strategy = Arc::new(ExponentialBackoff::default());
let mut endpoint = EndpointBuilder::client(serializer)
    .with_tcp_caller("server", addr)
    .with_reconnection_strategy("server", strategy)
    .with_caller("Service", 1)
    .build()
    .await?;
let conn = endpoint.connect_transport("server").await?;
```

### Pattern 4: Peer-to-Peer

```rust
// OLD
let mut endpoint = Endpoint::new(serializer, config);
endpoint.register_bidirectional("Chat", 1).await?;
let listener = endpoint.listen("0.0.0.0:8080").await?;
let conn = endpoint.connect("127.0.0.1:8081").await?;

// NEW
let mut endpoint = EndpointBuilder::peer(serializer)
    .with_tcp_listener("0.0.0.0:8080")
    .with_tcp_caller("peer", "127.0.0.1:8081")
    .with_bidirectional("Chat", 1)
    .build()
    .await?;
let conn = endpoint.connect_transport("peer").await?;
```

### Pattern 5: Custom Configuration

```rust
// OLD
let config = EndpointConfig::new()
    .with_channel_buffer_size(1000)
    .with_handshake_timeout(Duration::from_secs(30));
let mut endpoint = Endpoint::new(serializer, config);

// NEW
let endpoint = EndpointBuilder::new(serializer)
    .configure(|config| {
        config
            .with_channel_buffer_size(1000)
            .with_handshake_timeout(Duration::from_secs(30))
    })
    .build()
    .await?;
```

## New Features in v0.2.0

### Multiple Transports

You can now configure multiple transports on a single endpoint:

```rust
let mut endpoint = EndpointBuilder::client(serializer)
    .with_tcp_caller("primary", "server1.example.com:8080")
    .with_tcp_caller("backup", "server2.example.com:8080")
    .with_caller("UserService", 1)
    .build()
    .await?;

// Connect to either transport
let conn1 = endpoint.connect_transport("primary").await?;
let conn2 = endpoint.connect_transport("backup").await?;
```

### Multiple Listeners

Servers can listen on multiple ports/protocols:

```rust
let endpoint = EndpointBuilder::server(serializer)
    .with_tcp_listener("0.0.0.0:8080")
    .with_tcp_listener("0.0.0.0:8081")
    .with_responder("UserService", 1)
    .build()
    .await?;
```

### TLS Support

Easy TLS configuration:

```rust
#[cfg(feature = "tls")]
let endpoint = EndpointBuilder::server(serializer)
    .with_tls_listener("0.0.0.0:8443", tls_config)
    .with_responder("UserService", 1)
    .build()
    .await?;
```

### Dynamic Transport Management

Enable/disable transports at runtime:

```rust
endpoint.disable_transport("backup").await?;
endpoint.enable_transport("backup").await?;
```

## Troubleshooting

### Issue: Deprecation Warnings

**Problem:** Getting deprecation warnings after upgrade

**Solution:** This is expected. Follow the migration guide to update your code. The old API will work until v0.3.0.

### Issue: Connection Fails

**Problem:** `connect_transport()` fails with "transport not found"

**Solution:** Make sure you've configured the transport with `with_tcp_caller()` before calling `connect_transport()`:

```rust
let mut endpoint = EndpointBuilder::client(serializer)
    .with_tcp_caller("server", "127.0.0.1:8080")  // Configure first
    .build()
    .await?;

let conn = endpoint.connect_transport("server").await?;  // Then connect
```

### Issue: Listener Not Working

**Problem:** Server not accepting connections

**Solution:** Ensure you've used `with_tcp_listener()` in the builder:

```rust
let endpoint = EndpointBuilder::server(serializer)
    .with_tcp_listener("0.0.0.0:8080")  // Add listener
    .with_responder("Service", 1)
    .build()
    .await?;
```

### Issue: Reconnection Not Working

**Problem:** Client doesn't reconnect after disconnection

**Solution:** Make sure you've configured the reconnection strategy:

```rust
let strategy = Arc::new(ExponentialBackoff::default());

let mut endpoint = EndpointBuilder::client(serializer)
    .with_tcp_caller("server", addr)
    .with_reconnection_strategy("server", strategy)  // Add this
    .build()
    .await?;
```

## Getting Help

If you encounter issues during migration:

1. Check the [Transport Configuration Guide](transport-configuration.md)
2. Review the [examples](../bdrpc/examples/)
3. Open an issue on [GitHub](https://github.com/huhlig/bdrpc/issues)
4. Join our community discussions

## Rollback Plan

If you need to rollback to v0.1.0:

1. Update `Cargo.toml`:
   ```toml
   [dependencies]
   bdrpc = "0.1.0"
   ```

2. Run:
   ```bash
   cargo update
   ```

3. Revert code changes

## Timeline

- **2026-02-07:** v0.2.0 released with deprecation warnings
- **2026-05-07:** v0.3.0 planned (old API removed)
- **Migration Period:** 3 months

## See Also

- [Transport Configuration Guide](transport-configuration.md)
- [Quick Start Guide](quick-start.md)
- [Architecture Guide](architecture-guide.md)
- [CHANGELOG](../CHANGELOG.md)

---

**Made with Bob**
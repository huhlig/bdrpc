# QUIC Transport Guide

**Version:** 0.2.0  
**Last Updated:** 2026-02-07

## Overview

The QUIC transport provides a modern, high-performance protocol for BDRPC communication. Built on UDP with mandatory TLS 1.3 encryption, QUIC offers superior performance on unreliable networks and includes features like connection migration, 0-RTT handshakes, and stream multiplexing.

## Features

- **Built-in TLS 1.3**: Mandatory encryption with no configuration needed
- **0-RTT Handshake**: Near-instant reconnection for returning clients
- **Connection Migration**: Survives IP address changes (WiFi ↔ cellular)
- **Stream Multiplexing**: No head-of-line blocking
- **Better Congestion Control**: Improved performance on lossy networks
- **UDP-based**: Lower latency than TCP in many scenarios

## Quick Start

### Server

```rust
use bdrpc::endpoint::EndpointBuilder;
use bdrpc::serialization::PostcardSerializer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = EndpointBuilder::server(PostcardSerializer::default())
        .with_quic_listener("0.0.0.0:4433")
        .with_responder("GameService", 1)
        .build()
        .await?;
    
    println!("QUIC server listening on 0.0.0.0:4433");
    
    // Keep server running
    tokio::signal::ctrl_c().await?;
    Ok(())
}
```

### Client

```rust
use bdrpc::endpoint::EndpointBuilder;
use bdrpc::serialization::PostcardSerializer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut endpoint = EndpointBuilder::client(PostcardSerializer::default())
        .with_quic_caller("server", "127.0.0.1:4433")
        .with_caller("GameService", 1)
        .build()
        .await?;
    
    let connection = endpoint.connect_transport("server").await?;
    println!("Connected to QUIC server");
    
    Ok(())
}
```

## Configuration

### QuicConfig

```rust
use bdrpc::transport::QuicConfig;
use std::time::Duration;

let config = QuicConfig {
    // Maximum idle timeout before connection is closed (default: 60s)
    max_idle_timeout: Duration::from_secs(60),
    
    // Interval for sending keepalive packets (default: 15s)
    keep_alive_interval: Duration::from_secs(15),
    
    // Maximum concurrent bidirectional streams (default: 100)
    max_concurrent_bidi_streams: 100,
    
    // Maximum concurrent unidirectional streams (default: 100)
    max_concurrent_uni_streams: 100,
    
    // Enable 0-RTT connection establishment (default: true)
    enable_0rtt: true,
    
    // Initial congestion window size (default: 128 KB)
    initial_window: 128 * 1024,
    
    // Maximum UDP payload size (default: 1350 bytes)
    max_udp_payload_size: 1350,
    
    // Enable connection migration (default: true)
    enable_migration: true,
};
```

### Using Custom Configuration

```rust
use bdrpc::transport::{TransportConfig, TransportType, QuicConfig};
use std::time::Duration;

let quic_config = QuicConfig {
    max_idle_timeout: Duration::from_secs(120),  // Longer timeout
    enable_0rtt: true,                           // Fast reconnection
    enable_migration: true,                      // Mobile support
    ..Default::default()
};

// Note: Custom QUIC config requires using the transport manager directly
// The builder methods use default configuration
```

## Connection Migration

One of QUIC's most powerful features is connection migration, which allows connections to survive network changes.

### Mobile Application Example

```rust
use bdrpc::endpoint::EndpointBuilder;
use bdrpc::serialization::PostcardSerializer;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure QUIC with connection migration
    let mut endpoint = EndpointBuilder::client(PostcardSerializer::default())
        .with_quic_caller("server", "api.example.com:4433")
        .with_caller("UserService", 1)
        .build()
        .await?;
    
    let connection = endpoint.connect_transport("server").await?;
    
    // Connection will automatically migrate when:
    // - Switching from WiFi to cellular
    // - Moving between cell towers
    // - IP address changes
    // - Network interface changes
    
    // Your application code doesn't need to handle reconnection!
    loop {
        // Use channels normally
        // Connection migration is transparent
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
```

### How Connection Migration Works

1. **Network Change Detected**: Client's IP address changes
2. **New Path Validation**: QUIC validates the new network path
3. **Seamless Migration**: Connection continues on new path
4. **No Application Impact**: Existing channels remain active

## 0-RTT Connection Establishment

QUIC's 0-RTT feature allows returning clients to send data immediately without waiting for a handshake.

### Benefits

- **Near-instant reconnection**: No handshake delay
- **Reduced latency**: First request sent immediately
- **Better user experience**: Faster app startup

### Example

```rust
// First connection: Full handshake (1-RTT)
let connection1 = endpoint.connect_transport("server").await?;
// ... use connection ...
drop(connection1);

// Subsequent connections: 0-RTT (if enabled)
let connection2 = endpoint.connect_transport("server").await?;
// Data can be sent immediately, no handshake wait!
```

### Security Considerations

0-RTT data is not forward-secret and can be replayed. BDRPC handles this automatically, but be aware:

- ✅ Safe for idempotent operations (GET requests)
- ⚠️ Use caution for state-changing operations
- ✅ BDRPC's channel system provides replay protection

## Performance Tuning

### Low-Latency Gaming

```rust
let config = QuicConfig {
    max_idle_timeout: Duration::from_secs(30),
    keep_alive_interval: Duration::from_secs(5),
    initial_window: 256 * 1024,  // Larger initial window
    enable_0rtt: true,            // Fast reconnection
    ..Default::default()
};
```

### High-Throughput Data Transfer

```rust
let config = QuicConfig {
    max_concurrent_bidi_streams: 1000,  // More streams
    initial_window: 512 * 1024,         // 512 KB initial window
    max_udp_payload_size: 1450,         // Larger packets
    ..Default::default()
};
```

### Mobile Networks

```rust
let config = QuicConfig {
    max_idle_timeout: Duration::from_secs(120),  // Longer timeout
    keep_alive_interval: Duration::from_secs(10), // More frequent
    enable_migration: true,                       // Essential for mobile
    max_udp_payload_size: 1200,                  // Smaller for reliability
    ..Default::default()
};
```

### Unreliable Networks

```rust
let config = QuicConfig {
    initial_window: 64 * 1024,           // Smaller initial window
    max_udp_payload_size: 1200,          // Smaller packets
    keep_alive_interval: Duration::from_secs(10),
    ..Default::default()
};
```

## TLS Configuration

QUIC includes TLS 1.3 by default. For testing, BDRPC generates self-signed certificates automatically.

### Production Certificates

For production, you should use proper certificates:

```rust
// Server with custom certificates
// Note: This requires using the transport manager directly
use bdrpc::transport::QuicListener;

// Production deployment typically uses:
// - Let's Encrypt certificates
// - Corporate CA certificates
// - Cloud provider certificates
```

### Certificate Validation

```rust
// Client certificate validation is automatic
// For testing, BDRPC skips validation (with warnings)
// In production, proper CA validation is performed
```

## Mobile App Patterns

### Handling Network Changes

```rust
use bdrpc::endpoint::EndpointBuilder;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut endpoint = EndpointBuilder::client(serializer)
        .with_quic_caller("server", "api.example.com:4433")
        .with_caller("UserService", 1)
        .build()
        .await?;
    
    let connection = endpoint.connect_transport("server").await?;
    
    // Monitor connection health
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(30)).await;
            // Connection automatically migrates on network changes
            // No manual intervention needed!
        }
    });
    
    Ok(())
}
```

### Background Data Sync

```rust
// QUIC's connection migration makes background sync reliable
async fn background_sync(endpoint: &mut Endpoint<impl Serializer>) {
    loop {
        // Sync types
        // Connection survives:
        // - App backgrounding
        // - Network switches
        // - IP changes
        
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}
```

### Push Notifications Alternative

```rust
// QUIC's persistent connection can replace push notifications
// for real-time updates while app is active

async fn listen_for_updates(
    receiver: &mut Receiver<UpdateMessage>
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        let update = receiver.recv().await?;
        // Process update immediately
        // No push notification delay!
    }
}
```

## Best Practices

### 1. Enable Connection Migration for Mobile

Always enable migration for mobile apps:

```rust
let config = QuicConfig {
    enable_migration: true,
    ..Default::default()
};
```

### 2. Use Appropriate Timeouts

Configure timeouts based on your use case:

```rust
// Real-time gaming: Short timeouts
let config = QuicConfig {
    max_idle_timeout: Duration::from_secs(30),
    keep_alive_interval: Duration::from_secs(5),
    ..Default::default()
};

// Background sync: Long timeouts
let config = QuicConfig {
    max_idle_timeout: Duration::from_secs(300),
    keep_alive_interval: Duration::from_secs(30),
    ..Default::default()
};
```

### 3. Monitor Connection Health

Implement health monitoring:

```rust
use bdrpc::transport::TransportEventHandler;

struct HealthMonitor;

impl TransportEventHandler for HealthMonitor {
    fn on_transport_connected(&self, transport_id: TransportId) {
        println!("QUIC connection established: {}", transport_id);
    }
    
    fn on_transport_disconnected(
        &self,
        transport_id: TransportId,
        error: Option<TransportError>
    ) {
        println!("QUIC connection lost: {:?}", error);
        // Implement reconnection logic if needed
    }
}
```

### 4. Handle Firewall Issues

Some networks block UDP. Implement fallback:

```rust
// Try QUIC first
match endpoint.connect_transport("quic-server").await {
    Ok(conn) => {
        println!("Connected via QUIC");
        // Use QUIC connection
    }
    Err(_) => {
        // Fallback to TCP or WebSocket
        let conn = endpoint.connect_transport("tcp-server").await?;
        println!("Connected via TCP (QUIC blocked)");
    }
}
```

### 5. Optimize Packet Size

Adjust packet size for your network:

```rust
// Internet: Standard size
let config = QuicConfig {
    max_udp_payload_size: 1350,
    ..Default::default()
};

// Local network: Larger packets
let config = QuicConfig {
    max_udp_payload_size: 1450,
    ..Default::default()
};

// Unreliable network: Smaller packets
let config = QuicConfig {
    max_udp_payload_size: 1200,
    ..Default::default()
};
```

## Troubleshooting

### Connection Fails

**Problem:** Can't connect to QUIC server

**Solutions:**
1. Check if UDP port is open
2. Verify firewall allows UDP traffic
3. Try different port (some networks block 443/UDP)
4. Implement TCP/WebSocket fallback

```rust
// Test UDP connectivity
use std::net::UdpSocket;

let socket = UdpSocket::bind("0.0.0.0:0")?;
socket.send_to(b"test", "server:4433")?;
```

### High Packet Loss

**Problem:** Poor performance on lossy networks

**Solutions:**
1. Reduce packet size
2. Decrease initial window
3. Increase keepalive frequency

```rust
let config = QuicConfig {
    max_udp_payload_size: 1200,
    initial_window: 64 * 1024,
    keep_alive_interval: Duration::from_secs(10),
    ..Default::default()
};
```

### Connection Migration Not Working

**Problem:** Connection drops on network change

**Solutions:**
1. Verify `enable_migration: true`
2. Check server supports migration
3. Ensure NAT allows new paths

```rust
let config = QuicConfig {
    enable_migration: true,
    max_idle_timeout: Duration::from_secs(120),  // Longer timeout
    ..Default::default()
};
```

### Certificate Errors

**Problem:** TLS handshake fails

**Solutions:**
1. For testing: Warnings are normal (self-signed certs)
2. For production: Use proper CA certificates
3. Check certificate expiration
4. Verify hostname matches certificate

## Performance Comparison

### QUIC vs TCP

| Metric | TCP | QUIC | Winner |
|--------|-----|------|--------|
| Initial Connection | ~100ms | ~50ms (0-RTT) | QUIC |
| Reconnection | ~100ms | ~0ms (0-RTT) | QUIC |
| Head-of-line Blocking | Yes | No | QUIC |
| Connection Migration | No | Yes | QUIC |
| Packet Loss Recovery | Slow | Fast | QUIC |
| Firewall Traversal | Easy | Harder | TCP |

### When to Use QUIC

✅ **Use QUIC for:**
- Mobile applications
- Real-time gaming
- Video streaming
- IoT devices
- Unreliable networks
- Low-latency requirements

❌ **Avoid QUIC for:**
- Strict firewall environments
- UDP-blocked networks
- Simple request/response patterns
- Legacy system integration

## See Also

- [Transport Configuration Guide](transport-configuration.md)
- [WebSocket Transport Guide](websocket-transport.md)
- [Architecture Guide](architecture-guide.md)
- [QUIC Examples](../bdrpc/examples/quic_server.rs)
- [Quinn Documentation](https://docs.rs/quinn)

---

**Made with Bob**
# WebSocket Transport Guide

**Version:** 0.2.0  
**Last Updated:** 2026-02-07

## Overview

The WebSocket transport enables BDRPC to communicate with web browsers and other WebSocket clients. It provides a standards-compliant WebSocket implementation that works seamlessly with JavaScript's WebSocket API while maintaining BDRPC's performance and reliability.

## Features

- **Browser Compatible**: Works with standard JavaScript WebSocket API
- **Binary Messages**: Efficient binary protocol for BDRPC communication
- **Automatic Keepalive**: Built-in ping/pong mechanism
- **Secure WebSocket (WSS)**: TLS encryption support
- **Compression**: Optional per-message deflate compression
- **Firewall Friendly**: Uses standard HTTP/HTTPS ports

## Quick Start

### Server

```rust
use bdrpc::endpoint::EndpointBuilder;
use bdrpc::serialization::JsonSerializer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = EndpointBuilder::server(JsonSerializer::default())
        .with_websocket_listener("0.0.0.0:8080")
        .with_responder("ChatService", 1)
        .build()
        .await?;
    
    println!("WebSocket server listening on ws://0.0.0.0:8080");
    
    // Keep server running
    tokio::signal::ctrl_c().await?;
    Ok(())
}
```

### Client (Rust)

```rust
use bdrpc::endpoint::EndpointBuilder;
use bdrpc::serialization::JsonSerializer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut endpoint = EndpointBuilder::client(JsonSerializer::default())
        .with_websocket_caller("server", "ws://127.0.0.1:8080")
        .with_caller("ChatService", 1)
        .build()
        .await?;
    
    let connection = endpoint.connect_transport("server").await?;
    println!("Connected to WebSocket server");
    
    Ok(())
}
```

### Client (JavaScript/Browser)

```javascript
// Connect to BDRPC WebSocket server
const ws = new WebSocket('ws://localhost:8080');
ws.binaryType = 'arraybuffer';

ws.onopen = () => {
    console.log('Connected to BDRPC server');
    
    // Send a message (you'll need to implement BDRPC framing)
    const message = encodeMessage({ type: 'hello', data: 'world' });
    ws.send(message);
};

ws.onmessage = (event) => {
    const data = new Uint8Array(event.data);
    const message = decodeMessage(data);
    console.log('Received:', message);
};

ws.onerror = (error) => {
    console.error('WebSocket error:', error);
};

ws.onclose = () => {
    console.log('Disconnected from server');
};
```

## Configuration

### WebSocketConfig

```rust
use bdrpc::transport::WebSocketConfig;
use std::time::Duration;

let config = WebSocketConfig {
    // Maximum size of a single WebSocket frame (default: 16 MB)
    max_frame_size: 16 * 1024 * 1024,
    
    // Maximum size of a complete message (default: 64 MB)
    max_message_size: 64 * 1024 * 1024,
    
    // Enable per-message deflate compression (default: false)
    compression: false,
    
    // Interval for sending ping frames (default: 30s)
    ping_interval: Duration::from_secs(30),
    
    // Timeout for pong response (default: 10s)
    pong_timeout: Duration::from_secs(10),
    
    // Accept unmasked frames (server-side only, default: false)
    accept_unmasked_frames: false,
};
```

### Using Custom Configuration

```rust
use bdrpc::transport::{TransportConfig, TransportType, WebSocketConfig};

let ws_config = WebSocketConfig {
    max_frame_size: 8 * 1024 * 1024,  // 8 MB
    compression: true,                 // Enable compression
    ..Default::default()
};

// Note: Custom WebSocket config requires using the transport manager directly
// The builder methods use default configuration
```

## Secure WebSocket (WSS)

### Server with TLS

```rust
// WebSocket over TLS requires configuring the underlying TCP listener with TLS
// This is typically done at the reverse proxy level (nginx, Apache, etc.)

// For development, you can use a reverse proxy:
// nginx configuration:
// server {
//     listen 443 ssl;
//     ssl_certificate /path/to/cert.pem;
//     ssl_certificate_key /path/to/key.pem;
//     
//     location / {
//         proxy_pass http://localhost:8080;
//         proxy_http_version 1.1;
//         proxy_set_header Upgrade $http_upgrade;
//         proxy_set_header Connection "upgrade";
//     }
// }
```

### Client with TLS

```rust
let mut endpoint = EndpointBuilder::client(serializer)
    .with_websocket_caller("server", "wss://example.com:443")
    .with_caller("ChatService", 1)
    .build()
    .await?;
```

## Browser Integration

### Complete Browser Example

```html
<!DOCTYPE html>
<html>
<head>
    <title>BDRPC WebSocket Client</title>
</head>
<body>
    <h1>BDRPC Chat Client</h1>
    <div id="messages"></div>
    <input type="text" id="messageInput" placeholder="Type a message...">
    <button onclick="sendMessage()">Send</button>
    
    <script>
        class BdrpcWebSocketClient {
            constructor(url) {
                this.ws = new WebSocket(url);
                this.ws.binaryType = 'arraybuffer';
                this.messageHandlers = new Map();
                
                this.ws.onopen = () => this.onOpen();
                this.ws.onmessage = (event) => this.onMessage(event);
                this.ws.onerror = (error) => this.onError(error);
                this.ws.onclose = () => this.onClose();
            }
            
            onOpen() {
                console.log('Connected to BDRPC server');
                document.getElementById('messages').innerHTML += 
                    '<div>Connected to server</div>';
            }
            
            onMessage(event) {
                const data = new Uint8Array(event.data);
                // Decode BDRPC message (implement based on your protocol)
                const message = this.decodeMessage(data);
                
                const handler = this.messageHandlers.get(message.channelId);
                if (handler) {
                    handler(message.payload);
                }
            }
            
            onError(error) {
                console.error('WebSocket error:', error);
            }
            
            onClose() {
                console.log('Disconnected from server');
                document.getElementById('messages').innerHTML += 
                    '<div>Disconnected from server</div>';
            }
            
            send(channelId, payload) {
                const message = this.encodeMessage(channelId, payload);
                this.ws.send(message);
            }
            
            onChannel(channelId, handler) {
                this.messageHandlers.set(channelId, handler);
            }
            
            encodeMessage(channelId, payload) {
                // Implement BDRPC framing protocol
                // Format: [length: u32][channel_id: u64][payload: bytes]
                const payloadStr = JSON.stringify(payload);
                const encoder = new TextEncoder();
                const payloadBytes = encoder.encode(payloadStr);
                
                const buffer = new ArrayBuffer(4 + 8 + payloadBytes.length);
                const view = new DataView(buffer);
                
                view.setUint32(0, 8 + payloadBytes.length, false); // Big-endian
                view.setBigUint64(4, BigInt(channelId), false);
                
                new Uint8Array(buffer, 12).set(payloadBytes);
                
                return buffer;
            }
            
            decodeMessage(data) {
                const view = new DataView(data.buffer);
                const length = view.getUint32(0, false);
                const channelId = Number(view.getBigUint64(4, false));
                
                const decoder = new TextDecoder();
                const payloadBytes = new Uint8Array(data.buffer, 12);
                const payload = JSON.parse(decoder.decode(payloadBytes));
                
                return { channelId, payload };
            }
        }
        
        // Usage
        const client = new BdrpcWebSocketClient('ws://localhost:8080');
        
        client.onChannel(1, (message) => {
            document.getElementById('messages').innerHTML += 
                `<div>Received: ${JSON.stringify(message)}</div>`;
        });
        
        function sendMessage() {
            const input = document.getElementById('messageInput');
            const message = { type: 'chat', text: input.value };
            client.send(1, message);
            input.value = '';
        }
    </script>
</body>
</html>
```

## Performance Tuning

### Disable Compression for Low-Latency

```rust
let config = WebSocketConfig {
    compression: false,  // Disable for better latency
    ..Default::default()
};
```

### Increase Buffer Sizes for High Throughput

```rust
let config = WebSocketConfig {
    max_frame_size: 32 * 1024 * 1024,    // 32 MB
    max_message_size: 128 * 1024 * 1024, // 128 MB
    ..Default::default()
};
```

### Adjust Keepalive for Mobile Networks

```rust
let config = WebSocketConfig {
    ping_interval: Duration::from_secs(15),  // More frequent
    pong_timeout: Duration::from_secs(5),    // Shorter timeout
    ..Default::default()
};
```

## Best Practices

### 1. Use Binary Messages

Always set `binaryType = 'arraybuffer'` in JavaScript for efficient communication:

```javascript
ws.binaryType = 'arraybuffer';
```

### 2. Handle Reconnection

Implement reconnection logic in browser clients:

```javascript
function connect() {
    const ws = new WebSocket('ws://localhost:8080');
    
    ws.onclose = () => {
        console.log('Disconnected, reconnecting in 5s...');
        setTimeout(connect, 5000);
    };
}
```

### 3. Validate Messages

Always validate incoming messages in both client and server:

```rust
// Server-side validation
if message.len() > MAX_MESSAGE_SIZE {
    return Err(TransportError::MessageTooLarge);
}
```

### 4. Use WSS in Production

Always use secure WebSocket (wss://) in production:

```javascript
const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
const ws = new WebSocket(`${protocol}//${window.location.host}/api`);
```

### 5. Monitor Connection Health

Implement health checks:

```javascript
setInterval(() => {
    if (ws.readyState === WebSocket.OPEN) {
        ws.send(new Uint8Array([0])); // Ping
    }
}, 30000);
```

## Troubleshooting

### Connection Refused

**Problem:** Browser can't connect to WebSocket server

**Solutions:**
1. Check CORS settings
2. Verify server is running
3. Check firewall rules
4. Use correct protocol (ws:// vs wss://)

### Messages Not Received

**Problem:** Messages sent but not received

**Solutions:**
1. Verify `binaryType = 'arraybuffer'`
2. Check message encoding/decoding
3. Verify channel IDs match
4. Check for errors in console

### High Latency

**Problem:** Slow message delivery

**Solutions:**
1. Disable compression
2. Reduce ping interval
3. Check network conditions
4. Use local server for testing

### Connection Drops

**Problem:** Connection frequently disconnects

**Solutions:**
1. Implement reconnection logic
2. Adjust keepalive settings
3. Check network stability
4. Use WSS for better reliability

## See Also

- [Transport Configuration Guide](transport-configuration.md)
- [QUIC Transport Guide](quic-transport.md)
- [Architecture Guide](architecture-guide.md)
- [WebSocket Examples](../bdrpc/examples/websocket_server.rs)

---

**Made with Bob**
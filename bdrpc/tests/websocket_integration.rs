//
// Copyright 2026 Hans W. Uhlig. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//

//! Integration tests for WebSocket transport.
//!
//! These tests verify WebSocket transport functionality including:
//! - Basic client-server communication
//! - Binary message support
//! - Connection lifecycle management
//! - Error handling and recovery
//! - Configuration options
//! - Concurrent connections

#![cfg(feature = "websocket")]

use bdrpc::transport::{Transport, WebSocketConfig, WebSocketListener, WebSocketTransport};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;

/// Test basic WebSocket connection and types transfer.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_websocket_basic_connection() {
    // Start WebSocket listener
    let config = WebSocketConfig::default();
    let listener = WebSocketListener::bind("127.0.0.1:0", config.clone())
        .await
        .expect("Failed to bind WebSocket listener");

    let server_addr = listener
        .local_addr()
        .expect("Failed to get listener address");

    // Spawn server task
    let server_handle = tokio::spawn(async move {
        let mut transport = listener
            .accept()
            .await
            .expect("Failed to accept connection");

        // Echo received types
        let mut buf = vec![0u8; 1024];
        let n = transport.read(&mut buf).await.expect("Failed to read");
        transport
            .write_all(&buf[..n])
            .await
            .expect("Failed to write");
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect client
    let url = format!("ws://{}", server_addr);
    let mut client = WebSocketTransport::connect(&url, config)
        .await
        .expect("Failed to connect");

    // Send test types
    let test_data = b"Hello, WebSocket!";
    client.write_all(test_data).await.expect("Failed to write");

    // Receive echo
    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.expect("Failed to read");

    assert_eq!(&buf[..n], test_data);

    // Cleanup
    drop(client);
    server_handle.await.expect("Server task failed");
}

/// Test WebSocket with large messages.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_websocket_large_messages() {
    let config = WebSocketConfig {
        max_message_size: 10 * 1024 * 1024, // 10 MB
        ..Default::default()
    };

    let listener = WebSocketListener::bind("127.0.0.1:0", config.clone())
        .await
        .expect("Failed to bind");

    let server_addr = listener.local_addr().expect("Failed to get address");

    // Server echoes types
    let server_handle = tokio::spawn(async move {
        let mut transport = listener.accept().await.expect("Failed to accept");
        let mut buf = vec![0u8; 10 * 1024 * 1024];
        let n = transport.read(&mut buf).await.expect("Failed to read");
        transport
            .write_all(&buf[..n])
            .await
            .expect("Failed to write");
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let url = format!("ws://{}", server_addr);
    let mut client = WebSocketTransport::connect(&url, config)
        .await
        .expect("Failed to connect");

    // Send 1 MB of types
    let test_data = vec![0x42u8; 1024 * 1024];
    client.write_all(&test_data).await.expect("Failed to write");

    // Receive echo
    let mut buf = vec![0u8; 2 * 1024 * 1024];
    let n = client.read(&mut buf).await.expect("Failed to read");

    assert_eq!(n, test_data.len());
    assert_eq!(&buf[..n], &test_data[..]);

    drop(client);
    server_handle.await.expect("Server failed");
}

/// Test multiple concurrent WebSocket connections.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_websocket_concurrent_connections() {
    let config = WebSocketConfig::default();
    let listener = WebSocketListener::bind("127.0.0.1:0", config.clone())
        .await
        .expect("Failed to bind");

    let server_addr = listener.local_addr().expect("Failed to get address");

    // Server handles multiple connections
    let server_handle = tokio::spawn(async move {
        let mut handles = Vec::new();

        for _ in 0..5 {
            let mut transport = listener.accept().await.expect("Failed to accept");

            let handle = tokio::spawn(async move {
                let mut buf = vec![0u8; 1024];
                let n = transport.read(&mut buf).await.expect("Failed to read");
                transport
                    .write_all(&buf[..n])
                    .await
                    .expect("Failed to write");
            });

            handles.push(handle);
        }

        for handle in handles {
            handle.await.expect("Connection handler failed");
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create 5 concurrent clients
    let url = format!("ws://{}", server_addr);
    let mut client_handles = Vec::new();

    for i in 0..5 {
        let url = url.clone();
        let config = config.clone();

        let handle = tokio::spawn(async move {
            let mut client = WebSocketTransport::connect(&url, config)
                .await
                .expect("Failed to connect");

            let test_data = format!("Client {}", i);
            client
                .write_all(test_data.as_bytes())
                .await
                .expect("Failed to write");

            let mut buf = vec![0u8; 1024];
            let n = client.read(&mut buf).await.expect("Failed to read");

            assert_eq!(&buf[..n], test_data.as_bytes());
        });

        client_handles.push(handle);
    }

    // Wait for all clients
    for handle in client_handles {
        handle.await.expect("Client failed");
    }

    server_handle.await.expect("Server failed");
}

/// Test WebSocket connection timeout.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_websocket_connection_timeout() {
    let config = WebSocketConfig::default();

    // Try to connect to non-existent server
    let result = timeout(
        Duration::from_secs(2),
        WebSocketTransport::connect("ws://127.0.0.1:1", config),
    )
    .await;

    // Should timeout or fail to connect
    assert!(result.is_err() || result.unwrap().is_err());
}

/// Test WebSocket with custom configuration.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_websocket_custom_config() {
    let config = WebSocketConfig {
        max_frame_size: 8 * 1024,    // 8 KB frames
        max_message_size: 32 * 1024, // 32 KB messages
        compression: false,
        ping_interval: Duration::from_secs(10),
        pong_timeout: Duration::from_secs(5),
        accept_unmasked_frames: false,
    };

    let listener = WebSocketListener::bind("127.0.0.1:0", config.clone())
        .await
        .expect("Failed to bind");

    let server_addr = listener.local_addr().expect("Failed to get address");

    let server_handle = tokio::spawn(async move {
        let mut transport = listener.accept().await.expect("Failed to accept");
        let mut buf = vec![0u8; 1024];
        let n = transport.read(&mut buf).await.expect("Failed to read");
        transport
            .write_all(&buf[..n])
            .await
            .expect("Failed to write");
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let url = format!("ws://{}", server_addr);
    let mut client = WebSocketTransport::connect(&url, config)
        .await
        .expect("Failed to connect");

    let test_data = b"Custom config test";
    client.write_all(test_data).await.expect("Failed to write");

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.expect("Failed to read");

    assert_eq!(&buf[..n], test_data);

    drop(client);
    server_handle.await.expect("Server failed");
}

/// Test WebSocket metadata.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_websocket_metadata() {
    let config = WebSocketConfig::default();
    let listener = WebSocketListener::bind("127.0.0.1:0", config.clone())
        .await
        .expect("Failed to bind");

    let server_addr = listener.local_addr().expect("Failed to get address");

    let server_handle = tokio::spawn(async move {
        let transport = listener.accept().await.expect("Failed to accept");

        // Check server-side metadata
        let metadata = transport.metadata();
        assert!(metadata.local_addr.is_some());
        assert!(metadata.peer_addr.is_some());
        assert!(metadata.id.as_u64() > 0);
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let url = format!("ws://{}", server_addr);
    let client = WebSocketTransport::connect(&url, config)
        .await
        .expect("Failed to connect");

    // Check client-side metadata
    let metadata = client.metadata();
    assert!(metadata.local_addr.is_some());
    assert!(metadata.peer_addr.is_some());
    assert!(metadata.id.as_u64() > 0);

    drop(client);
    server_handle.await.expect("Server failed");
}

/// Test WebSocket graceful shutdown.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_websocket_graceful_shutdown() {
    let config = WebSocketConfig::default();
    let listener = WebSocketListener::bind("127.0.0.1:0", config.clone())
        .await
        .expect("Failed to bind");

    let server_addr = listener.local_addr().expect("Failed to get address");

    let server_handle = tokio::spawn(async move {
        let mut transport = listener.accept().await.expect("Failed to accept");

        // Read until client closes
        let mut buf = vec![0u8; 1024];
        loop {
            match transport.read(&mut buf).await {
                Ok(0) => break, // Connection closed
                Ok(_) => continue,
                Err(_) => break,
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let url = format!("ws://{}", server_addr);
    let client = WebSocketTransport::connect(&url, config)
        .await
        .expect("Failed to connect");

    // Close client connection
    drop(client);

    // Server should detect closure
    timeout(Duration::from_secs(5), server_handle)
        .await
        .expect("Server didn't detect closure")
        .expect("Server failed");
}

/// Test WebSocket with binary types patterns.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_websocket_binary_patterns() {
    let config = WebSocketConfig::default();
    let listener = WebSocketListener::bind("127.0.0.1:0", config.clone())
        .await
        .expect("Failed to bind");

    let server_addr = listener.local_addr().expect("Failed to get address");

    let server_handle = tokio::spawn(async move {
        let mut transport = listener.accept().await.expect("Failed to accept");

        // Echo 4 messages (one for each pattern)
        for _ in 0..4 {
            let mut buf = vec![0u8; 1024];
            match transport.read(&mut buf).await {
                Ok(n) if n > 0 => {
                    transport
                        .write_all(&buf[..n])
                        .await
                        .expect("Failed to write");
                }
                Ok(_) => break, // Connection closed
                Err(_) => break,
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let url = format!("ws://{}", server_addr);
    let mut client = WebSocketTransport::connect(&url, config)
        .await
        .expect("Failed to connect");

    // Test various binary patterns
    #[allow(clippy::useless_vec)]
    let patterns: Vec<Vec<u8>> = vec![
        vec![0x00; 100],                // All zeros
        vec![0xFF; 100],                // All ones
        (0..=255).collect::<Vec<u8>>(), // Sequential bytes
        vec![0xAA, 0x55].repeat(50),    // Alternating pattern
    ];

    for pattern in patterns {
        client.write_all(&pattern).await.expect("Failed to write");

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.expect("Failed to read");

        assert_eq!(&buf[..n], &pattern[..]);
    }

    drop(client);
    server_handle.await.expect("Server failed");
}

/// Test WebSocket with rapid small messages.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_websocket_rapid_small_messages() {
    let config = WebSocketConfig::default();
    let listener = WebSocketListener::bind("127.0.0.1:0", config.clone())
        .await
        .expect("Failed to bind");

    let server_addr = listener.local_addr().expect("Failed to get address");

    let server_handle = tokio::spawn(async move {
        let mut transport = listener.accept().await.expect("Failed to accept");

        // Echo 100 messages
        for _ in 0..100 {
            let mut buf = vec![0u8; 1024];
            let n = transport.read(&mut buf).await.expect("Failed to read");
            transport
                .write_all(&buf[..n])
                .await
                .expect("Failed to write");
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let url = format!("ws://{}", server_addr);
    let mut client = WebSocketTransport::connect(&url, config)
        .await
        .expect("Failed to connect");

    // Send 100 small messages rapidly
    for i in 0..100 {
        let msg = format!("Message {}", i);
        client
            .write_all(msg.as_bytes())
            .await
            .expect("Failed to write");

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.expect("Failed to read");

        assert_eq!(&buf[..n], msg.as_bytes());
    }

    drop(client);
    server_handle.await.expect("Server failed");
}

// Made with Bob

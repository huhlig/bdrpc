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

//! Integration tests for QUIC transport.
//!
//! These tests verify QUIC transport functionality including:
//! - Basic client-server communication
//! - Binary data transfer
//! - Connection lifecycle management
//! - Error handling and recovery
//! - Configuration options
//! - Concurrent connections
//! - 0-RTT support
//! - Connection migration

#![cfg(feature = "quic")]

// Install default crypto provider for tests
#[ctor::ctor]
fn init() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

use bdrpc::transport::{QuicConfig, QuicListener, QuicTransport, Transport};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;

/// Test basic QUIC connection and data transfer.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_quic_basic_connection() {
    // Start QUIC listener
    let config = QuicConfig::default();
    let listener = QuicListener::bind("127.0.0.1:0", config.clone())
        .await
        .expect("Failed to bind QUIC listener");

    let server_addr = listener
        .local_addr()
        .expect("Failed to get listener address");

    // Spawn server task
    let server_handle = tokio::spawn(async move {
        let mut transport = listener
            .accept()
            .await
            .expect("Failed to accept connection");

        // Echo received data
        let mut buf = vec![0u8; 1024];
        let n = transport.read(&mut buf).await.expect("Failed to read");
        transport
            .write_all(&buf[..n])
            .await
            .expect("Failed to write");

        // Keep transport alive briefly to allow client to read
        tokio::time::sleep(Duration::from_millis(100)).await;
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect client
    let mut client = QuicTransport::connect(server_addr.to_string(), config)
        .await
        .expect("Failed to connect");

    // Send test data
    let test_data = b"Hello, QUIC!";
    client.write_all(test_data).await.expect("Failed to write");

    // Receive echo
    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.expect("Failed to read");

    assert_eq!(&buf[..n], test_data);

    // Cleanup
    drop(client);
    server_handle.await.expect("Server task failed");
}

/// Test QUIC with large messages.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_quic_large_messages() {
    let config = QuicConfig {
        initial_window: 1024 * 1024, // 1 MB initial window
        ..Default::default()
    };

    let listener = QuicListener::bind("127.0.0.1:0", config.clone())
        .await
        .expect("Failed to bind");

    let server_addr = listener.local_addr().expect("Failed to get address");

    // Server echoes data
    let server_handle = tokio::spawn(async move {
        let mut transport = listener.accept().await.expect("Failed to accept");
        let mut buf = vec![0u8; 10 * 1024 * 1024];

        // Read all data from client
        let mut total_read = 0;
        loop {
            match transport.read(&mut buf[total_read..]).await {
                Ok(0) => break,
                Ok(n) => {
                    total_read += n;
                    if total_read >= 1024 * 1024 {
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("Server read error: {}", e);
                    break;
                }
            }
        }

        // Echo all data back
        transport
            .write_all(&buf[..total_read])
            .await
            .expect("Failed to write");

        // Keep transport alive longer to allow client to read all data
        tokio::time::sleep(Duration::from_secs(2)).await;
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut client = QuicTransport::connect(server_addr.to_string(), config)
        .await
        .expect("Failed to connect");

    // Send 1 MB of data
    let test_data = vec![0x42u8; 1024 * 1024];
    client.write_all(&test_data).await.expect("Failed to write");

    // Receive echo - read in loop until we get all data
    let mut buf = vec![0u8; 2 * 1024 * 1024];
    let mut total_read = 0;
    while total_read < test_data.len() {
        let n = client
            .read(&mut buf[total_read..])
            .await
            .expect("Failed to read");
        if n == 0 {
            break;
        }
        total_read += n;
    }

    assert_eq!(total_read, test_data.len());
    assert_eq!(&buf[..total_read], &test_data[..]);

    drop(client);
    server_handle.await.expect("Server failed");
}

/// Test multiple concurrent QUIC connections.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_quic_concurrent_connections() {
    let config = QuicConfig::default();
    let listener = QuicListener::bind("127.0.0.1:0", config.clone())
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

                // Keep transport alive briefly to allow client to read
                tokio::time::sleep(Duration::from_millis(100)).await;
            });

            handles.push(handle);
        }

        for handle in handles {
            handle.await.expect("Connection handler failed");
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create 5 concurrent clients
    let mut client_handles = Vec::new();

    for i in 0..5 {
        let config = config.clone();

        let handle = tokio::spawn(async move {
            let mut client = QuicTransport::connect(server_addr.to_string(), config)
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

/// Test QUIC connection timeout.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_quic_connection_timeout() {
    let config = QuicConfig::default();

    // Try to connect to non-existent server
    let result = timeout(
        Duration::from_secs(2),
        QuicTransport::connect("127.0.0.1:1", config),
    )
    .await;

    // Should timeout or fail to connect
    assert!(result.is_err() || result.unwrap().is_err());
}

/// Test QUIC with custom configuration.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_quic_custom_config() {
    let config = QuicConfig {
        max_idle_timeout: Duration::from_secs(30),
        keep_alive_interval: Duration::from_secs(5),
        max_concurrent_bidi_streams: 50,
        max_concurrent_uni_streams: 50,
        enable_0rtt: false,        // Disable 0-RTT for this test
        initial_window: 64 * 1024, // 64 KB
        max_udp_payload_size: 1200,
        enable_migration: false,
    };

    let listener = QuicListener::bind("127.0.0.1:0", config.clone())
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

        // Keep transport alive briefly to allow client to read
        tokio::time::sleep(Duration::from_millis(100)).await;
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut client = QuicTransport::connect(server_addr.to_string(), config)
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

/// Test QUIC metadata.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_quic_metadata() {
    let config = QuicConfig::default();
    let listener = QuicListener::bind("127.0.0.1:0", config.clone())
        .await
        .expect("Failed to bind");

    let server_addr = listener.local_addr().expect("Failed to get address");

    let server_handle = tokio::spawn(async move {
        let mut transport = listener.accept().await.expect("Failed to accept");

        // Check server-side metadata
        let metadata = transport.metadata();
        assert!(metadata.local_addr.is_some());
        assert!(metadata.peer_addr.is_some());
        assert!(metadata.id.as_u64() > 0);

        // Read the data sent by client to keep stream alive
        let mut buf = vec![0u8; 1024];
        let _ = transport.read(&mut buf).await;

        // Keep transport alive longer
        tokio::time::sleep(Duration::from_millis(100)).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = QuicTransport::connect(server_addr.to_string(), config)
        .await
        .expect("Failed to connect");

    // Send a small message to establish the stream
    client
        .write_all(b"metadata_test")
        .await
        .expect("Failed to write");

    // Check client-side metadata
    let metadata = client.metadata();
    assert!(metadata.local_addr.is_some());
    assert!(metadata.peer_addr.is_some());
    assert!(metadata.id.as_u64() > 0);

    // Wait a bit before dropping to ensure server reads
    tokio::time::sleep(Duration::from_millis(50)).await;

    drop(client);
    server_handle.await.expect("Server failed");
}

/// Test QUIC graceful shutdown.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_quic_graceful_shutdown() {
    let config = QuicConfig::default();
    let listener = QuicListener::bind("127.0.0.1:0", config.clone())
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

    let mut client = QuicTransport::connect(server_addr.to_string(), config)
        .await
        .expect("Failed to connect");

    // Send a small message first to establish the stream
    client.write_all(b"test").await.expect("Failed to write");

    // Give server time to read
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Close client connection
    drop(client);

    // Server should detect closure
    timeout(Duration::from_secs(5), server_handle)
        .await
        .expect("Server didn't detect closure")
        .expect("Server failed");
}

/// Test QUIC with binary data patterns.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_quic_binary_patterns() {
    let config = QuicConfig::default();
    let listener = QuicListener::bind("127.0.0.1:0", config.clone())
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

        // Keep transport alive briefly
        tokio::time::sleep(Duration::from_millis(100)).await;
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut client = QuicTransport::connect(server_addr.to_string(), config)
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

/// Test QUIC with rapid small messages.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_quic_rapid_small_messages() {
    let config = QuicConfig::default();
    let listener = QuicListener::bind("127.0.0.1:0", config.clone())
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

        // Keep transport alive briefly
        tokio::time::sleep(Duration::from_millis(100)).await;
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut client = QuicTransport::connect(server_addr.to_string(), config)
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

/// Test QUIC with 0-RTT enabled.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_quic_0rtt_support() {
    let config = QuicConfig {
        enable_0rtt: true,
        ..Default::default()
    };

    let listener = QuicListener::bind("127.0.0.1:0", config.clone())
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

        // Keep transport alive briefly to allow client to read
        tokio::time::sleep(Duration::from_millis(100)).await;
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut client = QuicTransport::connect(server_addr.to_string(), config)
        .await
        .expect("Failed to connect");

    let test_data = b"0-RTT test data";
    client.write_all(test_data).await.expect("Failed to write");

    let mut buf = vec![0u8; 1024];
    let n = client.read(&mut buf).await.expect("Failed to read");

    assert_eq!(&buf[..n], test_data);

    drop(client);
    server_handle.await.expect("Server failed");
}

/// Test QUIC connection migration support.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_quic_connection_migration() {
    let config = QuicConfig {
        enable_migration: true,
        max_idle_timeout: Duration::from_secs(120),
        ..Default::default()
    };

    let listener = QuicListener::bind("127.0.0.1:0", config.clone())
        .await
        .expect("Failed to bind");

    let server_addr = listener.local_addr().expect("Failed to get address");

    let server_handle = tokio::spawn(async move {
        let mut transport = listener.accept().await.expect("Failed to accept");

        // Handle multiple messages to test migration
        for _ in 0..3 {
            let mut buf = vec![0u8; 1024];
            let n = transport.read(&mut buf).await.expect("Failed to read");
            transport
                .write_all(&buf[..n])
                .await
                .expect("Failed to write");
        }

        // Keep transport alive briefly
        tokio::time::sleep(Duration::from_millis(100)).await;
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut client = QuicTransport::connect(server_addr.to_string(), config)
        .await
        .expect("Failed to connect");

    // Send messages - connection should remain stable
    for i in 0..3 {
        let msg = format!("Migration test {}", i);
        client
            .write_all(msg.as_bytes())
            .await
            .expect("Failed to write");

        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.expect("Failed to read");

        assert_eq!(&buf[..n], msg.as_bytes());

        // Small delay between messages
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    drop(client);
    server_handle.await.expect("Server failed");
}

// Made with Bob

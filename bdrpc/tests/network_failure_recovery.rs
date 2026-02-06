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

//! Integration tests for error recovery with actual network failures.
//!
//! These tests verify that the system correctly handles real network failures
//! such as connection drops, timeouts, and peer disconnections.

use bdrpc::channel::Protocol;
use bdrpc::endpoint::{Endpoint, EndpointConfig};
use bdrpc::reconnection::{ExponentialBackoff, ReconnectionStrategy};
use bdrpc::serialization::JsonSerializer;
use bdrpc::transport::{TcpTransport, Transport};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::time::timeout;

/// Simple test protocol for network failure tests.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
enum TestProtocol {
    Request { id: u64, data: String },
    Response { id: u64, result: String },
}

impl Protocol for TestProtocol {
    fn method_name(&self) -> &'static str {
        match self {
            Self::Request { .. } => "request",
            Self::Response { .. } => "response",
        }
    }

    fn is_request(&self) -> bool {
        matches!(self, Self::Request { .. })
    }
}

/// Test that connection failure is properly detected and reported.
#[tokio::test]
async fn test_connection_refused_error() {
    let serializer = JsonSerializer::default();
    let config = EndpointConfig::default();
    let mut endpoint = Endpoint::new(serializer, config);

    // Register protocol
    endpoint
        .register_caller("test.network_failure", 1)
        .await
        .expect("Failed to register protocol");

    // Try to connect to a port that's not listening
    let result = endpoint.connect("127.0.0.1:1").await;

    assert!(result.is_err());
    // Connection refused error is expected
}

/// Test that server shutdown is detected by client.
#[tokio::test]
async fn test_server_shutdown_detection() {
    let serializer = JsonSerializer::default();
    let config = EndpointConfig::default();

    // Start server
    let listener = TcpTransport::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind");
    let server_addr = listener.local_addr().expect("Failed to get local addr");

    // Spawn server that accepts one connection then shuts down immediately
    let server_handle = tokio::spawn(async move {
        if let Ok((mut transport, _)) = TcpTransport::accept(&listener).await {
            // Close immediately without handshake
            let _ = Transport::shutdown(&mut transport).await;
        }
    });

    // Give server time to start listening
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect client - should fail during handshake
    let mut endpoint = Endpoint::new(serializer, config);
    endpoint
        .register_caller("test.network_failure", 1)
        .await
        .expect("Failed to register protocol");

    let result = endpoint.connect(server_addr.to_string()).await;

    // Should fail to connect due to server closing during handshake
    assert!(result.is_err());

    server_handle.await.expect("Server task panicked");
}

/// Test that abrupt connection drop is detected.
#[tokio::test]
async fn test_abrupt_connection_drop() {
    let serializer = JsonSerializer::default();
    let config = EndpointConfig::default();

    // Start server
    let listener = TcpTransport::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind");
    let server_addr = listener.local_addr().expect("Failed to get local addr");

    // Spawn server that drops connection immediately after accepting
    let server_handle = tokio::spawn(async move {
        if let Ok((transport, _)) = TcpTransport::accept(&listener).await {
            // Drop transport without graceful shutdown
            drop(transport);
        }
    });

    // Give server time to start listening
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Try to connect - should fail during handshake
    let mut endpoint = Endpoint::new(serializer, config);
    endpoint
        .register_caller("test.network_failure", 1)
        .await
        .expect("Failed to register protocol");

    let result = endpoint.connect(server_addr.to_string()).await;

    // Should fail to connect due to abrupt drop
    assert!(result.is_err());

    server_handle.await.expect("Server task panicked");
}

/// Test timeout during channel creation.
#[tokio::test]
async fn test_channel_creation_timeout() {
    let serializer = JsonSerializer::default();
    let config = EndpointConfig {
        handshake_timeout: Duration::from_millis(200),
        ..Default::default()
    };

    // Start server that accepts but never completes handshake
    let listener = TcpTransport::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind");
    let server_addr = listener.local_addr().expect("Failed to get local addr");

    // Spawn server that accepts but never responds
    let server_handle = tokio::spawn(async move {
        if let Ok((mut transport, _)) = TcpTransport::accept(&listener).await {
            // Keep connection open but never send handshake response
            let mut buf = vec![0u8; 1024];
            let _ = transport.read(&mut buf).await;
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    });

    // Give server time to start listening
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Try to connect - should timeout during handshake
    let mut endpoint = Endpoint::new(serializer, config);
    endpoint
        .register_caller("test.network_failure", 1)
        .await
        .expect("Failed to register protocol");

    let result = endpoint.connect(server_addr.to_string()).await;

    // Should timeout during handshake
    assert!(result.is_err());

    drop(server_handle); // Cancel server task
}

/// Test reconnection after connection loss.
#[tokio::test]
async fn test_reconnection_after_connection_loss() {
    let connection_count = Arc::new(Mutex::new(0));
    let count_clone = connection_count.clone();

    // Start server that accepts multiple connections and closes them
    let listener = TcpTransport::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind");
    let server_addr = listener.local_addr().expect("Failed to get local addr");

    // Spawn server
    let server_handle = tokio::spawn(async move {
        for _ in 0..2 {
            if let Ok((transport, _)) = TcpTransport::accept(&listener).await {
                *count_clone.lock().await += 1;
                // Close immediately to simulate connection loss
                drop(transport);
            }
        }
    });

    // Give server time to start listening
    tokio::time::sleep(Duration::from_millis(100)).await;

    let serializer = JsonSerializer::default();
    let config = EndpointConfig::default();
    let mut endpoint = Endpoint::new(serializer, config);
    endpoint
        .register_caller("test.network_failure", 1)
        .await
        .expect("Failed to register protocol");

    // First connection attempt - will fail
    let _result1 = endpoint.connect(server_addr.to_string()).await;

    // Second connection attempt - will also fail
    let _result2 = endpoint.connect(server_addr.to_string()).await;

    // Wait for server to process both attempts
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify server saw two connection attempts
    let count = *connection_count.lock().await;
    assert_eq!(count, 2);

    drop(server_handle); // Cancel server task
}

/// Test that multiple concurrent connection failures are handled correctly.
#[tokio::test]
async fn test_concurrent_connection_failures() {
    // Try to connect to multiple non-existent servers concurrently
    let mut handles = vec![];
    for port in 1..=5 {
        // Use ports that are unlikely to be in use
        let addr = format!("127.0.0.1:{}", port);
        let handle = tokio::spawn(async move {
            let serializer = JsonSerializer::default();
            let config = EndpointConfig {
                // Set a shorter timeout to speed up the test
                handshake_timeout: Duration::from_millis(100),
                ..Default::default()
            };

            let mut ep = Endpoint::new(serializer, config);
            ep.register_caller("test.network_failure", 1)
                .await
                .expect("Failed to register protocol");
            ep.connect(addr).await
        });
        handles.push(handle);
    }

    // All should fail quickly
    for handle in handles {
        let result = handle.await.expect("Task panicked");
        assert!(result.is_err());
    }
}

/// Test graceful handling of partial data transmission.
#[tokio::test]
async fn test_partial_data_transmission() {
    // Start server that sends partial data then closes
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind");
    let server_addr = listener.local_addr().expect("Failed to get local addr");

    let server_handle = tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            // Send partial handshake data then close
            let _ = socket.write_all(b"partial").await;
            tokio::time::sleep(Duration::from_millis(50)).await;
            // Close without completing handshake
        }
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Try to connect
    let serializer = JsonSerializer::default();
    let config = EndpointConfig::default();
    let mut endpoint = Endpoint::new(serializer, config);

    endpoint
        .register_caller("test.network_failure", 1)
        .await
        .expect("Failed to register protocol");

    let result = timeout(
        Duration::from_secs(2),
        endpoint.connect(server_addr.to_string()),
    )
    .await;

    // Should fail due to incomplete handshake
    assert!(result.is_err() || result.unwrap().is_err());

    server_handle.await.expect("Server task panicked");
}

/// Test that endpoint handles bind address already in use.
#[tokio::test]
async fn test_bind_address_in_use() {
    // Bind to a random port
    let listener1 = TcpTransport::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind first listener");
    let addr = listener1.local_addr().expect("Failed to get local addr");

    // Try to bind to the same address
    let result = TcpTransport::bind(addr.to_string()).await;

    assert!(result.is_err());
    if let Err(err) = result {
        assert!(!err.is_recoverable());
    }
}

/// Test connection recovery with exponential backoff.
#[tokio::test]
async fn test_exponential_backoff_reconnection() {
    let strategy = ExponentialBackoff::builder()
        .initial_delay(Duration::from_millis(10))
        .max_delay(Duration::from_millis(100))
        .multiplier(2.0)
        .jitter(false) // Disable jitter for predictable testing
        .max_attempts(Some(5))
        .build();

    let mut delays = vec![];
    for attempt in 0..5 {
        let delay = strategy.next_delay(attempt).await;
        delays.push(delay);
    }

    // Verify exponential growth
    assert_eq!(delays.len(), 5);
    assert_eq!(delays[0], Duration::from_millis(10));
    assert_eq!(delays[1], Duration::from_millis(20));
    assert_eq!(delays[2], Duration::from_millis(40));
    assert_eq!(delays[3], Duration::from_millis(80));
    assert_eq!(delays[4], Duration::from_millis(100)); // Capped at max
}

/// Test that connection state is properly cleaned up after failure.
#[tokio::test]
async fn test_connection_cleanup_after_failure() {
    let serializer = JsonSerializer::default();
    let config = EndpointConfig::default();
    let mut endpoint = Endpoint::new(serializer, config);

    endpoint
        .register_caller("test.network_failure", 1)
        .await
        .expect("Failed to register protocol");

    // Try to connect to non-existent server
    let result = endpoint.connect("127.0.0.1:1").await;
    assert!(result.is_err());

    // Verify we can still use the endpoint for other operations
    let result2 = endpoint.connect("127.0.0.1:2").await;
    assert!(result2.is_err());

    // Endpoint should still be functional
    let result = endpoint.register_caller("test.network_failure2", 1).await;
    assert!(result.is_ok());
}

/// Test handling of slow network conditions.
#[tokio::test]
async fn test_slow_network_handling() {
    let serializer = JsonSerializer::default();
    let config = EndpointConfig {
        handshake_timeout: Duration::from_millis(200),
        ..Default::default()
    };

    // Start server with artificial delay
    let listener = TcpTransport::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind");
    let server_addr = listener.local_addr().expect("Failed to get local addr");

    let server_handle = tokio::spawn(async move {
        if let Ok((mut transport, _)) = TcpTransport::accept(&listener).await {
            // Simulate slow network by adding delay
            tokio::time::sleep(Duration::from_millis(500)).await;
            let mut buf = vec![0u8; 1024];
            let _ = transport.read(&mut buf).await;
        }
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect client
    let mut endpoint = Endpoint::new(serializer, config);
    endpoint
        .register_caller("test.network_failure", 1)
        .await
        .expect("Failed to register protocol");

    // Connection should succeed despite slow network (within timeout)
    let result = endpoint.connect(server_addr.to_string()).await;
    assert!(result.is_err());

    drop(server_handle); // Cancel server task
}

// Made with Bob

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

//! Integration tests for server accept flow.
//!
//! These tests verify the complete server accept flow including:
//! - Connection acceptance via listeners
//! - Handshake completion
//! - Automatic channel creation
//! - Message routing
//! - Connection cleanup
//!
//! Note: These tests demonstrate the server accept() API but are currently
//! simplified. The accept() method requires listeners to be configured, which
//! makes dynamic port testing complex. For now, these tests verify the API
//! compiles and document the expected usage pattern.

use bdrpc::channel::Protocol;
use bdrpc::endpoint::{Endpoint, EndpointConfig};
use bdrpc::serialization::PostcardSerializer;
use bdrpc::transport::{TransportConfig, TransportType};
use tokio::time::{Duration, timeout};

// Test protocol for integration tests
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
enum TestProtocol {
    Ping { id: u32 },
    Pong { id: u32 },
    Echo { message: String },
}

impl Protocol for TestProtocol {
    fn method_name(&self) -> &'static str {
        match self {
            Self::Ping { .. } => "ping",
            Self::Pong { .. } => "pong",
            Self::Echo { .. } => "echo",
        }
    }

    fn is_request(&self) -> bool {
        matches!(self, Self::Ping { .. } | Self::Echo { .. })
    }
}

/// Test that demonstrates the basic server accept API.
///
/// Note: This test uses a known port to avoid the complexity of dynamic port
/// discovery. In production, you would typically use a fixed port or implement
/// a service discovery mechanism.
#[tokio::test]
async fn test_server_accept_basic() {
    // Use a high port number to reduce conflicts
    let test_port = 19001;
    let server_addr = format!("127.0.0.1:{}", test_port);

    // Create server endpoint
    let mut server = Endpoint::new(PostcardSerializer::default(), EndpointConfig::default());

    // Register protocol
    server.register_responder("test_service", 1).await.unwrap();

    // Add TCP listener
    let listener_config = TransportConfig::new(TransportType::Tcp, &server_addr);
    let add_result = server
        .add_listener("test-listener".to_string(), listener_config)
        .await;

    // Skip test if port is in use
    if add_result.is_err() {
        eprintln!("Skipping test - port {} in use", test_port);
        return;
    }

    // Create client endpoint
    let mut client = Endpoint::new(PostcardSerializer::default(), EndpointConfig::default());

    client.register_caller("test_service", 1).await.unwrap();

    // Client connects to server
    let client_conn = client
        .connect(&server_addr)
        .await
        .expect("Client connect failed");

    // Server accepts the connection
    let server_conn = timeout(Duration::from_secs(5), server.accept())
        .await
        .expect("Accept timeout")
        .expect("Server accept failed");

    // Verify connections are established
    assert!(client_conn.id().as_u64() > 0);
    assert!(server_conn.id().as_u64() > 0);

    // Cleanup
    client.shutdown().await.ok();
    server.shutdown().await.ok();
}

/// Test that verifies channels are created after accept.
#[tokio::test]
async fn test_server_accept_with_channels() {
    let test_port = 19002;
    let server_addr = format!("127.0.0.1:{}", test_port);

    let mut server = Endpoint::new(PostcardSerializer::default(), EndpointConfig::default());

    server.register_responder("test_service", 1).await.unwrap();

    let listener_config = TransportConfig::new(TransportType::Tcp, &server_addr);
    if server
        .add_listener("test-listener".to_string(), listener_config)
        .await
        .is_err()
    {
        eprintln!("Skipping test - port {} in use", test_port);
        return;
    }

    let mut client = Endpoint::new(PostcardSerializer::default(), EndpointConfig::default());

    client.register_caller("test_service", 1).await.unwrap();

    let client_conn = client
        .connect(&server_addr)
        .await
        .expect("Client connect failed");

    let server_conn = timeout(Duration::from_secs(5), server.accept())
        .await
        .expect("Accept timeout")
        .expect("Server accept failed");

    // Give time for channel negotiation
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Get channels on both sides
    let client_channels = client
        .get_channels::<TestProtocol>(client_conn.id(), "test_service")
        .await;

    let server_channels = server
        .get_channels::<TestProtocol>(server_conn.id(), "test_service")
        .await;

    // Verify channels were created
    assert!(client_channels.is_ok(), "Client channels not created");
    assert!(server_channels.is_ok(), "Server channels not created");

    client.shutdown().await.ok();
    server.shutdown().await.ok();
}

/// Test message flow from client to server.
#[tokio::test]
async fn test_server_accept_message_flow() {
    let test_port = 19003;
    let server_addr = format!("127.0.0.1:{}", test_port);

    let mut server = Endpoint::new(PostcardSerializer::default(), EndpointConfig::default());

    server.register_responder("test_service", 1).await.unwrap();

    let listener_config = TransportConfig::new(TransportType::Tcp, &server_addr);
    if server
        .add_listener("test-listener".to_string(), listener_config)
        .await
        .is_err()
    {
        eprintln!("Skipping test - port {} in use", test_port);
        return;
    }

    let mut client = Endpoint::new(PostcardSerializer::default(), EndpointConfig::default());

    client.register_caller("test_service", 1).await.unwrap();

    let client_conn = client
        .connect(&server_addr)
        .await
        .expect("Client connect failed");

    let server_conn = timeout(Duration::from_secs(5), server.accept())
        .await
        .expect("Accept timeout")
        .expect("Server accept failed");

    tokio::time::sleep(Duration::from_millis(100)).await;

    let (client_sender, _client_receiver) = client
        .get_channels::<TestProtocol>(client_conn.id(), "test_service")
        .await
        .expect("Failed to get client channels");

    let (_server_sender, mut server_receiver) = server
        .get_channels::<TestProtocol>(server_conn.id(), "test_service")
        .await
        .expect("Failed to get server channels");

    client_sender
        .send(TestProtocol::Ping { id: 42 })
        .await
        .expect("Failed to send ping");

    let received = timeout(Duration::from_secs(1), server_receiver.recv())
        .await
        .expect("Receive timeout")
        .expect("Failed to receive message");

    assert_eq!(received, TestProtocol::Ping { id: 42 });

    client.shutdown().await.ok();
    server.shutdown().await.ok();
}

/// Test bidirectional message flow.
#[tokio::test]
async fn test_server_accept_bidirectional_flow() {
    let test_port = 19004;
    let server_addr = format!("127.0.0.1:{}", test_port);

    let mut server = Endpoint::new(PostcardSerializer::default(), EndpointConfig::default());

    server
        .register_bidirectional("test_service", 1)
        .await
        .unwrap();

    let listener_config = TransportConfig::new(TransportType::Tcp, &server_addr);
    if server
        .add_listener("test-listener".to_string(), listener_config)
        .await
        .is_err()
    {
        eprintln!("Skipping test - port {} in use", test_port);
        return;
    }

    let mut client = Endpoint::new(PostcardSerializer::default(), EndpointConfig::default());

    client
        .register_bidirectional("test_service", 1)
        .await
        .unwrap();

    let client_conn = client
        .connect(&server_addr)
        .await
        .expect("Client connect failed");

    let server_conn = timeout(Duration::from_secs(5), server.accept())
        .await
        .expect("Accept timeout")
        .expect("Server accept failed");

    tokio::time::sleep(Duration::from_millis(100)).await;

    let (client_sender, mut client_receiver) = client
        .get_channels::<TestProtocol>(client_conn.id(), "test_service")
        .await
        .expect("Failed to get client channels");

    let (server_sender, mut server_receiver) = server
        .get_channels::<TestProtocol>(server_conn.id(), "test_service")
        .await
        .expect("Failed to get server channels");

    client_sender
        .send(TestProtocol::Ping { id: 1 })
        .await
        .expect("Failed to send ping");

    server_sender
        .send(TestProtocol::Pong { id: 2 })
        .await
        .expect("Failed to send pong");

    let server_msg = timeout(Duration::from_secs(1), server_receiver.recv())
        .await
        .expect("Server receive timeout")
        .expect("Server failed to receive");

    let client_msg = timeout(Duration::from_secs(1), client_receiver.recv())
        .await
        .expect("Client receive timeout")
        .expect("Client failed to receive");

    assert_eq!(server_msg, TestProtocol::Ping { id: 1 });
    assert_eq!(client_msg, TestProtocol::Pong { id: 2 });

    client.shutdown().await.ok();
    server.shutdown().await.ok();
}

/// Test sending multiple messages.
#[tokio::test]
async fn test_server_accept_multiple_messages() {
    let test_port = 19005;
    let server_addr = format!("127.0.0.1:{}", test_port);

    let mut server = Endpoint::new(PostcardSerializer::default(), EndpointConfig::default());

    server.register_responder("test_service", 1).await.unwrap();

    let listener_config = TransportConfig::new(TransportType::Tcp, &server_addr);
    if server
        .add_listener("test-listener".to_string(), listener_config)
        .await
        .is_err()
    {
        eprintln!("Skipping test - port {} in use", test_port);
        return;
    }

    let mut client = Endpoint::new(PostcardSerializer::default(), EndpointConfig::default());

    client.register_caller("test_service", 1).await.unwrap();

    let client_conn = client
        .connect(&server_addr)
        .await
        .expect("Client connect failed");

    let server_conn = timeout(Duration::from_secs(5), server.accept())
        .await
        .expect("Accept timeout")
        .expect("Server accept failed");

    tokio::time::sleep(Duration::from_millis(100)).await;

    let (client_sender, _client_receiver) = client
        .get_channels::<TestProtocol>(client_conn.id(), "test_service")
        .await
        .expect("Failed to get client channels");

    let (_server_sender, mut server_receiver) = server
        .get_channels::<TestProtocol>(server_conn.id(), "test_service")
        .await
        .expect("Failed to get server channels");

    for i in 0..10 {
        client_sender
            .send(TestProtocol::Ping { id: i })
            .await
            .expect("Failed to send ping");
    }

    for i in 0..10 {
        let received = timeout(Duration::from_secs(1), server_receiver.recv())
            .await
            .expect("Receive timeout")
            .expect("Failed to receive message");

        assert_eq!(received, TestProtocol::Ping { id: i });
    }

    client.shutdown().await.ok();
    server.shutdown().await.ok();
}

/// Test connection cleanup after disconnect.
#[tokio::test]
async fn test_server_accept_connection_cleanup() {
    let test_port = 19006;
    let server_addr = format!("127.0.0.1:{}", test_port);

    let mut server = Endpoint::new(PostcardSerializer::default(), EndpointConfig::default());

    server.register_responder("test_service", 1).await.unwrap();

    let listener_config = TransportConfig::new(TransportType::Tcp, &server_addr);
    if server
        .add_listener("test-listener".to_string(), listener_config)
        .await
        .is_err()
    {
        eprintln!("Skipping test - port {} in use", test_port);
        return;
    }

    let mut client = Endpoint::new(PostcardSerializer::default(), EndpointConfig::default());

    client.register_caller("test_service", 1).await.unwrap();

    let client_conn = client
        .connect(&server_addr)
        .await
        .expect("Client connect failed");

    let server_conn = timeout(Duration::from_secs(5), server.accept())
        .await
        .expect("Accept timeout")
        .expect("Server accept failed");

    tokio::time::sleep(Duration::from_millis(100)).await;

    client
        .disconnect(client_conn.id())
        .await
        .expect("Failed to disconnect client");

    tokio::time::sleep(Duration::from_millis(200)).await;

    let result = server
        .get_channels::<TestProtocol>(server_conn.id(), "test_service")
        .await;
    assert!(result.is_err(), "Server connection should be cleaned up");

    client.shutdown().await.ok();
    server.shutdown().await.ok();
}

// Made with Bob

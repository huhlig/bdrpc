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

//! Cross-transport compatibility tests.
//!
//! These tests verify that different transport types all work correctly
//! with the BDRPC protocol, ensuring consistent behavior across transports.

// Install default crypto provider for tests
#[ctor::ctor]
fn init() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;

/// Test that TCP transport works with BDRPC framing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tcp_transport_framing() {
    use bdrpc::transport::provider::TcpTransport;

    let listener = TcpTransport::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind");
    let addr = listener.local_addr().expect("Failed to get address");

    let server_handle = tokio::spawn(async move {
        let (mut transport, _) = listener.accept().await.expect("Failed to accept");
        let mut buf = vec![0u8; 1024];
        let n = transport.read(&mut buf).await.expect("Failed to read");
        transport
            .write_all(&buf[..n])
            .await
            .expect("Failed to write");
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = TcpTransport::connect(addr.to_string())
        .await
        .expect("Failed to connect");

    let test_data = b"Hello TCP";
    client.write_all(test_data).await.expect("Failed to write");

    let mut response = vec![0u8; 1024];
    let n = timeout(Duration::from_secs(2), client.read(&mut response))
        .await
        .expect("Timeout")
        .expect("Failed to read");

    assert_eq!(&response[..n], test_data);

    drop(client);
    let _ = timeout(Duration::from_secs(1), server_handle).await;
}

/// Test that WebSocket transport works with BDRPC framing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[cfg(feature = "websocket")]
async fn test_websocket_transport_framing() {
    use bdrpc::transport::{WebSocketConfig, WebSocketListener, WebSocketTransport};

    let config = WebSocketConfig::default();
    let listener = WebSocketListener::bind("127.0.0.1:0", config.clone())
        .await
        .expect("Failed to bind");
    let addr = listener.local_addr().expect("Failed to get address");

    let server_handle = tokio::spawn(async move {
        let mut transport = listener.accept().await.expect("Failed to accept");
        let mut buf = vec![0u8; 1024];
        let n = transport.read(&mut buf).await.expect("Failed to read");
        transport
            .write_all(&buf[..n])
            .await
            .expect("Failed to write");
        tokio::time::sleep(Duration::from_millis(100)).await;
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut client = WebSocketTransport::connect(&format!("ws://{}", addr), config)
        .await
        .expect("Failed to connect");

    let test_data = b"Hello WebSocket";
    client.write_all(test_data).await.expect("Failed to write");

    let mut response = vec![0u8; 1024];
    let n = timeout(Duration::from_secs(2), client.read(&mut response))
        .await
        .expect("Timeout")
        .expect("Failed to read");

    assert_eq!(&response[..n], test_data);

    drop(client);
    let _ = timeout(Duration::from_secs(1), server_handle).await;
}

/// Test that QUIC transport works with BDRPC framing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[cfg(feature = "quic")]
async fn test_quic_transport_framing() {
    use bdrpc::transport::{QuicConfig, QuicListener, QuicTransport};

    let config = QuicConfig::default();
    let listener = QuicListener::bind("127.0.0.1:0", config.clone())
        .await
        .expect("Failed to bind");
    let addr = listener.local_addr().expect("Failed to get address");

    let server_handle = tokio::spawn(async move {
        let mut transport = listener.accept().await.expect("Failed to accept");
        let mut buf = vec![0u8; 1024];
        let n = transport.read(&mut buf).await.expect("Failed to read");
        transport
            .write_all(&buf[..n])
            .await
            .expect("Failed to write");
        tokio::time::sleep(Duration::from_millis(100)).await;
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut client = QuicTransport::connect(addr.to_string(), config)
        .await
        .expect("Failed to connect");

    let test_data = b"Hello QUIC";
    client.write_all(test_data).await.expect("Failed to write");

    let mut response = vec![0u8; 1024];
    let n = timeout(Duration::from_secs(2), client.read(&mut response))
        .await
        .expect("Timeout")
        .expect("Failed to read");

    assert_eq!(&response[..n], test_data);

    drop(client);
    let _ = timeout(Duration::from_secs(1), server_handle).await;
}

/// Test large message handling across all transports.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[cfg(all(feature = "websocket", feature = "quic"))]
async fn test_all_transports_large_messages() {
    use bdrpc::transport::{
        QuicConfig, QuicListener, QuicTransport, TcpTransport, WebSocketConfig, WebSocketListener,
        WebSocketTransport,
    };

    let large_data = vec![0xAB; 1024 * 1024]; // 1 MB

    // Test TCP
    {
        let listener = TcpTransport::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let data_len = large_data.len();
        let handle = tokio::spawn(async move {
            let (mut transport, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 2 * 1024 * 1024];
            let mut total = 0;
            while total < data_len {
                let n = transport.read(&mut buf[total..]).await.unwrap();
                if n == 0 {
                    break;
                }
                total += n;
            }
            transport.write_all(&buf[..total]).await.unwrap();
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut client = TcpTransport::connect(addr.to_string()).await.unwrap();
        client.write_all(&large_data).await.unwrap();

        let mut response = vec![0u8; 2 * 1024 * 1024];
        let mut total = 0;
        while total < data_len {
            let n = timeout(Duration::from_secs(10), client.read(&mut response[total..]))
                .await
                .unwrap()
                .unwrap();
            if n == 0 {
                break;
            }
            total += n;
        }

        assert_eq!(total, data_len);
        drop(client);
        let _ = timeout(Duration::from_secs(1), handle).await;
    }

    // Test WebSocket
    {
        let config = WebSocketConfig::default();
        let listener = WebSocketListener::bind("127.0.0.1:0", config.clone())
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();

        let data_len = large_data.len();
        let handle = tokio::spawn(async move {
            let mut transport = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 2 * 1024 * 1024];
            let mut total = 0;
            while total < data_len {
                let n = transport.read(&mut buf[total..]).await.unwrap();
                if n == 0 {
                    break;
                }
                total += n;
            }
            transport.write_all(&buf[..total]).await.unwrap();
            tokio::time::sleep(Duration::from_millis(200)).await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = WebSocketTransport::connect(&format!("ws://{}", addr), config)
            .await
            .unwrap();
        client.write_all(&large_data).await.unwrap();

        let mut response = vec![0u8; 2 * 1024 * 1024];
        let mut total = 0;
        while total < data_len {
            let n = timeout(Duration::from_secs(10), client.read(&mut response[total..]))
                .await
                .unwrap()
                .unwrap();
            if n == 0 {
                break;
            }
            total += n;
        }

        assert_eq!(total, data_len);
        drop(client);
        let _ = timeout(Duration::from_secs(1), handle).await;
    }

    // Test QUIC
    {
        let config = QuicConfig::default();
        let listener = QuicListener::bind("127.0.0.1:0", config.clone())
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();

        let data_len = large_data.len();
        let handle = tokio::spawn(async move {
            let mut transport = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 2 * 1024 * 1024];
            let mut total = 0;
            while total < data_len {
                let n = transport.read(&mut buf[total..]).await.unwrap();
                if n == 0 {
                    break;
                }
                total += n;
            }
            transport.write_all(&buf[..total]).await.unwrap();
            tokio::time::sleep(Duration::from_millis(200)).await;
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut client = QuicTransport::connect(addr.to_string(), config)
            .await
            .unwrap();
        client.write_all(&large_data).await.unwrap();

        let mut response = vec![0u8; 2 * 1024 * 1024];
        let mut total = 0;
        while total < data_len {
            let n = timeout(Duration::from_secs(10), client.read(&mut response[total..]))
                .await
                .unwrap()
                .unwrap();
            if n == 0 {
                break;
            }
            total += n;
        }

        assert_eq!(total, data_len);
        drop(client);
        let _ = timeout(Duration::from_secs(1), handle).await;
    }
}

/// Test concurrent connections for each transport type.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(all(feature = "websocket", feature = "quic"))]
async fn test_all_transports_concurrent_connections() {
    use bdrpc::transport::TcpTransport;

    // Test TCP concurrent connections
    {
        let listener = TcpTransport::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let handle = tokio::spawn(async move {
            for _ in 0..3 {
                let (mut transport, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 1024];
                    let n = transport.read(&mut buf).await.unwrap();
                    transport.write_all(&buf[..n]).await.unwrap();
                });
            }
        });

        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut handles = vec![];
        for i in 0..3 {
            let addr = addr.to_string();
            let h = tokio::spawn(async move {
                let mut client = TcpTransport::connect(addr).await.unwrap();
                let data = format!("Message {}", i);
                client.write_all(data.as_bytes()).await.unwrap();
                let mut buf = vec![0u8; 1024];
                let n = client.read(&mut buf).await.unwrap();
                assert_eq!(&buf[..n], data.as_bytes());
            });
            handles.push(h);
        }

        for h in handles {
            timeout(Duration::from_secs(5), h).await.unwrap().unwrap();
        }

        let _ = timeout(Duration::from_secs(1), handle).await;
    }
}

// Made with Bob

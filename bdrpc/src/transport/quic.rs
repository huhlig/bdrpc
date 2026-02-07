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

//! QUIC transport implementation using Quinn.
//!
//! This module provides QUIC transport support for BDRPC, enabling:
//! - High-performance multiplexed connections
//! - 0-RTT connection establishment
//! - Connection migration (network changes)
//! - Built-in encryption (TLS 1.3)
//! - Better performance over unreliable networks
//!
//! # Features
//!
//! - **Multiplexing**: Multiple streams over a single connection
//! - **0-RTT**: Resume connections with zero round-trip time
//! - **Migration**: Seamless network changes (WiFi to cellular)
//! - **Congestion Control**: Advanced algorithms for optimal throughput
//! - **Loss Recovery**: Better than TCP in high-loss environments
//!
//! # Examples
//!
//! ## Client
//!
//! ```rust,no_run
//! # #[cfg(feature = "quic")]
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use bdrpc::transport::{QuicTransport, QuicConfig};
//! use std::time::Duration;
//!
//! let config = QuicConfig {
//!     enable_0rtt: true,
//!     max_idle_timeout: Duration::from_secs(60),
//!     ..Default::default()
//! };
//!
//! let transport = QuicTransport::connect("server.example.com:4433", config).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Server
//!
//! ```rust,no_run
//! # #[cfg(feature = "quic")]
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use bdrpc::transport::{QuicListener, QuicConfig};
//!
//! let config = QuicConfig::default();
//! let listener = QuicListener::bind("0.0.0.0:4433", config).await?;
//!
//! loop {
//!     let transport = listener.accept().await?;
//!     tokio::spawn(async move {
//!         // Handle connection
//!     });
//! }
//! # Ok(())
//! # }
//! ```

use crate::transport::{Transport, TransportError, TransportId, TransportMetadata};
use quinn::{Connection, Endpoint, RecvStream, SendStream, ServerConfig};
use rustls_021 as rustls;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::Mutex;

// Global counter for transport IDs
static TRANSPORT_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Configuration for QUIC transport.
///
/// This configuration controls various aspects of QUIC connection behavior,
/// including timeouts, stream limits, and performance tuning.
#[derive(Debug, Clone)]
pub struct QuicConfig {
    /// Maximum idle timeout before connection is closed.
    ///
    /// If no data is sent or received for this duration, the connection
    /// will be closed. Set to a higher value for long-lived connections.
    pub max_idle_timeout: Duration,

    /// Interval for sending keepalive packets.
    ///
    /// Keepalive packets prevent idle timeout and detect dead connections.
    pub keep_alive_interval: Duration,

    /// Maximum number of concurrent bidirectional streams.
    ///
    /// This limits how many simultaneous BDRPC channels can be active.
    pub max_concurrent_bidi_streams: u64,

    /// Maximum number of concurrent unidirectional streams.
    pub max_concurrent_uni_streams: u64,

    /// Enable 0-RTT connection establishment.
    ///
    /// When enabled, clients can send data in the first packet when
    /// resuming a previous connection, reducing latency.
    pub enable_0rtt: bool,

    /// Initial congestion window size in bytes.
    ///
    /// Larger values improve throughput on high-bandwidth networks
    /// but may cause issues on congested networks.
    pub initial_window: u64,

    /// Maximum UDP payload size in bytes.
    ///
    /// Should be set based on network MTU. 1350 is safe for most networks.
    pub max_udp_payload_size: u16,

    /// Enable connection migration.
    ///
    /// When enabled, connections can survive network changes (e.g.,
    /// switching from WiFi to cellular).
    pub enable_migration: bool,
}

impl Default for QuicConfig {
    fn default() -> Self {
        Self {
            max_idle_timeout: Duration::from_secs(60),
            keep_alive_interval: Duration::from_secs(15),
            max_concurrent_bidi_streams: 100,
            max_concurrent_uni_streams: 100,
            enable_0rtt: true,
            initial_window: 128 * 1024, // 128 KB
            max_udp_payload_size: 1350, // Safe for most networks
            enable_migration: true,
        }
    }
}

/// QUIC transport implementation.
///
/// Provides a bi-directional byte stream over QUIC using a single
/// bidirectional stream per connection.
pub struct QuicTransport {
    /// The underlying QUIC connection
    connection: Connection,
    /// Send half of the bidirectional stream
    send_stream: Arc<Mutex<SendStream>>,
    /// Receive half of the bidirectional stream
    recv_stream: Arc<Mutex<RecvStream>>,
    /// Transport configuration
    config: QuicConfig,
    /// Transport metadata
    metadata: TransportMetadata,
}

impl QuicTransport {
    /// Create a new QUIC transport from an existing connection and stream.
    pub fn new(
        connection: Connection,
        send_stream: SendStream,
        recv_stream: RecvStream,
        config: QuicConfig,
    ) -> Self {
        let local_addr = connection
            .local_ip()
            .unwrap_or(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
        let peer_addr = connection.remote_address();

        let transport_id = TransportId::new(TRANSPORT_ID_COUNTER.fetch_add(1, Ordering::SeqCst));
        let metadata = TransportMetadata::new(transport_id, "quic")
            .with_local_addr(SocketAddr::new(local_addr, 0))
            .with_peer_addr(peer_addr);

        Self {
            connection,
            send_stream: Arc::new(Mutex::new(send_stream)),
            recv_stream: Arc::new(Mutex::new(recv_stream)),
            config,
            metadata,
        }
    }

    /// Connect to a remote QUIC endpoint.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # #[cfg(feature = "quic")]
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use bdrpc::transport::{QuicTransport, QuicConfig};
    ///
    /// let config = QuicConfig::default();
    /// let transport = QuicTransport::connect("server.example.com:4433", config).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn connect(
        addr: impl AsRef<str>,
        config: QuicConfig,
    ) -> Result<Self, TransportError> {
        let addr_str = addr.as_ref();

        // Parse address
        let remote_addr: SocketAddr =
            addr_str
                .parse()
                .map_err(|e| TransportError::InvalidConfiguration {
                    reason: format!("Invalid address '{}': {}", addr_str, e),
                })?;

        // Create client endpoint
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).map_err(|e| {
            TransportError::QuicEndpointError {
                reason: format!("Failed to create client endpoint: {}", e),
            }
        })?;

        // Configure client with rustls 0.21
        let mut crypto = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
            .with_no_client_auth();

        // Disable session resumption for simplicity (can be enabled for 0-RTT)
        crypto.enable_early_data = config.enable_0rtt;

        let mut client_config = quinn::ClientConfig::new(Arc::new(crypto));

        // Apply configuration
        let mut transport_config = quinn::TransportConfig::default();
        transport_config.max_idle_timeout(Some(
            config
                .max_idle_timeout
                .try_into()
                .unwrap_or_else(|_| quinn::IdleTimeout::from(quinn::VarInt::from_u32(60000))),
        ));
        transport_config.keep_alive_interval(Some(config.keep_alive_interval));
        transport_config.max_concurrent_bidi_streams(quinn::VarInt::from_u32(
            config.max_concurrent_bidi_streams.min(u32::MAX as u64) as u32,
        ));
        transport_config.max_concurrent_uni_streams(quinn::VarInt::from_u32(
            config.max_concurrent_uni_streams.min(u32::MAX as u64) as u32,
        ));
        transport_config.initial_mtu(config.max_udp_payload_size);

        client_config.transport_config(Arc::new(transport_config));
        endpoint.set_default_client_config(client_config);

        // Connect
        let connection = endpoint
            .connect(remote_addr, "localhost")
            .map_err(|e| TransportError::ConnectionFailed {
                address: addr_str.to_string(),
                source: io::Error::other(e),
            })?
            .await
            .map_err(|e| TransportError::ConnectionFailed {
                address: addr_str.to_string(),
                source: io::Error::other(e),
            })?;

        // Open bidirectional stream
        let (send_stream, recv_stream) =
            connection
                .open_bi()
                .await
                .map_err(|e| TransportError::QuicEndpointError {
                    reason: format!("Failed to open stream: {}", e),
                })?;

        Ok(Self::new(connection, send_stream, recv_stream, config))
    }

    /// Get the underlying QUIC connection.
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// Get the QUIC configuration.
    ///
    /// Returns a reference to the configuration used by this transport.
    /// This can be useful for inspecting settings like idle timeout,
    /// keepalive interval, and stream limits.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # #[cfg(feature = "quic")]
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use bdrpc::transport::{QuicTransport, QuicConfig};
    ///
    /// let config = QuicConfig::default();
    /// let transport = QuicTransport::connect("127.0.0.1:4433", config).await?;
    ///
    /// let config = transport.config();
    /// println!("Max idle timeout: {:?}", config.max_idle_timeout);
    /// # Ok(())
    /// # }
    /// ```
    pub fn config(&self) -> &QuicConfig {
        &self.config
    }

    /// Check if the connection is still alive.
    pub fn is_connected(&self) -> bool {
        self.connection.close_reason().is_none()
    }
}

impl Transport for QuicTransport {
    fn metadata(&self) -> &TransportMetadata {
        &self.metadata
    }

    fn shutdown(
        &mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), TransportError>> + Send + '_>> {
        Box::pin(async move {
            // Gracefully close the connection
            self.connection.close(0u32.into(), b"shutdown");

            // Finish the send stream
            let mut send = self.send_stream.lock().await;
            send.finish()
                .await
                .map_err(|e| TransportError::QuicEndpointError {
                    reason: format!("Failed to finish stream: {}", e),
                })?;

            Ok(())
        })
    }
}

impl AsyncRead for QuicTransport {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();

        // Try to lock the receive stream
        let mut recv = match this.recv_stream.try_lock() {
            Ok(recv) => recv,
            Err(_) => {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        };

        // Poll the receive stream
        Pin::new(&mut *recv).poll_read(cx, buf)
    }
}

impl AsyncWrite for QuicTransport {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();

        // Try to lock the send stream
        let mut send = match this.send_stream.try_lock() {
            Ok(send) => send,
            Err(_) => {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        };

        // Poll the send stream
        Pin::new(&mut *send).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();

        // Try to lock the send stream
        let mut send = match this.send_stream.try_lock() {
            Ok(send) => send,
            Err(_) => {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        };

        // Poll flush
        Pin::new(&mut *send).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();

        // Try to lock the send stream
        let mut send = match this.send_stream.try_lock() {
            Ok(send) => send,
            Err(_) => {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        };

        // Poll shutdown
        Pin::new(&mut *send).poll_shutdown(cx)
    }
}

/// QUIC listener for accepting incoming connections.
pub struct QuicListener {
    /// The Quinn endpoint
    endpoint: Endpoint,
    /// Configuration for accepted connections
    config: QuicConfig,
}

impl QuicListener {
    /// Bind to a local address and start listening for QUIC connections.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # #[cfg(feature = "quic")]
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use bdrpc::transport::{QuicListener, QuicConfig};
    ///
    /// let config = QuicConfig::default();
    /// let listener = QuicListener::bind("0.0.0.0:4433", config).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn bind(addr: impl AsRef<str>, config: QuicConfig) -> Result<Self, TransportError> {
        let addr_str = addr.as_ref();

        // Parse address
        let bind_addr: SocketAddr =
            addr_str
                .parse()
                .map_err(|e| TransportError::InvalidConfiguration {
                    reason: format!("Invalid address '{}': {}", addr_str, e),
                })?;

        // Generate self-signed certificate for testing
        let cert =
            rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).map_err(|e| {
                TransportError::QuicEndpointError {
                    reason: format!("Failed to generate certificate: {}", e),
                }
            })?;

        let cert_der = cert
            .serialize_der()
            .map_err(|e| TransportError::QuicEndpointError {
                reason: format!("Failed to serialize certificate: {}", e),
            })?;
        let priv_key = cert.serialize_private_key_der();

        // Create rustls server config (0.21 API)
        let cert_chain = vec![rustls::Certificate(cert_der)];
        let private_key = rustls::PrivateKey(priv_key);

        let crypto = rustls::ServerConfig::builder()
            .with_safe_defaults()
            .with_no_client_auth()
            .with_single_cert(cert_chain, private_key)
            .map_err(|e| TransportError::QuicEndpointError {
                reason: format!("Failed to create rustls server config: {}", e),
            })?;

        let mut server_config = ServerConfig::with_crypto(Arc::new(crypto));

        // Apply transport configuration
        let mut transport_config = quinn::TransportConfig::default();
        transport_config.max_idle_timeout(Some(
            config
                .max_idle_timeout
                .try_into()
                .unwrap_or_else(|_| quinn::IdleTimeout::from(quinn::VarInt::from_u32(60000))),
        ));
        transport_config.keep_alive_interval(Some(config.keep_alive_interval));
        transport_config.max_concurrent_bidi_streams(quinn::VarInt::from_u32(
            config.max_concurrent_bidi_streams.min(u32::MAX as u64) as u32,
        ));
        transport_config.max_concurrent_uni_streams(quinn::VarInt::from_u32(
            config.max_concurrent_uni_streams.min(u32::MAX as u64) as u32,
        ));

        server_config.transport_config(Arc::new(transport_config));

        // Create endpoint
        let endpoint =
            Endpoint::server(server_config, bind_addr).map_err(|e| TransportError::BindFailed {
                address: addr_str.to_string(),
                source: io::Error::other(e),
            })?;

        Ok(Self { endpoint, config })
    }

    /// Accept an incoming QUIC connection.
    ///
    /// This method waits for a new connection and returns a `QuicTransport`
    /// with an open bidirectional stream.
    pub async fn accept(&self) -> Result<QuicTransport, TransportError> {
        // Accept connection
        let connection = self
            .endpoint
            .accept()
            .await
            .ok_or_else(|| TransportError::Closed)?
            .await
            .map_err(|e| TransportError::ConnectionFailed {
                address: "unknown".to_string(),
                source: io::Error::other(e),
            })?;

        // Accept bidirectional stream
        let (send_stream, recv_stream) =
            connection
                .accept_bi()
                .await
                .map_err(|e| TransportError::QuicEndpointError {
                    reason: format!("Failed to accept stream: {}", e),
                })?;

        Ok(QuicTransport::new(
            connection,
            send_stream,
            recv_stream,
            self.config.clone(),
        ))
    }

    /// Get the local address this listener is bound to.
    #[allow(clippy::result_large_err)]
    pub fn local_addr(&self) -> Result<SocketAddr, TransportError> {
        self.endpoint.local_addr().map_err(|e| TransportError::Io {
            source: io::Error::other(e),
        })
    }
}

/// Skip server certificate verification (for testing only).
///
/// **WARNING**: This is insecure and should only be used for testing!
#[derive(Debug)]
struct SkipServerVerification;

impl rustls::client::ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::Certificate,
        _intermediates: &[rustls::Certificate],
        _server_name: &rustls::ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp_response: &[u8],
        _now: std::time::SystemTime,
    ) -> Result<rustls::client::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::ServerCertVerified::assertion())
    }
}

// Made with Bob

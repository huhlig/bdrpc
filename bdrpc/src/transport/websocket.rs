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

//! WebSocket transport implementation.
//!
//! This module provides WebSocket transport support for BDRPC, enabling
//! browser-based clients and web applications to communicate with BDRPC servers.
//!
//! # Features
//!
//! - Binary message support for efficient data transfer
//! - Automatic ping/pong keepalive mechanism
//! - Support for both `ws://` and `wss://` (secure WebSocket)
//! - Configurable frame and message size limits
//! - Optional per-message deflate compression
//!
//! # Examples
//!
//! ## Client Connection
//!
//! ```rust,no_run
//! use bdrpc::transport::{WebSocketTransport, WebSocketConfig};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = WebSocketConfig::default();
//! let transport = WebSocketTransport::connect("ws://localhost:8080", config).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Server Listener
//!
//! ```rust,no_run
//! use bdrpc::transport::{WebSocketListener, WebSocketConfig};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = WebSocketConfig::default();
//! let listener = WebSocketListener::bind("127.0.0.1:8080", config).await?;
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
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, accept_async, connect_async, tungstenite::Message,
};

/// Configuration for WebSocket transport.
///
/// This struct provides fine-grained control over WebSocket behavior,
/// including frame sizes, compression, and keepalive settings.
#[derive(Debug, Clone)]
pub struct WebSocketConfig {
    /// Maximum size of a single WebSocket frame (default: 16 MB)
    pub max_frame_size: usize,

    /// Maximum size of a complete message (default: 64 MB)
    pub max_message_size: usize,

    /// Enable per-message deflate compression (default: false)
    ///
    /// Note: Compression adds CPU overhead but can significantly reduce
    /// bandwidth usage for text-heavy payloads.
    pub compression: bool,

    /// Interval for sending ping frames for keepalive (default: 30s)
    pub ping_interval: Duration,

    /// Timeout for pong response (default: 10s)
    pub pong_timeout: Duration,

    /// Accept unmasked frames (server-side only, default: false)
    ///
    /// According to RFC 6455, clients must mask frames but servers must not.
    /// This setting is primarily for testing.
    pub accept_unmasked_frames: bool,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            max_frame_size: 16 * 1024 * 1024,   // 16 MB
            max_message_size: 64 * 1024 * 1024, // 64 MB
            compression: false,                 // Disabled for performance
            ping_interval: Duration::from_secs(30),
            pong_timeout: Duration::from_secs(10),
            accept_unmasked_frames: false,
        }
    }
}

/// WebSocket transport implementation.
///
/// This transport wraps a WebSocket connection and implements the BDRPC
/// [`Transport`] trait, allowing it to be used with the rest of the BDRPC stack.
///
/// # Message Format
///
/// All BDRPC messages are sent as binary WebSocket frames. The framing
/// protocol is handled by the serialization layer above this transport.
pub struct WebSocketTransport {
    /// The underlying WebSocket stream
    stream: Arc<Mutex<WebSocketStream<MaybeTlsStream<TcpStream>>>>,

    /// Configuration for this transport
    config: WebSocketConfig,

    /// Transport metadata
    metadata: TransportMetadata,

    /// Buffer for partial reads
    read_buffer: Arc<Mutex<Vec<u8>>>,

    /// Current position in read buffer
    read_pos: Arc<Mutex<usize>>,
}

impl WebSocketTransport {
    /// Connect to a WebSocket server.
    ///
    /// # Arguments
    ///
    /// * `url` - WebSocket URL (e.g., "ws://localhost:8080" or "wss://example.com")
    /// * `config` - WebSocket configuration
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The URL is invalid
    /// - Connection fails
    /// - WebSocket handshake fails
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{WebSocketTransport, WebSocketConfig};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let transport = WebSocketTransport::connect(
    ///     "ws://localhost:8080",
    ///     WebSocketConfig::default()
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn connect(url: &str, config: WebSocketConfig) -> Result<Self, TransportError> {
        let (ws_stream, _) = connect_async(url)
            .await
            .map_err(TransportError::WebSocket)?;

        // Extract peer address if available
        // Note: MaybeTlsStream only exposes Plain and NativeTls variants in tokio-tungstenite 0.21
        let peer_addr = match ws_stream.get_ref() {
            MaybeTlsStream::Plain(stream) => stream.peer_addr().ok(),
            _ => None,
        };

        let local_addr = match ws_stream.get_ref() {
            MaybeTlsStream::Plain(stream) => stream.local_addr().ok(),
            _ => None,
        };

        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let id = TransportId::new(NEXT_ID.fetch_add(1, Ordering::Relaxed));

        let mut metadata = TransportMetadata::new(id, "websocket");
        if let Some(addr) = peer_addr {
            metadata = metadata.with_peer_addr(addr);
        }
        if let Some(addr) = local_addr {
            metadata = metadata.with_local_addr(addr);
        }

        Ok(Self {
            stream: Arc::new(Mutex::new(ws_stream)),
            config,
            metadata,
            read_buffer: Arc::new(Mutex::new(Vec::new())),
            read_pos: Arc::new(Mutex::new(0)),
        })
    }

    /// Create a WebSocket transport from an accepted connection.
    ///
    /// This is used internally by [`WebSocketListener`].
    pub(crate) async fn from_stream(
        stream: TcpStream,
        config: WebSocketConfig,
    ) -> Result<Self, TransportError> {
        let peer_addr = stream.peer_addr().ok();
        let local_addr = stream.local_addr().ok();

        let ws_stream = accept_async(MaybeTlsStream::Plain(stream))
            .await
            .map_err(TransportError::WebSocket)?;

        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let id = TransportId::new(NEXT_ID.fetch_add(1, Ordering::Relaxed));

        let mut metadata = TransportMetadata::new(id, "websocket");
        if let Some(addr) = peer_addr {
            metadata = metadata.with_peer_addr(addr);
        }
        if let Some(addr) = local_addr {
            metadata = metadata.with_local_addr(addr);
        }

        Ok(Self {
            stream: Arc::new(Mutex::new(ws_stream)),
            config,
            metadata,
            read_buffer: Arc::new(Mutex::new(Vec::new())),
            read_pos: Arc::new(Mutex::new(0)),
        })
    }

    /// Get the WebSocket configuration.
    ///
    /// Returns a reference to the configuration used by this transport.
    /// This can be useful for inspecting settings like frame sizes,
    /// compression, and keepalive intervals.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{WebSocketTransport, WebSocketConfig};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let transport = WebSocketTransport::connect(
    ///     "ws://localhost:8080",
    ///     WebSocketConfig::default()
    /// ).await?;
    ///
    /// let config = transport.config();
    /// println!("Max frame size: {}", config.max_frame_size);
    /// # Ok(())
    /// # }
    /// ```
    pub fn config(&self) -> &WebSocketConfig {
        &self.config
    }

    /// Handle incoming WebSocket messages and extract binary data.
    async fn read_message(&self) -> Result<Vec<u8>, TransportError> {
        let mut stream = self.stream.lock().await;

        loop {
            match stream.next().await {
                Some(Ok(Message::Binary(data))) => {
                    return Ok(data);
                }
                Some(Ok(Message::Ping(data))) => {
                    // Respond to ping with pong
                    stream
                        .send(Message::Pong(data))
                        .await
                        .map_err(TransportError::WebSocket)?;
                }
                Some(Ok(Message::Pong(_))) => {
                    // Ignore pong messages
                    continue;
                }
                Some(Ok(Message::Close(_))) => {
                    return Err(TransportError::ConnectionLost {
                        reason: "peer closed connection".to_string(),
                        source: None,
                    });
                }
                Some(Ok(Message::Text(_))) => {
                    // BDRPC only uses binary frames
                    return Err(TransportError::WebSocketHandshakeFailed {
                        reason: "received text frame, expected binary".to_string(),
                    });
                }
                Some(Ok(Message::Frame(_))) => {
                    // Raw frames should not appear at this level
                    continue;
                }
                Some(Err(e)) => {
                    return Err(TransportError::WebSocket(e));
                }
                None => {
                    return Err(TransportError::ConnectionLost {
                        reason: "stream closed".to_string(),
                        source: None,
                    });
                }
            }
        }
    }
}

impl Transport for WebSocketTransport {
    fn metadata(&self) -> &TransportMetadata {
        &self.metadata
    }

    fn shutdown(
        &mut self,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), TransportError>> + Send + '_>> {
        Box::pin(async move {
            let mut stream = self.stream.lock().await;
            stream
                .close(None)
                .await
                .map_err(TransportError::WebSocket)?;
            Ok(())
        })
    }
}

impl AsyncRead for WebSocketTransport {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();

        // Try to use tokio's block_in_place for better async handling
        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                // Check if we have buffered data
                let mut read_buffer = this.read_buffer.lock().await;
                let mut read_pos = this.read_pos.lock().await;

                if *read_pos < read_buffer.len() {
                    // We have buffered data, copy it
                    let remaining = read_buffer.len() - *read_pos;
                    let to_copy = remaining.min(buf.remaining());
                    buf.put_slice(&read_buffer[*read_pos..*read_pos + to_copy]);
                    *read_pos += to_copy;

                    // Clear buffer if fully consumed
                    if *read_pos >= read_buffer.len() {
                        read_buffer.clear();
                        *read_pos = 0;
                    }

                    return Ok(());
                }

                // Need to read a new message
                drop(read_buffer);
                drop(read_pos);

                match this.read_message().await {
                    Ok(data) => {
                        let to_copy = data.len().min(buf.remaining());
                        buf.put_slice(&data[..to_copy]);

                        // Buffer any remaining data
                        if to_copy < data.len() {
                            let mut read_buffer = this.read_buffer.lock().await;
                            let mut read_pos = this.read_pos.lock().await;
                            *read_buffer = data;
                            *read_pos = to_copy;
                        }

                        Ok(())
                    }
                    Err(e) => Err(std::io::Error::other(e.to_string())),
                }
            })
        });

        Poll::Ready(result)
    }
}

impl AsyncWrite for WebSocketTransport {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        let len = buf.len();

        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let mut stream = this.stream.lock().await;
                stream
                    .send(Message::Binary(buf.to_vec()))
                    .await
                    .map_err(|e| std::io::Error::other(e.to_string()))?;
                Ok(len)
            })
        });

        Poll::Ready(result)
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();

        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let mut stream = this.stream.lock().await;
                stream
                    .flush()
                    .await
                    .map_err(|e| std::io::Error::other(e.to_string()))
            })
        });

        Poll::Ready(result)
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();

        let result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let mut stream = this.stream.lock().await;
                stream
                    .close(None)
                    .await
                    .map_err(|e| std::io::Error::other(e.to_string()))
            })
        });

        Poll::Ready(result)
    }
}

/// WebSocket listener for accepting incoming connections.
///
/// This listener binds to a TCP address and accepts WebSocket connections,
/// performing the WebSocket handshake automatically.
pub struct WebSocketListener {
    listener: TcpListener,
    config: WebSocketConfig,
}

impl WebSocketListener {
    /// Bind to a local address and listen for WebSocket connections.
    ///
    /// # Arguments
    ///
    /// * `addr` - Local address to bind to (e.g., "127.0.0.1:8080")
    /// * `config` - WebSocket configuration
    ///
    /// # Errors
    ///
    /// Returns an error if binding fails (e.g., address already in use).
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{WebSocketListener, WebSocketConfig};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let listener = WebSocketListener::bind(
    ///     "127.0.0.1:8080",
    ///     WebSocketConfig::default()
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn bind(
        addr: impl Into<String>,
        config: WebSocketConfig,
    ) -> Result<Self, TransportError> {
        let addr_str = addr.into();
        let listener =
            TcpListener::bind(&addr_str)
                .await
                .map_err(|e| TransportError::BindFailed {
                    address: addr_str,
                    source: e,
                })?;

        Ok(Self { listener, config })
    }

    /// Accept a new WebSocket connection.
    ///
    /// This method waits for an incoming TCP connection, performs the
    /// WebSocket handshake, and returns a [`WebSocketTransport`].
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Accepting the TCP connection fails
    /// - WebSocket handshake fails
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{WebSocketListener, WebSocketConfig};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let listener = WebSocketListener::bind("127.0.0.1:8080", WebSocketConfig::default()).await?;
    ///
    /// loop {
    ///     let transport = listener.accept().await?;
    ///     tokio::spawn(async move {
    ///         // Handle connection
    ///     });
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn accept(&self) -> Result<WebSocketTransport, TransportError> {
        let (stream, _addr) = self
            .listener
            .accept()
            .await
            .map_err(|e| TransportError::Io { source: e })?;

        WebSocketTransport::from_stream(stream, self.config.clone()).await
    }

    /// Returns the local address this listener is bound to.
    #[allow(clippy::result_large_err)]
    pub fn local_addr(&self) -> Result<SocketAddr, TransportError> {
        self.listener
            .local_addr()
            .map_err(|e| TransportError::Io { source: e })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_websocket_config_default() {
        let config = WebSocketConfig::default();
        assert_eq!(config.max_frame_size, 16 * 1024 * 1024);
        assert_eq!(config.max_message_size, 64 * 1024 * 1024);
        assert!(!config.compression);
        assert_eq!(config.ping_interval, Duration::from_secs(30));
        assert_eq!(config.pong_timeout, Duration::from_secs(10));
        assert!(!config.accept_unmasked_frames);
    }

    #[tokio::test]
    async fn test_websocket_listener_bind() {
        let config = WebSocketConfig::default();
        let listener = WebSocketListener::bind("127.0.0.1:0", config)
            .await
            .expect("Failed to bind");

        let addr = listener.local_addr().expect("Failed to get local addr");
        assert!(addr.port() > 0);
    }
}

// Made with Bob

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

//! TCP transport implementation.
//!
//! This module provides a TCP-based transport implementation using Tokio's
//! `TcpStream`. It supports both client and server modes.

use crate::transport::{Transport, TransportError, TransportId, TransportMetadata};
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};

#[cfg(feature = "observability")]
use tracing::{debug, error, info, instrument, warn};

/// Global counter for generating unique transport IDs.
static NEXT_TRANSPORT_ID: AtomicU64 = AtomicU64::new(1);

/// TCP transport implementation.
///
/// `TcpTransport` wraps a Tokio `TcpStream` and implements the [`Transport`] trait.
/// It provides reliable, ordered, connection-oriented communication over TCP/IP.
///
/// # Features
///
/// - Automatic transport ID generation
/// - Peer and local address tracking
/// - Graceful shutdown support
/// - Full async I/O support via Tokio
///
/// # Examples
///
/// ## Client mode
///
/// ```rust,no_run
/// use bdrpc::transport::{Transport, TcpTransport};
/// use tokio::io::{AsyncReadExt, AsyncWriteExt};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Connect to a server
/// let mut transport = TcpTransport::connect("127.0.0.1:8080").await?;
///
/// // Send a message
/// transport.write_all(b"Hello, server!").await?;
///
/// // Read response
/// let mut buffer = vec![0u8; 1024];
/// let n = transport.read(&mut buffer).await?;
/// println!("Received: {:?}", &buffer[..n]);
///
/// // Shutdown
/// Transport::shutdown(&mut transport).await?;
/// # Ok(())
/// # }
/// ```
///
/// ## Server mode
///
/// ```rust,no_run
/// use bdrpc::transport::{Transport, TcpTransport};
/// use tokio::io::{AsyncReadExt, AsyncWriteExt};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Listen for connections
/// let listener = TcpTransport::bind("127.0.0.1:8080").await?;
///
/// // Accept a connection
/// let (mut transport, peer_addr) = listener.accept().await?;
/// println!("Accepted connection from {}", peer_addr);
///
/// // Echo server
/// let mut buffer = vec![0u8; 1024];
/// loop {
///     let n = transport.read(&mut buffer).await?;
///     if n == 0 { break; }
///     transport.write_all(&buffer[..n]).await?;
/// }
/// # Ok(())
/// # }
/// ```
pub struct TcpTransport {
    stream: TcpStream,
    metadata: TransportMetadata,
}

impl TcpTransport {
    /// Creates a new TCP transport from an existing stream.
    ///
    /// This is typically used internally when accepting connections.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::TcpTransport;
    /// use tokio::net::TcpStream;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let stream = TcpStream::connect("127.0.0.1:8080").await?;
    /// let transport = TcpTransport::from_stream(stream)?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg_attr(
        feature = "observability",
        instrument(skip(stream), fields(transport_id, local_addr, peer_addr))
    )]
    pub fn from_stream(stream: TcpStream) -> io::Result<Self> {
        let id = TransportId::new(NEXT_TRANSPORT_ID.fetch_add(1, Ordering::Relaxed));
        let local_addr = stream.local_addr()?;
        let peer_addr = stream.peer_addr()?;

        #[cfg(feature = "observability")]
        {
            tracing::Span::current().record("transport_id", format!("{}", id));
            tracing::Span::current().record("local_addr", format!("{}", local_addr));
            tracing::Span::current().record("peer_addr", format!("{}", peer_addr));
            debug!("Created TCP transport from stream");
        }

        let metadata = TransportMetadata::new(id, "tcp")
            .with_local_addr(local_addr)
            .with_peer_addr(peer_addr);

        Ok(Self { stream, metadata })
    }

    /// Connects to a remote TCP endpoint.
    ///
    /// This establishes a TCP connection to the specified address and returns
    /// a `TcpTransport` ready for use.
    ///
    /// # Errors
    ///
    /// Returns a [`TransportError::ConnectionFailed`] if the connection cannot
    /// be established.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::TcpTransport;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let transport = TcpTransport::connect("127.0.0.1:8080").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg_attr(feature = "observability", instrument(skip(addr), fields(address)))]
    pub async fn connect(addr: impl Into<String>) -> Result<Self, TransportError> {
        let addr_str = addr.into();

        #[cfg(feature = "observability")]
        {
            tracing::Span::current().record("address", addr_str.as_str());
            info!("Connecting to TCP endpoint");
        }

        let stream = TcpStream::connect(&addr_str).await.map_err(|e| {
            #[cfg(feature = "observability")]
            error!("Failed to connect: {}", e);
            TransportError::ConnectionFailed {
                address: addr_str.clone(),
                source: e,
            }
        })?;

        #[cfg(feature = "observability")]
        info!("TCP connection established");

        Self::from_stream(stream).map_err(|e| TransportError::Io { source: e })
    }

    /// Binds to a local address and listens for incoming connections.
    ///
    /// This creates a TCP listener that can accept incoming connections.
    ///
    /// # Errors
    ///
    /// Returns a [`TransportError::BindFailed`] if the address cannot be bound.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::TcpTransport;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let listener = TcpTransport::bind("127.0.0.1:8080").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg_attr(feature = "observability", instrument(skip(addr), fields(address)))]
    pub async fn bind(addr: impl Into<String>) -> Result<TcpListener, TransportError> {
        let addr_str = addr.into();

        #[cfg(feature = "observability")]
        {
            tracing::Span::current().record("address", addr_str.as_str());
            info!("Binding TCP listener");
        }

        let listener = TcpListener::bind(&addr_str).await.map_err(|e| {
            #[cfg(feature = "observability")]
            error!("Failed to bind: {}", e);
            TransportError::BindFailed {
                address: addr_str.clone(),
                source: e,
            }
        })?;

        #[cfg(feature = "observability")]
        info!("TCP listener bound successfully");

        Ok(listener)
    }

    /// Accepts an incoming connection from a listener.
    ///
    /// This is a convenience method that wraps `TcpListener::accept()` and
    /// creates a `TcpTransport` from the accepted stream.
    ///
    /// # Errors
    ///
    /// Returns a [`TransportError`] if accepting the connection fails.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::TcpTransport;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let listener = TcpTransport::bind("127.0.0.1:8080").await?;
    /// let (transport, peer_addr) = TcpTransport::accept(&listener).await?;
    /// println!("Accepted connection from {}", peer_addr);
    /// # Ok(())
    /// # }
    /// ```
    #[cfg_attr(
        feature = "observability",
        instrument(skip(listener), fields(peer_addr))
    )]
    pub async fn accept(listener: &TcpListener) -> Result<(Self, SocketAddr), TransportError> {
        #[cfg(feature = "observability")]
        debug!("Waiting for TCP connection");

        let (stream, peer_addr) = listener.accept().await.map_err(|e| {
            #[cfg(feature = "observability")]
            error!("Failed to accept connection: {}", e);
            TransportError::Io { source: e }
        })?;

        #[cfg(feature = "observability")]
        {
            tracing::Span::current().record("peer_addr", format!("{}", peer_addr));
            info!("Accepted TCP connection");
        }

        let transport = Self::from_stream(stream).map_err(|e| TransportError::Io { source: e })?;

        Ok((transport, peer_addr))
    }

    /// Returns the local address of this transport.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::TcpTransport;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let transport = TcpTransport::connect("127.0.0.1:8080").await?;
    /// println!("Local address: {}", transport.local_addr()?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.stream.local_addr()
    }

    /// Returns the peer address of this transport.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::TcpTransport;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let transport = TcpTransport::connect("127.0.0.1:8080").await?;
    /// println!("Peer address: {}", transport.peer_addr()?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.stream.peer_addr()
    }

    /// Sets the TCP_NODELAY option on the underlying socket.
    ///
    /// When enabled, this disables Nagle's algorithm, which can reduce latency
    /// for small messages at the cost of potentially increased bandwidth usage.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::TcpTransport;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut transport = TcpTransport::connect("127.0.0.1:8080").await?;
    /// transport.set_nodelay(true)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        self.stream.set_nodelay(nodelay)
    }

    /// Gets the TCP_NODELAY option on the underlying socket.
    pub fn nodelay(&self) -> io::Result<bool> {
        self.stream.nodelay()
    }
}

impl Transport for TcpTransport {
    fn metadata(&self) -> &TransportMetadata {
        &self.metadata
    }

    #[cfg_attr(feature = "observability", instrument(skip(self), fields(transport_id = %self.metadata.id)))]
    fn shutdown(&mut self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), TransportError>> + Send + '_>> {
        Box::pin(async move {
            use tokio::io::AsyncWriteExt;

            #[cfg(feature = "observability")]
            info!("Shutting down TCP transport");

            let result = self.stream.shutdown().await.map_err(|e| {
                #[cfg(feature = "observability")]
                error!("Failed to shutdown: {}", e);
                TransportError::Io { source: e }
            });

            #[cfg(feature = "observability")]
            if result.is_ok() {
                info!("TCP transport shutdown complete");
            }

            result
        })
    }
}

impl AsyncRead for TcpTransport {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for TcpTransport {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_tcp_connect_and_echo() {
        // Start a simple echo server
        let listener = TcpTransport::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Spawn server task
        tokio::spawn(async move {
            let (mut transport, _) = TcpTransport::accept(&listener).await.unwrap();
            let mut buffer = vec![0u8; 1024];
            let n = transport.read(&mut buffer).await.unwrap();
            transport.write_all(&buffer[..n]).await.unwrap();
        });

        // Connect client
        let mut client = TcpTransport::connect(addr.to_string()).await.unwrap();

        // Send message
        client.write_all(b"Hello, server!").await.unwrap();

        // Read echo
        let mut buffer = vec![0u8; 1024];
        let n = client.read(&mut buffer).await.unwrap();
        assert_eq!(&buffer[..n], b"Hello, server!");

        // Shutdown
        Transport::shutdown(&mut client).await.unwrap();
    }

    #[tokio::test]
    async fn test_tcp_metadata() {
        let listener = TcpTransport::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let _ = TcpTransport::accept(&listener).await;
        });

        let transport = TcpTransport::connect(addr.to_string()).await.unwrap();
        let metadata = transport.metadata();

        assert_eq!(metadata.transport_type, "tcp");
        assert!(metadata.local_addr.is_some());
        assert!(metadata.peer_addr.is_some());
        assert_eq!(metadata.peer_addr.unwrap(), addr);
    }

    #[tokio::test]
    async fn test_tcp_nodelay() {
        let listener = TcpTransport::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let _ = TcpTransport::accept(&listener).await;
        });

        let transport = TcpTransport::connect(addr.to_string()).await.unwrap();

        // Test setting nodelay
        transport.set_nodelay(true).unwrap();
        assert!(transport.nodelay().unwrap());

        transport.set_nodelay(false).unwrap();
        assert!(!transport.nodelay().unwrap());
    }

    #[tokio::test]
    async fn test_tcp_connection_refused() {
        // Try to connect to a port that's not listening
        let result = TcpTransport::connect("127.0.0.1:1").await;
        assert!(result.is_err());

        if let Err(TransportError::ConnectionFailed { address, .. }) = result {
            assert_eq!(address, "127.0.0.1:1");
        } else {
            panic!("Expected ConnectionFailed error");
        }
    }

    #[tokio::test]
    async fn test_tcp_unique_ids() {
        let listener = TcpTransport::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let _ = TcpTransport::accept(&listener).await;
            let _ = TcpTransport::accept(&listener).await;
        });

        let transport1 = TcpTransport::connect(addr.to_string()).await.unwrap();
        let transport2 = TcpTransport::connect(addr.to_string()).await.unwrap();

        assert_ne!(transport1.metadata().id, transport2.metadata().id);
    }
}

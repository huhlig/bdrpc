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

//! Core transport trait definitions.
//!
//! This module defines the [`Transport`] trait, which is the foundation of
//! BDRPC's transport layer abstraction.

use std::fmt;
use std::net::SocketAddr;
use tokio::io::{AsyncRead, AsyncWrite};

/// Unique identifier for a transport connection.
///
/// Transport IDs are used to track and manage individual transport instances
/// within the transport manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransportId(u64);

impl TransportId {
    /// Creates a new transport ID.
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Returns the raw ID value.
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for TransportId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Transport({})", self.0)
    }
}

/// Metadata associated with a transport connection.
///
/// This provides information about the transport that can be used for
/// logging, metrics, and debugging.
#[derive(Debug, Clone)]
pub struct TransportMetadata {
    /// Unique identifier for this transport
    pub id: TransportId,

    /// Local address of the connection, if available
    pub local_addr: Option<SocketAddr>,

    /// Remote peer address, if available
    pub peer_addr: Option<SocketAddr>,

    /// Transport type (e.g., "tcp", "memory", "tls")
    pub transport_type: String,

    /// When the transport was created
    pub created_at: std::time::Instant,
}

impl TransportMetadata {
    /// Creates new transport metadata.
    pub fn new(id: TransportId, transport_type: impl Into<String>) -> Self {
        Self {
            id,
            local_addr: None,
            peer_addr: None,
            transport_type: transport_type.into(),
            created_at: std::time::Instant::now(),
        }
    }

    /// Sets the local address.
    pub fn with_local_addr(mut self, addr: SocketAddr) -> Self {
        self.local_addr = Some(addr);
        self
    }

    /// Sets the peer address.
    pub fn with_peer_addr(mut self, addr: SocketAddr) -> Self {
        self.peer_addr = Some(addr);
        self
    }

    /// Returns the age of this transport.
    pub fn age(&self) -> std::time::Duration {
        self.created_at.elapsed()
    }
}

/// Core transport abstraction for bi-directional byte streams.
///
/// The `Transport` trait defines the interface for all network transports in BDRPC.
/// It combines Tokio's `AsyncRead` and `AsyncWrite` traits with additional metadata
/// and lifecycle management.
///
/// # Design Principles
///
/// 1. **Abstraction**: Hide transport-specific details behind a common interface
/// 2. **Bi-directionality**: Support simultaneous read and write operations
/// 3. **Metadata**: Provide access to connection information
/// 4. **Lifecycle**: Support graceful shutdown and cleanup
///
/// # Implementations
///
/// BDRPC provides several built-in transport implementations:
///
/// - [`TcpTransport`](crate::transport::TcpTransport): TCP/IP networking
/// - [`MemoryTransport`](crate::transport::MemoryTransport): In-memory channels for testing
///
/// # Examples
///
/// ## Using a TCP transport
///
/// ```rust,no_run
/// use bdrpc::transport::{Transport, TcpTransport};
/// use tokio::io::{AsyncReadExt, AsyncWriteExt};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Connect to a server
/// let mut transport = TcpTransport::connect("127.0.0.1:8080").await?;
///
/// // Get metadata
/// let metadata = transport.metadata();
/// println!("Connected via {}", metadata.transport_type);
/// println!("Peer: {:?}", metadata.peer_addr);
///
/// // Write data
/// transport.write_all(b"Hello, server!").await?;
///
/// // Read response
/// let mut buffer = vec![0u8; 1024];
/// let n = transport.read(&mut buffer).await?;
/// println!("Received {} bytes", n);
///
/// // Graceful shutdown
/// Transport::shutdown(&mut transport).await?;
/// # Ok(())
/// # }
/// ```
///
/// ## Implementing a custom transport
///
/// ```rust
/// use bdrpc::transport::{Transport, TransportMetadata, TransportId, TransportError};
/// use tokio::io::{AsyncRead, AsyncWrite};
/// use std::pin::Pin;
/// use std::task::{Context, Poll};
///
/// struct CustomTransport {
///     metadata: TransportMetadata,
///     // ... other fields
/// }
///
/// impl Transport for CustomTransport {
///     fn metadata(&self) -> &TransportMetadata {
///         &self.metadata
///     }
///
///     fn shutdown(&mut self) -> Pin<Box<dyn std::future::Future<Output = Result<(), TransportError>> + Send + '_>> {
///         Box::pin(async move {
///             // Implement graceful shutdown
///             Ok(())
///         })
///     }
/// }
///
/// impl AsyncRead for CustomTransport {
///     fn poll_read(
///         self: Pin<&mut Self>,
///         cx: &mut Context<'_>,
///         buf: &mut tokio::io::ReadBuf<'_>,
///     ) -> Poll<std::io::Result<()>> {
///         // Implement read logic
///         Poll::Ready(Ok(()))
///     }
/// }
///
/// impl AsyncWrite for CustomTransport {
///     fn poll_write(
///         self: Pin<&mut Self>,
///         cx: &mut Context<'_>,
///         buf: &[u8],
///     ) -> Poll<std::io::Result<usize>> {
///         // Implement write logic
///         Poll::Ready(Ok(buf.len()))
///     }
///
///     fn poll_flush(
///         self: Pin<&mut Self>,
///         cx: &mut Context<'_>,
///     ) -> Poll<std::io::Result<()>> {
///         Poll::Ready(Ok(()))
///     }
///
///     fn poll_shutdown(
///         self: Pin<&mut Self>,
///         cx: &mut Context<'_>,
///     ) -> Poll<std::io::Result<()>> {
///         Poll::Ready(Ok(()))
///     }
/// }
/// ```
pub trait Transport: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static {
    /// Returns metadata about this transport.
    ///
    /// The metadata includes information such as the transport ID, peer address,
    /// and transport type. This is useful for logging, metrics, and debugging.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{Transport, TcpTransport};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let transport = TcpTransport::connect("127.0.0.1:8080").await?;
    /// let metadata = transport.metadata();
    /// println!("Transport ID: {}", metadata.id);
    /// println!("Peer address: {:?}", metadata.peer_addr);
    /// # Ok(())
    /// # }
    /// ```
    fn metadata(&self) -> &TransportMetadata;

    /// Gracefully shuts down the transport.
    ///
    /// This method should:
    /// 1. Flush any pending writes
    /// 2. Send a shutdown signal to the peer (if applicable)
    /// 3. Close the underlying connection
    ///
    /// After calling `shutdown()`, the transport should not be used for
    /// further I/O operations.
    ///
    /// # Errors
    ///
    /// Returns an error if the shutdown process fails. Common errors include:
    /// - I/O errors during flush
    /// - Transport already closed
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{Transport, TcpTransport};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut transport = TcpTransport::connect("127.0.0.1:8080").await?;
    /// // ... use transport ...
    /// transport.shutdown().await?;
    /// # Ok(())
    /// # }
    /// ```
    fn shutdown(
        &mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), crate::transport::TransportError>> + Send + '_>>;

    /// Splits the transport into separate read and write halves.
    ///
    /// This allows for concurrent reading and writing on the same transport,
    /// which is essential for bi-directional RPC communication.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{Transport, TcpTransport};
    /// use tokio::io::{AsyncReadExt, AsyncWriteExt};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let transport = TcpTransport::connect("127.0.0.1:8080").await?;
    /// let (mut reader, mut writer) = transport.split();
    ///
    /// // Spawn a task to read
    /// tokio::spawn(async move {
    ///     let mut buffer = vec![0u8; 1024];
    ///     while let Ok(n) = reader.read(&mut buffer).await {
    ///         if n == 0 { break; }
    ///         println!("Received {} bytes", n);
    ///     }
    /// });
    ///
    /// // Write in the main task
    /// writer.write_all(b"Hello!").await?;
    /// # Ok(())
    /// # }
    /// ```
    fn split(
        self,
    ) -> (
        Box<dyn AsyncRead + Send + Unpin>,
        Box<dyn AsyncWrite + Send + Unpin>,
    )
    where
        Self: Sized,
    {
        let (reader, writer) = tokio::io::split(self);
        (Box::new(reader), Box::new(writer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_id_creation() {
        let id = TransportId::new(42);
        assert_eq!(id.as_u64(), 42);
    }

    #[test]
    fn test_transport_id_display() {
        let id = TransportId::new(123);
        assert_eq!(format!("{}", id), "Transport(123)");
    }

    #[test]
    fn test_transport_metadata_creation() {
        let id = TransportId::new(1);
        let metadata = TransportMetadata::new(id, "test");

        assert_eq!(metadata.id, id);
        assert_eq!(metadata.transport_type, "test");
        assert!(metadata.local_addr.is_none());
        assert!(metadata.peer_addr.is_none());
    }

    #[test]
    fn test_transport_metadata_with_addresses() {
        let id = TransportId::new(1);
        let local = "127.0.0.1:8080".parse().unwrap();
        let peer = "127.0.0.1:9090".parse().unwrap();

        let metadata = TransportMetadata::new(id, "test")
            .with_local_addr(local)
            .with_peer_addr(peer);

        assert_eq!(metadata.local_addr, Some(local));
        assert_eq!(metadata.peer_addr, Some(peer));
    }

    #[test]
    fn test_transport_metadata_age() {
        let id = TransportId::new(1);
        let metadata = TransportMetadata::new(id, "test");

        std::thread::sleep(std::time::Duration::from_millis(10));

        let age = metadata.age();
        assert!(age.as_millis() >= 10);
    }
}

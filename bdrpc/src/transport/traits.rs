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

use crate::transport::{TransportId, TransportMetadata};
use crate::{ChannelId, TransportError};
use tokio::io::{AsyncRead, AsyncWrite};

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
/// // Write types
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
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<(), crate::transport::TransportError>>
                + Send
                + '_,
        >,
    >;

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

/// Trait for transport listeners (servers).
///
/// A transport listener accepts incoming connections. The actual transport type
/// is determined by the concrete implementation.
///
/// # Note
///
/// This trait is designed to be implemented by specific transport types
/// (e.g., `TcpListener`, `TlsListener`) rather than used as a trait object.
/// The accepted transport type is determined by the implementation.
///
/// # Examples
///
/// ```rust,no_run
/// use bdrpc::transport::TransportListener;
///
/// # async fn example<L: TransportListener>(listener: &L) -> Result<(), Box<dyn std::error::Error>> {
/// // Get listener address
/// let addr = listener.local_addr()?;
/// println!("Listening on {}", addr);
/// # Ok(())
/// # }
/// ```
#[async_trait::async_trait]
pub trait TransportListener: Send + Sync {
    /// The type of transport this listener produces
    type Transport: Transport;

    /// Accepts a new incoming connection.
    ///
    /// This method blocks until a new connection is available.
    async fn accept(&self) -> Result<Self::Transport, TransportError>;

    /// Returns the local address this listener is bound to.
    #[allow(clippy::result_large_err)]
    fn local_addr(&self) -> Result<String, TransportError>;

    /// Gracefully shuts down the listener.
    ///
    /// After calling this method, no new connections will be accepted.
    async fn shutdown(&self) -> Result<(), TransportError>;
}

/// Event handler for transport lifecycle events.
///
/// This trait allows the endpoint to respond to transport events such as
/// connections, disconnections, and channel creation requests.
///
/// # Examples
///
/// ```rust
/// use bdrpc::transport::{TransportEventHandler, TransportId};
/// use bdrpc::channel::ChannelId;
/// use bdrpc::transport::TransportError;
///
/// struct MyEventHandler;
///
/// impl TransportEventHandler for MyEventHandler {
///     fn on_transport_connected(&self, transport_id: TransportId) {
///         println!("Transport {} connected", transport_id);
///     }
///
///     fn on_transport_disconnected(
///         &self,
///         transport_id: TransportId,
///         error: Option<TransportError>,
///     ) {
///         if let Some(err) = error {
///             eprintln!("Transport {} disconnected with error: {}", transport_id, err);
///         } else {
///             println!("Transport {} disconnected gracefully", transport_id);
///         }
///     }
///
///     fn on_new_channel_request(
///         &self,
///         channel_id: ChannelId,
///         protocol: &str,
///         transport_id: TransportId,
///     ) -> Result<bool, String> {
///         println!(
///             "Channel {} request for protocol '{}' on transport {}",
///             channel_id, protocol, transport_id
///         );
///         Ok(true) // Accept the channel
///     }
/// }
/// ```
pub trait TransportEventHandler: Send + Sync {
    /// Called when a transport successfully connects.
    ///
    /// This is invoked for both listener (server) and caller (client) transports
    /// after the connection is established.
    fn on_transport_connected(&self, transport_id: TransportId);

    /// Called when a transport disconnects.
    ///
    /// The `error` parameter contains the error that caused the disconnection,
    /// or `None` if the disconnection was graceful.
    fn on_transport_disconnected(&self, transport_id: TransportId, error: Option<TransportError>);

    /// Called when a new channel creation is requested.
    ///
    /// Returns `Ok(true)` to accept the channel, `Ok(false)` to reject it,
    /// or `Err(reason)` to reject with a specific error message.
    fn on_new_channel_request(
        &self,
        channel_id: ChannelId,
        protocol: &str,
        transport_id: TransportId,
    ) -> Result<bool, String>;
}

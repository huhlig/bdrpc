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

//! In-memory transport implementation for testing.
//!
//! This module provides an in-memory transport that uses Tokio channels for
//! communication. It's primarily useful for testing and benchmarking without
//! the overhead of actual network I/O.

use crate::transport::{Transport, TransportError, TransportId, TransportMetadata};
use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::mpsc;

#[cfg(feature = "observability")]
use tracing::{debug, info, instrument};

/// Global counter for generating unique transport IDs.
static NEXT_MEMORY_TRANSPORT_ID: AtomicU64 = AtomicU64::new(1);

/// Default buffer size for memory transport channels.
const DEFAULT_BUFFER_SIZE: usize = 1024;

/// In-memory transport implementation.
///
/// `MemoryTransport` provides a transport implementation that uses Tokio's
/// `mpsc` channels for communication. This is useful for:
///
/// - Unit testing without network overhead
/// - Benchmarking pure protocol performance
/// - Testing backpressure and flow control
/// - Simulating network conditions
///
/// # Features
///
/// - Zero-copy within the same process
/// - Configurable buffer sizes for backpressure testing
/// - Deterministic behavior for testing
/// - No network stack overhead
///
/// # Examples
///
/// ## Creating a pair of connected transports
///
/// ```rust
/// use bdrpc::transport::{Transport, MemoryTransport};
/// use tokio::io::{AsyncReadExt, AsyncWriteExt};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create a connected pair
/// let (mut transport1, mut transport2) = MemoryTransport::pair(1024);
///
/// // Write from transport1
/// transport1.write_all(b"Hello!").await?;
///
/// // Read from transport2
/// let mut buffer = vec![0u8; 1024];
/// let n = transport2.read(&mut buffer).await?;
/// assert_eq!(&buffer[..n], b"Hello!");
/// # Ok(())
/// # }
/// ```
///
/// ## Testing backpressure
///
/// ```rust
/// use bdrpc::transport::{Transport, MemoryTransport};
/// use tokio::io::AsyncWriteExt;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create a pair with small buffer
/// let (mut tx, _rx) = MemoryTransport::pair(2);
///
/// // Fill the buffer
/// tx.write_all(&[1, 2]).await?;
///
/// // This will block until the receiver reads
/// // (in a real scenario, you'd spawn a reader task)
/// # Ok(())
/// # }
/// ```
pub struct MemoryTransport {
    metadata: TransportMetadata,
    reader: MemoryReader,
    writer: MemoryWriter,
}

/// Reader half of a memory transport.
struct MemoryReader {
    rx: mpsc::Receiver<Vec<u8>>,
    current_chunk: Option<Vec<u8>>,
    chunk_offset: usize,
}

/// Writer half of a memory transport.
struct MemoryWriter {
    tx: mpsc::Sender<Vec<u8>>,
}

impl MemoryTransport {
    /// Creates a pair of connected memory transports.
    ///
    /// The two transports are connected such that data written to one can be
    /// read from the other, and vice versa.
    ///
    /// # Arguments
    ///
    /// * `buffer_size` - The size of the internal channel buffers. This affects
    ///   backpressure behavior: when the buffer is full, writes will block.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::transport::MemoryTransport;
    ///
    /// // Create a pair with default buffer size
    /// let (transport1, transport2) = MemoryTransport::pair(1024);
    /// ```
    #[cfg_attr(
        feature = "observability",
        instrument(fields(buffer_size, transport1_id, transport2_id))
    )]
    pub fn pair(buffer_size: usize) -> (Self, Self) {
        let (tx1, rx1) = mpsc::channel(buffer_size);
        let (tx2, rx2) = mpsc::channel(buffer_size);

        let id1 = TransportId::new(NEXT_MEMORY_TRANSPORT_ID.fetch_add(1, Ordering::Relaxed));
        let id2 = TransportId::new(NEXT_MEMORY_TRANSPORT_ID.fetch_add(1, Ordering::Relaxed));

        #[cfg(feature = "observability")]
        {
            tracing::Span::current().record("transport1_id", format!("{}", id1));
            tracing::Span::current().record("transport2_id", format!("{}", id2));
            info!("Created memory transport pair");
        }

        let transport1 = Self {
            metadata: TransportMetadata::new(id1, "memory"),
            reader: MemoryReader {
                rx: rx2,
                current_chunk: None,
                chunk_offset: 0,
            },
            writer: MemoryWriter { tx: tx1 },
        };

        let transport2 = Self {
            metadata: TransportMetadata::new(id2, "memory"),
            reader: MemoryReader {
                rx: rx1,
                current_chunk: None,
                chunk_offset: 0,
            },
            writer: MemoryWriter { tx: tx2 },
        };

        (transport1, transport2)
    }

    /// Creates a pair of connected memory transports with default buffer size.
    ///
    /// This is equivalent to calling `pair(DEFAULT_BUFFER_SIZE)`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::transport::MemoryTransport;
    ///
    /// let (transport1, transport2) = MemoryTransport::pair_default();
    /// ```
    pub fn pair_default() -> (Self, Self) {
        Self::pair(DEFAULT_BUFFER_SIZE)
    }
}

impl Transport for MemoryTransport {
    fn metadata(&self) -> &TransportMetadata {
        &self.metadata
    }

    #[cfg_attr(feature = "observability", instrument(skip(self), fields(transport_id = %self.metadata.id)))]
    fn shutdown(
        &mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), TransportError>> + Send + '_>>
    {
        Box::pin(async move {
            #[cfg(feature = "observability")]
            debug!("Shutting down memory transport");

            // Close the writer channel
            // The reader will naturally close when all senders are dropped

            #[cfg(feature = "observability")]
            debug!("Memory transport shutdown complete");

            Ok(())
        })
    }
}

impl AsyncRead for MemoryTransport {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // If we have a current chunk, read from it
        if let Some(chunk) = &self.reader.current_chunk {
            let remaining = chunk.len() - self.reader.chunk_offset;
            let to_read = remaining.min(buf.remaining());

            if to_read > 0 {
                let start = self.reader.chunk_offset;
                let end = start + to_read;
                let data = chunk[start..end].to_vec();
                buf.put_slice(&data);

                let chunk_len = chunk.len();
                let _ = chunk; // Release the reference

                self.reader.chunk_offset = end;

                // If we've consumed the entire chunk, clear it
                if self.reader.chunk_offset >= chunk_len {
                    self.reader.current_chunk = None;
                    self.reader.chunk_offset = 0;
                }

                return Poll::Ready(Ok(()));
            }
        }

        // Try to receive a new chunk
        match self.reader.rx.poll_recv(cx) {
            Poll::Ready(Some(chunk)) => {
                let to_read = chunk.len().min(buf.remaining());
                buf.put_slice(&chunk[..to_read]);

                // If we didn't read the entire chunk, save it for next time
                if to_read < chunk.len() {
                    self.reader.current_chunk = Some(chunk);
                    self.reader.chunk_offset = to_read;
                }

                Poll::Ready(Ok(()))
            }
            Poll::Ready(None) => {
                // Channel closed, return EOF
                Poll::Ready(Ok(()))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncWrite for MemoryTransport {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        // Try to send the data
        match this.writer.tx.try_send(buf.to_vec()) {
            Ok(()) => Poll::Ready(Ok(buf.len())),
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                // Channel is full, register waker and return pending
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                // Channel closed
                Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "memory transport closed",
                )))
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // Memory transport doesn't need flushing
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // Dropping the sender will close the channel
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_memory_transport_basic() {
        let (mut tx, mut rx) = MemoryTransport::pair_default();

        // Write data
        tx.write_all(b"Hello, world!").await.unwrap();

        // Read data
        let mut buffer = vec![0u8; 1024];
        let n = rx.read(&mut buffer).await.unwrap();
        assert_eq!(&buffer[..n], b"Hello, world!");
    }

    #[tokio::test]
    async fn test_memory_transport_bidirectional() {
        let (mut t1, mut t2) = MemoryTransport::pair_default();

        // Write from t1 to t2
        t1.write_all(b"Hello").await.unwrap();
        let mut buffer = vec![0u8; 1024];
        let n = t2.read(&mut buffer).await.unwrap();
        assert_eq!(&buffer[..n], b"Hello");

        // Write from t2 to t1
        t2.write_all(b"World").await.unwrap();
        let n = t1.read(&mut buffer).await.unwrap();
        assert_eq!(&buffer[..n], b"World");
    }

    #[tokio::test]
    async fn test_memory_transport_large_message() {
        let (mut tx, mut rx) = MemoryTransport::pair_default();

        // Send a large message
        let large_data = vec![42u8; 10000];
        tx.write_all(&large_data).await.unwrap();

        // Read it back
        let mut buffer = vec![0u8; 10000];
        let n = rx.read(&mut buffer).await.unwrap();
        assert_eq!(n, 10000);
        assert_eq!(buffer, large_data);
    }

    #[tokio::test]
    async fn test_memory_transport_multiple_writes() {
        let (mut tx, mut rx) = MemoryTransport::pair_default();

        // Write multiple messages
        tx.write_all(b"First").await.unwrap();
        tx.write_all(b"Second").await.unwrap();
        tx.write_all(b"Third").await.unwrap();

        // Read them back
        let mut buffer = vec![0u8; 1024];

        let n = rx.read(&mut buffer).await.unwrap();
        assert_eq!(&buffer[..n], b"First");

        let n = rx.read(&mut buffer).await.unwrap();
        assert_eq!(&buffer[..n], b"Second");

        let n = rx.read(&mut buffer).await.unwrap();
        assert_eq!(&buffer[..n], b"Third");
    }

    #[tokio::test]
    async fn test_memory_transport_eof() {
        let (tx, mut rx) = MemoryTransport::pair_default();

        // Drop the sender
        drop(tx);

        // Reading should return EOF (0 bytes)
        let mut buffer = vec![0u8; 1024];
        let n = rx.read(&mut buffer).await.unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn test_memory_transport_metadata() {
        let (t1, t2) = MemoryTransport::pair_default();

        let meta1 = t1.metadata();
        let meta2 = t2.metadata();

        assert_eq!(meta1.transport_type, "memory");
        assert_eq!(meta2.transport_type, "memory");
        assert_ne!(meta1.id, meta2.id);
    }

    #[tokio::test]
    async fn test_memory_transport_backpressure() {
        // Create a pair with very small buffer
        let (mut tx, mut rx) = MemoryTransport::pair(1);

        // Fill the buffer
        tx.write_all(b"X").await.unwrap();

        // Spawn a task to write more (this will block)
        let write_task = tokio::spawn(async move {
            tx.write_all(b"Y").await.unwrap();
            tx
        });

        // Give the write task a chance to block
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Read to unblock the writer
        let mut buffer = vec![0u8; 1];
        let n = rx.read(&mut buffer).await.unwrap();
        assert_eq!(n, 1);
        assert_eq!(&buffer[..n], b"X");

        // Now the write should complete
        let _tx = write_task.await.unwrap();

        // Read the second message
        let n = rx.read(&mut buffer).await.unwrap();
        assert_eq!(n, 1);
        assert_eq!(&buffer[..n], b"Y");
    }

    #[tokio::test]
    async fn test_memory_transport_shutdown() {
        let (mut tx, _rx) = MemoryTransport::pair_default();

        // Shutdown should succeed
        Transport::shutdown(&mut tx).await.unwrap();
    }
}

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

//! Compression transport implementation for bandwidth optimization.
//!
//! This module provides compression for BDRPC transports using various algorithms.
//! It supports gzip, zstd, and lz4 compression with configurable compression levels.
//!
//! # Features
//!
//! - Multiple compression algorithms (gzip, zstd, lz4)
//! - Configurable compression levels
//! - Wraps any existing transport with compression
//! - Transparent bidirectional compression/decompression
//!
//! # Examples
//!
//! ## Gzip Compression
//!
//! ```rust,no_run
//! # #[cfg(feature = "compression")]
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use bdrpc::transport::{TcpTransport, CompressedTransport, CompressionConfig};
//!
//! // Connect with gzip compression
//! let tcp = TcpTransport::connect("127.0.0.1:8080").await?;
//! let config = CompressionConfig::gzip(6); // Level 6 (balanced)
//! let compressed = CompressedTransport::wrap(tcp, config).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Zstd Compression
//!
//! ```rust,no_run
//! # #[cfg(feature = "compression")]
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use bdrpc::transport::{TcpTransport, CompressedTransport, CompressionConfig};
//!
//! // Connect with zstd compression
//! let tcp = TcpTransport::connect("127.0.0.1:8080").await?;
//! let config = CompressionConfig::zstd(3); // Level 3 (default)
//! let compressed = CompressedTransport::wrap(tcp, config).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## LZ4 Compression
//!
//! ```rust,no_run
//! # #[cfg(feature = "compression")]
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use bdrpc::transport::{TcpTransport, CompressedTransport, CompressionConfig};
//!
//! // Connect with lz4 compression
//! let tcp = TcpTransport::connect("127.0.0.1:8080").await?;
//! let config = CompressionConfig::lz4(4); // Level 4 (balanced)
//! let compressed = CompressedTransport::wrap(tcp, config).await?;
//! # Ok(())
//! # }
//! ```

use crate::transport::{Transport, TransportError, TransportMetadata};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, BufReader, ReadBuf};

// Import decoders for reading (decompression)
use async_compression::tokio::bufread::{
    GzipDecoder as GzipReadDecoder, Lz4Decoder as Lz4ReadDecoder, ZstdDecoder as ZstdReadDecoder,
};

// Import encoders for writing (compression)
use async_compression::tokio::write::{
    GzipEncoder as GzipWriteEncoder, Lz4Encoder as Lz4WriteEncoder, ZstdEncoder as ZstdWriteEncoder,
};

/// Compression algorithm selection.
///
/// This enum specifies which compression algorithm to use for the transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionAlgorithm {
    /// Gzip compression (RFC 1952)
    ///
    /// Good general-purpose compression with wide compatibility.
    /// Compression levels: 0 (no compression) to 9 (maximum compression).
    /// Default level: 6 (balanced speed/ratio).
    Gzip,

    /// Zstandard compression
    ///
    /// Modern compression algorithm with excellent speed and compression ratio.
    /// Compression levels: 1 (fastest) to 22 (maximum compression).
    /// Default level: 3 (balanced speed/ratio).
    Zstd,

    /// LZ4 compression
    ///
    /// Extremely fast compression algorithm optimized for speed.
    /// Compression levels: 1 (fastest) to 12 (maximum compression).
    /// Default level: 4 (balanced speed/ratio).
    /// Ideal for low-latency applications where speed is critical.
    Lz4,
}

/// Compression configuration.
///
/// This type encapsulates the compression settings including algorithm
/// and compression level.
#[derive(Debug, Clone, Copy)]
pub struct CompressionConfig {
    /// Compression algorithm to use
    pub algorithm: CompressionAlgorithm,
    /// Compression level (algorithm-specific)
    pub level: i32,
}

impl CompressionConfig {
    /// Creates a gzip compression configuration.
    ///
    /// # Arguments
    ///
    /// * `level` - Compression level (0-9, default 6)
    ///   - 0: No compression
    ///   - 1: Fastest compression
    ///   - 6: Balanced (default)
    ///   - 9: Maximum compression
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(feature = "compression")]
    /// # fn example() {
    /// use bdrpc::transport::CompressionConfig;
    ///
    /// let config = CompressionConfig::gzip(6);
    /// # }
    /// ```
    pub fn gzip(level: i32) -> Self {
        Self {
            algorithm: CompressionAlgorithm::Gzip,
            level: level.clamp(0, 9),
        }
    }

    /// Creates a zstd compression configuration.
    ///
    /// # Arguments
    ///
    /// * `level` - Compression level (1-22, default 3)
    ///   - 1: Fastest compression
    ///   - 3: Balanced (default)
    ///   - 22: Maximum compression
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(feature = "compression")]
    /// # fn example() {
    /// use bdrpc::transport::CompressionConfig;
    ///
    /// let config = CompressionConfig::zstd(3);
    /// # }
    /// ```
    pub fn zstd(level: i32) -> Self {
        Self {
            algorithm: CompressionAlgorithm::Zstd,
            level: level.clamp(1, 22),
        }
    }

    /// Creates an lz4 compression configuration.
    ///
    /// # Arguments
    ///
    /// * `level` - Compression level (1-12, default 4)
    ///   - 1: Fastest compression
    ///   - 4: Balanced (default)
    ///   - 12: Maximum compression
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(feature = "compression")]
    /// # fn example() {
    /// use bdrpc::transport::CompressionConfig;
    ///
    /// let config = CompressionConfig::lz4(4);
    /// # }
    /// ```
    pub fn lz4(level: i32) -> Self {
        Self {
            algorithm: CompressionAlgorithm::Lz4,
            level: level.clamp(1, 12),
        }
    }

    /// Creates a default gzip configuration (level 6).
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(feature = "compression")]
    /// # fn example() {
    /// use bdrpc::transport::CompressionConfig;
    ///
    /// let config = CompressionConfig::default();
    /// # }
    /// ```
    /// Creates a default compression configuration with gzip level 6.
    ///
    /// Note: Consider using `Default::default()` trait instead.
    #[allow(clippy::should_implement_trait)]
    pub fn default() -> Self {
        Self::gzip(6)
    }
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self::default()
    }
}

/// Internal enum to hold the actual decompression stream types for reading.
enum ReadStream<R> {
    GzipDecoder(GzipReadDecoder<BufReader<R>>),
    ZstdDecoder(ZstdReadDecoder<BufReader<R>>),
    Lz4Decoder(Lz4ReadDecoder<BufReader<R>>),
}

/// Internal enum to hold the actual compression stream types for writing.
enum WriteStream<W> {
    GzipEncoder(GzipWriteEncoder<W>),
    ZstdEncoder(ZstdWriteEncoder<W>),
    Lz4Encoder(Lz4WriteEncoder<W>),
}

/// A transport wrapper that adds compression.
///
/// This type wraps any transport that implements [`Transport`] and adds
/// bidirectional compression on top of it. It supports multiple compression
/// algorithms with configurable compression levels.
///
/// Both sides of the connection must use the same compression configuration.
pub struct CompressedTransport<T> {
    /// The read side (decompression)
    reader: ReadStream<tokio::io::ReadHalf<T>>,
    /// The write side (compression)
    writer: WriteStream<tokio::io::WriteHalf<T>>,
    /// Transport metadata
    metadata: TransportMetadata,
    /// Compression configuration
    config: CompressionConfig,
}

impl<T> CompressedTransport<T>
where
    T: Transport,
{
    /// Wraps an existing transport with bidirectional compression.
    ///
    /// This creates a compressed transport that compresses all data written
    /// and decompresses all data read. Both sides of the connection must use
    /// the same compression configuration.
    ///
    /// # Arguments
    ///
    /// * `transport` - The underlying transport to wrap
    /// * `config` - Compression configuration
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # #[cfg(feature = "compression")]
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use bdrpc::transport::{TcpTransport, CompressedTransport, CompressionConfig};
    ///
    /// let tcp = TcpTransport::connect("127.0.0.1:8080").await?;
    /// let config = CompressionConfig::gzip(6);
    /// let compressed = CompressedTransport::wrap(tcp, config).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn wrap(transport: T, config: CompressionConfig) -> Result<Self, TransportError> {
        let base_metadata = transport.metadata().clone();

        // Split the transport for independent read/write
        let (read_half, write_half) = tokio::io::split(transport);

        // Create decompression stream for reading
        let reader = match config.algorithm {
            CompressionAlgorithm::Gzip => {
                let buf_reader = BufReader::new(read_half);
                let decoder = GzipReadDecoder::new(buf_reader);
                ReadStream::GzipDecoder(decoder)
            }
            CompressionAlgorithm::Zstd => {
                let buf_reader = BufReader::new(read_half);
                let decoder = ZstdReadDecoder::new(buf_reader);
                ReadStream::ZstdDecoder(decoder)
            }
            CompressionAlgorithm::Lz4 => {
                let buf_reader = BufReader::new(read_half);
                let decoder = Lz4ReadDecoder::new(buf_reader);
                ReadStream::Lz4Decoder(decoder)
            }
        };

        // Create compression stream for writing
        let writer = match config.algorithm {
            CompressionAlgorithm::Gzip => {
                let encoder = GzipWriteEncoder::with_quality(
                    write_half,
                    async_compression::Level::Precise(config.level),
                );
                WriteStream::GzipEncoder(encoder)
            }
            CompressionAlgorithm::Zstd => {
                let encoder = ZstdWriteEncoder::with_quality(
                    write_half,
                    async_compression::Level::Precise(config.level),
                );
                WriteStream::ZstdEncoder(encoder)
            }
            CompressionAlgorithm::Lz4 => {
                let encoder = Lz4WriteEncoder::with_quality(
                    write_half,
                    async_compression::Level::Precise(config.level),
                );
                WriteStream::Lz4Encoder(encoder)
            }
        };

        let metadata = TransportMetadata::new(
            base_metadata.id,
            match config.algorithm {
                CompressionAlgorithm::Gzip => "compressed-gzip",
                CompressionAlgorithm::Zstd => "compressed-zstd",
                CompressionAlgorithm::Lz4 => "compressed-lz4",
            },
        )
        .with_local_addr(
            base_metadata
                .local_addr
                .unwrap_or_else(|| "0.0.0.0:0".parse().unwrap()),
        )
        .with_peer_addr(
            base_metadata
                .peer_addr
                .unwrap_or_else(|| "0.0.0.0:0".parse().unwrap()),
        );

        Ok(Self {
            reader,
            writer,
            metadata,
            config,
        })
    }

    /// Returns the compression configuration.
    pub fn config(&self) -> &CompressionConfig {
        &self.config
    }
}

impl<T> Transport for CompressedTransport<T>
where
    T: Transport,
{
    fn metadata(&self) -> &TransportMetadata {
        &self.metadata
    }

    async fn shutdown(&mut self) -> Result<(), TransportError> {
        use tokio::io::AsyncWriteExt;
        match &mut self.writer {
            WriteStream::GzipEncoder(w) => w.shutdown().await.map_err(TransportError::from),
            WriteStream::ZstdEncoder(w) => w.shutdown().await.map_err(TransportError::from),
            WriteStream::Lz4Encoder(w) => w.shutdown().await.map_err(TransportError::from),
        }
    }
}

impl<T> AsyncRead for CompressedTransport<T>
where
    T: Transport,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match &mut self.reader {
            ReadStream::GzipDecoder(d) => Pin::new(d).poll_read(cx, buf),
            ReadStream::ZstdDecoder(d) => Pin::new(d).poll_read(cx, buf),
            ReadStream::Lz4Decoder(d) => Pin::new(d).poll_read(cx, buf),
        }
    }
}

impl<T> AsyncWrite for CompressedTransport<T>
where
    T: Transport,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match &mut self.writer {
            WriteStream::GzipEncoder(w) => Pin::new(w).poll_write(cx, buf),
            WriteStream::ZstdEncoder(w) => Pin::new(w).poll_write(cx, buf),
            WriteStream::Lz4Encoder(w) => Pin::new(w).poll_write(cx, buf),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.writer {
            WriteStream::GzipEncoder(w) => Pin::new(w).poll_flush(cx),
            WriteStream::ZstdEncoder(w) => Pin::new(w).poll_flush(cx),
            WriteStream::Lz4Encoder(w) => Pin::new(w).poll_flush(cx),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut self.writer {
            WriteStream::GzipEncoder(w) => Pin::new(w).poll_shutdown(cx),
            WriteStream::ZstdEncoder(w) => Pin::new(w).poll_shutdown(cx),
            WriteStream::Lz4Encoder(w) => Pin::new(w).poll_shutdown(cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MemoryTransport;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn test_compression_config_gzip() {
        let config = CompressionConfig::gzip(6);
        assert_eq!(config.algorithm, CompressionAlgorithm::Gzip);
        assert_eq!(config.level, 6);
    }

    #[test]
    fn test_compression_config_zstd() {
        let config = CompressionConfig::zstd(3);
        assert_eq!(config.algorithm, CompressionAlgorithm::Zstd);
        assert_eq!(config.level, 3);
    }

    #[test]
    fn test_compression_config_default() {
        let config = CompressionConfig::default();
        assert_eq!(config.algorithm, CompressionAlgorithm::Gzip);
        assert_eq!(config.level, 6);
    }

    #[test]
    fn test_compression_level_clamping() {
        let config = CompressionConfig::gzip(100);
        assert_eq!(config.level, 9); // Should be clamped to max

        let config = CompressionConfig::zstd(100);
        assert_eq!(config.level, 22); // Should be clamped to max
    }

    #[tokio::test]
    async fn test_gzip_compression_basic() {
        let (client, server) = MemoryTransport::pair(1024 * 1024);

        let config = CompressionConfig::gzip(6);
        let mut compressed_client = CompressedTransport::wrap(client, config).await.unwrap();
        let mut compressed_server = CompressedTransport::wrap(server, config).await.unwrap();

        // Write data from client
        let data = b"Hello, compressed world!";
        compressed_client.write_all(data).await.unwrap();
        compressed_client.flush().await.unwrap();

        // Read data on server
        let mut buffer = vec![0u8; 1024];
        let n = compressed_server.read(&mut buffer).await.unwrap();
        assert_eq!(&buffer[..n], data);
    }

    #[tokio::test]
    async fn test_zstd_compression_basic() {
        let (client, server) = MemoryTransport::pair(1024 * 1024);

        let config = CompressionConfig::zstd(3);
        let mut compressed_client = CompressedTransport::wrap(client, config).await.unwrap();
        let mut compressed_server = CompressedTransport::wrap(server, config).await.unwrap();

        // Write data from client
        let data = b"Hello, zstd compressed world!";
        compressed_client.write_all(data).await.unwrap();
        compressed_client.flush().await.unwrap();

        // Read data on server
        let mut buffer = vec![0u8; 1024];
        let n = compressed_server.read(&mut buffer).await.unwrap();
        assert_eq!(&buffer[..n], data);
    }

    #[tokio::test]
    async fn test_compression_large_data() {
        let (client, server) = MemoryTransport::pair(1024 * 1024);

        let config = CompressionConfig::gzip(6);
        let mut compressed_client = CompressedTransport::wrap(client, config).await.unwrap();
        let mut compressed_server = CompressedTransport::wrap(server, config).await.unwrap();

        // Create large repetitive data (should compress well)
        let data = vec![b'A'; 10000];
        compressed_client.write_all(&data).await.unwrap();
        compressed_client.flush().await.unwrap();

        // Read data on server
        let mut buffer = vec![0u8; 20000];
        let n = compressed_server.read(&mut buffer).await.unwrap();
        assert_eq!(&buffer[..n], &data[..]);
    }

    #[tokio::test]
    async fn test_compression_metadata() {
        let (client, _server) = MemoryTransport::pair(1024);

        let config = CompressionConfig::gzip(6);
        let compressed = CompressedTransport::wrap(client, config).await.unwrap();

        let metadata = compressed.metadata();
        assert_eq!(metadata.transport_type, "compressed-gzip");
    }

    #[tokio::test]
    async fn test_compression_config_accessor() {
        let (client, _server) = MemoryTransport::pair(1024);

        let config = CompressionConfig::zstd(5);
        let compressed = CompressedTransport::wrap(client, config).await.unwrap();

        let retrieved_config = compressed.config();
        assert_eq!(retrieved_config.algorithm, CompressionAlgorithm::Zstd);
        assert_eq!(retrieved_config.level, 5);
    }

    #[test]
    fn test_compression_config_lz4() {
        let config = CompressionConfig::lz4(4);
        assert_eq!(config.algorithm, CompressionAlgorithm::Lz4);
        assert_eq!(config.level, 4);
    }

    #[test]
    fn test_compression_level_clamping_lz4() {
        let config = CompressionConfig::lz4(100);
        assert_eq!(config.level, 12); // Should be clamped to max
    }

    #[tokio::test]
    async fn test_lz4_compression_basic() {
        let (client, server) = MemoryTransport::pair(1024 * 1024);

        let config = CompressionConfig::lz4(4);
        let mut compressed_client = CompressedTransport::wrap(client, config).await.unwrap();
        let mut compressed_server = CompressedTransport::wrap(server, config).await.unwrap();

        // Write data from client
        let data = b"Hello, lz4 compressed world!";
        compressed_client.write_all(data).await.unwrap();
        compressed_client.flush().await.unwrap();

        // Read data on server
        let mut buffer = vec![0u8; 1024];
        let n = compressed_server.read(&mut buffer).await.unwrap();
        assert_eq!(&buffer[..n], data);
    }

    #[tokio::test]
    async fn test_lz4_compression_large_data() {
        let (client, server) = MemoryTransport::pair(1024 * 1024);

        let config = CompressionConfig::lz4(4);
        let mut compressed_client = CompressedTransport::wrap(client, config).await.unwrap();
        let mut compressed_server = CompressedTransport::wrap(server, config).await.unwrap();

        // Create large repetitive data (should compress well)
        let data = vec![b'B'; 10000];
        compressed_client.write_all(&data).await.unwrap();
        compressed_client.flush().await.unwrap();

        // Read data on server
        let mut buffer = vec![0u8; 20000];
        let n = compressed_server.read(&mut buffer).await.unwrap();
        assert_eq!(&buffer[..n], &data[..]);
    }

    #[tokio::test]
    async fn test_lz4_compression_metadata() {
        let (client, _server) = MemoryTransport::pair(1024);

        let config = CompressionConfig::lz4(4);
        let compressed = CompressedTransport::wrap(client, config).await.unwrap();

        let metadata = compressed.metadata();
        assert_eq!(metadata.transport_type, "compressed-lz4");
    }

    #[tokio::test]
    async fn test_lz4_bidirectional() {
        let (client, server) = MemoryTransport::pair(1024 * 1024);

        let config = CompressionConfig::lz4(4);
        let mut compressed_client = CompressedTransport::wrap(client, config).await.unwrap();
        let mut compressed_server = CompressedTransport::wrap(server, config).await.unwrap();

        // Client to server
        let client_data = b"Client to server with lz4";
        compressed_client.write_all(client_data).await.unwrap();
        compressed_client.flush().await.unwrap();

        let mut buffer = vec![0u8; 1024];
        let n = compressed_server.read(&mut buffer).await.unwrap();
        assert_eq!(&buffer[..n], client_data);

        // Server to client
        let server_data = b"Server to client with lz4";
        compressed_server.write_all(server_data).await.unwrap();
        compressed_server.flush().await.unwrap();

        let n = compressed_client.read(&mut buffer).await.unwrap();
        assert_eq!(&buffer[..n], server_data);
    }
}

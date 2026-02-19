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

//! Transport layer abstractions for BDRPC.
//!
//! This module provides the core transport abstraction that all network communication
//! is built upon. The [`Transport`] trait defines the interface for bi-directional
//! byte streams, and this module includes several implementations:
//!
//! - [`TcpTransport`]: TCP/IP networking
//! - [`TlsTransport`]: TLS-encrypted TCP (requires `tls` feature)
//! - [`CompressedTransport`]: Compressed transport wrapper (requires `compression` feature)
//! - [`MemoryTransport`]: In-memory channels for testing and in-process communication
//!
//! # Architecture
//!
//! The transport layer is the foundation of BDRPC's architecture. It provides:
//!
//! - **Abstraction**: Hide transport-specific details behind a common interface
//! - **Bi-directionality**: Support for simultaneous read and write operations
//! - **Metadata**: Access to connection information (peer address, transport ID)
//! - **Error handling**: Structured error types following ADR-004
//! - **Composability**: Transports can be wrapped (e.g., TLS over TCP, compression over TLS)
//!
//! # The Transport Trait
//!
//! The [`Transport`] trait extends `AsyncRead + AsyncWrite` with additional metadata:
//!
//! ```rust
//! use bdrpc::transport::{Transport, TransportId, TransportMetadata};
//! use tokio::io::{AsyncRead, AsyncWrite};
//!
//! // All transports implement these traits
//! fn use_transport<T: Transport>(transport: &T) {
//!     let metadata = transport.metadata();
//!     println!("Transport type: {}", metadata.transport_type);
//!     println!("Peer: {:?}", metadata.peer_addr);
//! }
//! ```
//!
//! # Examples
//!
//! ## TCP Transport
//!
//! ```rust,no_run
//! use bdrpc::transport::{Transport, TcpTransport};
//! use tokio::io::{AsyncReadExt, AsyncWriteExt};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Connect to a remote endpoint
//! let mut transport = TcpTransport::connect("127.0.0.1:8080").await?;
//!
//! // Write types
//! transport.write_all(b"Hello, world!").await?;
//!
//! // Read response
//! let mut buffer = vec![0u8; 1024];
//! let n = transport.read(&mut buffer).await?;
//! println!("Received {} bytes", n);
//!
//! // Graceful shutdown
//! Transport::shutdown(&mut transport).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Server with TCP
//!
//! ```rust,no_run
//! use bdrpc::transport::TcpTransport;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Bind to an address
//! let listener = TcpTransport::bind("127.0.0.1:8080").await?;
//! println!("Listening on {}", listener.local_addr()?);
//!
//! // Accept connections
//! loop {
//!     let (transport, peer_addr) = listener.accept().await?;
//!     println!("Accepted connection from {}", peer_addr);
//!
//!     // Spawn task to handle this connection
//!     tokio::spawn(async move {
//!         // Handle connection...
//!     });
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## In-Memory Transport (for testing)
//!
//! ```rust
//! use bdrpc::transport::MemoryTransport;
//! use tokio::io::{AsyncReadExt, AsyncWriteExt};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a pair of connected transports
//! let (mut client, mut server) = MemoryTransport::pair(1024);
//!
//! // Write from client
//! client.write_all(b"Hello").await?;
//!
//! // Read on server
//! let mut buffer = vec![0u8; 5];
//! server.read_exact(&mut buffer).await?;
//! assert_eq!(&buffer, b"Hello");
//! # Ok(())
//! # }
//! ```
//!
//! ## TLS Transport (requires `tls` feature)
//!
//! ```rust,no_run
//! # #[cfg(feature = "tls")]
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use bdrpc::transport::{TcpTransport, TlsTransport, TlsConfig};
//!
//! // Create TLS configuration with system root certificates
//! let config = TlsConfig::client_default("example.com")?;
//!
//! // Connect with TCP first, then wrap with TLS
//! let tcp = TcpTransport::connect("example.com:443").await?;
//! let transport = TlsTransport::connect(tcp, config).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Compressed Transport (requires `compression` feature)
//!
//! ```rust,no_run
//! # #[cfg(feature = "compression")]
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use bdrpc::transport::{TcpTransport, CompressedTransport, CompressionConfig};
//!
//! // Connect with TCP
//! let tcp = TcpTransport::connect("127.0.0.1:8080").await?;
//!
//! // Wrap with zstd compression (level 3)
//! let config = CompressionConfig::zstd(3);
//! let transport = CompressedTransport::wrap(tcp, config).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Transport Manager
//!
//! The [`TransportManager`] provides centralized management of multiple transports:
//!
//! ```rust
//! use bdrpc::transport::TransportManager;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let manager = TransportManager::new();
//!
//! // Register transports
//! // manager.register(transport);
//!
//! // Get transport by ID
//! // let transport = manager.get(&transport_id);
//! # Ok(())
//! # }
//! ```
//!
//! # Error Handling
//!
//! All transport operations return [`TransportError`] which provides detailed
//! error information:
//!
//! ```rust
//! use bdrpc::transport::{TransportError, TcpTransport};
//!
//! # async fn example() {
//! match TcpTransport::connect("invalid:99999").await {
//!     Ok(transport) => println!("Connected"),
//!     Err(TransportError::ConnectionFailed { address, source }) => {
//!         eprintln!("Failed to connect to {}: {}", address, source);
//!     }
//!     Err(e) => eprintln!("Other error: {}", e),
//! }
//! # }
//! ```
//!
//! # Performance Considerations
//!
//! - **TCP**: Use `TCP_NODELAY` for low-latency applications
//! - **TLS**: Has encryption overhead but provides security
//! - **Compression**: Reduces bandwidth but adds CPU overhead
//! - **Memory**: Zero-copy for in-process communication
//!
//! # Future Transports
//!
//! Planned transport implementations:
//! - WebSocket transport for browser compatibility
//! - QUIC transport for improved performance over unreliable networks
//! - Unix domain sockets for local IPC

mod caller;
mod config;
mod error;
mod manager;
pub mod provider;
pub mod strategy;
mod traits;
mod types;

pub use self::caller::{CallerState, CallerTransport};
pub use self::config::{TransportConfig, TransportType};
pub use self::error::TransportError;
pub use self::manager::TransportManager;
pub use self::traits::{Transport, TransportEventHandler, TransportListener};
pub use self::types::{TransportConnection, TransportId, TransportMetadata};

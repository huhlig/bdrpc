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

#![doc = include_str!("../../README.md")]
#![allow(clippy::module_inception)]
#![warn(missing_docs)]
#![warn(clippy::all)]
#![deny(unsafe_code)]

//! # BDRPC - Bi-Directional RPC Framework
//!
//! BDRPC is a high-performance, bi-directional RPC framework for Rust that provides:
//!
//! - **Bi-directional communication**: Both peers can initiate requests
//! - **Pluggable transports**: TCP, TLS, in-memory, and custom transports
//! - **Pluggable serialization**: Support for multiple formats (JSON, Postcard, rkyv)
//! - **Type-safe protocols**: Implement the [`Protocol`] trait for your message types
//! - **Robust error handling**: Three-layer error hierarchy with recovery strategies
//! - **Flow control**: Pluggable backpressure strategies
//! - **Reconnection**: Configurable strategy strategies with exponential backoff
//! - **Protocol evolution**: Version negotiation and feature flags
//! - **Observability**: Built-in metrics, tracing, and health checks
//!
//! ## Architecture
//!
//! BDRPC is organized into several layers:
//!
//! - **[`transport`]**: Low-level byte streams (TCP, TLS, memory)
//! - **[`serialization`]**: Message encoding/decoding with framing
//! - **[`channel`]**: Multiplexed, typed message passing
//! - **[`endpoint`]**: High-level connection management and protocol negotiation
//! - **[`backpressure`]**: Flow control strategies
//! - **[`strategy`]**: Automatic strategy with configurable strategies
//! - **[`observability`]**: Metrics, tracing, and health monitoring
//!
//! ## Quick Start
//!
//! ### Simple In-Memory Communication
//!
//! ```rust
//! use bdrpc::channel::{Channel, Protocol};
//! use std::collections::HashSet;
//!
//! // Define your protocol
//! #[derive(Debug, Clone, PartialEq)]
//! enum MyProtocol {
//!     Request { id: u32, types: String },
//!     Response { id: u32, result: String },
//! }
//!
//! impl Protocol for MyProtocol {
//!     fn method_name(&self) -> &'static str {
//!         match self {
//!             Self::Request { .. } => "request",
//!             Self::Response { .. } => "response",
//!         }
//!     }
//!
//!     fn is_request(&self) -> bool {
//!         matches!(self, Self::Request { .. })
//!     }
//! }
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use bdrpc::channel::ChannelId;
//! // Create a pair of connected channels
//! let (client, mut server) = Channel::<MyProtocol>::new_in_memory(
//!     ChannelId::new(),
//!     100, // buffer size
//! );
//!
//! // Send a request from client to server
//! client.send(MyProtocol::Request {
//!     id: 1,
//!     types: "Hello".to_string(),
//! }).await?;
//!
//! // Receive on server
//! let request = server.recv().await.unwrap();
//! println!("Server received: {:?}", request);
//!
//! // Send response back
//! client.send(MyProtocol::Response {
//!     id: 1,
//!     result: "World".to_string(),
//! }).await?;
//!
//! // Receive response on client (note: in real usage you'd have separate sender/receiver)
//! let response = server.recv().await.unwrap();
//! println!("Client received: {:?}", response);
//! # Ok(())
//! # }
//! ```
//!
//! ### Network Communication with Endpoint API
//!
//! ```rust,no_run
//! use bdrpc::endpoint::EndpointBuilder;
//! use bdrpc::serialization::JsonSerializer;
//! use bdrpc::channel::Protocol;
//!
//! #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
//! enum ChatProtocol {
//!     Message { from: String, text: String },
//!     Response { ok: bool },
//! }
//!
//! impl Protocol for ChatProtocol {
//!     fn method_name(&self) -> &'static str {
//!         match self {
//!             Self::Message { .. } => "message",
//!             Self::Response { .. } => "response",
//!         }
//!     }
//!
//!     fn is_request(&self) -> bool {
//!         matches!(self, Self::Message { .. })
//!     }
//! }
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create server endpoint with TCP listener using EndpointBuilder
//! let server = EndpointBuilder::server(JsonSerializer::default())
//!     .with_tcp_listener("127.0.0.1:8080")
//!     .with_bidirectional("ChatProtocol", 1)
//!     .build()
//!     .await?;
//!
//! // Create client endpoint with TCP caller
//! let mut client = EndpointBuilder::client(JsonSerializer::default())
//!     .with_tcp_caller("server", "127.0.0.1:8080")
//!     .with_bidirectional("ChatProtocol", 1)
//!     .build()
//!     .await?;
//!
//! // Connect to server via named transport
//! let connection = client.connect_transport("server").await?;
//!
//! // Get typed channels for communication
//! let (sender, receiver) = client
//!     .get_channels::<ChatProtocol>(connection.id(), "ChatProtocol")
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! ### Using the Builder Pattern (Recommended)
//!
//! The [`EndpointBuilder`] provides a more ergonomic way to create endpoints with
//! protocols and transports pre-configured:
//!
//! ```rust,no_run
//! use bdrpc::endpoint::EndpointBuilder;
//! use bdrpc::serialization::PostcardSerializer;
//! use bdrpc::channel::SystemProtocol;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a client endpoint with TCP transport and protocols
//! let mut client = EndpointBuilder::client(PostcardSerializer::default())
//!     .with_tcp_caller("backend", "127.0.0.1:8080")
//!     .with_caller("UserService", 1)
//!     .with_caller("OrderService", 1)
//!     .build()
//!     .await?;
//!
//! // Create a server endpoint with TCP listener
//! let server = EndpointBuilder::server(PostcardSerializer::default())
//!     .with_tcp_listener("127.0.0.1:8080")
//!     .with_responder("UserService", 1)
//!     .with_responder("OrderService", 1)
//!     .build()
//!     .await?;
//!
//! // Create a peer endpoint for bidirectional communication
//! let peer = EndpointBuilder::peer(PostcardSerializer::default())
//!     .with_tcp_listener("127.0.0.1:9090")
//!     .with_bidirectional("ChatProtocol", 1)
//!     .build()
//!     .await?;
//!
//! // Connect and get typed channels
//! let connection = client.connect_transport("backend").await?;
//! let (sender, receiver) = client
//!     .get_channels::<SystemProtocol>(connection.id(), "UserService")
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Examples
//!
//! The `examples/` directory contains several complete examples:
//!
//! ### Modern API Examples (Recommended)
//! - **`endpoint_builder`**: Demonstrates the EndpointBuilder pattern
//! - **`network_chat`**: Full network application with Endpoint API
//! - **`echo_server_manual`**: Server using `add_listener()` pattern
//! - **`multi_transport_server`**: Multiple transport types (TCP, WebSocket, QUIC)
//! - **`transport_failover`**: Automatic failover between transports
//!
//! ### Protocol Examples
//! - **`hello_world`**: Basic usage with all layers
//! - **`calculator`**: Bi-directional RPC over TCP
//! - **`chat_server`**: Multiple concurrent clients
//! - **`file_transfer`**: Streaming large files with progress tracking
//!
//! ### Channel Examples
//! - **`channel_basics`**: Channel-only communication patterns
//! - **`advanced_channels`**: Channel management and lifecycle
//! - **`dynamic_channels`**: Dynamic channel creation and negotiation
//!
//! Run an example with:
//! ```bash
//! cargo run --example endpoint_builder
//! cargo run --example network_chat
//! ```
//!
//! ## Features
//!
//! - **`derive`** (default): Enable procedural macros for service generation
//! - **`serde`** (default): Enable serde-based serialization (JSON, Postcard)
//! - **`rkyv`**: Enable zero-copy rkyv serialization
//! - **`observability`**: Enable metrics and tracing integration
//! - **`tls`**: Enable TLS transport support
//! - **`compression`**: Enable transport compression
//!
//! ## Performance
//!
//! BDRPC is designed for high performance:
//!
//! - **Throughput**: 4M+ messages/second (batch mode)
//! - **Latency**: 2-4 microseconds (p99)
//! - **Memory**: Minimal per-channel overhead
//! - **Zero-copy**: Where possible with rkyv serialization
//!
//! See `benches/` for detailed benchmarks.
//!
//! ## Error Handling
//!
//! BDRPC uses a three-layer error hierarchy:
//!
//! - [`TransportError`]: Low-level I/O and connection errors
//! - [`ChannelError`]: Channel-level errors (closed, timeout, etc.)
//! - [`BdrpcError`]: Top-level application errors
//!
//! All errors implement `std::error::Error` and provide detailed context.
//!
//! ## Safety
//!
//! BDRPC is written in 100% safe Rust with `#![deny(unsafe_code)]`.
//! All concurrency is handled through Tokio's async runtime.

pub mod backpressure;
pub mod channel;
pub mod endpoint;
pub mod error;
pub mod observability;
pub mod serialization;
pub mod transport;

// Re-export procedural macros when the derive feature is enabled
#[cfg(feature = "derive")]
pub use bdrpc_macros::service;

pub use backpressure::{BackpressureMetrics, BackpressureStrategy, BoundedQueue, Unlimited};
pub use channel::{Channel, ChannelError, ChannelId, ChannelManager, Protocol};
pub use endpoint::{Endpoint, EndpointBuilder, EndpointConfig, EndpointError, ProtocolDirection};
pub use error::BdrpcError;
pub use observability::{ChannelMetrics, ErrorMetrics, ErrorObserver, TransportMetrics, log_error};
pub use transport::{Transport, TransportError, TransportManager};

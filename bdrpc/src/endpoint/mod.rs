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

//! Endpoint layer for BDRPC.
//!
//! This module implements the main [`Endpoint`] struct that orchestrates all aspects
//! of bi-directional RPC communication, including transport management, channel
//! multiplexing, protocol negotiation, and connection lifecycle.
//!
//! # Overview
//!
//! The endpoint layer is the highest-level abstraction in BDRPC, providing:
//!
//! - **Connection Management**: Connect to peers and accept incoming connections
//! - **Protocol Registration**: Register protocols with directional capabilities
//! - **Channel Creation**: Create typed channels for type-safe communication
//! - **Handshake Protocol**: Negotiate capabilities and versions with peers
//! - **Bi-directional Support**: Act as client, server, or peer simultaneously
//! - **Reconnection**: Automatic reconnection with configurable strategies
//!
//! # Architecture
//!
//! The endpoint layer consists of several key components:
//!
//! ## Core Types
//!
//! - **[`Endpoint`]**: Main orchestrator for all communication
//! - **[`EndpointConfig`]**: Configuration for endpoint behavior
//! - **[`Connection`]**: Handle to an active connection
//! - **[`Listener`]**: Server-side connection acceptor
//!
//! ## Protocol Negotiation
//!
//! - **[`ProtocolDirection`]**: Specifies call/respond capabilities (per ADR-008)
//! - **[`ProtocolCapability`]**: Metadata about supported protocols
//! - **[`NegotiatedProtocol`]**: Result of successful protocol negotiation
//! - **[`HandshakeMessage`]**: Messages exchanged during connection setup
//!
//! ## Channel Negotiation
//!
//! - **[`ChannelNegotiator`]**: Trait for custom channel negotiation logic
//! - **[`DefaultChannelNegotiator`]**: Default implementation
//!
//! # Communication Patterns
//!
//! BDRPC supports multiple communication patterns through protocol directionality:
//!
//! ## Client-Server Pattern
//!
//! Traditional RPC where client calls methods and server responds:
//!
//! ```text
//! Client (CallOnly) ──────> Server (RespondOnly)
//!                   <──────
//! ```
//!
//! ## Server Push Pattern
//!
//! Server initiates calls to client:
//!
//! ```text
//! Client (RespondOnly) <────── Server (CallOnly)
//!                      ──────>
//! ```
//!
//! ## Peer-to-Peer Pattern
//!
//! Both sides can call and respond:
//!
//! ```text
//! Peer A (Bidirectional) <──────> Peer B (Bidirectional)
//! ```
//!
//! ## Hybrid Pattern
//!
//! Different protocols with different directions on the same connection:
//!
//! ```text
//! Endpoint A                    Endpoint B
//! - UserService (CallOnly)      - UserService (RespondOnly)
//! - Notifications (RespondOnly) - Notifications (CallOnly)
//! ```
//!
//! # Examples
//!
//! ## Simple Client-Server
//!
//! ```rust,no_run
//! use bdrpc::endpoint::{Endpoint, EndpointConfig};
//! use bdrpc::serialization::PostcardSerializer;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Client endpoint
//! let config = EndpointConfig::default();
//! let mut client = Endpoint::new(PostcardSerializer::default(), config);
//!
//! // Register as caller (can initiate requests)
//! client.register_caller("MyProtocol", 1).await?;
//!
//! // Connect to server
//! let connection = client.connect("127.0.0.1:8080").await?;
//! println!("Connected: {}", connection.id());
//! # Ok(())
//! # }
//! ```
//!
//! ## Server with Handler
//!
//! ```rust,no_run
//! use bdrpc::endpoint::{Endpoint, EndpointConfig};
//! use bdrpc::serialization::PostcardSerializer;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Server endpoint
//! let config = EndpointConfig::default();
//! let mut server = Endpoint::new(PostcardSerializer::default(), config);
//!
//! // Register as responder (can respond to requests)
//! server.register_responder("MyProtocol", 1).await?;
//!
//! // Listen for connections
//! let listener = server.listen("127.0.0.1:8080").await?;
//! println!("Listening on port 8080");
//! # Ok(())
//! # }
//! ```
//!
//! ## Peer-to-Peer Communication
//!
//! ```rust,no_run
//! use bdrpc::endpoint::{Endpoint, EndpointConfig};
//! use bdrpc::serialization::PostcardSerializer;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Both peers can send and receive
//! let config = EndpointConfig::default();
//! let mut peer = Endpoint::new(PostcardSerializer::default(), config);
//!
//! // Register as bidirectional (can both call and respond)
//! peer.register_bidirectional("ChatProtocol", 1).await?;
//!
//! // Connect to another peer
//! let connection = peer.connect("127.0.0.1:8080").await?;
//! println!("Connected to peer: {}", connection.id());
//! # Ok(())
//! # }
//! ```
//!
//! ## Multiple Protocols
//!
//! ```rust,no_run
//! use bdrpc::endpoint::{Endpoint, EndpointConfig};
//! use bdrpc::serialization::PostcardSerializer;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = EndpointConfig::default();
//! let mut endpoint = Endpoint::new(PostcardSerializer::default(), config);
//!
//! // Register multiple protocols with different directions
//! endpoint.register_caller("UserService", 1).await?;
//! endpoint.register_responder("NotificationService", 1).await?;
//!
//! let connection = endpoint.connect("127.0.0.1:8080").await?;
//! println!("Connected with multiple protocols");
//! # Ok(())
//! # }
//! ```
//!
//! ## Custom Configuration
//!
//! ```rust
//! use bdrpc::endpoint::EndpointConfig;
//! use bdrpc::reconnection::{ExponentialBackoff, ReconnectionStrategy};
//! use std::sync::Arc;
//! use std::time::Duration;
//!
//! let config = EndpointConfig::new()
//!     .with_channel_buffer_size(1000)
//!     .with_handshake_timeout(Duration::from_secs(10))
//!     .with_max_connections(Some(100))
//!     .with_max_frame_size(32 * 1024 * 1024)
//!     .with_frame_integrity(true)
//!     .with_endpoint_id("my-service".to_string())
//!     .with_reconnection_strategy(Arc::new(
//!         ExponentialBackoff::builder()
//!             .initial_delay(Duration::from_secs(1))
//!             .max_delay(Duration::from_secs(60))
//!             .multiplier(2.0)
//!             .max_attempts(Some(10))
//!             .build()
//!     ));
//! ```
//!
//! ## Automatic Reconnection
//!
//! ```rust,no_run
//! use bdrpc::endpoint::{Endpoint, EndpointConfig};
//! use bdrpc::serialization::PostcardSerializer;
//! use bdrpc::reconnection::ExponentialBackoff;
//! use std::sync::Arc;
//! use std::time::Duration;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = EndpointConfig::new()
//!     .with_reconnection_strategy(Arc::new(
//!         ExponentialBackoff::builder()
//!             .initial_delay(Duration::from_secs(1))
//!             .max_delay(Duration::from_secs(30))
//!             .multiplier(2.0)
//!             .max_attempts(Some(5))
//!             .build()
//!     ));
//!
//! let mut endpoint = Endpoint::new(PostcardSerializer::default(), config);
//!
//! // Connect with automatic reconnection
//! let connection = endpoint.connect_with_reconnection("127.0.0.1:8080").await?;
//!
//! // Connection will automatically reconnect on failure
//! # Ok(())
//! # }
//! ```
//!
//! # Protocol Negotiation
//!
//! During connection establishment, endpoints exchange capabilities and negotiate:
//!
//! 1. **Protocol Compatibility**: Which protocols both sides support
//! 2. **Version Selection**: Highest common version for each protocol
//! 3. **Feature Negotiation**: Common features (e.g., compression, streaming)
//! 4. **Direction Validation**: Ensure compatible call/respond capabilities
//!
//! ## Handshake Flow
//!
//! ```text
//! Client                          Server
//!   |                               |
//!   |--- Hello (capabilities) ----->|
//!   |                               |
//!   |<--- Hello (capabilities) -----|
//!   |                               |
//!   |--- Ack (negotiated) --------->|
//!   |                               |
//!   |<--- Ack (negotiated) ---------|
//!   |                               |
//!   |  Connection Ready             |
//! ```
//!
//! ## Version Negotiation
//!
//! The highest common version is selected:
//!
//! ```text
//! Client supports: [1, 2, 3]
//! Server supports: [2, 3, 4]
//! Negotiated: 3 (highest common)
//! ```
//!
//! ## Feature Negotiation
//!
//! Only features supported by both sides are enabled:
//!
//! ```text
//! Client features: [streaming, compression]
//! Server features: [streaming, encryption]
//! Negotiated: [streaming] (intersection)
//! ```
//!
//! # Error Handling
//!
//! The endpoint layer provides comprehensive error handling through [`EndpointError`]:
//!
//! ```rust,no_run
//! use bdrpc::endpoint::{Endpoint, EndpointError};
//! # use bdrpc::serialization::PostcardSerializer;
//! # use bdrpc::endpoint::EndpointConfig;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let mut endpoint = Endpoint::new(PostcardSerializer::default(), EndpointConfig::default());
//! match endpoint.connect("127.0.0.1:8080").await {
//!     Ok(connection) => {
//!         println!("Connected successfully");
//!     }
//!     Err(EndpointError::Transport(e)) => {
//!         eprintln!("Transport error: {}", e);
//!     }
//!     Err(e) => {
//!         eprintln!("Other error: {}", e);
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Performance Considerations
//!
//! ## Buffer Sizing
//!
//! - Larger channel buffers increase throughput but use more memory
//! - Default of 100 messages is suitable for most use cases
//! - Adjust based on message rate and processing time
//!
//! ## Connection Limits
//!
//! - Set `max_connections` to prevent resource exhaustion
//! - Monitor active connections and adjust limits as needed
//!
//! ## Frame Size
//!
//! - Default 16 MB limit prevents memory exhaustion
//! - Increase for large message scenarios (e.g., file transfer)
//! - Consider streaming for very large types
//!
//! ## Reconnection Strategy
//!
//! - Exponential backoff prevents connection storms
//! - Set reasonable max delays and attempt limits
//! - Consider circuit breaker for persistent failures
//!
//! # Thread Safety
//!
//! All endpoint types are thread-safe and can be shared across tasks:
//!
//! - [`Endpoint`] can be cloned and used from multiple tasks
//! - [`Connection`] handles can be shared and used concurrently
//! - Channel creation and usage is thread-safe
//!
//! # See Also
//!
//! - [ADR-008: Protocol Directionality](../adr/ADR-008-protocol-directionality.md)
//! - [ADR-010: Dynamic Channel Negotiation](../adr/ADR-010-dynamic-channel-negotiation.md)
//! - [`transport`](crate::transport) - Transport layer
//! - [`channel`](crate::channel) - Channel layer
//! - [`reconnection`](crate::reconnection) - Reconnection strategies

mod builder;
mod config;
mod direction;
#[allow(clippy::module_inception)]
mod endpoint;
mod error;
mod handshake;
mod negotiator;

pub use builder::EndpointBuilder;
pub use config::EndpointConfig;
pub use direction::ProtocolDirection;
pub use endpoint::{Connection, Endpoint, Listener};
pub use error::EndpointError;
pub use handshake::{
    HandshakeMessage, NegotiatedProtocol, ProtocolCapability, negotiate_protocols,
};
pub use negotiator::{ChannelNegotiator, DefaultChannelNegotiator};

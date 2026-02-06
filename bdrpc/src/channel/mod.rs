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

//! Channel layer for BDRPC.
//!
//! This module implements the channel abstraction that provides typed, multiplexed
//! communication over transports. Channels are the primary way to send and receive
//! messages in BDRPC.
//!
//! # Key Features
//!
//! - **Type Safety**: Each channel is typed to a specific [`Protocol`]
//! - **FIFO Ordering**: Messages within a channel are delivered in order (per ADR-007)
//! - **Multiplexing**: Multiple channels can share a single transport
//! - **Isolation**: Each channel is independent with its own message queue
//! - **Sequence Numbering**: Automatic sequence number assignment and verification
//! - **Backpressure**: Configurable flow control strategies
//! - **Timeouts**: Send and receive operations support timeouts
//!
//! # Architecture
//!
//! The channel layer consists of:
//!
//! - [`Channel`]: A typed channel for a specific protocol
//! - [`ChannelSender`]: Send half of a channel (can be cloned)
//! - [`ChannelReceiver`]: Receive half of a channel (single owner)
//! - [`ChannelId`]: Unique identifier for a channel
//! - [`ChannelManager`]: Manages multiple channels and routes messages
//! - [`Envelope`]: Wraps messages with metadata (sequence numbers, channel ID)
//! - [`Protocol`]: Trait that all message types must implement
//!
//! # The Protocol Trait
//!
//! All message types must implement the [`Protocol`] trait:
//!
//! ```rust
//! use bdrpc::channel::Protocol;
//!
//! #[derive(Debug, Clone)]
//! enum MyProtocol {
//!     Request { id: u32, data: String },
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
//! ```
//!
//! # Examples
//!
//! ## Basic In-Memory Channel
//!
//! ```rust,no_run
//! use bdrpc::channel::{Channel, ChannelId, Protocol};
//!
//! #[derive(Debug, Clone, PartialEq)]
//! enum MyProtocol {
//!     Ping,
//!     Pong,
//! }
//!
//! impl Protocol for MyProtocol {
//!     fn method_name(&self) -> &'static str {
//!         match self {
//!             Self::Ping => "ping",
//!             Self::Pong => "pong",
//!         }
//!     }
//!
//!     fn is_request(&self) -> bool {
//!         matches!(self, Self::Ping)
//!     }
//! }
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a pair of connected channels
//! let (client, mut server) = Channel::<MyProtocol>::new_in_memory(
//!     ChannelId::new(),
//!     100, // buffer size
//! );
//!
//! // Send from client to server
//! client.send(MyProtocol::Ping).await?;
//!
//! // Receive on server
//! let msg = server.recv().await.unwrap();
//! assert_eq!(msg, MyProtocol::Ping);
//! # Ok(())
//! # }
//! ```
//!
//! ## Channel with Timeout
//!
//! ```rust,no_run
//! use bdrpc::channel::{Channel, ChannelId, Protocol};
//! use std::time::Duration;
//!
//! # #[derive(Debug, Clone, PartialEq)]
//! # enum MyProtocol { Ping, Pong }
//! # impl Protocol for MyProtocol {
//! #     fn method_name(&self) -> &'static str { "ping" }
//! #     fn is_request(&self) -> bool { true }
//! # }
//! #
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let (client, mut server) = Channel::<MyProtocol>::new_in_memory(ChannelId::new(), 100);
//!
//! // Send with timeout
//! client.send_timeout(MyProtocol::Ping, Duration::from_secs(5)).await?;
//!
//! // Receive with timeout
//! match server.recv_timeout(Duration::from_millis(100)).await {
//!     Ok(msg) => println!("Received: {:?}", msg),
//!     Err(e) if e.is_timeout() => println!("Timed out"),
//!     Err(e) => return Err(e.into()),
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Splitting Channels
//!
//! ```rust,no_run
//! use bdrpc::channel::{Channel, ChannelId, Protocol};
//!
//! # #[derive(Debug, Clone, PartialEq)]
//! # enum MyProtocol { Ping, Pong }
//! # impl Protocol for MyProtocol {
//! #     fn method_name(&self) -> &'static str { "ping" }
//! #     fn is_request(&self) -> bool { true }
//! # }
//! #
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let (sender, mut receiver) = Channel::<MyProtocol>::new_in_memory(ChannelId::new(), 100);
//!
//! // Sender can be cloned and shared
//! let sender2 = sender.clone();
//!
//! // Spawn task with sender
//! tokio::spawn(async move {
//!     sender2.send(MyProtocol::Ping).await.ok();
//! });
//!
//! // Receive on main task
//! let msg = receiver.recv().await.unwrap();
//! # Ok(())
//! # }
//! ```
//!
//! ## Channel Manager
//!
//! ```rust
//! use bdrpc::channel::{ChannelManager, ChannelId, Protocol};
//!
//! # #[derive(Debug, Clone)]
//! # enum MyProtocol { Ping }
//! # impl Protocol for MyProtocol {
//! #     fn method_name(&self) -> &'static str { "ping" }
//! #     fn is_request(&self) -> bool { true }
//! # }
//! #
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let manager = ChannelManager::new();
//!
//! // Register a channel
//! let channel_id = ChannelId::new();
//! // manager.register_channel::<MyProtocol>(channel_id, channel).await?;
//!
//! // Get channel by ID
//! // let channel = manager.get_channel::<MyProtocol>(&channel_id).await?;
//!
//! // Remove channel
//! // manager.remove_channel(&channel_id).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # FIFO Ordering Guarantees
//!
//! Per ADR-007, BDRPC guarantees FIFO ordering within each channel:
//!
//! ```rust,no_run
//! # use bdrpc::channel::{Channel, ChannelId, Protocol};
//! # #[derive(Debug, Clone, PartialEq)]
//! # enum MyProtocol { Msg(u32) }
//! # impl Protocol for MyProtocol {
//! #     fn method_name(&self) -> &'static str { "msg" }
//! #     fn is_request(&self) -> bool { true }
//! # }
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let (sender, mut receiver) = Channel::<MyProtocol>::new_in_memory(ChannelId::new(), 100);
//!
//! // Send messages in order
//! sender.send(MyProtocol::Msg(1)).await?;
//! sender.send(MyProtocol::Msg(2)).await?;
//! sender.send(MyProtocol::Msg(3)).await?;
//!
//! // Receive in same order (guaranteed)
//! assert_eq!(receiver.recv().await.unwrap(), MyProtocol::Msg(1));
//! assert_eq!(receiver.recv().await.unwrap(), MyProtocol::Msg(2));
//! assert_eq!(receiver.recv().await.unwrap(), MyProtocol::Msg(3));
//! # Ok(())
//! # }
//! ```
//!
//! # Error Handling
//!
//! Channel operations return [`ChannelError`]:
//!
//! ```rust,no_run
//! use bdrpc::channel::{Channel, ChannelId, ChannelError, Protocol};
//!
//! # #[derive(Debug, Clone)]
//! # enum MyProtocol { Ping }
//! # impl Protocol for MyProtocol {
//! #     fn method_name(&self) -> &'static str { "ping" }
//! #     fn is_request(&self) -> bool { true }
//! # }
//! # async fn example() {
//! let (sender, receiver) = Channel::<MyProtocol>::new_in_memory(ChannelId::new(), 100);
//!
//! // Drop receiver to close channel
//! drop(receiver);
//!
//! // Send will fail with channel closed error
//! match sender.send(MyProtocol::Ping).await {
//!     Err(e) => {
//!         println!("Channel send failed: {}", e);
//!     }
//!     _ => {}
//! }
//! # }
//! ```
//!
//! # Performance
//!
//! Channels are designed for high performance:
//!
//! - **Low latency**: 2-4 microseconds for send/recv
//! - **High throughput**: 4M+ messages/second in batch mode
//! - **Minimal overhead**: 173 nanoseconds per channel creation
//! - **Zero-copy**: Where possible with appropriate serializers
//!
//! See the benchmarks in `benches/` for detailed performance metrics.

#[allow(clippy::module_inception)]
mod channel;
mod envelope;
mod error;
mod id;
mod manager;
mod system;

pub use channel::{Channel, ChannelReceiver, ChannelSender};
pub use envelope::Envelope;
pub use error::ChannelError;
pub use id::ChannelId;
pub use manager::ChannelManager;
pub use system::{SYSTEM_CHANNEL_ID, SystemProtocol};

/// Protocol trait that all generated protocol enums must implement.
///
/// This trait is automatically implemented by the `#[bdrpc::service]` macro.
/// It provides metadata about the protocol for routing and debugging.
pub trait Protocol: Send + Sync + 'static {
    /// Returns the name of the method this message is for.
    fn method_name(&self) -> &'static str;

    /// Returns true if this is a request message, false if it's a response.
    fn is_request(&self) -> bool;

    /// Returns the features required by this message's method.
    ///
    /// This is used for runtime feature validation to ensure that methods
    /// requiring specific features are only called when those features have
    /// been negotiated during handshake.
    fn required_features(&self) -> &'static [&'static str] {
        &[]
    }
}

#[cfg(test)]
mod tests;

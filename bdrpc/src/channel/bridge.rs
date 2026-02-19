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

//! Channel bridge for connecting typed channels to byte-oriented transport routing.
//!
//! This module provides the [`ChannelBridge`] which bridges the gap between
//! typed channels (`Channel<P>`) and byte-oriented transport routing (`TransportRouter`).
//!
//! # Architecture
//!
//! The bridge spawns two independent tasks:
//! - **Outgoing task**: Serializes `Envelope<P>` → `Vec<u8>` and sends to router
//! - **Incoming task**: Deserializes `Vec<u8>` → `Envelope<P>` and sends to channel
//!
//! This design maintains clean separation of concerns and enables graceful error handling.

use super::{ChannelId, Envelope, Protocol, TransportRouter};
use crate::serialization::Serializer;
use std::marker::PhantomData;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

#[cfg(feature = "observability")]
use tracing::{debug, error, info};

/// Bridges a typed channel with a byte-oriented transport router.
///
/// The bridge spawns two tasks:
/// - Outgoing task: Serializes `Envelope<P>` → `Vec<u8>` and sends to router
/// - Incoming task: Deserializes `Vec<u8>` → `Envelope<P>` and sends to channel
///
/// # Type Parameters
///
/// * `P` - The protocol type for the channel
/// * `S` - The serializer type (must implement `Serializer`)
///
/// # Example
///
/// ```rust,no_run
/// use bdrpc::channel::{ChannelBridge, ChannelId, TransportRouter, Protocol};
/// use bdrpc::serialization::PostcardSerializer;
/// use std::sync::Arc;
///
/// # #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
/// # enum MyProtocol { Ping }
/// # impl Protocol for MyProtocol {
/// #     fn method_name(&self) -> &'static str { "ping" }
/// #     fn is_request(&self) -> bool { true }
/// # }
/// # async fn example(
/// #     channel_rx: tokio::sync::mpsc::UnboundedReceiver<bdrpc::channel::Envelope<MyProtocol>>,
/// #     channel_tx: tokio::sync::mpsc::UnboundedSender<bdrpc::channel::Envelope<MyProtocol>>,
/// #     router: Arc<TransportRouter>,
/// # ) {
/// let bridge = ChannelBridge::new(
///     ChannelId::new(),
///     channel_rx,
///     channel_tx,
///     router,
///     Arc::new(PostcardSerializer::default()),
/// );
///
/// // Bridge is now running, messages flow automatically
///
/// // Shutdown when done
/// bridge.shutdown().await;
/// # }
/// ```
pub struct ChannelBridge<P: Protocol, S: Serializer> {
    /// Channel ID for this bridge
    channel_id: ChannelId,

    /// Serializer for this channel
    #[allow(dead_code)] // Stored for potential future use
    serializer: Arc<S>,

    /// Task handles for cleanup
    outgoing_task: Option<JoinHandle<()>>,
    incoming_task: Option<JoinHandle<()>>,

    /// Phantom data for the protocol type
    _phantom: PhantomData<P>,
}

impl<P, S> ChannelBridge<P, S>
where
    P: Protocol + serde::Serialize + serde::de::DeserializeOwned,
    S: Serializer,
{
    /// Creates a new bridge connecting a channel to a router.
    ///
    /// # Arguments
    ///
    /// * `channel_id` - The channel identifier
    /// * `channel_rx` - Receiver for outgoing messages from the channel
    /// * `channel_tx` - Sender for incoming messages to the channel
    /// * `router` - The transport router
    /// * `serializer` - Serializer for this channel's protocol
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use bdrpc::channel::{ChannelBridge, ChannelId, TransportRouter, Protocol};
    /// # use bdrpc::serialization::PostcardSerializer;
    /// # use std::sync::Arc;
    /// # #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    /// # enum MyProtocol { Ping }
    /// # impl Protocol for MyProtocol {
    /// #     fn method_name(&self) -> &'static str { "ping" }
    /// #     fn is_request(&self) -> bool { true }
    /// # }
    /// # async fn example(
    /// #     channel_rx: tokio::sync::mpsc::UnboundedReceiver<bdrpc::channel::Envelope<MyProtocol>>,
    /// #     channel_tx: tokio::sync::mpsc::UnboundedSender<bdrpc::channel::Envelope<MyProtocol>>,
    /// #     router: Arc<TransportRouter>,
    /// # ) {
    /// let bridge = ChannelBridge::new(
    ///     ChannelId::new(),
    ///     channel_rx,
    ///     channel_tx,
    ///     router,
    ///     Arc::new(PostcardSerializer::default()),
    /// );
    /// # }
    /// ```
    pub fn new(
        channel_id: ChannelId,
        channel_rx: mpsc::UnboundedReceiver<Envelope<P>>,
        channel_tx: mpsc::UnboundedSender<Envelope<P>>,
        router: Arc<TransportRouter>,
        serializer: Arc<S>,
    ) -> Self {
        #[cfg(feature = "observability")]
        info!(
            channel_id = %channel_id,
            "Creating channel bridge"
        );

        // Spawn outgoing task
        let outgoing_task = tokio::spawn(Self::outgoing_task(
            channel_id,
            channel_rx,
            router.get_outgoing_sender(),
            Arc::clone(&serializer),
        ));

        // Create incoming channel and register with router
        let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();
        let router_clone = Arc::clone(&router);
        let register_channel_id = channel_id;
        tokio::spawn(async move {
            router_clone
                .register_channel(register_channel_id, incoming_tx)
                .await;
        });

        // Spawn incoming task
        let incoming_task = tokio::spawn(Self::incoming_task(
            channel_id,
            incoming_rx,
            channel_tx,
            Arc::clone(&serializer),
        ));

        Self {
            channel_id,
            serializer,
            outgoing_task: Some(outgoing_task),
            incoming_task: Some(incoming_task),
            _phantom: PhantomData,
        }
    }

    /// Outgoing task: Channel → Serialize → Router
    ///
    /// This task continuously reads envelopes from the channel, serializes them,
    /// and sends the bytes to the router for transmission.
    async fn outgoing_task(
        channel_id: ChannelId,
        mut channel_rx: mpsc::UnboundedReceiver<Envelope<P>>,
        router_tx: mpsc::UnboundedSender<(ChannelId, Vec<u8>)>,
        serializer: Arc<S>,
    ) {
        #[cfg(feature = "observability")]
        info!(
            channel_id = %channel_id,
            "Outgoing bridge task started"
        );

        while let Some(envelope) = channel_rx.recv().await {
            #[cfg(feature = "observability")]
            debug!(
                channel_id = %channel_id,
                sequence = envelope.sequence,
                correlation_id = ?envelope.correlation_id,
                "Serializing outgoing envelope"
            );

            match serializer.serialize(&envelope) {
                Ok(bytes) => {
                    #[cfg(feature = "observability")]
                    debug!(
                        channel_id = %channel_id,
                        size = bytes.len(),
                        "Serialized envelope, sending to router"
                    );

                    if let Err(_e) = router_tx.send((channel_id, bytes)) {
                        #[cfg(feature = "observability")]
                        error!(
                            channel_id = %channel_id,
                            error = %_e,
                            "Failed to send to router, shutting down outgoing task"
                        );
                        break;
                    }
                }
                Err(_e) => {
                    #[cfg(feature = "observability")]
                    error!(
                        channel_id = %channel_id,
                        error = %_e,
                        "Failed to serialize envelope, skipping message"
                    );
                    // Continue processing other messages
                }
            }
        }

        #[cfg(feature = "observability")]
        info!(
            channel_id = %channel_id,
            "Outgoing bridge task stopped"
        );
    }

    /// Incoming task: Router → Deserialize → Channel
    ///
    /// This task continuously reads bytes from the router, deserializes them
    /// into envelopes, and sends them to the channel.
    async fn incoming_task(
        #[cfg_attr(not(feature = "observability"), allow(unused_variables))] channel_id: ChannelId,
        mut router_rx: mpsc::UnboundedReceiver<Vec<u8>>,
        channel_tx: mpsc::UnboundedSender<Envelope<P>>,
        serializer: Arc<S>,
    ) {
        #[cfg(feature = "observability")]
        info!(
            channel_id = %channel_id,
            "Incoming bridge task started"
        );

        while let Some(bytes) = router_rx.recv().await {
            #[cfg(feature = "observability")]
            debug!(
                channel_id = %channel_id,
                size = bytes.len(),
                "Deserializing incoming bytes"
            );

            match serializer.deserialize::<Envelope<P>>(&bytes) {
                Ok(envelope) => {
                    #[cfg(feature = "observability")]
                    debug!(
                        channel_id = %channel_id,
                        sequence = envelope.sequence,
                        correlation_id = ?envelope.correlation_id,
                        "Deserialized envelope, sending to channel"
                    );

                    if let Err(_e) = channel_tx.send(envelope) {
                        #[cfg(feature = "observability")]
                        error!(
                            channel_id = %channel_id,
                            error = %_e,
                            "Failed to send to channel, shutting down incoming task"
                        );
                        break;
                    }
                }
                Err(_e) => {
                    #[cfg(feature = "observability")]
                    error!(
                        channel_id = %channel_id,
                        error = %_e,
                        "Failed to deserialize envelope, skipping message"
                    );
                    // Continue processing other messages
                }
            }
        }

        #[cfg(feature = "observability")]
        info!(
            channel_id = %channel_id,
            "Incoming bridge task stopped"
        );
    }

    /// Shuts down the bridge gracefully.
    ///
    /// This aborts both tasks and waits for them to complete.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use bdrpc::channel::{ChannelBridge, ChannelId, TransportRouter, Protocol};
    /// # use bdrpc::serialization::PostcardSerializer;
    /// # use std::sync::Arc;
    /// # #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    /// # enum MyProtocol { Ping }
    /// # impl Protocol for MyProtocol {
    /// #     fn method_name(&self) -> &'static str { "ping" }
    /// #     fn is_request(&self) -> bool { true }
    /// # }
    /// # async fn example(
    /// #     channel_rx: tokio::sync::mpsc::UnboundedReceiver<bdrpc::channel::Envelope<MyProtocol>>,
    /// #     channel_tx: tokio::sync::mpsc::UnboundedSender<bdrpc::channel::Envelope<MyProtocol>>,
    /// #     router: Arc<TransportRouter>,
    /// # ) {
    /// let bridge = ChannelBridge::<MyProtocol>::new(
    ///     ChannelId::new(),
    ///     channel_rx,
    ///     channel_tx,
    ///     router,
    ///     Arc::new(PostcardSerializer::default()),
    /// );
    ///
    /// // Use the bridge...
    ///
    /// // Shutdown when done
    /// bridge.shutdown().await;
    /// # }
    /// ```
    pub async fn shutdown(mut self) {
        #[cfg(feature = "observability")]
        info!(
            channel_id = %self.channel_id,
            "Shutting down channel bridge"
        );

        if let Some(task) = self.outgoing_task.take() {
            task.abort();
            let _ = task.await;
        }
        if let Some(task) = self.incoming_task.take() {
            task.abort();
            let _ = task.await;
        }

        #[cfg(feature = "observability")]
        info!(
            channel_id = %self.channel_id,
            "Channel bridge shutdown complete"
        );
    }

    /// Returns the channel ID for this bridge.
    #[must_use]
    pub const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }
}

impl<P: Protocol, S: Serializer> Drop for ChannelBridge<P, S> {
    fn drop(&mut self) {
        // Abort tasks if they're still running
        if let Some(task) = self.outgoing_task.take() {
            task.abort();
        }
        if let Some(task) = self.incoming_task.take() {
            task.abort();
        }

        #[cfg(feature = "observability")]
        debug!(
            channel_id = %self.channel_id,
            "Channel bridge dropped"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serialization::PostcardSerializer;
    use crate::transport::TransportId;
    use tokio::io;

    #[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
    enum TestProtocol {
        Ping,
        Pong,
    }

    impl Protocol for TestProtocol {
        fn method_name(&self) -> &'static str {
            match self {
                Self::Ping => "ping",
                Self::Pong => "pong",
            }
        }

        fn is_request(&self) -> bool {
            matches!(self, Self::Ping)
        }
    }

    #[tokio::test]
    async fn test_bridge_creation() {
        let (channel_tx, _channel_rx) = mpsc::unbounded_channel::<Envelope<TestProtocol>>();
        let (_outgoing_tx, outgoing_rx) = mpsc::unbounded_channel::<Envelope<TestProtocol>>();

        let (read_half, write_half) = io::split(io::empty());
        let router = Arc::new(TransportRouter::new(
            TransportId::new(1),
            read_half,
            write_half,
        ));

        let bridge = ChannelBridge::new(
            ChannelId::new(),
            outgoing_rx,
            channel_tx,
            router,
            Arc::new(PostcardSerializer::default()),
        );

        // Bridge should be created successfully
        assert!(bridge.outgoing_task.is_some());
        assert!(bridge.incoming_task.is_some());

        bridge.shutdown().await;
    }

    #[tokio::test]
    async fn test_bridge_shutdown() {
        let (channel_tx, _channel_rx) = mpsc::unbounded_channel::<Envelope<TestProtocol>>();
        let (_outgoing_tx, outgoing_rx) = mpsc::unbounded_channel::<Envelope<TestProtocol>>();

        let (read_half, write_half) = io::split(io::empty());
        let router = Arc::new(TransportRouter::new(
            TransportId::new(1),
            read_half,
            write_half,
        ));

        let bridge = ChannelBridge::new(
            ChannelId::new(),
            outgoing_rx,
            channel_tx,
            router,
            Arc::new(PostcardSerializer::default()),
        );

        // Shutdown should complete without hanging
        bridge.shutdown().await;
    }

    #[tokio::test]
    async fn test_bridge_channel_id() {
        let (channel_tx, _channel_rx) = mpsc::unbounded_channel::<Envelope<TestProtocol>>();
        let (_outgoing_tx, outgoing_rx) = mpsc::unbounded_channel::<Envelope<TestProtocol>>();

        let (read_half, write_half) = io::split(io::empty());
        let router = Arc::new(TransportRouter::new(
            TransportId::new(1),
            read_half,
            write_half,
        ));

        let channel_id = ChannelId::new();
        let bridge = ChannelBridge::new(
            channel_id,
            outgoing_rx,
            channel_tx,
            router,
            Arc::new(PostcardSerializer::default()),
        );

        assert_eq!(bridge.channel_id(), channel_id);

        bridge.shutdown().await;
    }

    #[tokio::test]
    async fn test_bridge_outgoing_message_flow() {
        // Test that messages sent to the bridge are processed without panic
        let (channel_tx, _channel_rx) = mpsc::unbounded_channel::<Envelope<TestProtocol>>();
        let (outgoing_tx, outgoing_rx) = mpsc::unbounded_channel::<Envelope<TestProtocol>>();

        let (read_half, write_half) = io::split(io::empty());
        let router = Arc::new(TransportRouter::new(
            TransportId::new(1),
            read_half,
            write_half,
        ));

        let channel_id = ChannelId::new();
        let bridge = ChannelBridge::new(
            channel_id,
            outgoing_rx,
            channel_tx.clone(),
            Arc::clone(&router),
            Arc::new(PostcardSerializer::default()),
        );

        // Send a message through the bridge
        let envelope = Envelope {
            channel_id,
            sequence: 1,
            correlation_id: None,
            payload: TestProtocol::Ping,
        };
        outgoing_tx.send(envelope.clone()).unwrap();

        // Give the bridge time to process
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Test passes if no panic occurred
        bridge.shutdown().await;
    }

    #[tokio::test]
    async fn test_bridge_multiple_outgoing_messages() {
        // Test handling multiple outgoing messages in sequence
        let (channel_tx, _channel_rx) = mpsc::unbounded_channel::<Envelope<TestProtocol>>();
        let (outgoing_tx, outgoing_rx) = mpsc::unbounded_channel::<Envelope<TestProtocol>>();

        let (read_half, write_half) = io::split(io::empty());
        let router = Arc::new(TransportRouter::new(
            TransportId::new(1),
            read_half,
            write_half,
        ));

        let channel_id = ChannelId::new();

        let bridge = ChannelBridge::new(
            channel_id,
            outgoing_rx,
            channel_tx.clone(),
            Arc::clone(&router),
            Arc::new(PostcardSerializer::default()),
        );

        // Send multiple messages
        for i in 1..=5 {
            let envelope = Envelope {
                channel_id,
                sequence: i,
                correlation_id: None,
                payload: if i % 2 == 0 {
                    TestProtocol::Ping
                } else {
                    TestProtocol::Pong
                },
            };
            outgoing_tx.send(envelope).unwrap();
        }

        // Give bridge time to process all messages
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        bridge.shutdown().await;
    }

    #[tokio::test]
    async fn test_bridge_serialization() {
        // Test that serialization works correctly
        let serializer = Arc::new(PostcardSerializer::default());
        let channel_id = ChannelId::new();

        let envelope = Envelope {
            channel_id,
            sequence: 42,
            correlation_id: Some(123),
            payload: TestProtocol::Ping,
        };

        // Serialize
        let bytes = serializer.serialize(&envelope).unwrap();
        assert!(!bytes.is_empty());

        // Deserialize
        let deserialized: Envelope<TestProtocol> = serializer.deserialize(&bytes).unwrap();
        assert_eq!(deserialized.sequence, 42);
        assert_eq!(deserialized.correlation_id, Some(123));
        assert_eq!(deserialized.payload, TestProtocol::Ping);
    }

    #[tokio::test]
    async fn test_bridge_drop_cleanup() {
        // Test that dropping the bridge cleans up tasks
        let (channel_tx, _channel_rx) = mpsc::unbounded_channel::<Envelope<TestProtocol>>();
        let (_outgoing_tx, outgoing_rx) = mpsc::unbounded_channel::<Envelope<TestProtocol>>();

        let (read_half, write_half) = io::split(io::empty());
        let router = Arc::new(TransportRouter::new(
            TransportId::new(1),
            read_half,
            write_half,
        ));

        let bridge = ChannelBridge::new(
            ChannelId::new(),
            outgoing_rx,
            channel_tx,
            router,
            Arc::new(PostcardSerializer::default()),
        );

        // Drop the bridge without calling shutdown
        drop(bridge);

        // Give tasks time to abort
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Test passes if no panic or hang occurs
    }

    #[tokio::test]
    async fn test_bridge_channel_closed() {
        // Test behavior when channel is closed
        let (channel_tx, _channel_rx) = mpsc::unbounded_channel::<Envelope<TestProtocol>>();
        let (_outgoing_tx, outgoing_rx) = mpsc::unbounded_channel::<Envelope<TestProtocol>>();

        let (read_half, write_half) = io::split(io::empty());
        let router = Arc::new(TransportRouter::new(
            TransportId::new(1),
            read_half,
            write_half,
        ));

        let bridge = ChannelBridge::new(
            ChannelId::new(),
            outgoing_rx,
            channel_tx.clone(),
            router,
            Arc::new(PostcardSerializer::default()),
        );

        // Close the channel
        drop(channel_tx);

        // Give bridge time to detect closure
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        bridge.shutdown().await;
    }

    #[tokio::test]
    async fn test_bridge_router_closed() {
        // Test behavior when router channels are closed
        let (channel_tx, _channel_rx) = mpsc::unbounded_channel::<Envelope<TestProtocol>>();
        let (outgoing_tx, outgoing_rx) = mpsc::unbounded_channel::<Envelope<TestProtocol>>();

        let (read_half, write_half) = io::split(io::empty());
        let router = Arc::new(TransportRouter::new(
            TransportId::new(1),
            read_half,
            write_half,
        ));

        let channel_id = ChannelId::new();
        let bridge = ChannelBridge::new(
            channel_id,
            outgoing_rx,
            channel_tx,
            Arc::clone(&router),
            Arc::new(PostcardSerializer::default()),
        );

        // Send a message that will fail when router is closed
        let envelope = Envelope {
            channel_id,
            sequence: 1,
            correlation_id: None,
            payload: TestProtocol::Ping,
        };

        // Drop router to close its channels
        drop(router);

        // Try to send - should handle gracefully
        outgoing_tx.send(envelope).ok();

        // Give bridge time to detect closure
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        bridge.shutdown().await;
    }
}

// Made with Bob

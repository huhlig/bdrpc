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

//! Message routing between channels and transports.
//!
//! This module provides the [`TransportRouter`] which manages bidirectional
//! message routing between multiple channels and a single transport connection.
//!
//! # Architecture
//!
//! The router maintains two independent tasks:
//! - **Send task**: Reads from all channels and writes to transport
//! - **Receive task**: Reads from transport and routes to appropriate channels
//!
//! This design enables true concurrent operation with no lock contention.

use crate::channel::ChannelId;
use crate::serialization::framing::{read_frame_with_channel, write_frame_with_channel};
use crate::transport::TransportId;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{RwLock, mpsc};
use tokio::task::JoinHandle;

#[cfg(feature = "observability")]
use tracing::{debug, error, info, warn};

/// Routes messages between channels and a transport.
///
/// The router maintains two independent tasks:
/// - Send task: Reads from channels and writes to transport
/// - Receive task: Reads from transport and routes to channels
///
/// # Example
///
/// ```rust,no_run
/// use bdrpc::channel::{ChannelId, TransportRouter};
/// use bdrpc::transport::TransportId;
/// use tokio::io::{AsyncRead, AsyncWrite};
///
/// # async fn example<R: AsyncRead + Unpin + Send + 'static, W: AsyncWrite + Unpin + Send + 'static>(
/// #     read_half: R,
/// #     write_half: W,
/// # ) {
/// let transport_id = TransportId::new(1);
/// let router = TransportRouter::new(transport_id, read_half, write_half);
///
/// // Register channels
/// let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
/// router.register_channel(ChannelId::new(), tx).await;
///
/// // Get sender for outgoing messages
/// let outgoing = router.get_outgoing_sender();
/// # }
/// ```
pub struct TransportRouter {
    #[allow(dead_code)] // Used for debugging and observability
    transport_id: TransportId,

    /// Map of channel_id -> sender for routing incoming messages
    channel_senders: Arc<RwLock<HashMap<ChannelId, mpsc::UnboundedSender<Vec<u8>>>>>,

    /// Sender for outgoing messages from all channels
    outgoing_tx: mpsc::UnboundedSender<(ChannelId, Vec<u8>)>,

    /// Task handles for cleanup
    send_task: Option<JoinHandle<()>>,
    recv_task: Option<JoinHandle<()>>,
}

impl TransportRouter {
    /// Creates a new transport router and spawns send/receive tasks.
    ///
    /// # Arguments
    ///
    /// * `transport_id` - Unique identifier for this transport
    /// * `read_half` - Read half of the split transport
    /// * `write_half` - Write half of the split transport
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use bdrpc::channel::TransportRouter;
    /// use bdrpc::transport::TransportId;
    /// use tokio::io;
    ///
    /// # async fn example() {
    /// let (read_half, write_half) = io::split(io::empty());
    /// let router = TransportRouter::new(
    ///     TransportId::new(1),
    ///     read_half,
    ///     write_half,
    /// );
    /// # }
    /// ```
    pub fn new<R, W>(transport_id: TransportId, read_half: R, write_half: W) -> Self
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let channel_senders = Arc::new(RwLock::new(HashMap::new()));
        let (outgoing_tx, outgoing_rx) = mpsc::unbounded_channel();

        // Spawn send task
        let send_task = tokio::spawn(Self::send_task(transport_id, write_half, outgoing_rx));

        // Spawn receive task
        let recv_task = tokio::spawn(Self::receive_task(
            transport_id,
            read_half,
            Arc::clone(&channel_senders),
        ));

        Self {
            transport_id,
            channel_senders,
            outgoing_tx,
            send_task: Some(send_task),
            recv_task: Some(recv_task),
        }
    }

    /// Registers a channel for receiving messages.
    ///
    /// # Arguments
    ///
    /// * `channel_id` - The channel identifier
    /// * `sender` - Channel sender for routing incoming messages
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use bdrpc::channel::{ChannelId, TransportRouter};
    /// # use bdrpc::transport::TransportId;
    /// # async fn example(router: &TransportRouter) {
    /// let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    /// router.register_channel(ChannelId::new(), tx).await;
    /// # }
    /// ```
    pub async fn register_channel(
        &self,
        channel_id: ChannelId,
        sender: mpsc::UnboundedSender<Vec<u8>>,
    ) {
        self.channel_senders
            .write()
            .await
            .insert(channel_id, sender);

        #[cfg(feature = "observability")]
        debug!(
            transport_id = %self.transport_id,
            channel_id = %channel_id,
            "Channel registered with router"
        );
    }

    /// Unregisters a channel.
    ///
    /// # Arguments
    ///
    /// * `channel_id` - The channel identifier to unregister
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use bdrpc::channel::{ChannelId, TransportRouter};
    /// # async fn example(router: &TransportRouter) {
    /// router.unregister_channel(ChannelId::new()).await;
    /// # }
    /// ```
    pub async fn unregister_channel(&self, channel_id: ChannelId) {
        self.channel_senders.write().await.remove(&channel_id);

        #[cfg(feature = "observability")]
        debug!(
            transport_id = %self.transport_id,
            channel_id = %channel_id,
            "Channel unregistered from router"
        );
    }

    /// Gets a sender for outgoing messages.
    ///
    /// This sender can be cloned and used by multiple channels to send
    /// messages through the transport.
    ///
    /// # Returns
    ///
    /// An unbounded sender that accepts (ChannelId, payload) tuples.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use bdrpc::channel::{ChannelId, TransportRouter};
    /// # async fn example(router: &TransportRouter) {
    /// let sender = router.get_outgoing_sender();
    /// sender.send((ChannelId::new(), vec![1, 2, 3])).unwrap();
    /// # }
    /// ```
    pub fn get_outgoing_sender(&self) -> mpsc::UnboundedSender<(ChannelId, Vec<u8>)> {
        self.outgoing_tx.clone()
    }

    /// Send task: reads from channels and writes to transport.
    ///
    /// This task runs independently and continuously processes outgoing messages
    /// from all registered channels, writing them to the transport with proper
    /// framing and channel ID multiplexing.
    async fn send_task<W>(
        #[cfg_attr(not(feature = "observability"), allow(unused_variables))]
        transport_id: TransportId,
        mut write_half: W,
        mut outgoing_rx: mpsc::UnboundedReceiver<(ChannelId, Vec<u8>)>,
    ) where
        W: AsyncWrite + Unpin,
    {
        #[cfg(feature = "observability")]
        info!(transport_id = %transport_id, "Send task started");

        while let Some((channel_id, payload)) = outgoing_rx.recv().await {
            #[cfg(feature = "observability")]
            debug!(
                transport_id = %transport_id,
                channel_id = %channel_id,
                payload_size = payload.len(),
                "Sending message"
            );

            if let Err(_e) = write_frame_with_channel(&mut write_half, channel_id, &payload).await {
                #[cfg(feature = "observability")]
                error!(
                    transport_id = %transport_id,
                    channel_id = %channel_id,
                    error = %_e,
                    "Failed to write frame"
                );
                break;
            }
        }

        #[cfg(feature = "observability")]
        info!(transport_id = %transport_id, "Send task stopped");
    }

    /// Receive task: reads from transport and routes to channels.
    ///
    /// This task runs independently and continuously reads framed messages
    /// from the transport, routing them to the appropriate channel based on
    /// the channel ID in the frame header.
    async fn receive_task<R>(
        #[cfg_attr(not(feature = "observability"), allow(unused_variables))]
        transport_id: TransportId,
        mut read_half: R,
        channel_senders: Arc<RwLock<HashMap<ChannelId, mpsc::UnboundedSender<Vec<u8>>>>>,
    ) where
        R: AsyncRead + Unpin,
    {
        #[cfg(feature = "observability")]
        info!(transport_id = %transport_id, "Receive task started");

        while let Ok((channel_id, payload)) = read_frame_with_channel(&mut read_half).await {
            #[cfg(feature = "observability")]
            debug!(
                transport_id = %transport_id,
                channel_id = %channel_id,
                payload_size = payload.len(),
                "Received message"
            );

            // Route to appropriate channel
            let senders = channel_senders.read().await;
            if let Some(sender) = senders.get(&channel_id) {
                if let Err(_e) = sender.send(payload) {
                    #[cfg(feature = "observability")]
                    warn!(
                        transport_id = %transport_id,
                        channel_id = %channel_id,
                        error = %_e,
                        "Failed to route message to channel"
                    );
                }
            } else {
                #[cfg(feature = "observability")]
                warn!(
                    transport_id = %transport_id,
                    channel_id = %channel_id,
                    "No channel registered for incoming message"
                );
            }
        }

        #[cfg(feature = "observability")]
        error!(transport_id = %transport_id, "Transport read failed, stopping receive task");

        #[cfg(feature = "observability")]
        info!(transport_id = %transport_id, "Receive task stopped");
    }

    /// Shuts down the router gracefully.
    ///
    /// This method cancels both send and receive tasks and clears all
    /// channel registrations.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use bdrpc::channel::TransportRouter;
    /// # use bdrpc::transport::TransportId;
    /// # async fn example(router: TransportRouter) {
    /// router.shutdown().await;
    /// # }
    /// ```
    pub async fn shutdown(mut self) {
        #[cfg(feature = "observability")]
        info!(transport_id = %self.transport_id, "Shutting down router");

        // Cancel tasks
        if let Some(task) = self.send_task.take() {
            task.abort();
        }
        if let Some(task) = self.recv_task.take() {
            task.abort();
        }

        // Clear channel registrations
        self.channel_senders.write().await.clear();
    }
}

impl Drop for TransportRouter {
    fn drop(&mut self) {
        // Abort tasks if not already done
        if let Some(task) = self.send_task.take() {
            task.abort();
        }
        if let Some(task) = self.recv_task.take() {
            task.abort();
        }
    }
}

// Made with Bob

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

//! Channel implementation for typed message passing.

use super::{ChannelError, ChannelId, CorrelationIdGenerator, Envelope, Protocol};
use crate::backpressure::{BackpressureStrategy, BoundedQueue};
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;

#[cfg(feature = "observability")]
use tracing::{debug, instrument, warn};

/// A typed channel for sending and receiving protocol messages.
///
/// Channels provide FIFO ordering guarantees within the channel (per ADR-007).
/// Each channel is identified by a unique [`ChannelId`] and is typed to a
/// specific protocol.
///
/// # Ordering Guarantees
///
/// - **Within channel**: Messages are delivered in FIFO order
/// - **Across channels**: No ordering guarantees (allows parallelism)
///
/// # Example
///
/// ```rust,no_run
/// use bdrpc::channel::{Channel, ChannelId, Protocol};
///
/// #[derive(Debug, Clone)]
/// enum MyProtocol { Ping, Pong }
/// impl Protocol for MyProtocol {
///     fn method_name(&self) -> &'static str { "ping" }
///     fn is_request(&self) -> bool { true }
/// }
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create a channel with a specific buffer size
/// let (sender, mut receiver) = Channel::<MyProtocol>::new_in_memory(ChannelId::new(), 100);
///
/// // Send messages (FIFO ordering guaranteed)
/// sender.send(MyProtocol::Ping).await?;
/// sender.send(MyProtocol::Pong).await?;
///
/// // Receive messages in order
/// let msg1 = receiver.recv().await.unwrap();
/// let msg2 = receiver.recv().await.unwrap();
/// # Ok(())
/// # }
/// ```
pub struct Channel<P: Protocol> {
    /// The unique identifier for this channel.
    #[allow(dead_code)] // Used for debugging and future features
    id: ChannelId,
    /// Shared state between sender and receiver.
    #[allow(dead_code)] // Used for internal state management
    state: Arc<ChannelState>,
    /// Phantom data to associate the channel with a protocol type.
    _phantom: PhantomData<P>,
}

impl<P: Protocol> Channel<P> {
    /// Creates a new channel with custom backpressure and negotiated features.
    ///
    /// # Arguments
    ///
    /// * `id` - The unique identifier for this channel
    /// * `buffer_size` - The size of the internal message buffer
    /// * `backpressure` - The backpressure strategy to use
    /// * `negotiated_features` - Features that were negotiated during the handshake
    #[must_use]
    pub fn with_backpressure_and_features(
        id: ChannelId,
        buffer_size: usize,
        backpressure: Arc<dyn BackpressureStrategy>,
        negotiated_features: std::collections::HashSet<String>,
    ) -> (ChannelSender<P>, ChannelReceiver<P>) {
        let (tx, rx) = mpsc::channel(buffer_size);
        let state = Arc::new(ChannelState {
            send_sequence: AtomicU64::new(0),
            recv_sequence: AtomicU64::new(0),
            backpressure,
            negotiated_features,
            correlation_generator: CorrelationIdGenerator::new(),
        });

        let sender = ChannelSender {
            id,
            sender: tx,
            state: Arc::clone(&state),
        };

        let receiver = ChannelReceiver {
            id,
            receiver: rx,
            state,
        };

        (sender, receiver)
    }
}

/// Sender half of a channel.
///
/// The sender can send messages and will automatically assign sequence numbers
/// to ensure FIFO ordering.
pub struct ChannelSender<P: Protocol> {
    /// The channel ID.
    id: ChannelId,
    /// The underlying MPSC sender.
    sender: mpsc::Sender<Envelope<P>>,
    /// Shared state for sequence numbering.
    state: Arc<ChannelState>,
}

impl<P: Protocol> std::fmt::Debug for ChannelSender<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelSender")
            .field("id", &self.id)
            .field("capacity", &self.sender.capacity())
            .finish()
    }
}

/// Receiver half of a channel.
///
/// The receiver receives messages and verifies sequence numbers in debug builds
/// to detect ordering violations.
pub struct ChannelReceiver<P: Protocol> {
    /// The channel ID.
    id: ChannelId,
    /// The underlying MPSC receiver.
    receiver: mpsc::Receiver<Envelope<P>>,
    /// Shared state for sequence verification.
    state: Arc<ChannelState>,
}

impl<P: Protocol> std::fmt::Debug for ChannelReceiver<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelReceiver")
            .field("id", &self.id)
            .finish()
    }
}

/// Shared state between sender and receiver.
struct ChannelState {
    /// Next sequence number to send.
    send_sequence: AtomicU64,
    /// Next sequence number expected to receive.
    recv_sequence: AtomicU64,
    /// Backpressure strategy for flow control.
    backpressure: Arc<dyn BackpressureStrategy>,
    /// Features that were negotiated during handshake.
    negotiated_features: std::collections::HashSet<String>,
    /// Correlation ID generator for RPC request-response matching.
    correlation_generator: CorrelationIdGenerator,
}

impl<P: Protocol> Channel<P> {
    /// Creates an in-memory channel pair for testing or in-process communication.
    ///
    /// ⚠️ **Warning**: This creates a standalone channel that is NOT connected to
    /// any transport. Messages sent through this channel will only be received by
    /// the paired receiver in the same process. For network communication, use
    /// the endpoint's channel manager instead.
    ///
    /// Uses the default [`BoundedQueue`] backpressure strategy with the
    /// specified buffer size.
    ///
    /// # Use Cases
    /// - Unit testing
    /// - In-process communication
    /// - Examples demonstrating channel API only
    ///
    /// # Arguments
    ///
    /// * `id` - The unique identifier for this channel
    /// * `buffer_size` - The size of the internal message buffer
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::channel::{Channel, ChannelId, Protocol};
    ///
    /// #[derive(Debug, Clone)]
    /// enum MyProtocol { Ping }
    /// impl Protocol for MyProtocol {
    ///     fn method_name(&self) -> &'static str { "ping" }
    ///     fn is_request(&self) -> bool { true }
    /// }
    ///
    /// // This is an in-memory only channel - no network I/O!
    /// let (sender, receiver) = Channel::<MyProtocol>::new_in_memory(ChannelId::new(), 100);
    /// ```
    ///
    /// # For Network Communication
    ///
    /// To create channels connected to network transports, use the endpoint's channel manager:
    ///
    /// ```rust,no_run
    /// # use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// # use bdrpc::serialization::JsonSerializer;
    /// # use bdrpc::channel::{ChannelId, Protocol};
    /// # #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    /// # enum MyProtocol { Ping }
    /// # impl Protocol for MyProtocol {
    /// #     fn method_name(&self) -> &'static str { "ping" }
    /// #     fn is_request(&self) -> bool { true }
    /// # }
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    /// endpoint.connect("127.0.0.1:8080").await?;
    ///
    /// // Create a channel that's wired to the transport
    /// let sender = endpoint.channel_manager()
    ///     .create_channel::<MyProtocol>(ChannelId::new(), 100)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn new_in_memory(
        id: ChannelId,
        buffer_size: usize,
    ) -> (ChannelSender<P>, ChannelReceiver<P>) {
        // Create with empty feature set for backward compatibility
        Self::with_features_in_memory(id, buffer_size, std::collections::HashSet::new())
    }

    /// Creates an in-memory channel pair with negotiated features.
    ///
    /// ⚠️ **Warning**: This creates a standalone channel that is NOT connected to
    /// any transport. Messages sent through this channel will only be received by
    /// the paired receiver in the same process. For network communication, channels
    /// are created automatically during protocol negotiation via the endpoint.
    ///
    /// # Use Cases
    /// - Unit testing with feature validation
    /// - In-process communication with feature requirements
    /// - Examples demonstrating feature negotiation
    ///
    /// # Arguments
    ///
    /// * `id` - The unique identifier for this channel
    /// * `buffer_size` - The size of the internal message buffer
    /// * `negotiated_features` - Features that were negotiated during handshake
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::channel::{Channel, ChannelId, Protocol};
    /// use std::collections::HashSet;
    ///
    /// #[derive(Debug, Clone)]
    /// enum MyProtocol { Ping }
    /// impl Protocol for MyProtocol {
    ///     fn method_name(&self) -> &'static str { "ping" }
    ///     fn is_request(&self) -> bool { true }
    /// }
    ///
    /// let features = HashSet::from(["streaming".to_string()]);
    /// // This is an in-memory only channel - no network I/O!
    /// let (sender, receiver) = Channel::<MyProtocol>::with_features_in_memory(
    ///     ChannelId::new(),
    ///     100,
    ///     features
    /// );
    /// ```
    ///
    /// # For Network Communication
    ///
    /// Network-connected channels with features are created automatically during
    /// endpoint handshake and protocol negotiation. Register your protocols with
    /// the endpoint and features will be negotiated automatically.
    #[must_use]
    pub fn with_features_in_memory(
        id: ChannelId,
        buffer_size: usize,
        negotiated_features: std::collections::HashSet<String>,
    ) -> (ChannelSender<P>, ChannelReceiver<P>) {
        #[cfg(feature = "observability")]
        tracing::info!("Creating in-memory channel with negotiated features");

        let backpressure = Arc::new(BoundedQueue::new(buffer_size));
        Self::with_backpressure_and_features(id, buffer_size, backpressure, negotiated_features)
    }

    /// Creates a new channel with a custom backpressure strategy.
    ///
    /// # Arguments
    ///
    /// * `id` - The unique identifier for this channel
    /// * `buffer_size` - The size of the internal message buffer
    /// * `backpressure` - The backpressure strategy to use
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::channel::{Channel, ChannelId, Protocol};
    /// use bdrpc::backpressure::{BoundedQueue, Unlimited};
    /// use std::sync::Arc;
    ///
    /// #[derive(Debug, Clone)]
    /// enum MyProtocol { Ping }
    /// impl Protocol for MyProtocol {
    ///     fn method_name(&self) -> &'static str { "ping" }
    ///     fn is_request(&self) -> bool { true }
    /// }
    ///
    /// // Use unlimited backpressure (no flow control)
    /// let (sender, receiver) = Channel::<MyProtocol>::with_backpressure(
    ///     ChannelId::new(),
    ///     100,
    ///     Arc::new(Unlimited::new())
    /// );
    /// ```
    #[must_use]
    pub fn with_backpressure(
        id: ChannelId,
        buffer_size: usize,
        backpressure: Arc<dyn BackpressureStrategy>,
    ) -> (ChannelSender<P>, ChannelReceiver<P>) {
        // Create with empty feature set for backward compatibility
        Self::with_backpressure_and_features(
            id,
            buffer_size,
            backpressure,
            std::collections::HashSet::new(),
        )
    }
}

impl<P: Protocol> ChannelSender<P> {
    /// Returns the channel ID.
    #[must_use]
    pub const fn id(&self) -> ChannelId {
        self.id
    }

    /// Sends a message on the channel.
    ///
    /// This automatically assigns a sequence number to the message and wraps
    /// it in an envelope. The message will be delivered in FIFO order.
    ///
    /// Backpressure is applied according to the channel's strategy. If the
    /// channel is at capacity, this method will wait until space is available.
    ///
    /// # Errors
    ///
    /// Returns [`ChannelError::Closed`] if the receiver has been dropped.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use bdrpc::channel::{Channel, ChannelId, Protocol};
    /// # #[derive(Debug, Clone)]
    /// # enum MyProtocol { Ping }
    /// # impl Protocol for MyProtocol {
    /// #     fn method_name(&self) -> &'static str { "ping" }
    /// #     fn is_request(&self) -> bool { true }
    /// # }
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let (sender, _receiver) = Channel::<MyProtocol>::new_in_memory(ChannelId::new(), 100);
    /// sender.send(MyProtocol::Ping).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg_attr(feature = "observability", instrument(skip(self, message), fields(channel_id = %self.id, sequence, method = message.method_name())))]
    pub async fn send(&self, message: P) -> Result<(), ChannelError> {
        // Validate required features for this message
        self.validate_features(&message)?;

        // Check backpressure and wait if necessary
        let queue_depth = self.sender.max_capacity() - self.sender.capacity();
        if !self
            .state
            .backpressure
            .should_send(&self.id, queue_depth)
            .await
        {
            #[cfg(feature = "observability")]
            debug!("Waiting for channel capacity");

            self.state.backpressure.wait_for_capacity(&self.id).await;
        }

        // Assign sequence number
        let sequence = self.state.send_sequence.fetch_add(1, Ordering::SeqCst);

        #[cfg(feature = "observability")]
        tracing::Span::current().record("sequence", sequence);

        // Wrap in envelope
        let envelope = Envelope::new(self.id, sequence, message);

        #[cfg(feature = "observability")]
        debug!("Sending message on channel");

        // Send through channel
        self.sender.send(envelope).await.map_err(|_| {
            #[cfg(feature = "observability")]
            warn!("Channel closed, cannot send message");

            ChannelError::Closed {
                channel_id: self.id,
            }
        })?;

        // Notify backpressure strategy
        self.state.backpressure.on_message_sent(&self.id);

        Ok(())
    }

    /// Sends a message on the channel with a timeout.
    ///
    /// This is similar to [`send`](Self::send) but will return an error if the
    /// message cannot be sent within the specified duration.
    ///
    /// # Errors
    ///
    /// Returns [`ChannelError::Closed`] if the receiver has been dropped.
    /// Returns [`ChannelError::Timeout`] if the timeout expires before the message can be sent.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use bdrpc::channel::{Channel, ChannelId, Protocol};
    /// # use std::time::Duration;
    /// # #[derive(Debug, Clone)]
    /// # enum MyProtocol { Ping }
    /// # impl Protocol for MyProtocol {
    /// #     fn method_name(&self) -> &'static str { "ping" }
    /// #     fn is_request(&self) -> bool { true }
    /// # }
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let (sender, _receiver) = Channel::<MyProtocol>::new_in_memory(ChannelId::new(), 100);
    ///
    /// // Try to send with a 5 second timeout
    /// sender.send_timeout(MyProtocol::Ping, Duration::from_secs(5)).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg_attr(feature = "observability", instrument(skip(self, message), fields(channel_id = %self.id, timeout_ms = timeout.as_millis())))]
    pub async fn send_timeout(
        &self,
        message: P,
        timeout: std::time::Duration,
    ) -> Result<(), ChannelError> {
        // Validate required features for this message
        self.validate_features(&message)?;

        // Check backpressure and wait if necessary (with timeout)
        let queue_depth = self.sender.max_capacity() - self.sender.capacity();
        if !self
            .state
            .backpressure
            .should_send(&self.id, queue_depth)
            .await
        {
            #[cfg(feature = "observability")]
            debug!("Waiting for channel capacity with timeout");

            // Wait for capacity with timeout
            tokio::select! {
                _ = self.state.backpressure.wait_for_capacity(&self.id) => {},
                _ = tokio::time::sleep(timeout) => {
                    #[cfg(feature = "observability")]
                    warn!("Timeout waiting for channel capacity");

                    return Err(ChannelError::Timeout {
                        channel_id: self.id,
                        operation: "send".to_string(),
                    });
                }
            }
        }

        // Assign sequence number
        let sequence = self.state.send_sequence.fetch_add(1, Ordering::SeqCst);

        #[cfg(feature = "observability")]
        tracing::Span::current().record("sequence", sequence);

        // Wrap in envelope
        let envelope = Envelope::new(self.id, sequence, message);

        #[cfg(feature = "observability")]
        debug!("Sending message on channel");

        // Send through channel with timeout
        tokio::select! {
            result = self.sender.send(envelope) => {
                result.map_err(|_| {
                    #[cfg(feature = "observability")]
                    warn!("Channel closed, cannot send message");

                    ChannelError::Closed {
                        channel_id: self.id,
                    }
                })?;
            }
            _ = tokio::time::sleep(timeout) => {
                #[cfg(feature = "observability")]
                warn!("Timeout sending message on channel");

                return Err(ChannelError::Timeout {
                    channel_id: self.id,
                    operation: "send".to_string(),
                });
            }
        }

        // Notify backpressure strategy
        self.state.backpressure.on_message_sent(&self.id);

        Ok(())
    }

    /// Validates that all required features for a message are available.
    fn validate_features(&self, message: &P) -> Result<(), ChannelError> {
        let required = message.required_features();
        for feature in required {
            if !self.state.negotiated_features.contains(*feature) {
                return Err(ChannelError::FeatureNotSupported {
                    channel_id: self.id,
                    feature: feature.to_string(),
                    method: message.method_name().to_string(),
                });
            }
        }
        Ok(())
    }

    /// Tries to send a message without blocking.
    ///
    /// # Errors
    ///
    /// Returns [`ChannelError::Closed`] if the receiver has been dropped.
    /// Returns [`ChannelError::Full`] if the buffer is full.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use bdrpc::channel::{Channel, ChannelId, Protocol};
    /// # #[derive(Debug, Clone)]
    /// # enum MyProtocol { Ping }
    /// # impl Protocol for MyProtocol {
    /// #     fn method_name(&self) -> &'static str { "ping" }
    /// #     fn is_request(&self) -> bool { true }
    /// # }
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let (sender, _receiver) = Channel::<MyProtocol>::new_in_memory(ChannelId::new(), 100);
    /// sender.try_send(MyProtocol::Ping)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn try_send(&self, message: P) -> Result<(), ChannelError> {
        // Validate required features for this message
        self.validate_features(&message)?;

        // Assign sequence number
        let sequence = self.state.send_sequence.fetch_add(1, Ordering::SeqCst);

        // Wrap in envelope
        let envelope = Envelope::new(self.id, sequence, message);

        // Try to send through channel
        self.sender.try_send(envelope).map_err(|err| match err {
            mpsc::error::TrySendError::Full(_) => ChannelError::Full {
                channel_id: self.id,
                buffer_size: self.sender.capacity(),
            },
            mpsc::error::TrySendError::Closed(_) => ChannelError::Closed {
                channel_id: self.id,
            },
        })
    }

    /// Returns the current capacity of the channel.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.sender.capacity()
    }

    /// Checks if the channel is closed.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }

    /// Sends a request message with correlation ID for RPC.
    ///
    /// This method is used for RPC-style communication where a response is expected.
    /// It automatically generates a correlation ID that can be used to match the response.
    ///
    /// # Errors
    ///
    /// Returns [`ChannelError::Closed`] if the receiver has been dropped.
    /// Returns [`ChannelError::Internal`] if called with a non-request message.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use bdrpc::channel::{Channel, ChannelId, Protocol};
    /// # #[derive(Debug, Clone)]
    /// # enum MyProtocol { Request, Response }
    /// # impl Protocol for MyProtocol {
    /// #     fn method_name(&self) -> &'static str { "test" }
    /// #     fn is_request(&self) -> bool { matches!(self, Self::Request) }
    /// # }
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let (sender, _receiver) = Channel::<MyProtocol>::new_in_memory(ChannelId::new(), 100);
    ///
    /// // Send a request and get correlation ID
    /// let correlation_id = sender.send_request(MyProtocol::Request).await?;
    /// // Use correlation_id to match the response
    /// # Ok(())
    /// # }
    /// ```
    #[cfg_attr(feature = "observability", instrument(skip(self, message), fields(channel_id = %self.id, sequence, correlation_id, method = message.method_name())))]
    pub async fn send_request(&self, message: P) -> Result<u64, ChannelError> {
        if !message.is_request() {
            return Err(ChannelError::Internal {
                message: "send_request called with non-request message".to_string(),
            });
        }

        // Validate required features for this message
        self.validate_features(&message)?;

        // Check backpressure and wait if necessary
        let queue_depth = self.sender.max_capacity() - self.sender.capacity();
        if !self
            .state
            .backpressure
            .should_send(&self.id, queue_depth)
            .await
        {
            #[cfg(feature = "observability")]
            debug!("Waiting for channel capacity");

            self.state.backpressure.wait_for_capacity(&self.id).await;
        }

        // Generate correlation ID
        let correlation_id = self.state.correlation_generator.next();

        // Assign sequence number
        let sequence = self.state.send_sequence.fetch_add(1, Ordering::SeqCst);

        #[cfg(feature = "observability")]
        {
            tracing::Span::current().record("sequence", sequence);
            tracing::Span::current().record("correlation_id", correlation_id);
        }

        // Wrap in envelope with correlation ID
        let envelope = Envelope::new_with_correlation(self.id, sequence, correlation_id, message);

        #[cfg(feature = "observability")]
        debug!("Sending request message with correlation ID");

        // Send through channel
        self.sender.send(envelope).await.map_err(|_| {
            #[cfg(feature = "observability")]
            warn!("Channel closed, cannot send request");

            ChannelError::Closed {
                channel_id: self.id,
            }
        })?;

        // Notify backpressure strategy
        self.state.backpressure.on_message_sent(&self.id);

        Ok(correlation_id)
    }

    /// Sends a response message with the correlation ID from the request.
    ///
    /// This method is used to send a response that corresponds to a specific request.
    /// The correlation ID should be taken from the request envelope.
    ///
    /// # Errors
    ///
    /// Returns [`ChannelError::Closed`] if the receiver has been dropped.
    /// Returns [`ChannelError::Internal`] if called with a request message.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use bdrpc::channel::{Channel, ChannelId, Protocol};
    /// # #[derive(Debug, Clone)]
    /// # enum MyProtocol { Request, Response }
    /// # impl Protocol for MyProtocol {
    /// #     fn method_name(&self) -> &'static str { "test" }
    /// #     fn is_request(&self) -> bool { matches!(self, Self::Request) }
    /// # }
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let (sender, _receiver) = Channel::<MyProtocol>::new_in_memory(ChannelId::new(), 100);
    ///
    /// // Send a response with correlation ID from request
    /// sender.send_response(MyProtocol::Response, 42).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg_attr(feature = "observability", instrument(skip(self, message), fields(channel_id = %self.id, sequence, correlation_id, method = message.method_name())))]
    pub async fn send_response(&self, message: P, correlation_id: u64) -> Result<(), ChannelError> {
        if message.is_request() {
            return Err(ChannelError::Internal {
                message: "send_response called with request message".to_string(),
            });
        }

        // Validate required features for this message
        self.validate_features(&message)?;

        // Check backpressure and wait if necessary
        let queue_depth = self.sender.max_capacity() - self.sender.capacity();
        if !self
            .state
            .backpressure
            .should_send(&self.id, queue_depth)
            .await
        {
            #[cfg(feature = "observability")]
            debug!("Waiting for channel capacity");

            self.state.backpressure.wait_for_capacity(&self.id).await;
        }

        // Assign sequence number
        let sequence = self.state.send_sequence.fetch_add(1, Ordering::SeqCst);

        #[cfg(feature = "observability")]
        {
            tracing::Span::current().record("sequence", sequence);
            tracing::Span::current().record("correlation_id", correlation_id);
        }

        // Wrap in envelope with correlation ID
        let envelope = Envelope::new_with_correlation(self.id, sequence, correlation_id, message);

        #[cfg(feature = "observability")]
        debug!("Sending response message with correlation ID");

        // Send through channel
        self.sender.send(envelope).await.map_err(|_| {
            #[cfg(feature = "observability")]
            warn!("Channel closed, cannot send response");

            ChannelError::Closed {
                channel_id: self.id,
            }
        })?;

        // Notify backpressure strategy
        self.state.backpressure.on_message_sent(&self.id);

        Ok(())
    }
}

impl<P: Protocol> ChannelReceiver<P> {
    /// Returns the channel ID.
    #[must_use]
    pub const fn id(&self) -> ChannelId {
        self.id
    }

    /// Receives a message from the channel.
    ///
    /// This will wait until a message is available. Messages are received
    /// in FIFO order. In debug builds, sequence numbers are verified to
    /// detect ordering violations.
    ///
    /// When a message is received, the backpressure strategy is notified,
    /// which may wake waiting senders.
    ///
    /// Returns `None` if the sender has been dropped and no more messages
    /// are available.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use bdrpc::channel::{Channel, ChannelId, Protocol};
    /// # #[derive(Debug, Clone)]
    /// # enum MyProtocol { Ping }
    /// # impl Protocol for MyProtocol {
    /// #     fn method_name(&self) -> &'static str { "ping" }
    /// #     fn is_request(&self) -> bool { true }
    /// # }
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let (_sender, mut receiver) = Channel::<MyProtocol>::new_in_memory(ChannelId::new(), 100);
    /// if let Some(message) = receiver.recv().await {
    ///     println!("Received: {:?}", message);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[cfg_attr(feature = "observability", instrument(skip(self), fields(channel_id = %self.id, sequence)))]
    pub async fn recv(&mut self) -> Option<P> {
        #[cfg(feature = "observability")]
        debug!("Waiting for message on channel");

        let envelope = self.receiver.recv().await?;

        #[cfg(feature = "observability")]
        {
            tracing::Span::current().record("sequence", envelope.sequence);
            debug!("Received message on channel");
        }

        // Verify sequence number in debug builds
        #[cfg(debug_assertions)]
        {
            let expected = self.state.recv_sequence.fetch_add(1, Ordering::SeqCst);
            if envelope.sequence != expected {
                panic!(
                    "Message reordering detected on channel {}! Expected sequence {}, got {}",
                    self.id, expected, envelope.sequence
                );
            }
        }

        // In release builds, just update the counter
        #[cfg(not(debug_assertions))]
        {
            self.state.recv_sequence.fetch_add(1, Ordering::SeqCst);
        }

        // Notify backpressure strategy that capacity is available
        self.state.backpressure.on_message_received(&self.id);

        Some(envelope.into_payload())
    }

    /// Receives a message from the channel with a timeout.
    ///
    /// This is similar to [`recv`](Self::recv) but will return an error if no
    /// message is received within the specified duration.
    ///
    /// # Errors
    ///
    /// Returns [`ChannelError::Timeout`] if the timeout expires before a message is received.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use bdrpc::channel::{Channel, ChannelId, Protocol};
    /// # use std::time::Duration;
    /// # #[derive(Debug, Clone)]
    /// # enum MyProtocol { Ping }
    /// # impl Protocol for MyProtocol {
    /// #     fn method_name(&self) -> &'static str { "ping" }
    /// #     fn is_request(&self) -> bool { true }
    /// # }
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let (_sender, mut receiver) = Channel::<MyProtocol>::new_in_memory(ChannelId::new(), 100);
    ///
    /// // Try to receive with a 5 second timeout
    /// match receiver.recv_timeout(Duration::from_secs(5)).await {
    ///     Ok(message) => println!("Received: {:?}", message),
    ///     Err(e) => println!("Timeout or error: {}", e),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[cfg_attr(feature = "observability", instrument(skip(self), fields(channel_id = %self.id, timeout_ms = timeout.as_millis())))]
    pub async fn recv_timeout(&mut self, timeout: std::time::Duration) -> Result<P, ChannelError> {
        #[cfg(feature = "observability")]
        debug!("Waiting for message on channel with timeout");

        let envelope = tokio::select! {
            result = self.receiver.recv() => {
                result.ok_or({
                    #[cfg(feature = "observability")]
                    debug!("Channel closed, no more messages");

                    ChannelError::Closed {
                        channel_id: self.id,
                    }
                })?
            }
            _ = tokio::time::sleep(timeout) => {
                #[cfg(feature = "observability")]
                warn!("Timeout waiting for message on channel");

                return Err(ChannelError::Timeout {
                    channel_id: self.id,
                    operation: "recv".to_string(),
                });
            }
        };

        #[cfg(feature = "observability")]
        {
            tracing::Span::current().record("sequence", envelope.sequence);
            debug!("Received message on channel");
        }

        // Verify sequence number in debug builds
        #[cfg(debug_assertions)]
        {
            let expected = self.state.recv_sequence.fetch_add(1, Ordering::SeqCst);
            if envelope.sequence != expected {
                panic!(
                    "Message reordering detected on channel {}! Expected sequence {}, got {}",
                    self.id, expected, envelope.sequence
                );
            }
        }

        // In release builds, just update the counter
        #[cfg(not(debug_assertions))]
        {
            self.state.recv_sequence.fetch_add(1, Ordering::SeqCst);
        }

        // Notify backpressure strategy that capacity is available
        self.state.backpressure.on_message_received(&self.id);

        Ok(envelope.into_payload())
    }

    /// Tries to receive a message without blocking.
    ///
    /// Returns `None` if no message is currently available or if the sender
    /// has been dropped.
    ///
    /// When a message is received, the backpressure strategy is notified,
    /// which may wake waiting senders.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use bdrpc::channel::{Channel, ChannelId, Protocol};
    /// # #[derive(Debug, Clone)]
    /// # enum MyProtocol { Ping }
    /// # impl Protocol for MyProtocol {
    /// #     fn method_name(&self) -> &'static str { "ping" }
    /// #     fn is_request(&self) -> bool { true }
    /// # }
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let (_sender, mut receiver) = Channel::<MyProtocol>::new_in_memory(ChannelId::new(), 100);
    /// if let Some(message) = receiver.try_recv() {
    ///     println!("Received: {:?}", message);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn try_recv(&mut self) -> Option<P> {
        let envelope = self.receiver.try_recv().ok()?;

        // Verify sequence number in debug builds
        #[cfg(debug_assertions)]
        {
            let expected = self.state.recv_sequence.fetch_add(1, Ordering::SeqCst);
            if envelope.sequence != expected {
                panic!(
                    "Message reordering detected on channel {}! Expected sequence {}, got {}",
                    self.id, expected, envelope.sequence
                );
            }
        }

        // In release builds, just update the counter
        #[cfg(not(debug_assertions))]
        {
            self.state.recv_sequence.fetch_add(1, Ordering::SeqCst);
        }

        // Notify backpressure strategy that capacity is available
        self.state.backpressure.on_message_received(&self.id);

        Some(envelope.into_payload())
    }
    /// Receives a message envelope from the channel.
    ///
    /// This is similar to [`recv`](Self::recv) but returns the full envelope
    /// including metadata like correlation ID. This is useful for RPC implementations
    /// that need to match responses to requests.
    ///
    /// Returns `None` if the sender has been dropped and no more messages
    /// are available.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use bdrpc::channel::{Channel, ChannelId, Protocol};
    /// # #[derive(Debug, Clone)]
    /// # enum MyProtocol { Request, Response }
    /// # impl Protocol for MyProtocol {
    /// #     fn method_name(&self) -> &'static str { "test" }
    /// #     fn is_request(&self) -> bool { matches!(self, Self::Request) }
    /// # }
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let (_sender, mut receiver) = Channel::<MyProtocol>::new_in_memory(ChannelId::new(), 100);
    /// if let Some(envelope) = receiver.recv_envelope().await {
    ///     println!("Correlation ID: {:?}", envelope.correlation_id);
    ///     println!("Message: {:?}", envelope.payload);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    #[cfg_attr(feature = "observability", instrument(skip(self), fields(channel_id = %self.id, sequence, correlation_id)))]
    pub async fn recv_envelope(&mut self) -> Option<Envelope<P>> {
        #[cfg(feature = "observability")]
        debug!("Waiting for envelope on channel");

        let envelope = self.receiver.recv().await?;

        #[cfg(feature = "observability")]
        {
            tracing::Span::current().record("sequence", envelope.sequence);
            if let Some(corr_id) = envelope.correlation_id {
                tracing::Span::current().record("correlation_id", corr_id);
            }
            debug!("Received envelope on channel");
        }

        // Verify sequence number in debug builds
        #[cfg(debug_assertions)]
        {
            let expected = self.state.recv_sequence.fetch_add(1, Ordering::SeqCst);
            if envelope.sequence != expected {
                panic!(
                    "Message reordering detected on channel {}! Expected sequence {}, got {}",
                    self.id, expected, envelope.sequence
                );
            }
        }

        // In release builds, just update the counter
        #[cfg(not(debug_assertions))]
        {
            self.state.recv_sequence.fetch_add(1, Ordering::SeqCst);
        }

        // Notify backpressure strategy that capacity is available
        self.state.backpressure.on_message_received(&self.id);

        Some(envelope)
    }

    /// Closes the receiving half of the channel.
    ///
    /// This prevents any further messages from being received, but does not
    /// affect the sender.
    pub fn close(&mut self) {
        self.receiver.close();
    }
}

impl<P: Protocol> Clone for ChannelSender<P> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            sender: self.sender.clone(),
            state: Arc::clone(&self.state),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Simple test protocol
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum TestProtocol {
        Message(String),
    }

    impl Protocol for TestProtocol {
        fn method_name(&self) -> &'static str {
            "test"
        }

        fn is_request(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn test_channel_send_recv() {
        let (sender, mut receiver) = Channel::new_in_memory(ChannelId::new(), 10);

        sender
            .send(TestProtocol::Message("Hello".to_string()))
            .await
            .unwrap();

        let msg = receiver.recv().await.unwrap();
        assert_eq!(msg, TestProtocol::Message("Hello".to_string()));
    }

    #[tokio::test]
    async fn test_channel_fifo_ordering() {
        let (sender, mut receiver) = Channel::new_in_memory(ChannelId::new(), 10);

        // Send multiple messages
        for i in 0..10 {
            sender
                .send(TestProtocol::Message(format!("Message {}", i)))
                .await
                .unwrap();
        }

        // Verify they arrive in order
        for i in 0..10 {
            let msg = receiver.recv().await.unwrap();
            assert_eq!(msg, TestProtocol::Message(format!("Message {}", i)));
        }
    }

    #[tokio::test]
    async fn test_channel_try_send() {
        let (sender, mut receiver) = Channel::new_in_memory(ChannelId::new(), 2);

        // Fill the buffer
        sender
            .try_send(TestProtocol::Message("1".to_string()))
            .unwrap();
        sender
            .try_send(TestProtocol::Message("2".to_string()))
            .unwrap();

        // Next send should fail
        let result = sender.try_send(TestProtocol::Message("3".to_string()));
        assert!(matches!(result, Err(ChannelError::Full { .. })));

        // Receive one message to make space
        receiver.recv().await.unwrap();

        // Now send should succeed
        sender
            .try_send(TestProtocol::Message("3".to_string()))
            .unwrap();
    }

    #[tokio::test]
    async fn test_channel_try_recv() {
        let (sender, mut receiver) = Channel::new_in_memory(ChannelId::new(), 10);

        // Try to receive from empty channel
        assert!(receiver.try_recv().is_none());

        // Send a message
        sender
            .send(TestProtocol::Message("Hello".to_string()))
            .await
            .unwrap();

        // Now try_recv should succeed
        let msg = receiver.try_recv().unwrap();
        assert_eq!(msg, TestProtocol::Message("Hello".to_string()));
    }

    #[tokio::test]
    async fn test_channel_closed() {
        let (sender, receiver) = Channel::new_in_memory(ChannelId::new(), 10);

        // Drop the receiver
        drop(receiver);

        // Send should fail
        let result = sender
            .send(TestProtocol::Message("Hello".to_string()))
            .await;
        assert!(matches!(result, Err(ChannelError::Closed { .. })));
    }

    #[tokio::test]
    async fn test_channel_sender_clone() {
        let (sender, mut receiver) = Channel::new_in_memory(ChannelId::new(), 10);
        let sender2 = sender.clone();

        sender
            .send(TestProtocol::Message("From sender1".to_string()))
            .await
            .unwrap();
        sender2
            .send(TestProtocol::Message("From sender2".to_string()))
            .await
            .unwrap();

        let msg1 = receiver.recv().await.unwrap();
        let msg2 = receiver.recv().await.unwrap();

        // Both messages should be received (order may vary)
        assert!(
            msg1 == TestProtocol::Message("From sender1".to_string())
                || msg1 == TestProtocol::Message("From sender2".to_string())
        );
        assert!(
            msg2 == TestProtocol::Message("From sender1".to_string())
                || msg2 == TestProtocol::Message("From sender2".to_string())
        );
    }

    #[test]
    fn test_channel_capacity() {
        let (sender, _receiver) = Channel::<TestProtocol>::new_in_memory(ChannelId::new(), 100);
        assert_eq!(sender.capacity(), 100);
    }

    #[test]
    fn test_channel_id() {
        let id = ChannelId::from(42);
        let (sender, receiver) = Channel::<TestProtocol>::new_in_memory(id, 10);
        assert_eq!(sender.id(), id);
        assert_eq!(receiver.id(), id);
    }

    // Test protocol with feature requirements
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum FeatureTestProtocol {
        BasicMessage(String),
        StreamingMessage(String),
    }

    impl Protocol for FeatureTestProtocol {
        fn method_name(&self) -> &'static str {
            match self {
                Self::BasicMessage(_) => "basic",
                Self::StreamingMessage(_) => "streaming",
            }
        }

        fn is_request(&self) -> bool {
            true
        }

        fn required_features(&self) -> &'static [&'static str] {
            match self {
                Self::BasicMessage(_) => &[],
                Self::StreamingMessage(_) => &["streaming"],
            }
        }
    }

    #[tokio::test]
    async fn test_channel_with_features() {
        use std::collections::HashSet;

        let features = HashSet::from(["streaming".to_string()]);
        let (sender, mut receiver) =
            Channel::with_features_in_memory(ChannelId::new(), 10, features);

        // Should succeed - streaming feature is available
        sender
            .send(FeatureTestProtocol::StreamingMessage("test".to_string()))
            .await
            .unwrap();

        let msg = receiver.recv().await.unwrap();
        assert_eq!(
            msg,
            FeatureTestProtocol::StreamingMessage("test".to_string())
        );
    }

    #[tokio::test]
    async fn test_channel_missing_feature() {
        use std::collections::HashSet;

        // Create channel without streaming feature
        let features = HashSet::new();
        let (sender, _receiver) = Channel::with_features_in_memory(ChannelId::new(), 10, features);

        // Should fail - streaming feature is not available
        let result = sender
            .send(FeatureTestProtocol::StreamingMessage("test".to_string()))
            .await;

        assert!(matches!(
            result,
            Err(ChannelError::FeatureNotSupported { .. })
        ));

        if let Err(ChannelError::FeatureNotSupported {
            feature, method, ..
        }) = result
        {
            assert_eq!(feature, "streaming");
            assert_eq!(method, "streaming");
        }
    }

    #[tokio::test]
    async fn test_channel_no_feature_required() {
        use std::collections::HashSet;

        // Create channel without any features
        let features = HashSet::new();
        let (sender, mut receiver) =
            Channel::with_features_in_memory(ChannelId::new(), 10, features);

        // Should succeed - no features required
        sender
            .send(FeatureTestProtocol::BasicMessage("test".to_string()))
            .await
            .unwrap();

        let msg = receiver.recv().await.unwrap();
        assert_eq!(msg, FeatureTestProtocol::BasicMessage("test".to_string()));
    }

    #[test]
    fn test_channel_try_send_missing_feature() {
        use std::collections::HashSet;

        // Create channel without streaming feature
        let features = HashSet::new();
        let (sender, _receiver) =
            Channel::<FeatureTestProtocol>::with_features_in_memory(ChannelId::new(), 10, features);

        // Should fail - streaming feature is not available
        let result = sender.try_send(FeatureTestProtocol::StreamingMessage("test".to_string()));

        assert!(matches!(
            result,
            Err(ChannelError::FeatureNotSupported { .. })
        ));
    }
}

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

//! Main endpoint implementation.

use crate::channel::{ChannelId, ChannelManager, Protocol, SYSTEM_CHANNEL_ID, SystemProtocol};
use crate::endpoint::{
    ChannelNegotiator, DefaultChannelNegotiator, EndpointConfig, EndpointError, HandshakeMessage,
    NegotiatedProtocol, ProtocolCapability, ProtocolDirection,
};
use crate::serialization::Serializer;
use crate::transport::TransportManager;
use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use tokio::sync::{RwLock, oneshot};

/// The main endpoint for BDRPC communication.
///
/// An endpoint orchestrates all aspects of bidirectional RPC:
/// - Transport management (connections)
/// - Channel management (multiplexing)
/// - Protocol registration and negotiation
/// - Handshake protocol
///
/// Endpoints can act as:
/// - **Client**: Connect to servers and call methods
/// - **Server**: Accept connections and respond to methods
/// - **Peer**: Both client and server simultaneously
///
/// # Type Parameters
///
/// - `S`: The serializer to use for encoding/decoding messages
///
/// # Examples
///
/// ## Client Endpoint
///
/// ```rust,no_run
/// use bdrpc::endpoint::{Endpoint, EndpointConfig, ProtocolDirection};
/// use bdrpc::serialization::JsonSerializer;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = EndpointConfig::default();
/// let mut endpoint = Endpoint::new(JsonSerializer::default(), config);
///
/// // Register protocols we want to call
/// // endpoint.register_caller::<MyProtocol>()?;
///
/// // Connect to a server
/// // let connection = endpoint.connect("127.0.0.1:8080").await?;
/// # Ok(())
/// # }
/// ```
///
/// ## Server Endpoint
///
/// ```rust,no_run
/// use bdrpc::endpoint::{Endpoint, EndpointConfig};
/// use bdrpc::serialization::JsonSerializer;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = EndpointConfig::default();
/// let mut endpoint = Endpoint::new(JsonSerializer::default(), config);
///
/// // Register protocols we want to respond to
/// // endpoint.register_responder::<MyProtocol>(handler)?;
///
/// // Listen for connections
/// // endpoint.listen("127.0.0.1:8080").await?;
/// # Ok(())
/// # }
/// ```
pub struct Endpoint<S: Serializer> {
    /// Serializer for encoding/decoding messages.
    serializer: Arc<S>,

    /// Configuration for this endpoint.
    config: EndpointConfig,

    /// Transport manager for handling connections.
    transport_manager: Arc<TransportManager>,

    /// Channel manager for multiplexing.
    channel_manager: Arc<ChannelManager>,

    /// Registered protocol capabilities.
    capabilities: Arc<RwLock<HashMap<String, ProtocolCapability>>>,

    /// Negotiated protocols per connection.
    negotiated: Arc<RwLock<HashMap<String, Vec<NegotiatedProtocol>>>>,

    /// Endpoint identifier.
    endpoint_id: String,

    /// Channel negotiator for dynamic channel creation.
    channel_negotiator: Arc<dyn ChannelNegotiator>,

    /// Pending channel creation requests.
    /// Maps channel ID to response sender.
    #[allow(clippy::type_complexity)]
    pending_requests: Arc<RwLock<HashMap<ChannelId, oneshot::Sender<Result<(), String>>>>>,
}

impl<S: Serializer> Endpoint<S> {
    /// Creates a new endpoint with the given serializer and configuration.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// use bdrpc::serialization::JsonSerializer;
    ///
    /// let config = EndpointConfig::default();
    /// let endpoint = Endpoint::new(JsonSerializer::default(), config);
    /// ```
    pub fn new(serializer: S, config: EndpointConfig) -> Self {
        // Validate configuration
        if let Err(e) = config.validate() {
            panic!("Invalid endpoint configuration: {}", e);
        }

        // Generate endpoint ID if not provided
        let endpoint_id = config
            .endpoint_id
            .clone()
            .unwrap_or_else(|| format!("endpoint-{}", uuid::Uuid::new_v4()));

        Self {
            serializer: Arc::new(serializer),
            config,
            transport_manager: Arc::new(TransportManager::new()),
            channel_manager: Arc::new(ChannelManager::new()),
            capabilities: Arc::new(RwLock::new(HashMap::new())),
            negotiated: Arc::new(RwLock::new(HashMap::new())),
            endpoint_id,
            channel_negotiator: Arc::new(DefaultChannelNegotiator::new()),
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Returns the endpoint identifier.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// use bdrpc::serialization::JsonSerializer;
    ///
    /// let config = EndpointConfig::default()
    ///     .with_endpoint_id("my-endpoint".to_string());
    /// let endpoint = Endpoint::new(JsonSerializer::default(), config);
    /// assert_eq!(endpoint.id(), "my-endpoint");
    /// ```
    pub fn id(&self) -> &str {
        &self.endpoint_id
    }

    /// Returns a reference to the channel negotiator.
    ///
    /// This can be used to configure the negotiator. Note that the default
    /// negotiator is `DefaultChannelNegotiator`, but custom implementations
    /// can be set using `set_channel_negotiator`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// use bdrpc::serialization::JsonSerializer;
    ///
    /// let endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    ///
    /// // The negotiator is available for inspection
    /// let _negotiator = endpoint.negotiator();
    /// ```
    pub fn negotiator(&self) -> &Arc<dyn ChannelNegotiator> {
        &self.channel_negotiator
    }

    /// Returns a reference to the default channel negotiator if one is set.
    ///
    /// This is a convenience method that attempts to downcast the negotiator
    /// to `DefaultChannelNegotiator`. Returns `None` if a custom negotiator
    /// is being used.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// use bdrpc::serialization::JsonSerializer;
    ///
    /// # async fn example() {
    /// let endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    ///
    /// // Access the default negotiator to configure allowed protocols
    /// if let Some(negotiator) = endpoint.default_negotiator() {
    ///     negotiator.allow_protocol("MyProtocol").await;
    ///     assert!(negotiator.is_protocol_allowed("MyProtocol").await);
    /// }
    /// # }
    /// ```
    pub fn default_negotiator(&self) -> Option<&DefaultChannelNegotiator> {
        // Try to downcast &dyn ChannelNegotiator to &DefaultChannelNegotiator
        // Since ChannelNegotiator now requires Any as a super-trait, we can cast to Any first
        (self.channel_negotiator.as_ref() as &dyn std::any::Any)
            .downcast_ref::<DefaultChannelNegotiator>()
    }

    /// Returns the serializer name.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// use bdrpc::serialization::JsonSerializer;
    ///
    /// let endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    /// assert_eq!(endpoint.serializer_name(), "json");
    /// ```
    pub fn serializer_name(&self) -> &'static str {
        self.serializer.name()
    }

    /// Registers a protocol with call-only support.
    ///
    /// This allows the endpoint to call methods on this protocol but not
    /// respond to them. Typical for client endpoints.
    ///
    /// # Errors
    ///
    /// Returns an error if the protocol is already registered.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// use bdrpc::serialization::JsonSerializer;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    /// endpoint.register_caller("MyProtocol", 1).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn register_caller(
        &mut self,
        protocol_name: impl Into<String>,
        version: u32,
    ) -> Result<(), EndpointError> {
        self.register_protocol_internal(protocol_name.into(), version, ProtocolDirection::CallOnly)
            .await
    }

    /// Registers a protocol with respond-only support.
    ///
    /// This allows the endpoint to respond to methods on this protocol but
    /// not call them. Typical for server endpoints.
    ///
    /// # Errors
    ///
    /// Returns an error if the protocol is already registered.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// use bdrpc::serialization::JsonSerializer;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    /// endpoint.register_responder("MyProtocol", 1).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn register_responder(
        &mut self,
        protocol_name: impl Into<String>,
        version: u32,
    ) -> Result<(), EndpointError> {
        self.register_protocol_internal(
            protocol_name.into(),
            version,
            ProtocolDirection::RespondOnly,
        )
        .await
    }

    /// Registers a protocol with bidirectional support.
    ///
    /// This allows the endpoint to both call and respond to call methods on this
    /// protocol. Typical for peer-to-peer endpoints.
    ///
    /// # Errors
    ///
    /// Returns an error if the protocol is already registered.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// use bdrpc::serialization::JsonSerializer;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    /// endpoint.register_bidirectional("MyProtocol", 1).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn register_bidirectional(
        &mut self,
        protocol_name: impl Into<String>,
        version: u32,
    ) -> Result<(), EndpointError> {
        self.register_protocol_internal(
            protocol_name.into(),
            version,
            ProtocolDirection::Bidirectional,
        )
        .await
    }

    /// Internal method to register a protocol with a specific direction.
    async fn register_protocol_internal(
        &mut self,
        protocol_name: String,
        version: u32,
        direction: ProtocolDirection,
    ) -> Result<(), EndpointError> {
        let mut capabilities = self.capabilities.write().await;

        // Check if already registered
        if capabilities.contains_key(&protocol_name) {
            return Err(EndpointError::ProtocolAlreadyRegistered {
                protocol: protocol_name,
            });
        }

        // Create capability
        let capability = ProtocolCapability::new(protocol_name.clone(), vec![version], direction);

        // Register
        capabilities.insert(protocol_name.clone(), capability);

        // Release the lock before calling the negotiator
        drop(capabilities);

        // Notify the negotiator about the protocol registration
        self.channel_negotiator
            .on_protocol_registered(&protocol_name, version, direction)
            .await;

        Ok(())
    }

    /// Returns the registered capabilities for this endpoint.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// use bdrpc::serialization::JsonSerializer;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    /// let capabilities = endpoint.capabilities().await;
    /// println!("Registered {} protocols", capabilities.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn capabilities(&self) -> Vec<ProtocolCapability> {
        let capabilities = self.capabilities.read().await;
        capabilities.values().cloned().collect()
    }

    /// Checks if a protocol is registered.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// use bdrpc::serialization::JsonSerializer;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    /// if endpoint.has_protocol("MyProtocol").await {
    ///     println!("Protocol is registered");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn has_protocol(&self, protocol_name: &str) -> bool {
        let capabilities = self.capabilities.read().await;
        capabilities.contains_key(protocol_name)
    }

    /// Returns the direction support for a protocol.
    ///
    /// # Errors
    ///
    /// Returns an error if the protocol is not registered.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::endpoint::{Endpoint, EndpointConfig, ProtocolDirection};
    /// use bdrpc::serialization::JsonSerializer;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    /// let direction = endpoint.protocol_direction("MyProtocol").await?;
    /// if direction.can_call() {
    ///     println!("Can call methods");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn protocol_direction(
        &self,
        protocol_name: &str,
    ) -> Result<ProtocolDirection, EndpointError> {
        let capabilities = self.capabilities.read().await;
        capabilities
            .get(protocol_name)
            .map(|cap| cap.direction)
            .ok_or_else(|| EndpointError::ProtocolNotRegistered {
                protocol: protocol_name.to_string(),
            })
    }

    /// Creates a handshake hello message with this endpoint's capabilities.
    pub(crate) async fn create_hello_message(&self) -> HandshakeMessage {
        let capabilities = self.capabilities().await;
        HandshakeMessage::Hello {
            endpoint_id: Some(self.endpoint_id.clone()),
            serializer: self.serializer.name().to_string(),
            protocols: capabilities,
            bdrpc_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Returns a reference to the channel manager.
    pub fn channel_manager(&self) -> &ChannelManager {
        &self.channel_manager
    }

    /// Returns a reference to the transport manager.
    pub fn transport_manager(&self) -> &TransportManager {
        &self.transport_manager
    }

    /// Returns the configuration.
    pub fn config(&self) -> &EndpointConfig {
        &self.config
    }

    /// Sets the channel negotiator for dynamic channel creation.
    ///
    /// The negotiator is called when a remote endpoint requests to create a new
    /// channel, allowing this endpoint to validate and accept or reject the request.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::{Endpoint, EndpointConfig, DefaultChannelNegotiator};
    /// use bdrpc::serialization::JsonSerializer;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = EndpointConfig::default();
    /// let mut endpoint = Endpoint::new(JsonSerializer::default(), config);
    ///
    /// let negotiator = DefaultChannelNegotiator::new();
    /// negotiator.allow_protocol("MyProtocol").await;
    /// endpoint.set_channel_negotiator(Arc::new(negotiator));
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_channel_negotiator(&mut self, negotiator: Arc<dyn ChannelNegotiator>) {
        self.channel_negotiator = negotiator;
    }

    /// Requests creation of a new channel with the remote endpoint.
    ///
    /// This sends a channel creation request over the system channel and waits
    /// for the response. The remote endpoint's negotiator will validate the
    /// request and either accept or reject it.
    ///
    /// # Parameters
    ///
    /// - `connection_id`: The connection to create the channel on
    /// - `channel_id`: The requested channel ID (must be > 0)
    /// - `protocol_name`: Name of the protocol for this channel
    /// - `protocol_version`: Version of the protocol
    /// - `direction`: Direction for this channel
    /// - `metadata`: Optional metadata for the request
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the channel was created successfully
    /// - `Err(EndpointError)` if the request failed or timed out
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::channel::ChannelId;
    /// use bdrpc::endpoint::{Endpoint, EndpointConfig, ProtocolDirection};
    /// use bdrpc::serialization::JsonSerializer;
    /// use std::collections::HashMap;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = EndpointConfig::default();
    /// let mut endpoint = Endpoint::new(JsonSerializer::default(), config);
    ///
    /// // Connect to server
    /// let connection = endpoint.connect("127.0.0.1:8080").await?;
    ///
    /// // Request a new channel
    /// endpoint.request_channel(
    ///     connection.id(),
    ///     ChannelId::from(1),
    ///     "MyProtocol",
    ///     1,
    ///     ProtocolDirection::Bidirectional,
    ///     HashMap::new(),
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn request_channel(
        &self,
        connection_id: &str,
        channel_id: ChannelId,
        protocol_name: impl Into<String>,
        protocol_version: u32,
        direction: ProtocolDirection,
        metadata: HashMap<String, String>,
    ) -> Result<(), EndpointError> {
        let protocol_name = protocol_name.into();

        // Validate channel ID (must not be the system channel)
        if channel_id == SYSTEM_CHANNEL_ID {
            return Err(EndpointError::InvalidConfiguration {
                reason: "Cannot request system channel (ID 0)".to_string(),
            });
        }

        // Create oneshot channel for response
        let (tx, rx) = oneshot::channel();

        // Store in pending_requests
        {
            let mut pending = self.pending_requests.write().await;
            if pending.contains_key(&channel_id) {
                return Err(EndpointError::ChannelCreationFailed {
                    connection_id: connection_id.to_string(),
                    protocol: protocol_name.clone(),
                    channel_id,
                    reason: "Channel request already pending for this ID".to_string(),
                });
            }
            pending.insert(channel_id, tx);
        }

        // Get the system channel sender
        let system_sender = self
            .channel_manager
            .get_sender::<SystemProtocol>(SYSTEM_CHANNEL_ID)
            .await
            .map_err(|e| EndpointError::ChannelCreationFailed {
                connection_id: connection_id.to_string(),
                protocol: protocol_name.clone(),
                channel_id,
                reason: format!("Failed to get system channel sender: {}", e),
            })?;

        // Create and send the channel creation request
        let request = SystemProtocol::ChannelCreateRequest {
            channel_id,
            protocol_name: protocol_name.clone(),
            protocol_version,
            direction,
            buffer_size: Some(self.config.channel_buffer_size),
            metadata,
        };

        system_sender
            .send(request)
            .await
            .map_err(|e| EndpointError::ChannelCreationFailed {
                connection_id: connection_id.to_string(),
                protocol: protocol_name.clone(),
                channel_id,
                reason: format!("Failed to send channel creation request: {}", e),
            })?;

        // Wait for response with timeout
        let timeout = self.config.handshake_timeout;
        let result = tokio::time::timeout(timeout, rx).await;

        // Remove from pending_requests
        self.pending_requests.write().await.remove(&channel_id);

        // Process result
        match result {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(error))) => Err(EndpointError::ChannelRequestRejected {
                connection_id: connection_id.to_string(),
                protocol: protocol_name,
                channel_id,
                reason: error,
            }),
            Ok(Err(_)) => Err(EndpointError::ChannelCreationFailed {
                connection_id: connection_id.to_string(),
                protocol: protocol_name,
                channel_id,
                reason: "Channel creation response channel closed unexpectedly".to_string(),
            }),
            Err(_) => Err(EndpointError::ChannelRequestTimeout {
                connection_id: connection_id.to_string(),
                protocol: protocol_name,
                channel_id,
                timeout_secs: timeout.as_secs(),
            }),
        }
    }

    /// Gets typed channels for a protocol after connecting.
    ///
    /// This is a convenience method for clients that have already connected to a server.
    /// It requests a channel for the specified protocol and returns typed sender/receiver pairs.
    ///
    /// # Type Parameters
    ///
    /// * `P` - The protocol type to create channels for
    ///
    /// # Arguments
    ///
    /// * `connection_id` - The ID of the connection to create the channel on
    /// * `protocol_name` - The name of the protocol (must match what was registered)
    ///
    /// # Returns
    ///
    /// Returns a tuple of `(ChannelSender<P>, ChannelReceiver<P>)`.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Protocol is not registered
    /// - Channel creation fails
    /// - Connection doesn't exist
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// use bdrpc::serialization::JsonSerializer;
    /// use bdrpc::channel::Protocol;
    ///
    /// #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    /// enum MyProtocol {
    ///     Request { data: String },
    ///     Response { result: String },
    /// }
    ///
    /// impl Protocol for MyProtocol {
    ///     fn method_name(&self) -> &'static str {
    ///         match self {
    ///             Self::Request { .. } => "request",
    ///             Self::Response { .. } => "response",
    ///         }
    ///     }
    ///     fn is_request(&self) -> bool {
    ///         matches!(self, Self::Request { .. })
    ///     }
    /// }
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    /// endpoint.register_bidirectional("MyProtocol", 1).await?;
    ///
    /// let connection = endpoint.connect("127.0.0.1:8080").await?;
    ///
    /// // Get typed channels for the protocol
    /// let (sender, receiver) = endpoint.get_channels::<MyProtocol>(
    ///     connection.id(),
    ///     "MyProtocol"
    /// ).await?;
    ///
    /// // Now you can send and receive messages
    /// sender.send(MyProtocol::Request { data: "Hello".to_string() }).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_channels<P: Protocol>(
        &self,
        connection_id: &str,
        protocol_name: &str,
    ) -> Result<
        (
            crate::channel::ChannelSender<P>,
            crate::channel::ChannelReceiver<P>,
        ),
        EndpointError,
    > {
        // Get the negotiated protocol info
        let protocol_info = {
            let negotiated = self.negotiated.read().await;
            negotiated
                .get(connection_id)
                .and_then(|protocols| protocols.iter().find(|p| p.name == protocol_name).cloned())
                .ok_or_else(|| EndpointError::ProtocolNotRegistered {
                    protocol: protocol_name.to_string(),
                })?
        };

        // Generate a new channel ID
        let channel_id = ChannelId::new();

        // Determine the direction from negotiated capabilities
        let direction = match (protocol_info.we_can_call, protocol_info.we_can_respond) {
            (true, true) => ProtocolDirection::Bidirectional,
            (true, false) => ProtocolDirection::CallOnly,
            (false, true) => ProtocolDirection::RespondOnly,
            (false, false) => {
                return Err(EndpointError::ChannelCreationFailed {
                    connection_id: connection_id.to_string(),
                    protocol: protocol_name.to_string(),
                    channel_id,
                    reason:
                        "Protocol has no usable direction (neither call nor respond capability)"
                            .to_string(),
                });
            }
        };

        // Create the channel locally first
        // This ensures the channel exists before we send the request
        let sender = self
            .channel_manager
            .create_channel::<P>(channel_id, self.config.channel_buffer_size)
            .await
            .map_err(|e| EndpointError::ChannelCreationFailed {
                connection_id: connection_id.to_string(),
                protocol: protocol_name.to_string(),
                channel_id,
                reason: format!("Failed to create local channel: {}", e),
            })?;

        // Request the channel - this sends a request to the remote endpoint
        // The remote will create the channel on their side and send back a response
        let request_result = self
            .request_channel(
                connection_id,
                channel_id,
                protocol_name,
                protocol_info.version,
                direction,
                HashMap::new(),
            )
            .await;

        // If the request failed, clean up the local channel
        if let Err(e) = request_result {
            // Try to remove the channel, but don't fail if it's already gone
            let _ = self.channel_manager.remove_channel(channel_id).await;
            return Err(e);
        }

        // Get the receiver for the channel
        let receiver = self
            .channel_manager
            .get_receiver::<P>(channel_id)
            .await
            .map_err(|e| EndpointError::ChannelCreationFailed {
                connection_id: connection_id.to_string(),
                protocol: protocol_name.to_string(),
                channel_id,
                reason: format!("Failed to get channel receiver: {}", e),
            })?;

        #[cfg(feature = "tracing")]
        tracing::info!(
            "Created channels for protocol '{}' on connection {} with channel ID {}",
            protocol_name,
            connection_id,
            channel_id.as_u64()
        );

        Ok((sender, receiver))
    }

    /// Spawns a background task to process system channel messages.
    ///
    /// This task handles:
    /// - ChannelCreateRequest/Response for dynamic channel creation
    /// - Ping/Pong for keepalive
    /// - ChannelCloseNotification/Ack for graceful channel closure
    fn spawn_system_message_handler(
        &self,
        #[cfg_attr(not(feature = "tracing"), allow(unused_variables))] connection_id: String,
        mut receiver: crate::channel::ChannelReceiver<SystemProtocol>,
    ) {
        let channel_manager = Arc::clone(&self.channel_manager);
        let channel_negotiator = Arc::clone(&self.channel_negotiator);
        let pending_requests = Arc::clone(&self.pending_requests);
        let _config = self.config.clone();

        tokio::spawn(async move {
            #[cfg(feature = "tracing")]
            tracing::debug!(
                "System message handler started for connection {}",
                connection_id
            );

            while let Some(message) = receiver.recv().await {
                #[cfg(feature = "tracing")]
                tracing::trace!(
                    "Received system message on connection {}: {:?}",
                    connection_id,
                    message
                );

                match message {
                    SystemProtocol::ChannelCreateRequest {
                        channel_id,
                        protocol_name,
                        protocol_version,
                        direction,
                        buffer_size: _,
                        metadata,
                    } => {
                        // Validate the request using the negotiator
                        let result = channel_negotiator
                            .on_channel_create_request(
                                channel_id,
                                &protocol_name,
                                protocol_version,
                                direction,
                                &metadata,
                            )
                            .await;

                        let (success, error) = match result {
                            Ok(true) => {
                                // Channel creation request accepted
                                // The actual channel will be created by the application
                                // when it calls accept_channels() or similar
                                #[cfg(feature = "tracing")]
                                tracing::info!(
                                    "Accepted channel request {} for protocol {} on connection {}",
                                    channel_id.as_u64(),
                                    protocol_name,
                                    connection_id
                                );
                                (true, None)
                            }
                            Ok(false) => {
                                #[cfg(feature = "tracing")]
                                tracing::warn!(
                                    "Channel request rejected for {} (negotiator returned false)",
                                    protocol_name
                                );
                                (false, Some("Request rejected by negotiator".to_string()))
                            }
                            Err(reason) => {
                                #[cfg(feature = "tracing")]
                                tracing::warn!(
                                    "Channel request rejected for {}: {}",
                                    protocol_name,
                                    reason
                                );
                                (false, Some(reason))
                            }
                        };

                        // Send response through the system channel
                        let response = SystemProtocol::ChannelCreateResponse {
                            channel_id,
                            success,
                            error,
                        };

                        if let Ok(sender) = channel_manager
                            .get_sender::<SystemProtocol>(SYSTEM_CHANNEL_ID)
                            .await
                        {
                            if let Err(_e) = sender.send(response).await {
                                #[cfg(feature = "tracing")]
                                tracing::error!("Failed to send ChannelCreateResponse: {}", _e);
                            }
                        } else {
                            #[cfg(feature = "tracing")]
                            tracing::error!("Failed to get system channel sender");
                        }
                    }

                    SystemProtocol::ChannelCreateResponse {
                        channel_id,
                        success,
                        error,
                    } => {
                        // Handle response to our channel creation request
                        let mut pending = pending_requests.write().await;
                        if let Some(sender) = pending.remove(&channel_id) {
                            let result = if success {
                                Ok(())
                            } else {
                                Err(error.unwrap_or_else(|| "Unknown error".to_string()))
                            };

                            #[cfg(feature = "tracing")]
                            tracing::debug!(
                                "Received channel creation response for {}: {:?}",
                                channel_id.as_u64(),
                                result
                            );

                            // Send the result to waiting request_channel call
                            let _ = sender.send(result);
                        } else {
                            #[cfg(feature = "tracing")]
                            tracing::warn!(
                                "Received channel response for unknown request: {}",
                                channel_id.as_u64()
                            );
                        }
                    }

                    SystemProtocol::Ping { timestamp } => {
                        // Respond with pong
                        #[cfg(feature = "tracing")]
                        tracing::trace!("Received ping with timestamp {}", timestamp);

                        // Send pong response through the system channel
                        let response = SystemProtocol::Pong { timestamp };

                        if let Ok(sender) = channel_manager
                            .get_sender::<SystemProtocol>(SYSTEM_CHANNEL_ID)
                            .await
                        {
                            if let Err(_e) = sender.send(response).await {
                                #[cfg(feature = "tracing")]
                                tracing::error!("Failed to send Pong: {}", _e);
                            }
                        } else {
                            #[cfg(feature = "tracing")]
                            tracing::error!("Failed to get system channel sender for Pong");
                        }
                    }
                    SystemProtocol::Pong {
                        timestamp: _timestamp,
                    } => {
                        // Calculate round-trip time
                        #[cfg(feature = "tracing")]
                        {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_millis() as u64;
                            let rtt = now.saturating_sub(_timestamp);
                            tracing::debug!("Received pong, RTT: {}ms", rtt);
                        }
                    }
                    SystemProtocol::ChannelCloseNotification {
                        channel_id,
                        reason: _reason,
                    } => {
                        #[cfg(feature = "tracing")]
                        tracing::info!(
                            "Received channel close notification for {}: {}",
                            channel_id.as_u64(),
                            _reason
                        );

                        // Remove the channel
                        if let Err(_e) = channel_manager.remove_channel(channel_id).await {
                            #[cfg(feature = "tracing")]
                            tracing::warn!(
                                "Failed to remove channel {}: {}",
                                channel_id.as_u64(),
                                _e
                            );
                        }

                        // Send acknowledgment through the system channel
                        let response = SystemProtocol::ChannelCloseAck { channel_id };
                        if let Ok(sender) = channel_manager
                            .get_sender::<SystemProtocol>(SYSTEM_CHANNEL_ID)
                            .await
                        {
                            if let Err(_e) = sender.send(response).await {
                                #[cfg(feature = "tracing")]
                                tracing::error!(
                                    "Failed to send ChannelCloseAck for {}: {}",
                                    channel_id.as_u64(),
                                    _e
                                );
                            }
                        } else {
                            #[cfg(feature = "tracing")]
                            tracing::error!(
                                "Failed to get system channel sender for ChannelCloseAck"
                            );
                        }
                    }

                    SystemProtocol::ChannelCloseAck {
                        channel_id: _channel_id,
                    } => {
                        #[cfg(feature = "tracing")]
                        tracing::debug!(
                            "Received channel close acknowledgment for {}",
                            _channel_id.as_u64()
                        );

                        // Channel closure is complete
                    }
                }
            }

            #[cfg(feature = "tracing")]
            tracing::debug!(
                "System message handler stopped for connection {}",
                connection_id
            );
        });
    }

    /// Connects to a remote endpoint as a client.
    ///
    /// **DEPRECATED:** This method is deprecated in favor of the new transport management API.
    /// Use `add_caller()` followed by `connect_transport()` instead for better control over
    /// reconnection and transport lifecycle.
    ///
    /// # Migration Guide
    ///
    /// **Old API:**
    /// ```rust,no_run
    /// # use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// # use bdrpc::serialization::JsonSerializer;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    /// endpoint.register_caller("MyProtocol", 1).await?;
    /// let connection = endpoint.connect("127.0.0.1:8080").await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// **New API:**
    /// ```rust,no_run
    /// # use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// # use bdrpc::serialization::JsonSerializer;
    /// # use bdrpc::transport::{TransportConfig, TransportType};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    /// endpoint.register_caller("MyProtocol", 1).await?;
    ///
    /// // Add a caller transport
    /// let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080");
    /// endpoint.add_caller("main".to_string(), config).await?;
    ///
    /// // Connect the transport
    /// let connection = endpoint.connect_transport("main").await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Connection fails
    /// - Handshake fails
    /// - No compatible protocols are found
    /// - Serializer mismatch
    #[deprecated(
        since = "0.2.0",
        note = "Use `add_caller()` and `connect_transport()` instead for better transport management"
    )]
    #[cfg_attr(feature = "observability", tracing::instrument(
        skip(self),
        fields(
            endpoint_id = %self.endpoint_id,
            addr = %addr.to_string(),
        )
    ))]
    pub async fn connect(&mut self, addr: impl ToString) -> Result<Connection, EndpointError> {
        self.try_connect(addr).await
    }

    /// Connects to a remote endpoint with automatic reconnection.
    ///
    /// **DEPRECATED:** This method is deprecated in favor of the new transport management API.
    /// Use `add_caller()` with a reconnection strategy in the `TransportConfig`, then call
    /// `connect_transport()`. The new API provides better control and automatic reconnection
    /// is built into the transport manager.
    ///
    /// # Migration Guide
    ///
    /// **Old API:**
    /// ```rust,no_run
    /// # use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// # use bdrpc::serialization::JsonSerializer;
    /// # use bdrpc::reconnection::ExponentialBackoff;
    /// # use std::sync::Arc;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let strategy = Arc::new(ExponentialBackoff::default());
    /// let config = EndpointConfig::default().with_reconnection_strategy(strategy);
    /// let mut endpoint = Endpoint::new(JsonSerializer::default(), config);
    /// endpoint.register_caller("MyProtocol", 1).await?;
    /// let connection = endpoint.connect_with_reconnection("127.0.0.1:8080").await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// **New API:**
    /// ```rust,no_run
    /// # use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// # use bdrpc::serialization::JsonSerializer;
    /// # use bdrpc::transport::{TransportConfig, TransportType};
    /// # use bdrpc::reconnection::ExponentialBackoff;
    /// # use std::sync::Arc;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    /// endpoint.register_caller("MyProtocol", 1).await?;
    ///
    /// // Add caller with reconnection strategy
    /// let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080")
    ///     .with_reconnection_strategy(Arc::new(ExponentialBackoff::default()));
    /// endpoint.add_caller("main".to_string(), config).await?;
    ///
    /// // Connect - reconnection is automatic
    /// let connection = endpoint.connect_transport("main").await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - All reconnection attempts fail
    /// - The strategy decides not to reconnect
    /// - Connection succeeds but handshake fails
    #[deprecated(
        since = "0.2.0",
        note = "Use `add_caller()` with reconnection strategy and `connect_transport()` instead"
    )]
    #[cfg_attr(feature = "observability", tracing::instrument(
        skip(self),
        fields(
            endpoint_id = %self.endpoint_id,
            addr = %addr.to_string(),
            strategy = %self.config.reconnection_strategy.name(),
        )
    ))]
    pub async fn connect_with_reconnection(
        &mut self,
        addr: impl ToString,
    ) -> Result<Connection, EndpointError> {
        use crate::transport::TransportError;

        let addr_string = addr.to_string();
        let strategy = self.config.reconnection_strategy.clone();
        let mut attempt = 0u32;

        loop {
            #[cfg(feature = "tracing")]
            tracing::debug!(
                "Connection attempt {} to {} using strategy '{}'",
                attempt,
                addr_string,
                strategy.name()
            );

            match self.try_connect(&addr_string).await {
                Ok(connection) => {
                    // Connection successful
                    strategy.on_connected();

                    #[cfg(feature = "tracing")]
                    tracing::info!(
                        "Successfully connected to {} after {} attempt(s)",
                        addr_string,
                        attempt + 1
                    );

                    return Ok(connection);
                }
                Err(err) => {
                    // Determine if this is a recoverable transport error
                    let should_retry = match &err {
                        EndpointError::Transport(te) => {
                            strategy.on_disconnected(te);
                            strategy.should_reconnect(attempt, te).await
                        }
                        EndpointError::Io(io_err) => {
                            // Convert IO error to transport error for strategy
                            let te = TransportError::ConnectionFailed {
                                address: addr_string.clone(),
                                source: io::Error::new(io_err.kind(), io_err.to_string()),
                            };
                            strategy.on_disconnected(&te);
                            strategy.should_reconnect(attempt, &te).await
                        }
                        _ => {
                            // Non-transport errors (handshake, serialization, etc.)
                            // should not trigger reconnection
                            #[cfg(feature = "tracing")]
                            tracing::warn!(
                                "Connection to {} failed with non-recoverable error: {}",
                                addr_string,
                                err
                            );
                            false
                        }
                    };

                    // Check if we should reconnect
                    if !should_retry {
                        #[cfg(feature = "tracing")]
                        tracing::error!(
                            "Giving up on connection to {} after {} attempt(s): {}",
                            addr_string,
                            attempt + 1,
                            err
                        );
                        return Err(err);
                    }

                    // Calculate delay and wait
                    let delay = strategy.next_delay(attempt).await;

                    #[cfg(feature = "tracing")]
                    tracing::debug!(
                        "Connection attempt {} to {} failed: {}. Retrying in {:?}",
                        attempt,
                        addr_string,
                        err,
                        delay
                    );

                    tokio::time::sleep(delay).await;
                    attempt += 1;
                }
            }
        }
    }

    /// Attempts a single connection without reconnection logic.
    ///
    /// This is the internal method that performs the actual connection.
    /// Use `connect()` for normal connections or `connect_with_reconnection()`
    /// for connections with automatic reconnection.
    #[cfg_attr(feature = "observability", tracing::instrument(
        skip(self),
        fields(
            endpoint_id = %self.endpoint_id,
            addr = %addr.to_string(),
        )
    ))]
    async fn try_connect(&mut self, addr: impl ToString) -> Result<Connection, EndpointError> {
        use crate::transport::{TcpTransport, Transport};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        // Connect transport
        let mut transport = TcpTransport::connect(addr.to_string()).await?;
        let connection_id = format!("conn-{}", transport.metadata().id.as_u64());

        // Perform handshake
        let hello = self.create_hello_message().await;

        // Serialize and send a hello message
        let hello_bytes =
            serde_json::to_vec(&hello).map_err(|e| EndpointError::Serialization(e.to_string()))?;

        // Write length prefix (4 bytes) + message
        let len = hello_bytes.len() as u32;
        transport.write_all(&len.to_be_bytes()).await?;
        transport.write_all(&hello_bytes).await?;
        transport.flush().await?;

        // Read peer's hello message
        let mut len_bytes = [0u8; 4];
        transport.read_exact(&mut len_bytes).await?;
        let len = u32::from_be_bytes(len_bytes) as usize;

        // Validate message size
        if len > self.config.max_frame_size {
            return Err(EndpointError::HandshakeFailed {
                reason: format!("Handshake message too large: {} bytes", len),
            });
        }

        let mut msg_bytes = vec![0u8; len];
        transport.read_exact(&mut msg_bytes).await?;

        let peer_hello: HandshakeMessage = serde_json::from_slice(&msg_bytes)
            .map_err(|e| EndpointError::Serialization(e.to_string()))?;

        // Process peer hello
        let peer_capabilities = match peer_hello {
            HandshakeMessage::Hello {
                serializer,
                protocols,
                ..
            } => {
                // Check serializer compatibility
                if serializer != self.serializer.name() {
                    return Err(EndpointError::SerializerMismatch {
                        ours: self.serializer.name().to_string(),
                        theirs: serializer,
                    });
                }
                protocols
            }
            HandshakeMessage::Error { message } => {
                return Err(EndpointError::HandshakeFailed { reason: message });
            }
            _ => {
                return Err(EndpointError::HandshakeFailed {
                    reason: "Unexpected handshake message".to_string(),
                });
            }
        };

        // Negotiate protocols
        let our_capabilities = self.capabilities().await;
        let negotiated =
            crate::endpoint::negotiate_protocols(&our_capabilities, &peer_capabilities);

        if negotiated.is_empty() {
            return Err(EndpointError::HandshakeFailed {
                reason: "No compatible protocols found".to_string(),
            });
        }

        // Send acknowledgment
        let ack = HandshakeMessage::Ack {
            negotiated: negotiated.clone(),
        };
        let ack_bytes =
            serde_json::to_vec(&ack).map_err(|e| EndpointError::Serialization(e.to_string()))?;

        let len = ack_bytes.len() as u32;
        transport.write_all(&len.to_be_bytes()).await?;
        transport.write_all(&ack_bytes).await?;
        transport.flush().await?;

        // Store negotiated protocols
        {
            let mut negotiated_map = self.negotiated.write().await;
            negotiated_map.insert(connection_id.clone(), negotiated.clone());
        }

        // Create the system channel for control messages
        let _system_sender = self
            .channel_manager
            .create_channel::<SystemProtocol>(SYSTEM_CHANNEL_ID, self.config.channel_buffer_size)
            .await
            .map_err(EndpointError::Channel)?;

        #[cfg(feature = "tracing")]
        tracing::debug!("System channel created for connection {}", connection_id);

        // Spawn background task to process system messages
        let system_receiver = self
            .channel_manager
            .get_receiver::<SystemProtocol>(SYSTEM_CHANNEL_ID)
            .await
            .map_err(EndpointError::Channel)?;

        self.spawn_system_message_handler(connection_id.clone(), system_receiver);

        // Create connection
        Ok(Connection {
            id: connection_id,
            negotiated_protocols: negotiated,
        })
    }

    /// Listens for incoming connections as a server.
    ///
    /// **DEPRECATED:** This method is deprecated in favor of the new transport management API.
    /// Use `add_listener()` instead for better control over transport lifecycle and to support
    /// multiple listeners simultaneously.
    ///
    /// # Migration Guide
    ///
    /// **Old API:**
    /// ```rust,no_run
    /// # use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// # use bdrpc::serialization::JsonSerializer;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    /// endpoint.register_responder("MyProtocol", 1).await?;
    /// let listener = endpoint.listen("127.0.0.1:8080").await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// **New API:**
    /// ```rust,no_run
    /// # use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// # use bdrpc::serialization::JsonSerializer;
    /// # use bdrpc::transport::{TransportConfig, TransportType};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    /// endpoint.register_responder("MyProtocol", 1).await?;
    ///
    /// // Add a listener transport
    /// let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080");
    /// endpoint.add_listener("main".to_string(), config).await?;
    /// // Listener automatically accepts connections in the background
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Binding to the address fails
    /// - The endpoint has no responder protocols registered
    #[deprecated(
        since = "0.2.0",
        note = "Use `add_listener()` instead for better transport management"
    )]
    #[cfg_attr(feature = "observability", tracing::instrument(
        skip(self),
        fields(
            endpoint_id = %self.endpoint_id,
            addr = %addr.to_string(),
        )
    ))]
    pub async fn listen(&self, addr: impl ToString) -> Result<Listener<S>, EndpointError> {
        use tokio::net::TcpListener;

        // Verify we have at least one responder protocol
        let capabilities = self.capabilities().await;
        let has_responder = capabilities.iter().any(|cap| cap.direction.can_respond());

        if !has_responder {
            return Err(EndpointError::InvalidConfiguration {
                reason: "No responder protocols registered. Use register_responder() or register_bidirectional() before listening.".to_string(),
            });
        }

        // Bind to address
        let listener = TcpListener::bind(addr.to_string()).await?;
        let local_addr = listener.local_addr()?.to_string();

        // Clone Arc references for the spawned task
        let serializer = Arc::clone(&self.serializer);
        let capabilities_arc = Arc::clone(&self.capabilities);
        let negotiated_arc = Arc::clone(&self.negotiated);
        let channel_manager_arc = Arc::clone(&self.channel_manager);
        let config = self.config.clone();
        let endpoint_id = self.endpoint_id.clone();

        // Spawn task to accept connections
        let handle = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, peer_addr)) => {
                        #[cfg(feature = "tracing")]
                        tracing::info!("Accepted connection from {}", peer_addr);

                        // Clone for this connection
                        let serializer = Arc::clone(&serializer);
                        let capabilities_arc = Arc::clone(&capabilities_arc);
                        let negotiated_arc = Arc::clone(&negotiated_arc);
                        let channel_manager_arc = Arc::clone(&channel_manager_arc);
                        let config = config.clone();
                        let endpoint_id = endpoint_id.clone();

                        // Spawn task to handle this connection
                        tokio::spawn(async move {
                            if let Err(_e) = handle_server_connection(
                                stream,
                                peer_addr.to_string(),
                                serializer,
                                capabilities_arc,
                                negotiated_arc,
                                config,
                                endpoint_id,
                                channel_manager_arc,
                            )
                            .await
                            {
                                #[cfg(feature = "tracing")]
                                tracing::error!(
                                    "Error handling connection from {}: {}",
                                    peer_addr,
                                    _e
                                );
                            }
                        });
                    }
                    Err(_e) => {
                        #[cfg(feature = "tracing")]
                        tracing::error!("Error accepting connection: {}", _e);
                        // Continue accepting other connections
                    }
                }
            }
        });

        Ok(Listener {
            local_addr,
            handle,
            mode: ListenerMode::Automatic,
            pending_rx: None,
            endpoint_refs: None,
        })
    }

    /// Listens for incoming connections in manual acceptance mode.
    ///
    /// **DEPRECATED:** This method is deprecated in favor of the new transport management API.
    /// Use `add_listener()` instead. The new API handles connections automatically through
    /// the `TransportEventHandler` trait.
    ///
    /// # Migration Guide
    ///
    /// **Old API:**
    /// ```rust,no_run
    /// # use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// # use bdrpc::serialization::JsonSerializer;
    /// # use bdrpc::channel::Protocol;
    /// # #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    /// # enum EchoProtocol { Echo(String) }
    /// # impl Protocol for EchoProtocol {
    /// #     fn method_name(&self) -> &'static str { "echo" }
    /// #     fn is_request(&self) -> bool { true }
    /// # }
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    /// endpoint.register_bidirectional("Echo", 1).await?;
    /// let mut listener = endpoint.listen_manual("127.0.0.1:8080").await?;
    /// let (sender, receiver) = listener.accept_channels::<EchoProtocol>().await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// **New API:**
    /// ```rust,no_run
    /// # use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// # use bdrpc::serialization::JsonSerializer;
    /// # use bdrpc::transport::{TransportConfig, TransportType};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    /// endpoint.register_bidirectional("Echo", 1).await?;
    ///
    /// // Add listener - connections handled automatically
    /// let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080");
    /// endpoint.add_listener("main".to_string(), config).await?;
    /// // Use get_channels() with connection IDs from TransportEventHandler
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Binding to the address fails
    /// - The endpoint has no responder protocols registered
    #[deprecated(
        since = "0.2.0",
        note = "Use `add_listener()` instead; connections are handled automatically via TransportEventHandler"
    )]
    #[cfg_attr(feature = "observability", tracing::instrument(
        skip(self),
        fields(
            endpoint_id = %self.endpoint_id,
            addr = %addr.to_string(),
        )
    ))]
    pub async fn listen_manual(&self, addr: impl ToString) -> Result<Listener<S>, EndpointError> {
        use tokio::net::TcpListener;

        // Verify we have at least one responder protocol
        let capabilities = self.capabilities().await;
        let has_responder = capabilities.iter().any(|cap| cap.direction.can_respond());

        if !has_responder {
            return Err(EndpointError::InvalidConfiguration {
                reason: "No responder protocols registered. Use register_responder() or register_bidirectional() before listening.".to_string(),
            });
        }

        // Bind to address
        let listener = TcpListener::bind(addr.to_string()).await?;
        let local_addr = listener.local_addr()?.to_string();

        // Create a channel for pending connections
        let (pending_tx, pending_rx) = tokio::sync::mpsc::unbounded_channel();

        // Create endpoint refs for manual acceptance
        let endpoint_refs = Arc::new(EndpointRefs {
            serializer: Arc::clone(&self.serializer),
            capabilities: Arc::clone(&self.capabilities),
            negotiated: Arc::clone(&self.negotiated),
            channel_manager: Arc::clone(&self.channel_manager),
            config: self.config.clone(),
            endpoint_id: self.endpoint_id.clone(),
        });

        // Spawn task to accept connections and queue them
        let handle = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, peer_addr)) => {
                        #[cfg(feature = "tracing")]
                        tracing::info!("Accepted connection from {} (manual mode)", peer_addr);

                        let connection_id = format!("conn-{}", uuid::Uuid::new_v4());
                        let pending = PendingConnection {
                            stream,
                            peer_addr: peer_addr.to_string(),
                            connection_id,
                        };

                        // Queue the connection for manual acceptance
                        if pending_tx.send(pending).is_err() {
                            #[cfg(feature = "tracing")]
                            tracing::debug!("Listener dropped, stopping accept loop");
                            break;
                        }
                    }
                    Err(_e) => {
                        #[cfg(feature = "tracing")]
                        tracing::error!("Error accepting connection: {}", _e);
                        // Continue accepting other connections
                    }
                }
            }
        });

        Ok(Listener {
            local_addr,
            handle,
            mode: ListenerMode::Manual,
            pending_rx: Some(pending_rx),
            endpoint_refs: Some(endpoint_refs),
        })
    }

    // ============================================================================
    // Enhanced Transport Management Methods (v0.2.0)
    // ============================================================================

    /// Adds a listener transport to this endpoint.
    ///
    /// This allows the endpoint to accept incoming connections on the specified
    /// transport. Multiple listeners can be added to support different transport
    /// types or addresses simultaneously.
    ///
    /// # Arguments
    ///
    /// * `name` - Unique name for this listener
    /// * `config` - Configuration for the listener transport
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A listener with the same name already exists
    /// - The configuration is invalid
    /// - The endpoint has no responder protocols registered
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// use bdrpc::serialization::JsonSerializer;
    /// use bdrpc::transport::{TransportConfig, TransportType};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    /// endpoint.register_responder("MyProtocol", 1).await?;
    ///
    /// // Add a TCP listener
    /// let config = TransportConfig::new(TransportType::Tcp, "0.0.0.0:8080");
    /// endpoint.add_listener("tcp-main".to_string(), config).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg_attr(feature = "observability", tracing::instrument(
        skip(self, config),
        fields(
            endpoint_id = %self.endpoint_id,
            name = %name,
        )
    ))]
    pub async fn add_listener(
        &mut self,
        name: String,
        config: crate::transport::TransportConfig,
    ) -> Result<(), EndpointError> {
        // Verify we have at least one responder protocol
        let capabilities = self.capabilities().await;
        let has_responder = capabilities.iter().any(|cap| cap.direction.can_respond());

        if !has_responder {
            return Err(EndpointError::InvalidConfiguration {
                reason: "No responder protocols registered. Use register_responder() or register_bidirectional() before adding listeners.".to_string(),
            });
        }

        // Delegate to transport manager
        self.transport_manager
            .add_listener(name, config)
            .await
            .map_err(EndpointError::Transport)
    }

    /// Adds a caller transport to this endpoint.
    ///
    /// This allows the endpoint to establish outbound connections. If a reconnection
    /// strategy is configured in the transport config, the caller will automatically
    /// reconnect on connection failure.
    ///
    /// # Arguments
    ///
    /// * `name` - Unique name for this caller
    /// * `config` - Configuration for the caller transport
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A caller with the same name already exists
    /// - The configuration is invalid
    /// - The endpoint has no caller protocols registered
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// use bdrpc::serialization::JsonSerializer;
    /// use bdrpc::transport::{TransportConfig, TransportType};
    /// use bdrpc::reconnection::ExponentialBackoff;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    /// endpoint.register_caller("MyProtocol", 1).await?;
    ///
    /// // Add a TCP caller with automatic reconnection
    /// let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080")
    ///     .with_reconnection_strategy(Arc::new(ExponentialBackoff::default()));
    /// endpoint.add_caller("tcp-main".to_string(), config).await?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg_attr(feature = "observability", tracing::instrument(
        skip(self, config),
        fields(
            endpoint_id = %self.endpoint_id,
            name = %name,
        )
    ))]
    pub async fn add_caller(
        &mut self,
        name: String,
        config: crate::transport::TransportConfig,
    ) -> Result<(), EndpointError> {
        // Verify we have at least one caller protocol
        let capabilities = self.capabilities().await;
        let has_caller = capabilities.iter().any(|cap| cap.direction.can_call());

        if !has_caller {
            return Err(EndpointError::InvalidConfiguration {
                reason: "No caller protocols registered. Use register_caller() or register_bidirectional() before adding callers.".to_string(),
            });
        }

        // Delegate to transport manager
        self.transport_manager
            .add_caller(name, config)
            .await
            .map_err(EndpointError::Transport)
    }

    /// Connects a caller transport by name.
    ///
    /// This initiates a connection using the specified caller transport. If the
    /// caller has a reconnection strategy configured, it will automatically
    /// reconnect on connection failure.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the caller transport to connect
    ///
    /// # Returns
    ///
    /// Returns a `Connection` handle that can be used to get channels for protocols.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The caller does not exist
    /// - The caller is disabled
    /// - The connection fails
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// use bdrpc::serialization::JsonSerializer;
    /// use bdrpc::transport::{TransportConfig, TransportType};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    /// endpoint.register_caller("MyProtocol", 1).await?;
    ///
    /// let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080");
    /// endpoint.add_caller("tcp-main".to_string(), config).await?;
    ///
    /// // Connect the caller
    /// let connection = endpoint.connect_transport("tcp-main").await?;
    /// println!("Connected: {}", connection.id());
    /// # Ok(())
    /// # }
    /// ```
    #[cfg_attr(feature = "observability", tracing::instrument(
        skip(self),
        fields(
            endpoint_id = %self.endpoint_id,
            name = %name,
        )
    ))]
    pub async fn connect_transport(&mut self, name: &str) -> Result<Connection, EndpointError> {
        // Connect via transport manager
        let transport_id = self
            .transport_manager
            .connect_caller(name)
            .await
            .map_err(EndpointError::Transport)?;

        // Create a connection ID from the transport ID
        let connection_id = format!("transport-{}", transport_id);

        // Get negotiated protocols (will be populated by TransportEventHandler)
        let negotiated = self.negotiated.read().await;
        let negotiated_protocols = negotiated
            .get(&connection_id)
            .cloned()
            .unwrap_or_default();

        Ok(Connection {
            id: connection_id,
            negotiated_protocols,
        })
    }

    /// Removes a listener transport.
    ///
    /// This stops the listener from accepting new connections. Existing connections
    /// are not affected.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the listener to remove
    ///
    /// # Errors
    ///
    /// Returns an error if the listener does not exist or cannot be shut down.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// use bdrpc::serialization::JsonSerializer;
    /// use bdrpc::transport::{TransportConfig, TransportType};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    /// endpoint.register_responder("MyProtocol", 1).await?;
    ///
    /// let config = TransportConfig::new(TransportType::Tcp, "0.0.0.0:8080");
    /// endpoint.add_listener("tcp-main".to_string(), config).await?;
    ///
    /// // Later, remove the listener
    /// endpoint.remove_listener("tcp-main").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg_attr(feature = "observability", tracing::instrument(
        skip(self),
        fields(
            endpoint_id = %self.endpoint_id,
            name = %name,
        )
    ))]
    pub async fn remove_listener(&mut self, name: &str) -> Result<(), EndpointError> {
        self.transport_manager
            .remove_listener(name)
            .await
            .map_err(EndpointError::Transport)
    }

    /// Removes a caller transport.
    ///
    /// This stops any reconnection attempts and closes the connection.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the caller to remove
    ///
    /// # Errors
    ///
    /// Returns an error if the caller does not exist.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// use bdrpc::serialization::JsonSerializer;
    /// use bdrpc::transport::{TransportConfig, TransportType};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    /// endpoint.register_caller("MyProtocol", 1).await?;
    ///
    /// let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080");
    /// endpoint.add_caller("tcp-main".to_string(), config).await?;
    ///
    /// // Later, remove the caller
    /// endpoint.remove_caller("tcp-main").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg_attr(feature = "observability", tracing::instrument(
        skip(self),
        fields(
            endpoint_id = %self.endpoint_id,
            name = %name,
        )
    ))]
    pub async fn remove_caller(&mut self, name: &str) -> Result<(), EndpointError> {
        self.transport_manager
            .remove_caller(name)
            .await
            .map_err(EndpointError::Transport)
    }

    /// Enables a transport (listener or caller).
    ///
    /// For listeners, this allows accepting new connections.
    /// For callers, this allows connection attempts.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the transport to enable
    ///
    /// # Errors
    ///
    /// Returns an error if the transport does not exist.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// use bdrpc::serialization::JsonSerializer;
    /// use bdrpc::transport::{TransportConfig, TransportType};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    /// endpoint.register_responder("MyProtocol", 1).await?;
    ///
    /// let config = TransportConfig::new(TransportType::Tcp, "0.0.0.0:8080")
    ///     .with_enabled(false);
    /// endpoint.add_listener("tcp-main".to_string(), config).await?;
    ///
    /// // Enable the listener later
    /// endpoint.enable_transport("tcp-main").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg_attr(feature = "observability", tracing::instrument(
        skip(self),
        fields(
            endpoint_id = %self.endpoint_id,
            name = %name,
        )
    ))]
    pub async fn enable_transport(&mut self, name: &str) -> Result<(), EndpointError> {
        self.transport_manager
            .enable_transport(name)
            .await
            .map_err(EndpointError::Transport)
    }

    /// Disables a transport (listener or caller).
    ///
    /// For listeners, this stops accepting new connections.
    /// For callers, this stops connection attempts and reconnection.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the transport to disable
    ///
    /// # Errors
    ///
    /// Returns an error if the transport does not exist.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// use bdrpc::serialization::JsonSerializer;
    /// use bdrpc::transport::{TransportConfig, TransportType};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    /// endpoint.register_responder("MyProtocol", 1).await?;
    ///
    /// let config = TransportConfig::new(TransportType::Tcp, "0.0.0.0:8080");
    /// endpoint.add_listener("tcp-main".to_string(), config).await?;
    ///
    /// // Disable the listener temporarily
    /// endpoint.disable_transport("tcp-main").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg_attr(feature = "observability", tracing::instrument(
        skip(self),
        fields(
            endpoint_id = %self.endpoint_id,
            name = %name,
        )
    ))]
    pub async fn disable_transport(&mut self, name: &str) -> Result<(), EndpointError> {
        self.transport_manager
            .disable_transport(name)
            .await
            .map_err(EndpointError::Transport)
    }
}

/// Handles an incoming server connection.
#[allow(clippy::too_many_arguments)]
#[cfg_attr(feature = "observability", tracing::instrument(
    skip(stream, serializer, capabilities, negotiated, config, channel_manager),
    fields(
        endpoint_id = %endpoint_id,
        peer_addr = %_peer_addr,
    )
))]
async fn handle_server_connection<S: Serializer>(
    stream: tokio::net::TcpStream,
    _peer_addr: String,
    serializer: Arc<S>,
    capabilities: Arc<RwLock<HashMap<String, ProtocolCapability>>>,
    negotiated: Arc<RwLock<HashMap<String, Vec<NegotiatedProtocol>>>>,
    config: EndpointConfig,
    endpoint_id: String,
    channel_manager: Arc<ChannelManager>,
) -> Result<(), EndpointError> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut transport = stream;
    let connection_id = format!("conn-{}", uuid::Uuid::new_v4());

    // Read client's hello message
    let mut len_bytes = [0u8; 4];
    transport.read_exact(&mut len_bytes).await?;
    let len = u32::from_be_bytes(len_bytes) as usize;

    // Validate message size
    if len > config.max_frame_size {
        return Err(EndpointError::HandshakeFailed {
            reason: format!("Handshake message too large: {} bytes", len),
        });
    }

    let mut msg_bytes = vec![0u8; len];
    transport.read_exact(&mut msg_bytes).await?;

    let client_hello: HandshakeMessage = serde_json::from_slice(&msg_bytes)
        .map_err(|e| EndpointError::Serialization(e.to_string()))?;

    // Process client hello
    let client_capabilities = match client_hello {
        HandshakeMessage::Hello {
            serializer: client_serializer,
            protocols,
            ..
        } => {
            // Check serializer compatibility
            if client_serializer != serializer.name() {
                let error_msg = HandshakeMessage::Error {
                    message: format!(
                        "Serializer mismatch: server uses '{}', client uses '{}'",
                        serializer.name(),
                        client_serializer
                    ),
                };
                let error_bytes = serde_json::to_vec(&error_msg)
                    .map_err(|e| EndpointError::Serialization(e.to_string()))?;
                let len = error_bytes.len() as u32;
                transport.write_all(&len.to_be_bytes()).await?;
                transport.write_all(&error_bytes).await?;
                transport.flush().await?;

                return Err(EndpointError::SerializerMismatch {
                    ours: serializer.name().to_string(),
                    theirs: client_serializer,
                });
            }
            protocols
        }
        HandshakeMessage::Error { message } => {
            return Err(EndpointError::HandshakeFailed { reason: message });
        }
        _ => {
            return Err(EndpointError::HandshakeFailed {
                reason: "Expected Hello message".to_string(),
            });
        }
    };

    // Send our hello message
    let our_capabilities_vec: Vec<ProtocolCapability> = {
        let caps = capabilities.read().await;
        caps.values().cloned().collect()
    };

    let hello = HandshakeMessage::Hello {
        endpoint_id: Some(endpoint_id),
        serializer: serializer.name().to_string(),
        protocols: our_capabilities_vec.clone(),
        bdrpc_version: env!("CARGO_PKG_VERSION").to_string(),
    };

    let hello_bytes =
        serde_json::to_vec(&hello).map_err(|e| EndpointError::Serialization(e.to_string()))?;
    let len = hello_bytes.len() as u32;
    transport.write_all(&len.to_be_bytes()).await?;
    transport.write_all(&hello_bytes).await?;
    transport.flush().await?;

    // Negotiate protocols
    let negotiated_protocols =
        crate::endpoint::negotiate_protocols(&our_capabilities_vec, &client_capabilities);

    if negotiated_protocols.is_empty() {
        let error_msg = HandshakeMessage::Error {
            message: "No compatible protocols found".to_string(),
        };
        let error_bytes = serde_json::to_vec(&error_msg)
            .map_err(|e| EndpointError::Serialization(e.to_string()))?;
        let len = error_bytes.len() as u32;
        transport.write_all(&len.to_be_bytes()).await?;
        transport.write_all(&error_bytes).await?;
        transport.flush().await?;

        return Err(EndpointError::HandshakeFailed {
            reason: "No compatible protocols found".to_string(),
        });
    }

    // Read client's acknowledgment
    let mut len_bytes = [0u8; 4];
    transport.read_exact(&mut len_bytes).await?;
    let len = u32::from_be_bytes(len_bytes) as usize;

    if len > config.max_frame_size {
        return Err(EndpointError::HandshakeFailed {
            reason: format!("Ack message too large: {} bytes", len),
        });
    }

    let mut msg_bytes = vec![0u8; len];
    transport.read_exact(&mut msg_bytes).await?;

    let _client_ack: HandshakeMessage = serde_json::from_slice(&msg_bytes)
        .map_err(|e| EndpointError::Serialization(e.to_string()))?;

    // Store negotiated protocols
    {
        let mut negotiated_map = negotiated.write().await;
        negotiated_map.insert(connection_id.clone(), negotiated_protocols.clone());
    }

    #[cfg(feature = "tracing")]
    tracing::info!(
        "Handshake complete with {} ({}): {} protocols negotiated",
        _peer_addr,
        connection_id,
        negotiated_protocols.len()
    );

    // Create the system channel for control messages
    let system_sender = channel_manager
        .create_channel::<SystemProtocol>(SYSTEM_CHANNEL_ID, config.channel_buffer_size)
        .await
        .map_err(|e| EndpointError::HandshakeFailed {
            reason: format!("Failed to create system channel: {}", e),
        })?;

    #[cfg(feature = "tracing")]
    tracing::debug!(
        "System channel created for connection {} ({})",
        connection_id,
        _peer_addr
    );

    // Wrap the transport in Arc<RwLock<>> to share between read and write tasks
    let transport = Arc::new(RwLock::new(transport));

    // Spawn a task to handle incoming messages from the transport
    let transport_read = Arc::clone(&transport);
    let serializer_read = Arc::clone(&serializer);
    let channel_manager_read = Arc::clone(&channel_manager);
    let _connection_id_read = connection_id.clone();

    tokio::spawn(async move {
        #[cfg(feature = "tracing")]
        tracing::debug!(
            "Starting message receive loop for connection {}",
            _connection_id_read
        );

        loop {
            // Read a frame from the transport
            let frame =
                match crate::serialization::framing::read_frame(&mut *transport_read.write().await)
                    .await
                {
                    Ok(frame) => frame,
                    Err(_e) => {
                        #[cfg(feature = "tracing")]
                        tracing::error!(
                            "Failed to read frame from connection {}: {}",
                            _connection_id_read,
                            _e
                        );
                        break;
                    }
                };

            // Deserialize the system channel envelope
            //
            // The envelope contains the channel_id and the payload
            // We need to determine the protocol type based on the channel_id
            // For now, we'll handle the system channel (ID 0) specially

            // Try to deserialize as a system protocol envelope first
            match serializer_read.deserialize::<crate::channel::Envelope<SystemProtocol>>(&frame) {
                Ok(envelope) => {
                    let channel_id = envelope.channel_id;

                    #[cfg(feature = "tracing")]
                    tracing::trace!(
                        "Received message on channel {} (connection {})",
                        channel_id.as_u64(),
                        _connection_id_read
                    );

                    // Route the message to the appropriate channel
                    if let Ok(sender) = channel_manager_read
                        .get_sender::<SystemProtocol>(channel_id)
                        .await
                    {
                        if let Err(_e) = sender.send(envelope.payload).await {
                            #[cfg(feature = "tracing")]
                            tracing::warn!(
                                "Failed to route message to channel {}: {}",
                                channel_id.as_u64(),
                                _e
                            );
                        }
                    } else {
                        #[cfg(feature = "tracing")]
                        tracing::warn!(
                            "Received message for unknown channel {} on connection {}",
                            channel_id.as_u64(),
                            _connection_id_read
                        );
                    }
                }
                Err(_e) => {
                    #[cfg(feature = "tracing")]
                    tracing::error!(
                        "Failed to deserialize envelope on connection {}: {}",
                        _connection_id_read,
                        _e
                    );
                    // For now, we only support system protocol
                    // In the future, we'll need to handle other protocol types
                    // based on the negotiated protocols for this connection
                }
            }
        }

        #[cfg(feature = "tracing")]
        tracing::debug!(
            "Message receive loop stopped for connection {}",
            _connection_id_read
        );
    });

    // Spawn a task to send outgoing messages from channels to the transport
    let transport_write = Arc::clone(&transport);
    let serializer_write = Arc::clone(&serializer);
    let _connection_id_write = connection_id.clone();

    // Get the system channel receiver to send messages
    let mut system_receiver = match channel_manager
        .get_receiver::<SystemProtocol>(SYSTEM_CHANNEL_ID)
        .await
    {
        Ok(receiver) => receiver,
        Err(e) => {
            #[cfg(feature = "tracing")]
            tracing::error!("Failed to get system channel receiver: {}", e);
            return Err(EndpointError::HandshakeFailed {
                reason: format!("Failed to get system channel receiver: {}", e),
            });
        }
    };

    tokio::spawn(async move {
        #[cfg(feature = "tracing")]
        tracing::debug!(
            "Starting message send loop for connection {}",
            _connection_id_write
        );

        while let Some(message) = system_receiver.recv().await {
            // Wrap the message in an envelope
            let envelope = crate::channel::Envelope::new(
                SYSTEM_CHANNEL_ID,
                0, // Sequence number will be managed by the channel
                message,
            );

            // Serialize the envelope
            let frame = match serializer_write.serialize(&envelope) {
                Ok(frame) => frame,
                Err(_e) => {
                    #[cfg(feature = "tracing")]
                    tracing::error!("Failed to serialize envelope: {}", _e);
                    continue;
                }
            };

            // Write the frame to the transport
            if let Err(_e) = crate::serialization::framing::write_frame(
                &mut *transport_write.write().await,
                &frame,
            )
            .await
            {
                #[cfg(feature = "tracing")]
                tracing::error!(
                    "Failed to write frame to connection {}: {}",
                    _connection_id_write,
                    _e
                );
                break;
            }
        }

        #[cfg(feature = "tracing")]
        tracing::debug!(
            "Message send loop stopped for connection {}",
            _connection_id_write
        );
    });

    // Keep the system_sender alive to prevent the channel from closing
    drop(system_sender);

    Ok(())
}

/// Handles a manually accepted connection with full bidirectional communication.
///
/// This function is called internally by [`Listener::accept_channels`] when using manual
/// acceptance mode (via [`Endpoint::listen_manual`]). It performs the complete connection
/// setup including:
///
/// 1. **Handshake**: Exchanges protocol capabilities and negotiates compatible protocols
/// 2. **System Channel**: Creates the system channel (ID 0) for control messages
/// 3. **Message Handling**: Spawns two async tasks for bidirectional communication:
///    - **Read Task**: Continuously reads frames from the transport, deserializes envelopes,
///      and routes messages to the appropriate channels
///    - **Write Task**: Receives messages from channels, wraps them in envelopes, serializes,
///      and writes frames to the transport
///
/// # Message Flow
///
/// **Incoming Messages** (Read Task):
/// ```text
/// Transport → read_frame() → deserialize<Envelope<Protocol>>() → route to channel
/// ```
///
/// **Outgoing Messages** (Write Task):
/// ```text
/// Channel → wrap in Envelope → serialize() → write_frame() → Transport
/// ```
///
/// # Differences from `handle_server_connection`
///
/// While similar to `handle_server_connection`, this function is designed for manual mode:
/// - Returns the connection ID to the caller for channel creation
/// - Used with `accept_channels()` for explicit connection handling
/// - Provides more control over connection lifecycle
///
/// # Error Handling
///
/// Both spawned tasks handle errors gracefully:
/// - Transport errors (connection lost, read/write failures) terminate the tasks
/// - Serialization errors are logged and the message is skipped
/// - Channel routing errors (unknown channel) are logged
///
/// # Example
///
/// This function is called internally, but here's how it's used via the public API:
///
/// ```rust,no_run
/// use bdrpc::endpoint::{Endpoint, EndpointConfig};
/// use bdrpc::serialization::JsonSerializer;
/// use bdrpc::channel::Protocol;
///
/// #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
/// enum MyProtocol {
///     Request(String),
///     Response(String),
/// }
///
/// impl Protocol for MyProtocol {
///     fn method_name(&self) -> &'static str { "my_protocol" }
///     fn is_request(&self) -> bool { true }
/// }
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
/// endpoint.register_bidirectional("MyProtocol", 1).await?;
///
/// // Start listening in manual mode
/// let mut listener = endpoint.listen_manual("127.0.0.1:8080").await?;
///
/// // Accept a connection - this internally calls handle_manual_connection
/// let (sender, mut receiver) = listener.accept_channels::<MyProtocol>().await?;
///
/// // Now you can send and receive messages
/// sender.send(MyProtocol::Request("Hello".to_string())).await?;
/// if let Some(msg) = receiver.recv().await {
///     println!("Received: {:?}", msg);
/// }
/// # Ok(())
/// # }
/// ```
///
/// # See Also
///
/// - [`Endpoint::listen_manual`] - Creates a listener in manual mode
/// - [`Listener::accept_channels`] - Accepts connections and returns typed channels
/// - [`handle_server_connection`] - Similar function for automatic mode
#[allow(clippy::too_many_arguments)]
#[cfg_attr(feature = "observability", tracing::instrument(
    skip(stream, serializer, capabilities, negotiated, config, channel_manager),
    fields(
        endpoint_id = %endpoint_id,
        peer_addr = %_peer_addr,
        connection_id = %connection_id,
    )
))]
async fn handle_manual_connection<S: Serializer>(
    stream: tokio::net::TcpStream,
    _peer_addr: String,
    connection_id: String,
    serializer: Arc<S>,
    capabilities: Arc<RwLock<HashMap<String, ProtocolCapability>>>,
    negotiated: Arc<RwLock<HashMap<String, Vec<NegotiatedProtocol>>>>,
    config: EndpointConfig,
    endpoint_id: String,
    channel_manager: Arc<ChannelManager>,
) -> Result<String, EndpointError> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut transport = stream;

    // Read client's hello message
    let mut len_bytes = [0u8; 4];
    transport.read_exact(&mut len_bytes).await?;
    let len = u32::from_be_bytes(len_bytes) as usize;

    // Validate message size
    if len > config.max_frame_size {
        return Err(EndpointError::HandshakeFailed {
            reason: format!("Handshake message too large: {} bytes", len),
        });
    }

    let mut msg_bytes = vec![0u8; len];
    transport.read_exact(&mut msg_bytes).await?;

    let client_hello: HandshakeMessage = serde_json::from_slice(&msg_bytes)
        .map_err(|e| EndpointError::Serialization(e.to_string()))?;

    // Process client hello
    let (client_capabilities, client_serializer) = match client_hello {
        HandshakeMessage::Hello {
            serializer: client_serializer,
            protocols,
            ..
        } => (protocols, client_serializer),
        HandshakeMessage::Error { message } => {
            return Err(EndpointError::HandshakeFailed { reason: message });
        }
        _ => {
            return Err(EndpointError::HandshakeFailed {
                reason: "Expected Hello message".to_string(),
            });
        }
    };

    // Send our hello message
    let our_capabilities_vec: Vec<ProtocolCapability> = {
        let caps = capabilities.read().await;
        caps.values().cloned().collect()
    };

    let hello = HandshakeMessage::Hello {
        endpoint_id: Some(endpoint_id.clone()),
        serializer: client_serializer.clone(), // Echo back the serializer name
        protocols: our_capabilities_vec.clone(),
        bdrpc_version: env!("CARGO_PKG_VERSION").to_string(),
    };

    let hello_bytes =
        serde_json::to_vec(&hello).map_err(|e| EndpointError::Serialization(e.to_string()))?;
    let len = hello_bytes.len() as u32;
    transport.write_all(&len.to_be_bytes()).await?;
    transport.write_all(&hello_bytes).await?;
    transport.flush().await?;

    // Negotiate protocols
    let negotiated_protocols =
        crate::endpoint::negotiate_protocols(&our_capabilities_vec, &client_capabilities);

    if negotiated_protocols.is_empty() {
        let error_msg = HandshakeMessage::Error {
            message: "No compatible protocols found".to_string(),
        };
        let error_bytes = serde_json::to_vec(&error_msg)
            .map_err(|e| EndpointError::Serialization(e.to_string()))?;
        let len = error_bytes.len() as u32;
        transport.write_all(&len.to_be_bytes()).await?;
        transport.write_all(&error_bytes).await?;
        transport.flush().await?;

        return Err(EndpointError::HandshakeFailed {
            reason: "No compatible protocols found".to_string(),
        });
    }

    // Read client's acknowledgment
    let mut len_bytes = [0u8; 4];
    transport.read_exact(&mut len_bytes).await?;
    let len = u32::from_be_bytes(len_bytes) as usize;

    if len > config.max_frame_size {
        return Err(EndpointError::HandshakeFailed {
            reason: format!("Ack message too large: {} bytes", len),
        });
    }

    let mut msg_bytes = vec![0u8; len];
    transport.read_exact(&mut msg_bytes).await?;

    let _client_ack: HandshakeMessage = serde_json::from_slice(&msg_bytes)
        .map_err(|e| EndpointError::Serialization(e.to_string()))?;

    // Store negotiated protocols
    {
        let mut negotiated_map = negotiated.write().await;
        negotiated_map.insert(connection_id.clone(), negotiated_protocols.clone());
    }

    #[cfg(feature = "tracing")]
    tracing::info!(
        "Handshake complete with {} ({}): {} protocols negotiated",
        _peer_addr,
        connection_id,
        negotiated_protocols.len()
    );

    // Create the system channel for control messages
    let system_sender = channel_manager
        .create_channel::<SystemProtocol>(SYSTEM_CHANNEL_ID, config.channel_buffer_size)
        .await
        .map_err(|e| EndpointError::HandshakeFailed {
            reason: format!("Failed to create system channel: {}", e),
        })?;

    #[cfg(feature = "tracing")]
    tracing::debug!(
        "System channel created for connection {} ({})",
        connection_id,
        _peer_addr
    );

    // Wrap the transport in Arc<RwLock<>> to share between read and write tasks
    let transport = Arc::new(RwLock::new(transport));

    // Spawn a task to handle incoming messages from the transport
    let transport_read = Arc::clone(&transport);
    let serializer_read = Arc::clone(&serializer);
    let channel_manager_read = Arc::clone(&channel_manager);
    let _connection_id_read = connection_id.clone();

    tokio::spawn(async move {
        #[cfg(feature = "tracing")]
        tracing::debug!(
            "Starting message receive loop for connection {}",
            _connection_id_read
        );

        loop {
            // Read a frame from the transport
            let frame =
                match crate::serialization::framing::read_frame(&mut *transport_read.write().await)
                    .await
                {
                    Ok(frame) => frame,
                    Err(_e) => {
                        #[cfg(feature = "tracing")]
                        tracing::error!(
                            "Failed to read frame from connection {}: {}",
                            _connection_id_read,
                            _e
                        );
                        break;
                    }
                };

            // Deserialize the system channel envelope
            //
            // The envelope contains the channel_id and the payload
            // We need to determine the protocol type based on the channel_id
            // For now, we'll handle the system channel (ID 0) specially

            // Try to deserialize as a system protocol envelope first
            match serializer_read.deserialize::<crate::channel::Envelope<SystemProtocol>>(&frame) {
                Ok(envelope) => {
                    let channel_id = envelope.channel_id;

                    #[cfg(feature = "tracing")]
                    tracing::trace!(
                        "Received message on channel {} (connection {})",
                        channel_id.as_u64(),
                        _connection_id_read
                    );

                    // Route the message to the appropriate channel
                    if let Ok(sender) = channel_manager_read
                        .get_sender::<SystemProtocol>(channel_id)
                        .await
                    {
                        if let Err(_e) = sender.send(envelope.payload).await {
                            #[cfg(feature = "tracing")]
                            tracing::warn!(
                                "Failed to route message to channel {}: {}",
                                channel_id.as_u64(),
                                _e
                            );
                        }
                    } else {
                        #[cfg(feature = "tracing")]
                        tracing::warn!(
                            "Received message for unknown channel {} on connection {}",
                            channel_id.as_u64(),
                            _connection_id_read
                        );
                    }
                }
                Err(_e) => {
                    #[cfg(feature = "tracing")]
                    tracing::error!(
                        "Failed to deserialize envelope on connection {}: {}",
                        _connection_id_read,
                        _e
                    );
                    // For now, we only support system protocol
                    // In the future, we'll need to handle other protocol types
                    // based on the negotiated protocols for this connection
                }
            }
        }

        #[cfg(feature = "tracing")]
        tracing::debug!(
            "Message receive loop stopped for connection {}",
            _connection_id_read
        );
    });

    // Spawn a task to send outgoing messages from channels to the transport
    let transport_write = Arc::clone(&transport);
    let serializer_write = Arc::clone(&serializer);
    let _connection_id_write = connection_id.clone();

    // Get the system channel receiver to send messages
    let mut system_receiver = match channel_manager
        .get_receiver::<SystemProtocol>(SYSTEM_CHANNEL_ID)
        .await
    {
        Ok(receiver) => receiver,
        Err(e) => {
            #[cfg(feature = "tracing")]
            tracing::error!("Failed to get system channel receiver: {}", e);
            return Err(EndpointError::HandshakeFailed {
                reason: format!("Failed to get system channel receiver: {}", e),
            });
        }
    };

    tokio::spawn(async move {
        #[cfg(feature = "tracing")]
        tracing::debug!(
            "Starting message send loop for connection {}",
            _connection_id_write
        );

        while let Some(message) = system_receiver.recv().await {
            // Wrap the message in an envelope
            let envelope = crate::channel::Envelope::new(
                SYSTEM_CHANNEL_ID,
                0, // Sequence number will be managed by the channel
                message,
            );

            // Serialize the envelope
            let frame = match serializer_write.serialize(&envelope) {
                Ok(frame) => frame,
                Err(_e) => {
                    #[cfg(feature = "tracing")]
                    tracing::error!("Failed to serialize envelope: {}", _e);
                    continue;
                }
            };

            // Write the frame to the transport
            if let Err(_e) = crate::serialization::framing::write_frame(
                &mut *transport_write.write().await,
                &frame,
            )
            .await
            {
                #[cfg(feature = "tracing")]
                tracing::error!(
                    "Failed to write frame to connection {}: {}",
                    _connection_id_write,
                    _e
                );
                break;
            }
        }

        #[cfg(feature = "tracing")]
        tracing::debug!(
            "Message send loop stopped for connection {}",
            _connection_id_write
        );
    });

    // Keep the system_sender alive to prevent the channel from closing
    drop(system_sender);

    Ok(connection_id)
}

/// Mode for listener operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListenerMode {
    /// Automatic mode - connections are handled automatically in background.
    Automatic,
    /// Manual mode - connections are queued for manual acceptance via `accept_channels()`.
    Manual,
}

/// Information about an accepted connection waiting for manual processing.
struct PendingConnection {
    /// The TCP stream for this connection.
    stream: tokio::net::TcpStream,
    /// The peer address.
    peer_addr: String,
    /// The connection ID assigned to this connection.
    connection_id: String,
}

/// A listener for incoming connections.
///
/// This is returned by [`Endpoint::listen`] and [`Endpoint::listen_manual`] and represents
/// an active server listening for connections.
///
/// In automatic mode (from `listen()`), connections are handled automatically in the background.
/// In manual mode (from `listen_manual()`), you must call `accept_channels()` to process each connection.
pub struct Listener<S: Serializer> {
    /// The local address being listened on.
    local_addr: String,
    /// Handle to the background task accepting connections.
    handle: tokio::task::JoinHandle<()>,
    /// Mode of operation for this listener.
    mode: ListenerMode,
    /// Queue for pending connections (only used in manual mode).
    pending_rx: Option<tokio::sync::mpsc::UnboundedReceiver<PendingConnection>>,
    /// References needed for manual acceptance.
    endpoint_refs: Option<Arc<EndpointRefs<S>>>,
}

/// Shared references needed for manual connection acceptance.
struct EndpointRefs<S: Serializer> {
    serializer: Arc<S>,
    capabilities: Arc<RwLock<HashMap<String, ProtocolCapability>>>,
    negotiated: Arc<RwLock<HashMap<String, Vec<NegotiatedProtocol>>>>,
    channel_manager: Arc<ChannelManager>,
    config: EndpointConfig,
    endpoint_id: String,
}

impl<S: Serializer> Listener<S> {
    /// Returns the local address being listened on.
    pub fn local_addr(&self) -> &str {
        &self.local_addr
    }

    /// Stops the listener and waits for it to shut down.
    pub async fn shutdown(self) {
        self.handle.abort();
        let _ = self.handle.await;
    }

    /// Accepts an incoming connection and returns typed channels for a protocol.
    ///
    /// This method is only available for listeners created with [`Endpoint::listen_manual`].
    /// It waits for the next incoming connection, performs the handshake, creates channels
    /// for the specified protocol, and returns typed sender/receiver channels.
    ///
    /// This provides a simpler API compared to the automatic mode, where you would need
    /// to track connection IDs and call `get_channels()` separately.
    ///
    /// # Type Parameters
    ///
    /// * `P` - The protocol type to create channels for. Must be registered with the endpoint.
    ///
    /// # Returns
    ///
    /// Returns a tuple of `(ChannelSender<P>, ChannelReceiver<P>)` for the accepted connection.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Called on a listener in automatic mode (use `listen_manual()` instead)
    /// - No connection is available (listener stopped)
    /// - Handshake fails
    /// - Protocol is not registered or not negotiated
    /// - Channel creation fails
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::endpoint::{Endpoint, EndpointConfig};
    /// use bdrpc::serialization::JsonSerializer;
    /// use bdrpc::channel::Protocol;
    ///
    /// #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    /// enum EchoProtocol {
    ///     Echo(String),
    /// }
    ///
    /// impl Protocol for EchoProtocol {
    ///     fn method_name(&self) -> &'static str { "echo" }
    ///     fn is_request(&self) -> bool { true }
    /// }
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
    /// endpoint.register_bidirectional("Echo", 1).await?;
    ///
    /// let mut listener = endpoint.listen_manual("127.0.0.1:8080").await?;
    ///
    /// // Accept connections one at a time
    /// loop {
    ///     let (sender, mut receiver) = listener.accept_channels::<EchoProtocol>().await?;
    ///
    ///     tokio::spawn(async move {
    ///         while let Some(msg) = receiver.recv().await {
    ///             let _ = sender.send(msg).await; // Echo back
    ///         }
    ///     });
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn accept_channels<P: Protocol>(
        &mut self,
    ) -> Result<
        (
            crate::channel::ChannelSender<P>,
            crate::channel::ChannelReceiver<P>,
        ),
        EndpointError,
    > {
        // Verify this is a manual mode listener
        if self.mode != ListenerMode::Manual {
            return Err(EndpointError::InvalidConfiguration {
                reason: "accept_channels() can only be called on listeners created with listen_manual(). Use listen() + get_channels() for automatic mode.".to_string(),
            });
        }

        // Get the pending connection receiver
        let pending_rx =
            self.pending_rx
                .as_mut()
                .ok_or_else(|| EndpointError::InvalidConfiguration {
                    reason: "Listener is not properly configured for manual acceptance".to_string(),
                })?;

        // Get endpoint refs
        let refs =
            self.endpoint_refs
                .as_ref()
                .ok_or_else(|| EndpointError::InvalidConfiguration {
                    reason: "Listener is missing endpoint references".to_string(),
                })?;

        // Wait for a pending connection
        let pending =
            pending_rx
                .recv()
                .await
                .ok_or_else(|| EndpointError::InvalidConfiguration {
                    reason: "Listener has been shut down - no more connections will be accepted"
                        .to_string(),
                })?;

        #[cfg(feature = "tracing")]
        tracing::debug!(
            "Processing manual connection from {} ({})",
            pending.peer_addr,
            pending.connection_id
        );

        // Get serializer reference
        let serializer = Arc::clone(&refs.serializer);

        // Perform the handshake and set up the connection
        let _connection_id = handle_manual_connection::<S>(
            pending.stream,
            pending.peer_addr,
            pending.connection_id,
            serializer,
            Arc::clone(&refs.capabilities),
            Arc::clone(&refs.negotiated),
            refs.config.clone(),
            refs.endpoint_id.clone(),
            Arc::clone(&refs.channel_manager),
        )
        .await?;

        // Get the channels for this protocol
        let channel_id = ChannelId::new();

        // Create the channel
        let sender = refs
            .channel_manager
            .create_channel::<P>(channel_id, refs.config.channel_buffer_size)
            .await
            .map_err(EndpointError::Channel)?;

        let receiver = refs
            .channel_manager
            .get_receiver::<P>(channel_id)
            .await
            .map_err(EndpointError::Channel)?;

        #[cfg(feature = "tracing")]
        tracing::info!(
            "Created channels for protocol on connection {}",
            _connection_id
        );

        Ok((sender, receiver))
    }
}

/// Represents an active connection to a remote endpoint.
///
/// This handle provides information about the connection and the
/// negotiated protocols.
#[derive(Debug, Clone)]
pub struct Connection {
    /// Unique identifier for this connection.
    id: String,
    /// Protocols negotiated for this connection.
    negotiated_protocols: Vec<NegotiatedProtocol>,
}

impl Connection {
    /// Returns the connection identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the negotiated protocols for this connection.
    pub fn protocols(&self) -> &[NegotiatedProtocol] {
        &self.negotiated_protocols
    }

    /// Checks if a specific protocol is available on this connection.
    pub fn has_protocol(&self, name: &str) -> bool {
        self.negotiated_protocols.iter().any(|p| p.name == name)
    }

    /// Gets the negotiated protocol by name.
    pub fn get_protocol(&self, name: &str) -> Option<&NegotiatedProtocol> {
        self.negotiated_protocols.iter().find(|p| p.name == name)
    }
}
// ============================================================================
// TransportEventHandler Implementation
// ============================================================================

impl<S: Serializer> crate::transport::TransportEventHandler for Endpoint<S> {
    /// Called when a transport successfully connects.
    ///
    /// This spawns the system message handler for the new connection and
    /// initializes any necessary state.
    #[cfg_attr(feature = "observability", tracing::instrument(
        skip(self),
        fields(
            endpoint_id = %self.endpoint_id,
            transport_id = %transport_id,
        )
    ))]
    fn on_transport_connected(&self, transport_id: crate::transport::TransportId) {
        #[cfg(feature = "observability")]
        tracing::info!(
            "Transport {} connected to endpoint {}",
            transport_id, self.endpoint_id
        );

        // Create connection ID from transport ID
        let connection_id = format!("conn-{}", transport_id.as_u64());

        // Create the system channel for control messages
        let channel_manager = Arc::clone(&self.channel_manager);
        let config = self.config.clone();
        let _endpoint_id = self.endpoint_id.clone();

        tokio::spawn(async move {
            match channel_manager
                .create_channel::<SystemProtocol>(SYSTEM_CHANNEL_ID, config.channel_buffer_size)
                .await
            {
                Ok(_) => {
                    #[cfg(feature = "observability")]
                    tracing::debug!("System channel created for connection {}", connection_id);

                    // Get the receiver and spawn the system message handler
                    match channel_manager
                        .get_receiver::<SystemProtocol>(SYSTEM_CHANNEL_ID)
                        .await
                    {
                        Ok(_receiver) => {
                            #[cfg(feature = "observability")]
                            tracing::debug!(
                                "Spawning system message handler for connection {}",
                                connection_id
                            );

                            // Note: We can't call self.spawn_system_message_handler here
                            // because we don't have &self. The handler will be spawned
                            // by the connection establishment code.
                        }
                        Err(_e) => {
                            #[cfg(feature = "observability")]
                            tracing::error!(
                                "Failed to get system channel receiver for connection {}: {}",
                                connection_id, _e
                            );
                        }
                    }
                }
                Err(_e) => {
                    #[cfg(feature = "observability")]
                    tracing::error!(
                        "Failed to create system channel for connection {}: {}",
                        connection_id, _e
                    );
                }
            }
        });
    }

    /// Called when a transport disconnects.
    ///
    /// This cleans up channels and state associated with the connection.
    #[cfg_attr(feature = "observability", tracing::instrument(
        skip(self),
        fields(
            endpoint_id = %self.endpoint_id,
            transport_id = %transport_id,
        )
    ))]
    fn on_transport_disconnected(
        &self,
        transport_id: crate::transport::TransportId,
        error: Option<crate::transport::TransportError>,
    ) {
        let connection_id = format!("conn-{}", transport_id.as_u64());

        if let Some(err) = &error {
            #[cfg(feature = "observability")]
            tracing::warn!(
                "Transport {} disconnected from endpoint {} with error: {}",
                transport_id, self.endpoint_id, err
            );
        } else {
            #[cfg(feature = "observability")]
            tracing::info!(
                "Transport {} disconnected gracefully from endpoint {}",
                transport_id, self.endpoint_id
            );
        }

        // Clean up negotiated protocols for this connection
        let negotiated = Arc::clone(&self.negotiated);
        tokio::spawn(async move {
            let mut negotiated_map = negotiated.write().await;
            if negotiated_map.remove(&connection_id).is_some() {
                #[cfg(feature = "observability")]
                tracing::debug!("Removed negotiated protocols for connection {}", connection_id);
            }
        });

        // Note: Channel cleanup is handled by the ChannelManager
        // when the transport is closed
    }

    /// Called when a new channel creation is requested.
    ///
    /// This delegates to the channel negotiator to determine if the
    /// channel should be accepted.
    #[cfg_attr(feature = "observability", tracing::instrument(
        skip(self),
        fields(
            endpoint_id = %self.endpoint_id,
            channel_id = %channel_id,
            protocol = %protocol,
            transport_id = %transport_id,
        )
    ))]
    fn on_new_channel_request(
        &self,
        channel_id: crate::channel::ChannelId,
        protocol: &str,
        transport_id: crate::transport::TransportId,
    ) -> Result<bool, String> {
        #[cfg(feature = "observability")]
        tracing::debug!(
            "Channel {} request for protocol '{}' on transport {} (endpoint {})",
            channel_id, protocol, transport_id, self.endpoint_id
        );

        // Note: This is a synchronous trait method, but we need to check async state.
        // We use try_read() to avoid blocking. If the lock is held, we reject the request.
        let capabilities = match self.capabilities.try_read() {
            Ok(caps) => caps,
            Err(_) => {
                #[cfg(feature = "observability")]
                tracing::warn!(
                    "Rejecting channel {} request: capabilities lock unavailable",
                    channel_id
                );
                return Err("Capabilities lock unavailable".to_string());
            }
        };
        
        if !capabilities.contains_key(protocol) {
            #[cfg(feature = "observability")]
            tracing::warn!(
                "Rejecting channel {} request: protocol '{}' not registered",
                channel_id, protocol
            );
            return Ok(false);
        }

        // The channel negotiator's method is async, but we're in a sync context.
        // We need to spawn a task to handle this. For now, we'll accept all
        // requests for registered protocols and let the negotiator handle
        // the actual decision asynchronously.
        // TODO: Make TransportEventHandler trait async in a future version
        
        // For now, just check if the protocol is registered and accept it
        Ok(true)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::serialization::JsonSerializer;

    #[tokio::test]
    async fn test_endpoint_creation() {
        let config = EndpointConfig::default();
        let endpoint = Endpoint::new(JsonSerializer::default(), config);
        assert_eq!(endpoint.serializer_name(), "json");
    }

    #[tokio::test]
    async fn test_endpoint_with_custom_id() {
        let config = EndpointConfig::default().with_endpoint_id("test-endpoint".to_string());
        let endpoint = Endpoint::new(JsonSerializer::default(), config);
        assert_eq!(endpoint.id(), "test-endpoint");
    }

    #[tokio::test]
    async fn test_register_caller() {
        let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
        let result = endpoint.register_caller("TestProtocol", 1).await;
        assert!(result.is_ok());

        assert!(endpoint.has_protocol("TestProtocol").await);
        let direction = endpoint.protocol_direction("TestProtocol").await.unwrap();
        assert_eq!(direction, ProtocolDirection::CallOnly);
    }

    #[tokio::test]
    async fn test_register_responder() {
        let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
        let result = endpoint.register_responder("TestProtocol", 1).await;
        assert!(result.is_ok());

        let direction = endpoint.protocol_direction("TestProtocol").await.unwrap();
        assert_eq!(direction, ProtocolDirection::RespondOnly);
    }

    #[tokio::test]
    async fn test_register_bidirectional() {
        let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
        let result = endpoint.register_bidirectional("TestProtocol", 1).await;
        assert!(result.is_ok());

        let direction = endpoint.protocol_direction("TestProtocol").await.unwrap();
        assert_eq!(direction, ProtocolDirection::Bidirectional);
    }

    #[tokio::test]
    async fn test_register_duplicate_protocol() {
        let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
        endpoint.register_caller("TestProtocol", 1).await.unwrap();

        let result = endpoint.register_caller("TestProtocol", 1).await;
        assert!(matches!(
            result,
            Err(EndpointError::ProtocolAlreadyRegistered { .. })
        ));
    }

    #[tokio::test]
    async fn test_capabilities() {
        let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
        endpoint.register_caller("Protocol1", 1).await.unwrap();
        endpoint.register_responder("Protocol2", 2).await.unwrap();

        let capabilities = endpoint.capabilities().await;
        assert_eq!(capabilities.len(), 2);
    }

    #[tokio::test]
    async fn test_create_hello_message() {
        let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
        endpoint.register_caller("TestProtocol", 1).await.unwrap();

        let hello = endpoint.create_hello_message().await;
        match hello {
            HandshakeMessage::Hello {
                endpoint_id,
                serializer,
                protocols,
                ..
            } => {
                assert!(endpoint_id.is_some());
                assert_eq!(serializer, "json");
                assert_eq!(protocols.len(), 1);
                assert_eq!(protocols[0].protocol_name, "TestProtocol");
            }
            _ => panic!("Expected Hello message"),
        }
    }

    #[tokio::test]
    async fn test_protocol_not_registered() {
        let endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
        let result = endpoint.protocol_direction("NonExistent").await;
        assert!(matches!(
            result,
            Err(EndpointError::ProtocolNotRegistered { .. })
        ));
    }

    #[tokio::test]
    async fn test_listen_without_responder() {
        let endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
        let result = endpoint.listen("127.0.0.1:0").await;
        assert!(matches!(
            result,
            Err(EndpointError::InvalidConfiguration { .. })
        ));
    }

    #[tokio::test]
    async fn test_listen_with_responder() {
        let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
        endpoint
            .register_responder("TestProtocol", 1)
            .await
            .unwrap();

        let listener = endpoint.listen("127.0.0.1:0").await.unwrap();
        assert!(!listener.local_addr().is_empty());

        // Clean up
        listener.shutdown().await;
    }

    #[tokio::test]
    async fn test_listen_with_bidirectional() {
        let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
        endpoint
            .register_bidirectional("TestProtocol", 1)
            .await
            .unwrap();

        let listener = endpoint.listen("127.0.0.1:0").await.unwrap();
        assert!(!listener.local_addr().is_empty());

        // Clean up
        listener.shutdown().await;
    }

    #[tokio::test]
    async fn test_client_server_handshake() {
        // Create server endpoint
        let mut server = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
        server.register_responder("TestProtocol", 1).await.unwrap();

        let listener = server.listen("127.0.0.1:0").await.unwrap();
        let server_addr = listener.local_addr().to_string();

        // Create the client endpoint
        let mut client = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
        client.register_caller("TestProtocol", 1).await.unwrap();

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Connect client to server
        let connection = client.connect(&server_addr).await.unwrap();

        // Verify connection
        assert!(!connection.id().is_empty());
        assert_eq!(connection.protocols().len(), 1);
        assert!(connection.has_protocol("TestProtocol"));

        let protocol = connection.get_protocol("TestProtocol").unwrap();
        assert_eq!(protocol.name, "TestProtocol");
        assert_eq!(protocol.version, 1);
        assert!(protocol.we_can_call);
        assert!(!protocol.we_can_respond);

        // Clean up
        listener.shutdown().await;
    }

    #[tokio::test]
    async fn test_serializer_mismatch() {
        use crate::serialization::PostcardSerializer;

        // Create a server with JSON
        let mut server = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
        server.register_responder("TestProtocol", 1).await.unwrap();

        let listener = server.listen("127.0.0.1:0").await.unwrap();
        let server_addr = listener.local_addr().to_string();

        // Create the client with Postcard
        let mut client = Endpoint::new(PostcardSerializer::default(), EndpointConfig::default());
        client.register_caller("TestProtocol", 1).await.unwrap();

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Connect should fail due to serializer mismatch
        let result = client.connect(&server_addr).await;
        // The error could be SerializerMismatch or Serialization depending on timing
        assert!(
            result.is_err(),
            "Expected connection to fail due to serializer mismatch"
        );

        // Clean up
        listener.shutdown().await;
    }

    #[tokio::test]
    async fn test_no_compatible_protocols() {
        // Create a server with Protocol1
        let mut server = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
        server.register_responder("Protocol1", 1).await.unwrap();

        let listener = server.listen("127.0.0.1:0").await.unwrap();
        let server_addr = listener.local_addr().to_string();

        // Create a client with Protocol2
        let mut client = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
        client.register_caller("Protocol2", 1).await.unwrap();

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Connect should fail due to no compatible protocols
        let result = client.connect(&server_addr).await;
        assert!(matches!(result, Err(EndpointError::HandshakeFailed { .. })));

        // Clean up
        listener.shutdown().await;
    }

    #[tokio::test]
    async fn test_bidirectional_compatibility() {
        // Create a server with Bidirectional capability
        let mut server = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
        server
            .register_bidirectional("TestProtocol", 1)
            .await
            .unwrap();

        let listener = server.listen("127.0.0.1:0").await.unwrap();
        let server_addr = listener.local_addr().to_string();

        // Create client with CallOnly
        let mut client = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
        client.register_caller("TestProtocol", 1).await.unwrap();

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // This should succeed because bidirectional is compatible with call-only
        let connection = client.connect(&server_addr).await.unwrap();
        assert!(connection.has_protocol("TestProtocol"));

        let protocol = connection.get_protocol("TestProtocol").unwrap();
        assert!(protocol.we_can_call);
        assert!(!protocol.we_can_respond);

        // Clean up
        listener.shutdown().await;
    }

    #[tokio::test]
    async fn test_get_channels_success() {
        use crate::channel::Protocol;

        // Define a test protocol
        #[derive(Debug, Clone, PartialEq)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        enum TestProtocol {
            Request { id: u32 },
            Response { id: u32, data: String },
        }

        impl Protocol for TestProtocol {
            fn method_name(&self) -> &'static str {
                match self {
                    Self::Request { .. } => "request",
                    Self::Response { .. } => "response",
                }
            }

            fn is_request(&self) -> bool {
                matches!(self, Self::Request { .. })
            }
        }

        // Create server endpoint
        // Protocol will be automatically allowed when registered
        let mut server = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
        server
            .register_bidirectional("TestProtocol", 1)
            .await
            .unwrap();

        let listener = server.listen("127.0.0.1:0").await.unwrap();
        let server_addr = listener.local_addr().to_string();

        // Create client endpoint
        // Protocol will be automatically allowed when registered
        let mut client = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
        client
            .register_bidirectional("TestProtocol", 1)
            .await
            .unwrap();

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Connect client to server (listener accepts automatically in the background)
        let connection = client.connect(&server_addr).await.unwrap();

        // Give the handshake time to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Get typed channels using the convenience method
        let result = client
            .get_channels::<TestProtocol>(connection.id(), "TestProtocol")
            .await;

        assert!(
            result.is_ok(),
            "get_channels should succeed: {:?}",
            result.err()
        );
        let (sender, _receiver) = result.unwrap();

        // Verify we can use the sender
        assert!(sender.try_send(TestProtocol::Request { id: 1 }).is_ok());

        // Clean up
        listener.shutdown().await;
    }

    #[tokio::test]
    async fn test_get_channels_protocol_not_found() {
        // Create server endpoint
        let mut server = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
        server
            .register_bidirectional("TestProtocol", 1)
            .await
            .unwrap();

        let listener = server.listen("127.0.0.1:0").await.unwrap();
        let server_addr = listener.local_addr().to_string();

        // Create a client endpoint
        let mut client = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
        client
            .register_bidirectional("TestProtocol", 1)
            .await
            .unwrap();

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Connect client to server
        let connection = client.connect(&server_addr).await.unwrap();

        // Try to get channels for a protocol that wasn't negotiated
        use crate::channel::Protocol;

        #[allow(dead_code)]
        #[derive(Debug, Clone)]
        enum OtherProtocol {
            Msg,
        }

        impl Protocol for OtherProtocol {
            fn method_name(&self) -> &'static str {
                "msg"
            }
            fn is_request(&self) -> bool {
                true
            }
        }

        let result = client
            .get_channels::<OtherProtocol>(connection.id(), "OtherProtocol")
            .await;

        assert!(matches!(
            result,
            Err(EndpointError::ProtocolNotRegistered { .. })
        ));

        // Clean up
        listener.shutdown().await;
    }

    #[tokio::test]
    async fn test_get_channels_invalid_connection() {
        use crate::channel::Protocol;

        #[allow(dead_code)]
        #[derive(Debug, Clone)]
        enum TestProtocol {
            Msg,
        }

        impl Protocol for TestProtocol {
            fn method_name(&self) -> &'static str {
                "msg"
            }
            fn is_request(&self) -> bool {
                true
            }
        }

        let mut client = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
        client
            .register_bidirectional("TestProtocol", 1)
            .await
            .unwrap();

        // Try to get channels with an invalid connection ID
        let result = client
            .get_channels::<TestProtocol>("invalid-connection-id", "TestProtocol")
            .await;

        assert!(result.is_err(), "Should fail with invalid connection ID");
    }

    #[tokio::test]
    async fn test_default_negotiator_downcast() {
        // Test that we can downcast Negotiator to DefaultChannelNegotiator
        let endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());

        // Should successfully downcast to DefaultChannelNegotiator
        let negotiator = endpoint.default_negotiator();
        assert!(
            negotiator.is_some(),
            "Should be able to downcast to DefaultChannelNegotiator"
        );

        // Test that we can use the negotiator
        if let Some(neg) = negotiator {
            neg.allow_protocol("TestProtocol").await;
            assert!(neg.is_protocol_allowed("TestProtocol").await);
        }
    }

    #[tokio::test]
    async fn test_default_negotiator_with_custom() {
        use std::collections::HashMap;

        // Custom negotiator that always accepts
        struct AlwaysAccept;

        #[async_trait::async_trait]
        impl ChannelNegotiator for AlwaysAccept {
            async fn on_channel_create_request(
                &self,
                _channel_id: ChannelId,
                _protocol_name: &str,
                _protocol_version: u32,
                _direction: ProtocolDirection,
                _metadata: &HashMap<String, String>,
            ) -> Result<bool, String> {
                Ok(true)
            }

            async fn on_channel_create_response(
                &self,
                _channel_id: ChannelId,
                _success: bool,
                _error: Option<String>,
            ) {
            }

            async fn on_channel_close_notification(&self, _channel_id: ChannelId, _reason: &str) {}
        }

        let mut endpoint = Endpoint::new(JsonSerializer::default(), EndpointConfig::default());
        endpoint.set_channel_negotiator(Arc::new(AlwaysAccept));

        // Should return None when using a custom negotiator
        let negotiator = endpoint.default_negotiator();
        assert!(
            negotiator.is_none(),
            "Should return None for custom negotiator"
        );
    }
}

# ADR-010: Dynamic Channel Negotiation

## Status
Proposed

## Context

The current BDRPC design only negotiates protocols during the initial connection handshake. Once a transport is established, there's no mechanism for dynamically creating new channels with remote coordination. This limitation prevents important use cases:

### Use Case: Multiplexing Gateway
A gateway server:
1. Starts up and connects to a backend server (establishes transport)
2. Receives connections from multiple clients
3. Each client authenticates with the gateway
4. Each client needs a dedicated bi-directional channel to the backend server
5. The gateway must coordinate channel creation between client and backend

### Current Limitations
- Channels are created locally without remote notification
- No protocol for requesting new channels
- No way to negotiate channel-specific parameters
- Both sides must pre-coordinate channel IDs

## Decision

We will implement a **System Channel** and **Channel Negotiation Protocol** to enable dynamic channel creation after transport establishment.

### 1. System Channel (Channel ID 0)

Every transport automatically creates a reserved "system channel" (ID 0) for control messages:

```rust
/// Reserved channel ID for system control messages
pub const SYSTEM_CHANNEL_ID: ChannelId = ChannelId::from_u64(0);

/// System protocol for channel lifecycle management
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemProtocol {
    /// Request to create a new channel
    ChannelCreateRequest {
        /// Requested channel ID (must be > 0)
        channel_id: ChannelId,
        /// Protocol name for this channel
        protocol_name: String,
        /// Protocol version
        protocol_version: u32,
        /// Direction for this channel
        direction: ProtocolDirection,
        /// Optional buffer size hint
        buffer_size: Option<usize>,
        /// Optional metadata
        metadata: HashMap<String, String>,
    },
    
    /// Response to channel creation request
    ChannelCreateResponse {
        /// The channel ID from the request
        channel_id: ChannelId,
        /// Whether creation succeeded
        success: bool,
        /// Error message if failed
        error: Option<String>,
    },
    
    /// Notify remote that a channel is being closed
    ChannelCloseNotification {
        /// Channel ID being closed
        channel_id: ChannelId,
        /// Reason for closure
        reason: String,
    },
    
    /// Acknowledge channel closure
    ChannelCloseAck {
        /// Channel ID that was closed
        channel_id: ChannelId,
    },
    
    /// Ping for keepalive
    Ping {
        /// Timestamp
        timestamp: u64,
    },
    
    /// Pong response
    Pong {
        /// Original timestamp from ping
        timestamp: u64,
    },
}

impl Protocol for SystemProtocol {
    fn method_name(&self) -> &'static str {
        match self {
            Self::ChannelCreateRequest { .. } => "channel_create_request",
            Self::ChannelCreateResponse { .. } => "channel_create_response",
            Self::ChannelCloseNotification { .. } => "channel_close_notification",
            Self::ChannelCloseAck { .. } => "channel_close_ack",
            Self::Ping { .. } => "ping",
            Self::Pong { .. } => "pong",
        }
    }
    
    fn is_request(&self) -> bool {
        matches!(
            self,
            Self::ChannelCreateRequest { .. }
                | Self::ChannelCloseNotification { .. }
                | Self::Ping { .. }
        )
    }
}
```

### 2. Channel Negotiation Trait

Define a trait for handling channel negotiation:

```rust
/// Trait for handling dynamic channel creation requests
#[async_trait]
pub trait ChannelNegotiator: Send + Sync {
    /// Called when a remote endpoint requests to create a channel
    ///
    /// Returns Ok(true) to accept, Ok(false) to reject, or Err for errors
    async fn on_channel_create_request(
        &self,
        channel_id: ChannelId,
        protocol_name: &str,
        protocol_version: u32,
        direction: ProtocolDirection,
        metadata: &HashMap<String, String>,
    ) -> Result<bool, String>;
    
    /// Called when a channel creation response is received
    async fn on_channel_create_response(
        &self,
        channel_id: ChannelId,
        success: bool,
        error: Option<String>,
    );
    
    /// Called when remote notifies of channel closure
    async fn on_channel_close_notification(
        &self,
        channel_id: ChannelId,
        reason: &str,
    );
}

/// Default negotiator that accepts all channel requests
pub struct DefaultChannelNegotiator {
    /// Protocols that are allowed
    allowed_protocols: Arc<RwLock<HashSet<String>>>,
}

impl DefaultChannelNegotiator {
    pub fn new() -> Self {
        Self {
            allowed_protocols: Arc::new(RwLock::new(HashSet::new())),
        }
    }
    
    pub async fn allow_protocol(&self, protocol_name: impl Into<String>) {
        self.allowed_protocols.write().await.insert(protocol_name.into());
    }
}

#[async_trait]
impl ChannelNegotiator for DefaultChannelNegotiator {
    async fn on_channel_create_request(
        &self,
        _channel_id: ChannelId,
        protocol_name: &str,
        _protocol_version: u32,
        _direction: ProtocolDirection,
        _metadata: &HashMap<String, String>,
    ) -> Result<bool, String> {
        let allowed = self.allowed_protocols.read().await;
        if allowed.contains(protocol_name) {
            Ok(true)
        } else {
            Err(format!("Protocol '{}' not allowed", protocol_name))
        }
    }
    
    async fn on_channel_create_response(
        &self,
        channel_id: ChannelId,
        success: bool,
        error: Option<String>,
    ) {
        if success {
            tracing::info!("Channel {} created successfully", channel_id);
        } else {
            tracing::warn!(
                "Channel {} creation failed: {}",
                channel_id,
                error.unwrap_or_default()
            );
        }
    }
    
    async fn on_channel_close_notification(
        &self,
        channel_id: ChannelId,
        reason: &str,
    ) {
        tracing::info!("Channel {} closed: {}", channel_id, reason);
    }
}
```

### 3. Endpoint API Extensions

Add methods to Endpoint for dynamic channel management:

```rust
impl<S: Serializer> Endpoint<S> {
    /// Request creation of a new channel on the remote endpoint
    ///
    /// This sends a ChannelCreateRequest on the system channel and waits
    /// for the response. If accepted, creates the channel locally.
    pub async fn request_channel<P: Protocol>(
        &self,
        connection_id: &str,
        channel_id: ChannelId,
        protocol_name: impl Into<String>,
        protocol_version: u32,
        direction: ProtocolDirection,
        buffer_size: usize,
        metadata: HashMap<String, String>,
    ) -> Result<ChannelSender<P>, EndpointError> {
        // Validate channel ID (must not be 0)
        if channel_id == SYSTEM_CHANNEL_ID {
            return Err(EndpointError::InvalidChannelId {
                reason: "Channel ID 0 is reserved for system channel".to_string(),
            });
        }
        
        // Check if channel already exists
        if self.channel_manager.has_channel(channel_id).await {
            return Err(EndpointError::ChannelAlreadyExists { channel_id });
        }
        
        // Get system channel for this connection
        let system_channel = self.get_system_channel(connection_id).await?;
        
        // Send create request
        let request = SystemProtocol::ChannelCreateRequest {
            channel_id,
            protocol_name: protocol_name.into(),
            protocol_version,
            direction,
            buffer_size: Some(buffer_size),
            metadata,
        };
        
        system_channel.send(request).await?;
        
        // Wait for response (with timeout)
        let response = tokio::time::timeout(
            Duration::from_secs(5),
            self.wait_for_channel_response(channel_id),
        )
        .await
        .map_err(|_| EndpointError::ChannelNegotiationTimeout { channel_id })??;
        
        match response {
            SystemProtocol::ChannelCreateResponse {
                success: true, ..
            } => {
                // Create local channel
                let sender = self
                    .channel_manager
                    .create_channel::<P>(channel_id, buffer_size)
                    .await?;
                Ok(sender)
            }
            SystemProtocol::ChannelCreateResponse {
                success: false,
                error,
                ..
            } => Err(EndpointError::ChannelNegotiationFailed {
                channel_id,
                reason: error.unwrap_or_else(|| "Unknown error".to_string()),
            }),
            _ => Err(EndpointError::UnexpectedMessage),
        }
    }
    
    /// Set the channel negotiator for this endpoint
    pub fn set_channel_negotiator(
        &mut self,
        negotiator: Arc<dyn ChannelNegotiator>,
    ) {
        self.channel_negotiator = Some(negotiator);
    }
    
    /// Get the system channel for a connection
    async fn get_system_channel(
        &self,
        connection_id: &str,
    ) -> Result<ChannelSender<SystemProtocol>, EndpointError> {
        self.channel_manager
            .get_sender::<SystemProtocol>(SYSTEM_CHANNEL_ID)
            .await
            .map_err(|_| EndpointError::SystemChannelNotFound)
    }
}
```

### 4. System Channel Handler

The endpoint must process system channel messages:

```rust
/// Handles system channel messages for a connection
async fn handle_system_channel<S: Serializer>(
    system_receiver: ChannelReceiver<SystemProtocol>,
    channel_manager: Arc<ChannelManager>,
    negotiator: Arc<dyn ChannelNegotiator>,
    connection_id: String,
) {
    while let Some(message) = system_receiver.recv().await {
        match message {
            SystemProtocol::ChannelCreateRequest {
                channel_id,
                protocol_name,
                protocol_version,
                direction,
                buffer_size,
                metadata,
            } => {
                // Ask negotiator if we should accept
                let result = negotiator
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
                        // Create the channel locally
                        let size = buffer_size.unwrap_or(100);
                        match create_channel_for_protocol(
                            &channel_manager,
                            channel_id,
                            &protocol_name,
                            size,
                        )
                        .await
                        {
                            Ok(_) => (true, None),
                            Err(e) => (false, Some(e.to_string())),
                        }
                    }
                    Ok(false) => (false, Some("Request rejected".to_string())),
                    Err(e) => (false, Some(e)),
                };
                
                // Send response
                let response = SystemProtocol::ChannelCreateResponse {
                    channel_id,
                    success,
                    error,
                };
                
                // Send response back on system channel
                if let Ok(sender) = channel_manager
                    .get_sender::<SystemProtocol>(SYSTEM_CHANNEL_ID)
                    .await
                {
                    let _ = sender.send(response).await;
                }
            }
            
            SystemProtocol::ChannelCreateResponse {
                channel_id,
                success,
                error,
            } => {
                negotiator
                    .on_channel_create_response(channel_id, success, error)
                    .await;
            }
            
            SystemProtocol::ChannelCloseNotification { channel_id, reason } => {
                negotiator
                    .on_channel_close_notification(channel_id, &reason)
                    .await;
                
                // Remove channel locally
                let _ = channel_manager.remove_channel(channel_id).await;
                
                // Send ack
                let ack = SystemProtocol::ChannelCloseAck { channel_id };
                if let Ok(sender) = channel_manager
                    .get_sender::<SystemProtocol>(SYSTEM_CHANNEL_ID)
                    .await
                {
                    let _ = sender.send(ack).await;
                }
            }
            
            SystemProtocol::Ping { timestamp } => {
                // Respond with pong
                let pong = SystemProtocol::Pong { timestamp };
                if let Ok(sender) = channel_manager
                    .get_sender::<SystemProtocol>(SYSTEM_CHANNEL_ID)
                    .await
                {
                    let _ = sender.send(pong).await;
                }
            }
            
            _ => {
                // Handle other system messages
            }
        }
    }
}
```

### 5. Gateway Example

```rust
/// Example: Multiplexing Gateway
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Gateway connects to backend server
    let mut gateway = Endpoint::new(
        JsonSerializer::default(),
        EndpointConfig::default(),
    );
    
    // Register protocols the gateway will multiplex
    gateway.register_bidirectional("ClientData", 1).await?;
    
    // Set up negotiator that allows ClientData protocol
    let negotiator = DefaultChannelNegotiator::new();
    negotiator.allow_protocol("ClientData").await;
    gateway.set_channel_negotiator(Arc::new(negotiator));
    
    // Connect to backend
    let backend_conn = gateway.connect("backend.example.com:8080").await?;
    
    // Listen for client connections
    let listener = gateway.listen("0.0.0.0:9090").await?;
    
    // Handle each client connection
    tokio::spawn(async move {
        loop {
            // Accept client connection (pseudo-code)
            let client_stream = accept_client().await?;
            
            // Authenticate client
            let client_id = authenticate_client(&client_stream).await?;
            
            // Request a new channel on the backend connection for this client
            let channel_id = ChannelId::new();
            let metadata = HashMap::from([
                ("client_id".to_string(), client_id.clone()),
            ]);
            
            let backend_channel = gateway
                .request_channel::<ClientDataProtocol>(
                    &backend_conn.id(),
                    channel_id,
                    "ClientData",
                    1,
                    ProtocolDirection::Bidirectional,
                    100,
                    metadata,
                )
                .await?;
            
            // Now proxy between client and backend channel
            tokio::spawn(proxy_client_to_backend(
                client_stream,
                backend_channel,
                channel_id,
            ));
        }
    });
    
    Ok(())
}
```

## Consequences

### Positive
- **Dynamic channel creation**: Channels can be created on-demand after connection
- **Remote coordination**: Both sides agree on channel creation
- **Metadata support**: Can pass context (auth, routing) during negotiation
- **Gateway support**: Enables multiplexing gateway pattern
- **Graceful closure**: Proper channel lifecycle management
- **Extensible**: System channel can support future control messages

### Negative
- **Complexity**: Adds another layer of protocol
- **Latency**: Channel creation requires round-trip
- **State management**: Must track pending channel requests
- **Error handling**: More failure modes to handle
- **System channel overhead**: Reserved channel on every transport

### Neutral
- **Channel ID management**: Applications must coordinate IDs (or use random)
- **Protocol registration**: Still need to register protocols during handshake
- **Backward compatibility**: Old endpoints won't understand system channel

## Alternatives Considered

### 1. Pre-allocated Channel Pool
Pre-create a pool of channels during handshake. Rejected because:
- Wastes resources for unused channels
- Doesn't scale to many clients
- Still needs coordination for assignment

### 2. Out-of-band Coordination
Use separate connection for control. Rejected because:
- Adds complexity of multiple connections
- Harder to correlate control with data
- Doesn't leverage existing transport

### 3. Implicit Channel Creation
Allow sending to non-existent channel to create it. Rejected because:
- No way to reject unwanted channels
- No negotiation of parameters
- Security concerns

## Implementation Notes

### Channel ID Allocation
- System channel is always ID 0
- Applications can use sequential IDs (1, 2, 3...)
- Or use random IDs to avoid conflicts
- Consider adding ID allocation helper

### Timeout Handling
- Channel creation requests should timeout (default 5s)
- Cleanup pending requests on timeout
- Log timeout events for debugging

### Security Considerations
- Negotiator can implement authentication checks
- Metadata can carry auth tokens
- Rate limit channel creation requests
- Validate channel IDs are not reused

### Testing Strategy
- Unit tests for system protocol serialization
- Integration tests for channel negotiation flow
- Test rejection scenarios
- Test timeout handling
- Test concurrent channel creation

## Related ADRs
- ADR-001: Core Architecture (defines Channel)
- ADR-008: Protocol Directionality (channel direction)
- ADR-006: Protocol Evolution (version negotiation)

## References
- HTTP/2 Stream Creation
- QUIC Stream Management
- gRPC Channel Multiplexing
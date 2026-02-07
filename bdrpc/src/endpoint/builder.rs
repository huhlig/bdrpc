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

//! Builder pattern for ergonomic endpoint configuration.

use crate::endpoint::{Endpoint, EndpointConfig, EndpointError, ProtocolDirection};
use crate::reconnection::ReconnectionStrategy;
use crate::serialization::Serializer;
use crate::transport::{TransportConfig, TransportType};
use std::sync::Arc;

/// A protocol registration to be applied when building the endpoint.
#[derive(Debug, Clone)]
struct ProtocolRegistration {
    name: String,
    version: u32,
    direction: ProtocolDirection,
}

/// A transport configuration to be applied when building the endpoint.
#[derive(Clone)]
struct TransportRegistration {
    name: String,
    config: TransportConfig,
    is_listener: bool,
}

/// Builder for creating and configuring endpoints.
///
/// `EndpointBuilder` provides a fluent API for creating endpoints with
/// protocols pre-registered. This is more ergonomic than creating an endpoint
/// and then registering protocols separately.
///
/// # Examples
///
/// ## Basic Client
///
/// ```rust
/// use bdrpc::endpoint::EndpointBuilder;
/// use bdrpc::serialization::PostcardSerializer;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let endpoint = EndpointBuilder::new(PostcardSerializer::default())
///     .with_caller("UserService", 1)
///     .with_caller("OrderService", 1)
///     .build()
///     .await?;
/// # Ok(())
/// # }
/// ```
///
/// ## Basic Server
///
/// ```rust
/// use bdrpc::endpoint::EndpointBuilder;
/// use bdrpc::serialization::PostcardSerializer;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let endpoint = EndpointBuilder::new(PostcardSerializer::default())
///     .with_responder("UserService", 1)
///     .with_responder("OrderService", 1)
///     .build()
///     .await?;
/// # Ok(())
/// # }
/// ```
///
/// ## Peer-to-Peer
///
/// ```rust
/// use bdrpc::endpoint::EndpointBuilder;
/// use bdrpc::serialization::PostcardSerializer;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let endpoint = EndpointBuilder::new(PostcardSerializer::default())
///     .with_bidirectional("ChatProtocol", 1)
///     .with_bidirectional("FileTransfer", 1)
///     .build()
///     .await?;
/// # Ok(())
/// # }
/// ```
///
/// ## Custom Configuration
///
/// ```rust
/// use bdrpc::endpoint::{EndpointBuilder, EndpointConfig};
/// use bdrpc::serialization::PostcardSerializer;
/// use std::time::Duration;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = EndpointConfig::new()
///     .with_channel_buffer_size(1000)
///     .with_handshake_timeout(Duration::from_secs(10));
///
/// let endpoint = EndpointBuilder::with_config(PostcardSerializer::default(), config)
///     .with_caller("UserService", 1)
///     .build()
///     .await?;
/// # Ok(())
/// # }
/// ```
///
/// ## Preset Configurations
///
/// ```rust
/// use bdrpc::endpoint::EndpointBuilder;
/// use bdrpc::serialization::PostcardSerializer;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Client preset - optimized for outgoing calls
/// let client = EndpointBuilder::client(PostcardSerializer::default())
///     .with_caller("UserService", 1)
///     .build()
///     .await?;
///
/// // Server preset - optimized for incoming requests
/// let server = EndpointBuilder::server(PostcardSerializer::default())
///     .with_responder("UserService", 1)
///     .build()
///     .await?;
///
/// // Peer preset - optimized for bidirectional communication
/// let peer = EndpointBuilder::peer(PostcardSerializer::default())
///     .with_bidirectional("ChatProtocol", 1)
///     .build()
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct EndpointBuilder<S: Serializer> {
    serializer: S,
    config: EndpointConfig,
    protocols: Vec<ProtocolRegistration>,
    transports: Vec<TransportRegistration>,
}

impl<S: Serializer> EndpointBuilder<S> {
    /// Creates a new endpoint builder with the given serializer and default configuration.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::EndpointBuilder;
    /// use bdrpc::serialization::JsonSerializer;
    ///
    /// let builder = EndpointBuilder::new(JsonSerializer::default());
    /// ```
    pub fn new(serializer: S) -> Self {
        Self {
            serializer,
            config: EndpointConfig::default(),
            protocols: Vec::new(),
            transports: Vec::new(),
        }
    }

    /// Creates a new endpoint builder with the given serializer and custom configuration.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::{EndpointBuilder, EndpointConfig};
    /// use bdrpc::serialization::JsonSerializer;
    /// use std::time::Duration;
    ///
    /// let config = EndpointConfig::new()
    ///     .with_channel_buffer_size(1000)
    ///     .with_handshake_timeout(Duration::from_secs(10));
    ///
    /// let builder = EndpointBuilder::with_config(JsonSerializer::default(), config);
    /// ```
    pub fn with_config(serializer: S, config: EndpointConfig) -> Self {
        Self {
            serializer,
            config,
            protocols: Vec::new(),
            transports: Vec::new(),
        }
    }

    /// Creates a client-optimized endpoint builder.
    ///
    /// Client endpoints are optimized for making outgoing calls. This preset
    /// uses default configuration suitable for client applications.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::EndpointBuilder;
    /// use bdrpc::serialization::PostcardSerializer;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = EndpointBuilder::client(PostcardSerializer::default())
    ///     .with_caller("UserService", 1)
    ///     .with_caller("OrderService", 1)
    ///     .build()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn client(serializer: S) -> Self {
        let config =
            EndpointConfig::new().with_endpoint_id(format!("client-{}", uuid::Uuid::new_v4()));
        Self::with_config(serializer, config)
    }

    /// Creates a server-optimized endpoint builder.
    ///
    /// Server endpoints are optimized for handling incoming requests. This preset
    /// uses configuration suitable for server applications with connection limits.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::EndpointBuilder;
    /// use bdrpc::serialization::PostcardSerializer;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let server = EndpointBuilder::server(PostcardSerializer::default())
    ///     .with_responder("UserService", 1)
    ///     .with_responder("OrderService", 1)
    ///     .build()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn server(serializer: S) -> Self {
        let config = EndpointConfig::new()
            .with_endpoint_id(format!("server-{}", uuid::Uuid::new_v4()))
            .with_max_connections(Some(1000)); // Reasonable default for servers
        Self::with_config(serializer, config)
    }

    /// Creates a peer-optimized endpoint builder.
    ///
    /// Peer endpoints are optimized for bidirectional communication where both
    /// sides can initiate calls. This preset uses configuration suitable for
    /// peer-to-peer applications.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::EndpointBuilder;
    /// use bdrpc::serialization::PostcardSerializer;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let peer = EndpointBuilder::peer(PostcardSerializer::default())
    ///     .with_bidirectional("ChatProtocol", 1)
    ///     .with_bidirectional("FileTransfer", 1)
    ///     .build()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn peer(serializer: S) -> Self {
        let config = EndpointConfig::new()
            .with_endpoint_id(format!("peer-{}", uuid::Uuid::new_v4()))
            .with_channel_buffer_size(200); // Larger buffer for bidirectional traffic
        Self::with_config(serializer, config)
    }

    /// Registers a protocol with call-only support.
    ///
    /// This allows the endpoint to call methods on this protocol but not
    /// respond to them. Typical for client endpoints.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::EndpointBuilder;
    /// use bdrpc::serialization::JsonSerializer;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let endpoint = EndpointBuilder::new(JsonSerializer::default())
    ///     .with_caller("UserService", 1)
    ///     .with_caller("OrderService", 2)
    ///     .build()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_caller(mut self, protocol_name: impl Into<String>, version: u32) -> Self {
        self.protocols.push(ProtocolRegistration {
            name: protocol_name.into(),
            version,
            direction: ProtocolDirection::CallOnly,
        });
        self
    }

    /// Registers a protocol with respond-only support.
    ///
    /// This allows the endpoint to respond to methods on this protocol but
    /// not call them. Typical for server endpoints.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::EndpointBuilder;
    /// use bdrpc::serialization::JsonSerializer;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let endpoint = EndpointBuilder::new(JsonSerializer::default())
    ///     .with_responder("UserService", 1)
    ///     .with_responder("OrderService", 2)
    ///     .build()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_responder(mut self, protocol_name: impl Into<String>, version: u32) -> Self {
        self.protocols.push(ProtocolRegistration {
            name: protocol_name.into(),
            version,
            direction: ProtocolDirection::RespondOnly,
        });
        self
    }

    /// Registers a protocol with bidirectional support.
    ///
    /// This allows the endpoint to both call and respond to methods on this
    /// protocol. Typical for peer-to-peer endpoints.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::EndpointBuilder;
    /// use bdrpc::serialization::JsonSerializer;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let endpoint = EndpointBuilder::new(JsonSerializer::default())
    ///     .with_bidirectional("ChatProtocol", 1)
    ///     .with_bidirectional("FileTransfer", 2)
    ///     .build()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_bidirectional(mut self, protocol_name: impl Into<String>, version: u32) -> Self {
        self.protocols.push(ProtocolRegistration {
            name: protocol_name.into(),
            version,
            direction: ProtocolDirection::Bidirectional,
        });
        self
    }

    /// Modifies the endpoint configuration.
    ///
    /// This allows you to customize the configuration after creating the builder.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::EndpointBuilder;
    /// use bdrpc::serialization::JsonSerializer;
    /// use std::time::Duration;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let endpoint = EndpointBuilder::new(JsonSerializer::default())
    ///     .configure(|config| {
    ///         config
    ///             .with_channel_buffer_size(1000)
    ///             .with_handshake_timeout(Duration::from_secs(10))
    ///     })
    ///     .with_caller("UserService", 1)
    ///     .build()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn configure<F>(mut self, f: F) -> Self
    where
        F: FnOnce(EndpointConfig) -> EndpointConfig,
    {
        self.config = f(self.config);
        self
    }

    /// Adds a TCP listener transport.
    ///
    /// This configures the endpoint to listen for incoming TCP connections
    /// on the specified address.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::EndpointBuilder;
    /// use bdrpc::serialization::PostcardSerializer;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let endpoint = EndpointBuilder::server(PostcardSerializer::default())
    ///     .with_tcp_listener("0.0.0.0:8080")
    ///     .with_responder("UserService", 1)
    ///     .build()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_tcp_listener(mut self, addr: impl Into<String>) -> Self {
        let config = TransportConfig::new(TransportType::Tcp, addr);
        self.transports.push(TransportRegistration {
            name: format!("tcp-listener-{}", self.transports.len()),
            config,
            is_listener: true,
        });
        self
    }

    /// Adds a TLS listener transport.
    ///
    /// This configures the endpoint to listen for incoming TLS-encrypted
    /// connections on the specified address.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::endpoint::EndpointBuilder;
    /// use bdrpc::serialization::PostcardSerializer;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let endpoint = EndpointBuilder::server(PostcardSerializer::default())
    ///     .with_tls_listener("0.0.0.0:8443")
    ///     .with_responder("UserService", 1)
    ///     .build()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "tls")]
    pub fn with_tls_listener(mut self, addr: impl Into<String>) -> Self {
        let config = TransportConfig::new(TransportType::Tls, addr);
        self.transports.push(TransportRegistration {
            name: format!("tls-listener-{}", self.transports.len()),
            config,
            is_listener: true,
        });
        self
    }

    /// Adds a TCP caller transport.
    ///
    /// This configures the endpoint to connect to a remote TCP server.
    /// The connection can be initiated later using `connect_transport()`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::EndpointBuilder;
    /// use bdrpc::serialization::PostcardSerializer;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut endpoint = EndpointBuilder::client(PostcardSerializer::default())
    ///     .with_tcp_caller("main", "127.0.0.1:8080")
    ///     .with_caller("UserService", 1)
    ///     .build()
    ///     .await?;
    ///
    /// let conn = endpoint.connect_transport("main").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_tcp_caller(mut self, name: impl Into<String>, addr: impl Into<String>) -> Self {
        let config = TransportConfig::new(TransportType::Tcp, addr);
        self.transports.push(TransportRegistration {
            name: name.into(),
            config,
            is_listener: false,
        });
        self
    }

    /// Adds a TLS caller transport.
    ///
    /// This configures the endpoint to connect to a remote TLS server.
    /// The connection can be initiated later using `connect_transport()`.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::endpoint::EndpointBuilder;
    /// use bdrpc::serialization::PostcardSerializer;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut endpoint = EndpointBuilder::client(PostcardSerializer::default())
    ///     .with_tls_caller("secure", "127.0.0.1:8443")
    ///     .with_caller("UserService", 1)
    ///     .build()
    ///     .await?;
    ///
    /// let conn = endpoint.connect_transport("secure").await?;
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "tls")]
    pub fn with_tls_caller(mut self, name: impl Into<String>, addr: impl Into<String>) -> Self {
        let config = TransportConfig::new(TransportType::Tls, addr);
        self.transports.push(TransportRegistration {
            name: name.into(),
            config,
            is_listener: false,
        });
        self
    }

    /// Adds a custom transport configuration.
    ///
    /// This allows you to configure a transport with custom settings,
    /// including reconnection strategies and metadata.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::EndpointBuilder;
    /// use bdrpc::serialization::PostcardSerializer;
    /// use bdrpc::transport::{TransportConfig, TransportType};
    /// use bdrpc::reconnection::ExponentialBackoff;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let strategy = Arc::new(ExponentialBackoff::default());
    /// let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080")
    ///     .with_reconnection_strategy(strategy)
    ///     .with_metadata("region", "us-west");
    ///
    /// let mut endpoint = EndpointBuilder::client(PostcardSerializer::default())
    ///     .with_transport("main", config, false)
    ///     .with_caller("UserService", 1)
    ///     .build()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_transport(
        mut self,
        name: impl Into<String>,
        config: TransportConfig,
        is_listener: bool,
    ) -> Self {
        self.transports.push(TransportRegistration {
            name: name.into(),
            config,
            is_listener,
        });
        self
    }

    /// Sets the reconnection strategy for a named caller transport.
    ///
    /// This must be called after adding the transport with `with_tcp_caller()`,
    /// `with_tls_caller()`, or `with_transport()`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::EndpointBuilder;
    /// use bdrpc::serialization::PostcardSerializer;
    /// use bdrpc::reconnection::ExponentialBackoff;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let strategy = Arc::new(ExponentialBackoff::default());
    ///
    /// let mut endpoint = EndpointBuilder::client(PostcardSerializer::default())
    ///     .with_tcp_caller("main", "127.0.0.1:8080")
    ///     .with_reconnection_strategy("main", strategy)
    ///     .with_caller("UserService", 1)
    ///     .build()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_reconnection_strategy(
        mut self,
        name: &str,
        strategy: Arc<dyn ReconnectionStrategy>,
    ) -> Self {
        if let Some(transport) = self.transports.iter_mut().find(|t| t.name == name) {
            transport.config = transport
                .config
                .clone()
                .with_reconnection_strategy(strategy);
        }
        self
    }

    /// Builds the endpoint with all registered protocols.
    ///
    /// This creates the endpoint and registers all protocols that were added
    /// via `with_caller`, `with_responder`, or `with_bidirectional`.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The configuration is invalid
    /// - A protocol is registered multiple times
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::EndpointBuilder;
    /// use bdrpc::serialization::JsonSerializer;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let endpoint = EndpointBuilder::new(JsonSerializer::default())
    ///     .with_caller("UserService", 1)
    ///     .build()
    ///     .await?;
    ///
    /// assert!(endpoint.has_protocol("UserService").await);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn build(self) -> Result<Endpoint<S>, EndpointError> {
        // Create the endpoint
        let mut endpoint = Endpoint::new(self.serializer, self.config);

        // Register all protocols
        for protocol in self.protocols {
            match protocol.direction {
                ProtocolDirection::CallOnly => {
                    endpoint
                        .register_caller(protocol.name, protocol.version)
                        .await?;
                }
                ProtocolDirection::RespondOnly => {
                    endpoint
                        .register_responder(protocol.name, protocol.version)
                        .await?;
                }
                ProtocolDirection::Bidirectional => {
                    endpoint
                        .register_bidirectional(protocol.name, protocol.version)
                        .await?;
                }
            }
        }

        // Add all transports
        for transport in self.transports {
            if transport.is_listener {
                endpoint
                    .add_listener(transport.name, transport.config)
                    .await?;
            } else {
                endpoint.add_caller(transport.name, transport.config).await?;
            }
        }

        Ok(endpoint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serialization::JsonSerializer;

    #[tokio::test]
    async fn test_basic_builder() {
        let endpoint = EndpointBuilder::new(JsonSerializer::default())
            .with_caller("TestProtocol", 1)
            .build()
            .await
            .unwrap();

        assert!(endpoint.has_protocol("TestProtocol").await);
    }

    #[tokio::test]
    async fn test_multiple_protocols() {
        let endpoint = EndpointBuilder::new(JsonSerializer::default())
            .with_caller("Protocol1", 1)
            .with_responder("Protocol2", 2)
            .with_bidirectional("Protocol3", 3)
            .build()
            .await
            .unwrap();

        assert!(endpoint.has_protocol("Protocol1").await);
        assert!(endpoint.has_protocol("Protocol2").await);
        assert!(endpoint.has_protocol("Protocol3").await);
    }

    #[tokio::test]
    async fn test_client_preset() {
        let endpoint = EndpointBuilder::client(JsonSerializer::default())
            .with_caller("UserService", 1)
            .build()
            .await
            .unwrap();

        assert!(endpoint.has_protocol("UserService").await);
        assert!(endpoint.id().starts_with("client-"));
    }

    #[tokio::test]
    async fn test_server_preset() {
        let endpoint = EndpointBuilder::server(JsonSerializer::default())
            .with_responder("UserService", 1)
            .build()
            .await
            .unwrap();

        assert!(endpoint.has_protocol("UserService").await);
        assert!(endpoint.id().starts_with("server-"));
    }

    #[tokio::test]
    async fn test_peer_preset() {
        let endpoint = EndpointBuilder::peer(JsonSerializer::default())
            .with_bidirectional("ChatProtocol", 1)
            .build()
            .await
            .unwrap();

        assert!(endpoint.has_protocol("ChatProtocol").await);
        assert!(endpoint.id().starts_with("peer-"));
    }

    #[tokio::test]
    async fn test_custom_config() {
        use std::time::Duration;

        let config = EndpointConfig::new()
            .with_channel_buffer_size(1000)
            .with_handshake_timeout(Duration::from_secs(10));

        let endpoint = EndpointBuilder::with_config(JsonSerializer::default(), config)
            .with_caller("TestProtocol", 1)
            .build()
            .await
            .unwrap();

        assert!(endpoint.has_protocol("TestProtocol").await);
    }

    #[tokio::test]
    async fn test_configure_method() {
        use std::time::Duration;

        let endpoint = EndpointBuilder::new(JsonSerializer::default())
            .configure(|config| {
                config
                    .with_channel_buffer_size(500)
                    .with_handshake_timeout(Duration::from_secs(15))
            })
            .with_caller("TestProtocol", 1)
            .build()
            .await
            .unwrap();

        assert!(endpoint.has_protocol("TestProtocol").await);
    }

    #[tokio::test]
    async fn test_duplicate_protocol_error() {
        let result = EndpointBuilder::new(JsonSerializer::default())
            .with_caller("TestProtocol", 1)
            .with_caller("TestProtocol", 1)
            .build()
            .await;

        assert!(result.is_err());
        match result {
            Err(EndpointError::ProtocolAlreadyRegistered { protocol }) => {
                assert_eq!(protocol, "TestProtocol");
            }
            _ => panic!("Expected ProtocolAlreadyRegistered error"),
        }
    }

    #[tokio::test]
    async fn test_empty_builder() {
        let endpoint = EndpointBuilder::new(JsonSerializer::default())
            .build()
            .await
            .unwrap();

        let capabilities = endpoint.capabilities().await;
        assert_eq!(capabilities.len(), 0);
    }

    #[tokio::test]
    async fn test_with_tcp_caller() {
        let endpoint = EndpointBuilder::client(JsonSerializer::default())
            .with_tcp_caller("main", "127.0.0.1:8080")
            .with_caller("UserService", 1)
            .build()
            .await
            .unwrap();

        assert!(endpoint.has_protocol("UserService").await);
        // Transport is registered but not connected yet
    }

    #[tokio::test]
    async fn test_with_tcp_listener() {
        let endpoint = EndpointBuilder::server(JsonSerializer::default())
            .with_tcp_listener("127.0.0.1:0") // Use port 0 for auto-assignment
            .with_responder("UserService", 1)
            .build()
            .await
            .unwrap();

        assert!(endpoint.has_protocol("UserService").await);
        // Listener is registered
    }

    #[tokio::test]
    async fn test_multiple_transports() {
        let endpoint = EndpointBuilder::server(JsonSerializer::default())
            .with_tcp_listener("127.0.0.1:0")
            .with_tcp_listener("127.0.0.1:0")
            .with_responder("UserService", 1)
            .build()
            .await
            .unwrap();

        assert!(endpoint.has_protocol("UserService").await);
    }

    #[tokio::test]
    async fn test_with_reconnection_strategy() {
        use crate::reconnection::ExponentialBackoff;

        let strategy = Arc::new(ExponentialBackoff::default());

        let endpoint = EndpointBuilder::client(JsonSerializer::default())
            .with_tcp_caller("main", "127.0.0.1:8080")
            .with_reconnection_strategy("main", strategy)
            .with_caller("UserService", 1)
            .build()
            .await
            .unwrap();

        assert!(endpoint.has_protocol("UserService").await);
    }

    #[tokio::test]
    async fn test_with_custom_transport() {
        use crate::reconnection::ExponentialBackoff;

        let strategy = Arc::new(ExponentialBackoff::default());
        let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080")
            .with_reconnection_strategy(strategy)
            .with_metadata("region", "us-west");

        let endpoint = EndpointBuilder::client(JsonSerializer::default())
            .with_transport("custom", config, false)
            .with_caller("UserService", 1)
            .build()
            .await
            .unwrap();

        assert!(endpoint.has_protocol("UserService").await);
    }

    #[tokio::test]
    async fn test_mixed_protocols_and_transports() {
        let endpoint = EndpointBuilder::peer(JsonSerializer::default())
            .with_tcp_listener("127.0.0.1:0")
            .with_tcp_caller("peer1", "127.0.0.1:8081")
            .with_tcp_caller("peer2", "127.0.0.1:8082")
            .with_bidirectional("ChatProtocol", 1)
            .with_bidirectional("FileTransfer", 1)
            .build()
            .await
            .unwrap();

        assert!(endpoint.has_protocol("ChatProtocol").await);
        assert!(endpoint.has_protocol("FileTransfer").await);
    }

    #[tokio::test]
    async fn test_reconnection_strategy_for_nonexistent_transport() {
        use crate::reconnection::ExponentialBackoff;

        let strategy = Arc::new(ExponentialBackoff::default());

        // Should not panic, just silently ignore
        let endpoint = EndpointBuilder::client(JsonSerializer::default())
            .with_tcp_caller("main", "127.0.0.1:8080")
            .with_reconnection_strategy("nonexistent", strategy)
            .with_caller("UserService", 1)
            .build()
            .await
            .unwrap();

        assert!(endpoint.has_protocol("UserService").await);
    }

    #[tokio::test]
    async fn test_builder_with_all_features() {
        use crate::reconnection::ExponentialBackoff;
        use std::time::Duration;

        let strategy = Arc::new(ExponentialBackoff::default());

        let endpoint = EndpointBuilder::server(JsonSerializer::default())
            .configure(|config| {
                config
                    .with_channel_buffer_size(1000)
                    .with_handshake_timeout(Duration::from_secs(10))
                    .with_max_connections(Some(500))
            })
            .with_tcp_listener("0.0.0.0:0")
            .with_tcp_caller("backup", "127.0.0.1:9090")
            .with_reconnection_strategy("backup", strategy)
            .with_responder("UserService", 1)
            .with_responder("OrderService", 1)
            .with_bidirectional("AdminProtocol", 1)
            .build()
            .await
            .unwrap();

        assert!(endpoint.has_protocol("UserService").await);
        assert!(endpoint.has_protocol("OrderService").await);
        assert!(endpoint.has_protocol("AdminProtocol").await);
    }
}

// Made with Bob

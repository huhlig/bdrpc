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
use crate::serialization::Serializer;

/// A protocol registration to be applied when building the endpoint.
#[derive(Debug, Clone)]
struct ProtocolRegistration {
    name: String,
    version: u32,
    direction: ProtocolDirection,
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
}

// Made with Bob

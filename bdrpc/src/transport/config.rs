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

//! Transport configuration types for the enhanced Transport Manager.
//!
//! This module provides configuration types for managing multiple transports,
//! including listeners (servers) and callers (clients) with automatic reconnection.

use crate::reconnection::ReconnectionStrategy;
use std::collections::HashMap;
use std::sync::Arc;

/// Type of transport protocol.
///
/// This enum identifies the underlying transport protocol being used.
/// Each transport type may have different characteristics and requirements.
///
/// # Examples
///
/// ```rust
/// use bdrpc::transport::TransportType;
///
/// let tcp = TransportType::Tcp;
/// let tls = TransportType::Tls;
/// assert_ne!(tcp, tls);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransportType {
    /// TCP/IP transport
    Tcp,

    /// TLS-encrypted TCP transport
    #[cfg(feature = "tls")]
    Tls,

    /// In-memory transport for testing
    Memory,

    /// WebSocket transport (future)
    #[cfg(feature = "websocket")]
    WebSocket,

    /// QUIC transport (future)
    #[cfg(feature = "quic")]
    Quic,

    /// Unix domain socket transport (future)
    #[cfg(unix)]
    UnixSocket,
}

impl TransportType {
    /// Returns the string name of this transport type.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::transport::TransportType;
    ///
    /// assert_eq!(TransportType::Tcp.as_str(), "tcp");
    /// assert_eq!(TransportType::Memory.as_str(), "memory");
    /// ```
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            #[cfg(feature = "tls")]
            Self::Tls => "tls",
            Self::Memory => "memory",
            #[cfg(feature = "websocket")]
            Self::WebSocket => "websocket",
            #[cfg(feature = "quic")]
            Self::Quic => "quic",
            #[cfg(unix)]
            Self::UnixSocket => "unix",
        }
    }
}

impl std::fmt::Display for TransportType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Configuration for a transport (listener or caller).
///
/// This struct contains all the information needed to configure and manage
/// a transport connection, including the transport type, address, reconnection
/// strategy, and custom metadata.
///
/// # Examples
///
/// ## TCP Listener
///
/// ```rust
/// use bdrpc::transport::{TransportConfig, TransportType};
///
/// let config = TransportConfig::new(TransportType::Tcp, "0.0.0.0:8080");
/// assert_eq!(config.transport_type(), TransportType::Tcp);
/// assert_eq!(config.address(), "0.0.0.0:8080");
/// ```
///
/// ## TCP Caller with Reconnection
///
/// ```rust
/// use bdrpc::transport::{TransportConfig, TransportType};
/// use bdrpc::reconnection::ExponentialBackoff;
/// use std::sync::Arc;
///
/// let strategy = Arc::new(ExponentialBackoff::default());
/// let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080")
///     .with_reconnection_strategy(strategy);
/// assert!(config.reconnection_strategy().is_some());
/// ```
#[derive(Clone)]
pub struct TransportConfig {
    /// Type of transport protocol
    transport_type: TransportType,

    /// Address to bind (listener) or connect to (caller)
    address: String,

    /// Optional reconnection strategy for callers
    reconnection_strategy: Option<Arc<dyn ReconnectionStrategy>>,

    /// Whether this transport is enabled
    enabled: bool,

    /// Custom metadata for this transport
    metadata: HashMap<String, String>,
}

impl TransportConfig {
    /// Creates a new transport configuration.
    ///
    /// # Arguments
    ///
    /// * `transport_type` - The type of transport protocol
    /// * `address` - The address to bind or connect to
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::transport::{TransportConfig, TransportType};
    ///
    /// let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080");
    /// ```
    pub fn new(transport_type: TransportType, address: impl Into<String>) -> Self {
        Self {
            transport_type,
            address: address.into(),
            reconnection_strategy: None,
            enabled: true,
            metadata: HashMap::new(),
        }
    }

    /// Sets the reconnection strategy for this transport.
    ///
    /// This is typically used for caller transports to enable automatic
    /// reconnection on connection failure.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::transport::{TransportConfig, TransportType};
    /// use bdrpc::reconnection::ExponentialBackoff;
    /// use std::sync::Arc;
    ///
    /// let strategy = Arc::new(ExponentialBackoff::default());
    /// let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080")
    ///     .with_reconnection_strategy(strategy);
    /// ```
    pub fn with_reconnection_strategy(mut self, strategy: Arc<dyn ReconnectionStrategy>) -> Self {
        self.reconnection_strategy = Some(strategy);
        self
    }

    /// Sets whether this transport is enabled.
    ///
    /// Disabled transports will not accept connections (listeners) or
    /// attempt to connect (callers).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::transport::{TransportConfig, TransportType};
    ///
    /// let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080")
    ///     .with_enabled(false);
    /// assert!(!config.is_enabled());
    /// ```
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Adds metadata to this transport configuration.
    ///
    /// Metadata can be used for custom tracking, routing, or other purposes.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::transport::{TransportConfig, TransportType};
    ///
    /// let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080")
    ///     .with_metadata("region", "us-west")
    ///     .with_metadata("priority", "high");
    /// assert_eq!(config.metadata().get("region"), Some(&"us-west".to_string()));
    /// ```
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Returns the transport type.
    pub fn transport_type(&self) -> TransportType {
        self.transport_type
    }

    /// Returns the address.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Returns the reconnection strategy, if any.
    pub fn reconnection_strategy(&self) -> Option<&Arc<dyn ReconnectionStrategy>> {
        self.reconnection_strategy.as_ref()
    }

    /// Returns whether this transport is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Returns the metadata.
    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }

    /// Sets the enabled state.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
}

impl std::fmt::Debug for TransportConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransportConfig")
            .field("transport_type", &self.transport_type)
            .field("address", &self.address)
            .field(
                "has_reconnection_strategy",
                &self.reconnection_strategy.is_some(),
            )
            .field("enabled", &self.enabled)
            .field("metadata", &self.metadata)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reconnection::ExponentialBackoff;

    #[test]
    fn test_transport_type_as_str() {
        assert_eq!(TransportType::Tcp.as_str(), "tcp");
        assert_eq!(TransportType::Memory.as_str(), "memory");
        #[cfg(feature = "tls")]
        assert_eq!(TransportType::Tls.as_str(), "tls");
    }

    #[test]
    fn test_transport_type_display() {
        assert_eq!(format!("{}", TransportType::Tcp), "tcp");
        assert_eq!(format!("{}", TransportType::Memory), "memory");
    }

    #[test]
    fn test_transport_config_new() {
        let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080");
        assert_eq!(config.transport_type(), TransportType::Tcp);
        assert_eq!(config.address(), "127.0.0.1:8080");
        assert!(config.is_enabled());
        assert!(config.reconnection_strategy().is_none());
        assert!(config.metadata().is_empty());
    }

    #[test]
    fn test_transport_config_with_reconnection() {
        let strategy = Arc::new(ExponentialBackoff::default());
        let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080")
            .with_reconnection_strategy(strategy);
        assert!(config.reconnection_strategy().is_some());
    }

    #[test]
    fn test_transport_config_with_enabled() {
        let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080").with_enabled(false);
        assert!(!config.is_enabled());
    }

    #[test]
    fn test_transport_config_with_metadata() {
        let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080")
            .with_metadata("region", "us-west")
            .with_metadata("priority", "high");

        assert_eq!(
            config.metadata().get("region"),
            Some(&"us-west".to_string())
        );
        assert_eq!(config.metadata().get("priority"), Some(&"high".to_string()));
        assert_eq!(config.metadata().len(), 2);
    }

    #[test]
    fn test_transport_config_set_enabled() {
        let mut config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080");
        assert!(config.is_enabled());

        config.set_enabled(false);
        assert!(!config.is_enabled());

        config.set_enabled(true);
        assert!(config.is_enabled());
    }

    #[test]
    fn test_transport_config_debug() {
        let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080")
            .with_metadata("test", "value");
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("TransportConfig"));
        assert!(debug_str.contains("Tcp")); // Debug shows enum variant name
        assert!(debug_str.contains("127.0.0.1:8080"));
    }
}

// Made with Bob

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

//! Configuration types for endpoints.

use crate::transport::strategy::{ExponentialBackoff, ReconnectionStrategy};
use std::sync::Arc;
use std::time::Duration;

/// Configuration for an endpoint.
///
/// This controls various aspects of endpoint behavior including buffer sizes,
/// timeouts, and connection limits.
///
/// # Examples
///
/// ```rust
/// use bdrpc::endpoint::EndpointConfig;
/// use std::time::Duration;
///
/// // Use default configuration
/// let config = EndpointConfig::default();
///
/// // Customize configuration
/// let config = EndpointConfig {
///     channel_buffer_size: 1000,
///     handshake_timeout: Duration::from_secs(10),
///     max_connections: Some(100),
///     ..Default::default()
/// };
/// ```
#[derive(Clone)]
pub struct EndpointConfig {
    /// Buffer size for channels (number of messages).
    ///
    /// This controls how many messages can be buffered in a channel before
    /// backpressure is applied. Larger buffers allow more throughput but
    /// use more memory.
    ///
    /// Default: 100
    pub channel_buffer_size: usize,

    /// Timeout for handshake completion.
    ///
    /// If the handshake doesn't complete within this duration, the connection
    /// is closed. This prevents hanging connections from consuming resources.
    ///
    /// Default: 5 seconds
    pub handshake_timeout: Duration,

    /// Maximum number of concurrent connections.
    ///
    /// If set, the endpoint will reject new connections once this limit is
    /// reached. `None` means unlimited connections.
    ///
    /// Default: None (unlimited)
    pub max_connections: Option<usize>,

    /// Maximum frame size in bytes.
    ///
    /// Messages larger than this will be rejected. This prevents memory
    /// exhaustion from malicious or buggy peers.
    ///
    /// Default: 16 MB
    pub max_frame_size: usize,

    /// Enable frame integrity checking (CRC32).
    ///
    /// When enabled, each frame includes a CRC32 checksum to detect
    /// corruption. This adds a small overhead but improves reliability.
    ///
    /// Default: true
    pub enable_frame_integrity: bool,

    /// Endpoint identifier.
    ///
    /// An optional identifier for this endpoint, used in logging and
    /// debugging. If not set, a random ID will be generated.
    ///
    /// Default: None (auto-generated)
    pub endpoint_id: Option<String>,

    /// Reconnection strategy for client connections.
    ///
    /// Determines how the endpoint should behave when a client connection
    /// fails. Different strategies are appropriate for different use cases.
    ///
    /// Default: ExponentialBackoff with default settings
    pub reconnection_strategy: Arc<dyn ReconnectionStrategy>,
}

impl Default for EndpointConfig {
    fn default() -> Self {
        Self {
            channel_buffer_size: 100,
            handshake_timeout: Duration::from_secs(5),
            max_connections: None,
            max_frame_size: 16 * 1024 * 1024, // 16 MB
            enable_frame_integrity: true,
            endpoint_id: None,
            reconnection_strategy: Arc::new(ExponentialBackoff::default()),
        }
    }
}

impl std::fmt::Debug for EndpointConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EndpointConfig")
            .field("channel_buffer_size", &self.channel_buffer_size)
            .field("handshake_timeout", &self.handshake_timeout)
            .field("max_connections", &self.max_connections)
            .field("max_frame_size", &self.max_frame_size)
            .field("enable_frame_integrity", &self.enable_frame_integrity)
            .field("endpoint_id", &self.endpoint_id)
            .field("reconnection_strategy", &self.reconnection_strategy.name())
            .finish()
    }
}

impl EndpointConfig {
    /// Creates a new configuration with default values.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::EndpointConfig;
    ///
    /// let config = EndpointConfig::new();
    /// assert_eq!(config.channel_buffer_size, 100);
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the channel buffer size.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::EndpointConfig;
    ///
    /// let config = EndpointConfig::new().with_channel_buffer_size(1000);
    /// assert_eq!(config.channel_buffer_size, 1000);
    /// ```
    pub fn with_channel_buffer_size(mut self, size: usize) -> Self {
        self.channel_buffer_size = size;
        self
    }

    /// Sets the handshake timeout.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::EndpointConfig;
    /// use std::time::Duration;
    ///
    /// let config = EndpointConfig::new()
    ///     .with_handshake_timeout(Duration::from_secs(10));
    /// assert_eq!(config.handshake_timeout, Duration::from_secs(10));
    /// ```
    pub fn with_handshake_timeout(mut self, timeout: Duration) -> Self {
        self.handshake_timeout = timeout;
        self
    }

    /// Sets the maximum number of connections.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::EndpointConfig;
    ///
    /// let config = EndpointConfig::new().with_max_connections(Some(100));
    /// assert_eq!(config.max_connections, Some(100));
    /// ```
    pub fn with_max_connections(mut self, max: Option<usize>) -> Self {
        self.max_connections = max;
        self
    }

    /// Sets the maximum frame size.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::EndpointConfig;
    ///
    /// let config = EndpointConfig::new()
    ///     .with_max_frame_size(32 * 1024 * 1024); // 32 MB
    /// assert_eq!(config.max_frame_size, 32 * 1024 * 1024);
    /// ```
    pub fn with_max_frame_size(mut self, size: usize) -> Self {
        self.max_frame_size = size;
        self
    }

    /// Sets whether to enable frame integrity checking.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::EndpointConfig;
    ///
    /// let config = EndpointConfig::new().with_frame_integrity(false);
    /// assert_eq!(config.enable_frame_integrity, false);
    /// ```
    pub fn with_frame_integrity(mut self, enable: bool) -> Self {
        self.enable_frame_integrity = enable;
        self
    }

    /// Sets the endpoint identifier.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::EndpointConfig;
    ///
    /// let config = EndpointConfig::new()
    ///     .with_endpoint_id("my-endpoint".to_string());
    /// assert_eq!(config.endpoint_id, Some("my-endpoint".to_string()));
    /// ```
    pub fn with_endpoint_id(mut self, id: String) -> Self {
        self.endpoint_id = Some(id);
        self
    }

    /// Sets the strategy strategy.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::EndpointConfig;
    /// use bdrpc::strategy::{FixedDelay, ReconnectionStrategy};
    /// use std::sync::Arc;
    /// use std::time::Duration;
    ///
    /// let strategy: Arc<dyn ReconnectionStrategy> = Arc::new(
    ///     FixedDelay::new(Duration::from_secs(5))
    /// );
    /// let config = EndpointConfig::new()
    ///     .with_reconnection_strategy(strategy.clone());
    /// assert_eq!(config.reconnection_strategy.name(), strategy.name());
    /// ```
    pub fn with_reconnection_strategy(mut self, strategy: Arc<dyn ReconnectionStrategy>) -> Self {
        self.reconnection_strategy = strategy;
        self
    }

    /// Validates the configuration.
    ///
    /// Returns an error if the configuration is invalid.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Channel buffer size is 0
    /// - Max frame size is 0
    /// - Handshake timeout is 0
    pub fn validate(&self) -> Result<(), String> {
        if self.channel_buffer_size == 0 {
            return Err("channel_buffer_size must be greater than 0".to_string());
        }
        if self.max_frame_size == 0 {
            return Err("max_frame_size must be greater than 0".to_string());
        }
        if self.handshake_timeout.is_zero() {
            return Err("handshake_timeout must be greater than 0".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = EndpointConfig::default();
        assert_eq!(config.channel_buffer_size, 100);
        assert_eq!(config.handshake_timeout, Duration::from_secs(5));
        assert_eq!(config.max_connections, None);
        assert_eq!(config.max_frame_size, 16 * 1024 * 1024);
        assert!(config.enable_frame_integrity);
        assert_eq!(config.endpoint_id, None);
    }

    #[test]
    fn test_builder_pattern() {
        let config = EndpointConfig::new()
            .with_channel_buffer_size(1000)
            .with_handshake_timeout(Duration::from_secs(10))
            .with_max_connections(Some(100))
            .with_max_frame_size(32 * 1024 * 1024)
            .with_frame_integrity(false)
            .with_endpoint_id("test-endpoint".to_string());

        assert_eq!(config.channel_buffer_size, 1000);
        assert_eq!(config.handshake_timeout, Duration::from_secs(10));
        assert_eq!(config.max_connections, Some(100));
        assert_eq!(config.max_frame_size, 32 * 1024 * 1024);
        assert!(!config.enable_frame_integrity);
        assert_eq!(config.endpoint_id, Some("test-endpoint".to_string()));
    }

    #[test]
    fn test_validate_valid() {
        let config = EndpointConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_zero_buffer() {
        let config = EndpointConfig {
            channel_buffer_size: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_zero_frame_size() {
        let config = EndpointConfig {
            max_frame_size: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_zero_timeout() {
        let config = EndpointConfig {
            handshake_timeout: Duration::from_secs(0),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }
}

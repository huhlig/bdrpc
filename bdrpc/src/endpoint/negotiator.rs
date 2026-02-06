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

//! Channel negotiation for dynamic channel creation.
//!
//! This module provides traits and implementations for handling dynamic channel
//! creation requests. The negotiator is called when a remote endpoint requests
//! to create a new channel, allowing the local endpoint to validate and accept
//! or reject the request.
//!
//! # Overview
//!
//! The [`ChannelNegotiator`] trait defines callbacks for:
//! - Validating channel creation requests
//! - Handling channel creation responses
//! - Responding to channel closure notifications
//!
//! # Example
//!
//! ```rust
//! use bdrpc::endpoint::{ChannelNegotiator, DefaultChannelNegotiator};
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a negotiator that allows specific protocols
//! let negotiator = DefaultChannelNegotiator::new();
//! negotiator.allow_protocol("MyProtocol").await;
//! negotiator.allow_protocol("AnotherProtocol").await;
//!
//! // Use with endpoint
//! // endpoint.set_channel_negotiator(Arc::new(negotiator));
//! # Ok(())
//! # }
//! ```

use crate::channel::ChannelId;
use crate::endpoint::ProtocolDirection;
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Trait for handling dynamic channel creation requests.
///
/// Implementors of this trait can control which channels are allowed to be
/// created dynamically, validate metadata, and respond to channel lifecycle
/// events.
///
/// # Lifecycle
///
/// 1. Remote endpoint sends a channel creation request
/// 2. [`on_channel_create_request`] is called to validate the request
/// 3. If accepted, the channel is created locally
/// 4. [`on_channel_create_response`] is called with the result
/// 5. When a channel is closed, [`on_channel_close_notification`] is called
///
/// # Example
///
/// ```rust
/// use bdrpc::channel::ChannelId;
/// use bdrpc::endpoint::{ChannelNegotiator, ProtocolDirection};
/// use async_trait::async_trait;
/// use std::collections::HashMap;
///
/// struct MyNegotiator;
///
/// #[async_trait]
/// impl ChannelNegotiator for MyNegotiator {
///     async fn on_channel_create_request(
///         &self,
///         channel_id: ChannelId,
///         protocol_name: &str,
///         protocol_version: u32,
///         direction: ProtocolDirection,
///         metadata: &HashMap<String, String>,
///     ) -> Result<bool, String> {
///         // Validate the request
///         if protocol_name == "MyProtocol" && protocol_version == 1 {
///             Ok(true)
///         } else {
///             Err("Unsupported protocol".to_string())
///         }
///     }
///
///     async fn on_channel_create_response(
///         &self,
///         channel_id: ChannelId,
///         success: bool,
///         error: Option<String>,
///     ) {
///         if success {
///             println!("Channel {} created", channel_id);
///         } else {
///             eprintln!("Channel {} failed: {:?}", channel_id, error);
///         }
///     }
///
///     async fn on_channel_close_notification(
///         &self,
///         channel_id: ChannelId,
///         reason: &str,
///     ) {
///         println!("Channel {} closed: {}", channel_id, reason);
///     }
/// }
/// ```
///
/// [`on_channel_create_request`]: ChannelNegotiator::on_channel_create_request
/// [`on_channel_create_response`]: ChannelNegotiator::on_channel_create_response
/// [`on_channel_close_notification`]: ChannelNegotiator::on_channel_close_notification
#[async_trait]
pub trait ChannelNegotiator: Send + Sync {
    /// Called when a remote endpoint requests to create a channel.
    ///
    /// This method should validate the request and return:
    /// - `Ok(true)` to accept the request
    /// - `Ok(false)` to reject the request
    /// - `Err(message)` to reject with a specific error message
    ///
    /// # Parameters
    ///
    /// - `channel_id`: The requested channel ID
    /// - `protocol_name`: Name of the protocol for this channel
    /// - `protocol_version`: Version of the protocol
    /// - `direction`: Direction for this channel (Client, Server, or Bidirectional)
    /// - `metadata`: Optional key-value metadata from the requester
    ///
    /// # Example
    ///
    /// ```rust
    /// # use bdrpc::channel::ChannelId;
    /// # use bdrpc::endpoint::{ChannelNegotiator, ProtocolDirection};
    /// # use async_trait::async_trait;
    /// # use std::collections::HashMap;
    /// # struct MyNegotiator;
    /// # #[async_trait]
    /// # impl ChannelNegotiator for MyNegotiator {
    /// async fn on_channel_create_request(
    ///     &self,
    ///     channel_id: ChannelId,
    ///     protocol_name: &str,
    ///     protocol_version: u32,
    ///     direction: ProtocolDirection,
    ///     metadata: &HashMap<String, String>,
    /// ) -> Result<bool, String> {
    ///     // Check authentication token in metadata
    ///     if let Some(token) = metadata.get("auth_token") {
    ///         if validate_token(token) {
    ///             return Ok(true);
    ///         }
    ///     }
    ///     Err("Authentication required".to_string())
    /// }
    /// # async fn on_channel_create_response(&self, _: ChannelId, _: bool, _: Option<String>) {}
    /// # async fn on_channel_close_notification(&self, _: ChannelId, _: &str) {}
    /// # }
    /// # fn validate_token(_: &str) -> bool { true }
    /// ```
    async fn on_channel_create_request(
        &self,
        channel_id: ChannelId,
        protocol_name: &str,
        protocol_version: u32,
        direction: ProtocolDirection,
        metadata: &HashMap<String, String>,
    ) -> Result<bool, String>;

    /// Called when a channel creation response is received.
    ///
    /// This is called on the endpoint that initiated the channel creation
    /// request, after receiving the response from the remote endpoint.
    ///
    /// # Parameters
    ///
    /// - `channel_id`: The channel ID from the request
    /// - `success`: Whether the channel was created successfully
    /// - `error`: Error message if creation failed
    ///
    /// # Example
    ///
    /// ```rust
    /// # use bdrpc::channel::ChannelId;
    /// # use bdrpc::endpoint::{ChannelNegotiator, ProtocolDirection};
    /// # use async_trait::async_trait;
    /// # use std::collections::HashMap;
    /// # struct MyNegotiator;
    /// # #[async_trait]
    /// # impl ChannelNegotiator for MyNegotiator {
    /// # async fn on_channel_create_request(&self, _: ChannelId, _: &str, _: u32, _: ProtocolDirection, _: &HashMap<String, String>) -> Result<bool, String> { Ok(true) }
    /// async fn on_channel_create_response(
    ///     &self,
    ///     channel_id: ChannelId,
    ///     success: bool,
    ///     error: Option<String>,
    /// ) {
    ///     if success {
    ///         println!("Channel {} created successfully", channel_id);
    ///     } else {
    ///         println!(
    ///             "Channel {} creation failed: {}",
    ///             channel_id,
    ///             error.unwrap_or_default()
    ///         );
    ///     }
    /// }
    /// # async fn on_channel_close_notification(&self, _: ChannelId, _: &str) {}
    /// # }
    /// ```
    async fn on_channel_create_response(
        &self,
        channel_id: ChannelId,
        success: bool,
        error: Option<String>,
    );

    /// Called when a remote endpoint notifies of channel closure.
    ///
    /// This is called when the remote endpoint sends a channel close
    /// notification. The local endpoint should clean up its side of the
    /// channel.
    ///
    /// # Parameters
    ///
    /// - `channel_id`: The channel ID being closed
    /// - `reason`: Human-readable reason for closure
    ///
    /// # Example
    ///
    /// ```rust
    /// # use bdrpc::channel::ChannelId;
    /// # use bdrpc::endpoint::{ChannelNegotiator, ProtocolDirection};
    /// # use async_trait::async_trait;
    /// # use std::collections::HashMap;
    /// # struct MyNegotiator;
    /// # #[async_trait]
    /// # impl ChannelNegotiator for MyNegotiator {
    /// # async fn on_channel_create_request(&self, _: ChannelId, _: &str, _: u32, _: ProtocolDirection, _: &HashMap<String, String>) -> Result<bool, String> { Ok(true) }
    /// # async fn on_channel_create_response(&self, _: ChannelId, _: bool, _: Option<String>) {}
    /// async fn on_channel_close_notification(
    ///     &self,
    ///     channel_id: ChannelId,
    ///     reason: &str,
    /// ) {
    ///     println!("Channel {} closed: {}", channel_id, reason);
    ///     // Perform any cleanup needed
    /// }
    /// # }
    /// ```
    async fn on_channel_close_notification(&self, channel_id: ChannelId, reason: &str);
}

/// Default channel negotiator that uses an allowlist of protocols.
///
/// This negotiator accepts channel creation requests only for protocols that
/// have been explicitly allowed via [`allow_protocol`]. All other requests
/// are rejected.
///
/// # Example
///
/// ```rust
/// use bdrpc::endpoint::DefaultChannelNegotiator;
/// use std::sync::Arc;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let negotiator = DefaultChannelNegotiator::new();
///
/// // Allow specific protocols
/// negotiator.allow_protocol("MyProtocol").await;
/// negotiator.allow_protocol("AnotherProtocol").await;
///
/// // Disallow a protocol
/// negotiator.disallow_protocol("MyProtocol").await;
///
/// // Check if a protocol is allowed
/// assert!(!negotiator.is_protocol_allowed("MyProtocol").await);
/// assert!(negotiator.is_protocol_allowed("AnotherProtocol").await);
/// # Ok(())
/// # }
/// ```
///
/// [`allow_protocol`]: DefaultChannelNegotiator::allow_protocol
pub struct DefaultChannelNegotiator {
    /// Set of allowed protocol names.
    allowed_protocols: Arc<RwLock<HashSet<String>>>,
}

impl DefaultChannelNegotiator {
    /// Creates a new default channel negotiator.
    ///
    /// Initially, no protocols are allowed. Use [`allow_protocol`] to add
    /// protocols to the allowlist.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::endpoint::DefaultChannelNegotiator;
    ///
    /// let negotiator = DefaultChannelNegotiator::new();
    /// ```
    ///
    /// [`allow_protocol`]: DefaultChannelNegotiator::allow_protocol
    #[must_use]
    pub fn new() -> Self {
        Self {
            allowed_protocols: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Allows a protocol to be used for dynamic channel creation.
    ///
    /// # Parameters
    ///
    /// - `protocol_name`: Name of the protocol to allow
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::endpoint::DefaultChannelNegotiator;
    ///
    /// # async fn example() {
    /// let negotiator = DefaultChannelNegotiator::new();
    /// negotiator.allow_protocol("MyProtocol").await;
    /// negotiator.allow_protocol("AnotherProtocol").await;
    /// # }
    /// ```
    pub async fn allow_protocol(&self, protocol_name: impl Into<String>) {
        self.allowed_protocols
            .write()
            .await
            .insert(protocol_name.into());
    }

    /// Removes a protocol from the allowlist.
    ///
    /// # Parameters
    ///
    /// - `protocol_name`: Name of the protocol to disallow
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::endpoint::DefaultChannelNegotiator;
    ///
    /// # async fn example() {
    /// let negotiator = DefaultChannelNegotiator::new();
    /// negotiator.allow_protocol("MyProtocol").await;
    /// negotiator.disallow_protocol("MyProtocol").await;
    /// # }
    /// ```
    pub async fn disallow_protocol(&self, protocol_name: &str) {
        self.allowed_protocols.write().await.remove(protocol_name);
    }

    /// Checks if a protocol is allowed.
    ///
    /// # Parameters
    ///
    /// - `protocol_name`: Name of the protocol to check
    ///
    /// # Returns
    ///
    /// `true` if the protocol is in the allowlist, `false` otherwise.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::endpoint::DefaultChannelNegotiator;
    ///
    /// # async fn example() {
    /// let negotiator = DefaultChannelNegotiator::new();
    /// negotiator.allow_protocol("MyProtocol").await;
    ///
    /// assert!(negotiator.is_protocol_allowed("MyProtocol").await);
    /// assert!(!negotiator.is_protocol_allowed("UnknownProtocol").await);
    /// # }
    /// ```
    pub async fn is_protocol_allowed(&self, protocol_name: &str) -> bool {
        self.allowed_protocols.read().await.contains(protocol_name)
    }

    /// Returns a list of all allowed protocols.
    ///
    /// # Returns
    ///
    /// A vector of protocol names that are currently allowed.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::endpoint::DefaultChannelNegotiator;
    ///
    /// # async fn example() {
    /// let negotiator = DefaultChannelNegotiator::new();
    /// negotiator.allow_protocol("Protocol1").await;
    /// negotiator.allow_protocol("Protocol2").await;
    ///
    /// let allowed = negotiator.allowed_protocols().await;
    /// assert_eq!(allowed.len(), 2);
    /// # }
    /// ```
    pub async fn allowed_protocols(&self) -> Vec<String> {
        self.allowed_protocols
            .read()
            .await
            .iter()
            .cloned()
            .collect()
    }
}

impl Default for DefaultChannelNegotiator {
    fn default() -> Self {
        Self::new()
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
        #[cfg(feature = "tracing")]
        {
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
        #[cfg(not(feature = "tracing"))]
        {
            let _ = (channel_id, success, error);
        }
    }

    async fn on_channel_close_notification(&self, channel_id: ChannelId, reason: &str) {
        #[cfg(feature = "tracing")]
        tracing::info!("Channel {} closed: {}", channel_id, reason);
        #[cfg(not(feature = "tracing"))]
        {
            let _ = (channel_id, reason);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_default_negotiator_allowlist() {
        let negotiator = DefaultChannelNegotiator::new();

        // Initially no protocols allowed
        assert!(!negotiator.is_protocol_allowed("TestProtocol").await);

        // Allow a protocol
        negotiator.allow_protocol("TestProtocol").await;
        assert!(negotiator.is_protocol_allowed("TestProtocol").await);

        // Disallow the protocol
        negotiator.disallow_protocol("TestProtocol").await;
        assert!(!negotiator.is_protocol_allowed("TestProtocol").await);
    }

    #[tokio::test]
    async fn test_default_negotiator_multiple_protocols() {
        let negotiator = DefaultChannelNegotiator::new();

        negotiator.allow_protocol("Protocol1").await;
        negotiator.allow_protocol("Protocol2").await;
        negotiator.allow_protocol("Protocol3").await;

        let allowed = negotiator.allowed_protocols().await;
        assert_eq!(allowed.len(), 3);
        assert!(allowed.contains(&"Protocol1".to_string()));
        assert!(allowed.contains(&"Protocol2".to_string()));
        assert!(allowed.contains(&"Protocol3".to_string()));
    }

    #[tokio::test]
    async fn test_channel_negotiator_accept() {
        let negotiator = DefaultChannelNegotiator::new();
        negotiator.allow_protocol("TestProtocol").await;

        let result = negotiator
            .on_channel_create_request(
                ChannelId::from(1),
                "TestProtocol",
                1,
                ProtocolDirection::Bidirectional,
                &HashMap::new(),
            )
            .await;

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_channel_negotiator_reject() {
        let negotiator = DefaultChannelNegotiator::new();

        let result = negotiator
            .on_channel_create_request(
                ChannelId::from(1),
                "UnknownProtocol",
                1,
                ProtocolDirection::Bidirectional,
                &HashMap::new(),
            )
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not allowed"));
    }
}

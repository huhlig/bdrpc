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

//! System channel protocol for dynamic channel negotiation.
//!
//! The system channel (Channel ID 0) is a reserved channel used for control
//! messages between endpoints. It enables dynamic channel creation, closure
//! coordination, and keepalive functionality.
//!
//! # Overview
//!
//! Every transport automatically creates a system channel for control messages:
//! - **Channel Creation**: Request and negotiate new channels dynamically
//! - **Channel Closure**: Coordinate graceful channel shutdown
//! - **Keepalive**: Ping/pong messages to detect connection health
//!
//! # Example
//!
//! ```rust
//! use bdrpc::channel::{ChannelId, SystemProtocol};
//! use bdrpc::endpoint::ProtocolDirection;
//! use std::collections::HashMap;
//!
//! // Request a new channel
//! let request = SystemProtocol::ChannelCreateRequest {
//!     channel_id: ChannelId::from(1),
//!     protocol_name: "MyProtocol".to_string(),
//!     protocol_version: 1,
//!     direction: ProtocolDirection::Bidirectional,
//!     buffer_size: Some(100),
//!     metadata: HashMap::new(),
//! };
//! ```

use super::{ChannelId, Protocol};
use crate::endpoint::ProtocolDirection;
use std::collections::HashMap;

/// Reserved channel ID for system control messages.
///
/// Channel ID 0 is always reserved for the system channel and cannot be used
/// for application protocols.
pub const SYSTEM_CHANNEL_ID: ChannelId = ChannelId::from_u64(0);

/// System protocol for channel lifecycle management.
///
/// This protocol is used on the system channel (ID 0) to coordinate channel
/// creation, closure, and keepalive between endpoints.
///
/// # Message Flow
///
/// ## Channel Creation
/// 1. Requester sends `ChannelCreateRequest`
/// 2. Responder validates and sends `ChannelCreateResponse`
/// 3. If accepted, both sides create the channel locally
///
/// ## Channel Closure
/// 1. Initiator sends `ChannelCloseNotification`
/// 2. Receiver acknowledges with `ChannelCloseAck`
/// 3. Both sides clean up the channel
///
/// ## Keepalive
/// 1. Either side sends `Ping` with timestamp
/// 2. Receiver responds with `Pong` echoing the timestamp
///
/// # Examples
///
/// ```rust
/// use bdrpc::channel::{ChannelId, SystemProtocol};
/// use bdrpc::endpoint::ProtocolDirection;
/// use std::collections::HashMap;
///
/// // Create a channel request
/// let request = SystemProtocol::ChannelCreateRequest {
///     channel_id: ChannelId::from(42),
///     protocol_name: "MyProtocol".to_string(),
///     protocol_version: 1,
///     direction: ProtocolDirection::Bidirectional,
///     buffer_size: Some(100),
///     metadata: HashMap::from([
///         ("client_id".to_string(), "user123".to_string()),
///     ]),
/// };
///
/// // Create a successful response
/// let response = SystemProtocol::ChannelCreateResponse {
///     channel_id: ChannelId::from(42),
///     success: true,
///     error: None,
/// };
///
/// // Create a ping message
/// let ping = SystemProtocol::Ping {
///     timestamp: std::time::SystemTime::now()
///         .duration_since(std::time::UNIX_EPOCH)
///         .unwrap()
///         .as_millis() as u64,
/// };
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SystemProtocol {
    /// Request to create a new channel.
    ///
    /// Sent by the endpoint that wants to create a new channel. The remote
    /// endpoint will validate the request and respond with a
    /// `ChannelCreateResponse`.
    ///
    /// # Fields
    ///
    /// - `channel_id`: The requested channel ID (must be > 0)
    /// - `protocol_name`: Name of the protocol for this channel
    /// - `protocol_version`: Version of the protocol
    /// - `direction`: Direction for this channel (Client, Server, or Bidirectional)
    /// - `buffer_size`: Optional hint for the channel buffer size
    /// - `metadata`: Optional key-value metadata (e.g., authentication, routing)
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

    /// Response to a channel creation request.
    ///
    /// Sent by the endpoint that received a `ChannelCreateRequest`. Indicates
    /// whether the channel was created successfully or provides an error message.
    ///
    /// # Fields
    ///
    /// - `channel_id`: The channel ID from the request
    /// - `success`: Whether creation succeeded
    /// - `error`: Error message if creation failed
    ChannelCreateResponse {
        /// The channel ID from the request
        channel_id: ChannelId,
        /// Whether creation succeeded
        success: bool,
        /// Error message if failed
        error: Option<String>,
    },

    /// Notify remote endpoint that a channel is being closed.
    ///
    /// Sent when an endpoint wants to close a channel. The remote endpoint
    /// should clean up its side of the channel and respond with a
    /// `ChannelCloseAck`.
    ///
    /// # Fields
    ///
    /// - `channel_id`: Channel ID being closed
    /// - `reason`: Human-readable reason for closure
    ChannelCloseNotification {
        /// Channel ID being closed
        channel_id: ChannelId,
        /// Reason for closure
        reason: String,
    },

    /// Acknowledge channel closure.
    ///
    /// Sent in response to a `ChannelCloseNotification` to confirm that the
    /// channel has been cleaned up on the receiving side.
    ///
    /// # Fields
    ///
    /// - `channel_id`: Channel ID that was closed
    ChannelCloseAck {
        /// Channel ID that was closed
        channel_id: ChannelId,
    },

    /// Ping message for keepalive.
    ///
    /// Sent periodically to detect if the connection is still alive. The
    /// receiver should respond with a `Pong` message echoing the timestamp.
    ///
    /// # Fields
    ///
    /// - `timestamp`: Timestamp in milliseconds since UNIX epoch
    Ping {
        /// Timestamp in milliseconds since UNIX epoch
        timestamp: u64,
    },

    /// Pong response to a ping.
    ///
    /// Sent in response to a `Ping` message. Echoes the original timestamp
    /// to allow the sender to calculate round-trip time.
    ///
    /// # Fields
    ///
    /// - `timestamp`: Original timestamp from the ping
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_channel_id_is_zero() {
        assert_eq!(SYSTEM_CHANNEL_ID.as_u64(), 0);
    }

    #[test]
    fn test_channel_create_request_method_name() {
        let request = SystemProtocol::ChannelCreateRequest {
            channel_id: ChannelId::from(1),
            protocol_name: "Test".to_string(),
            protocol_version: 1,
            direction: ProtocolDirection::Bidirectional,
            buffer_size: Some(100),
            metadata: HashMap::new(),
        };
        assert_eq!(request.method_name(), "channel_create_request");
        assert!(request.is_request());
    }

    #[test]
    fn test_channel_create_response_method_name() {
        let response = SystemProtocol::ChannelCreateResponse {
            channel_id: ChannelId::from(1),
            success: true,
            error: None,
        };
        assert_eq!(response.method_name(), "channel_create_response");
        assert!(!response.is_request());
    }

    #[test]
    fn test_channel_close_notification_method_name() {
        let notification = SystemProtocol::ChannelCloseNotification {
            channel_id: ChannelId::from(1),
            reason: "Test".to_string(),
        };
        assert_eq!(notification.method_name(), "channel_close_notification");
        assert!(notification.is_request());
    }

    #[test]
    fn test_channel_close_ack_method_name() {
        let ack = SystemProtocol::ChannelCloseAck {
            channel_id: ChannelId::from(1),
        };
        assert_eq!(ack.method_name(), "channel_close_ack");
        assert!(!ack.is_request());
    }

    #[test]
    fn test_ping_method_name() {
        let ping = SystemProtocol::Ping { timestamp: 12345 };
        assert_eq!(ping.method_name(), "ping");
        assert!(ping.is_request());
    }

    #[test]
    fn test_pong_method_name() {
        let pong = SystemProtocol::Pong { timestamp: 12345 };
        assert_eq!(pong.method_name(), "pong");
        assert!(!pong.is_request());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn test_system_protocol_serialization() {
        use serde_json;

        let request = SystemProtocol::ChannelCreateRequest {
            channel_id: ChannelId::from(42),
            protocol_name: "TestProtocol".to_string(),
            protocol_version: 1,
            direction: ProtocolDirection::Bidirectional,
            buffer_size: Some(100),
            metadata: HashMap::from([("key".to_string(), "value".to_string())]),
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: SystemProtocol = serde_json::from_str(&json).unwrap();

        match deserialized {
            SystemProtocol::ChannelCreateRequest {
                channel_id,
                protocol_name,
                protocol_version,
                buffer_size,
                ..
            } => {
                assert_eq!(channel_id.as_u64(), 42);
                assert_eq!(protocol_name, "TestProtocol");
                assert_eq!(protocol_version, 1);
                assert_eq!(buffer_size, Some(100));
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn test_ping_pong_serialization() {
        use serde_json;

        let ping = SystemProtocol::Ping { timestamp: 12345 };
        let json = serde_json::to_string(&ping).unwrap();
        let deserialized: SystemProtocol = serde_json::from_str(&json).unwrap();

        match deserialized {
            SystemProtocol::Ping { timestamp } => {
                assert_eq!(timestamp, 12345);
            }
            _ => panic!("Wrong variant"),
        }

        let pong = SystemProtocol::Pong { timestamp: 67890 };
        let json = serde_json::to_string(&pong).unwrap();
        let deserialized: SystemProtocol = serde_json::from_str(&json).unwrap();

        match deserialized {
            SystemProtocol::Pong { timestamp } => {
                assert_eq!(timestamp, 67890);
            }
            _ => panic!("Wrong variant"),
        }
    }
}

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

//! Error types for the channel layer.

use super::ChannelId;
use std::fmt;

/// Errors that can occur in the channel layer.
///
/// This follows the error hierarchy defined in ADR-004.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelError {
    /// The channel is closed and cannot send or receive messages.
    ///
    /// This typically occurs when the remote endpoint has disconnected
    /// or the channel has been explicitly closed.
    Closed {
        /// The ID of the closed channel.
        channel_id: ChannelId,
    },

    /// The channel does not exist.
    ///
    /// This occurs when trying to send a message to a channel that
    /// hasn't been created or has been destroyed.
    NotFound {
        /// The ID of the channel that was not found.
        channel_id: ChannelId,
    },

    /// A message was received out of order.
    ///
    /// This indicates a bug in the channel implementation, as FIFO
    /// ordering should be guaranteed within a channel (per ADR-007).
    OutOfOrder {
        /// The ID of the channel where the ordering violation occurred.
        channel_id: ChannelId,
        /// The expected sequence number.
        expected: u64,
        /// The actual sequence number received.
        received: u64,
    },

    /// The channel's send buffer is full.
    ///
    /// This occurs when backpressure is applied and the sender should
    /// wait before sending more messages.
    Full {
        /// The ID of the full channel.
        channel_id: ChannelId,
        /// The current buffer size.
        buffer_size: usize,
    },

    /// A channel with this ID already exists.
    ///
    /// This occurs when trying to create a channel with an ID that's
    /// already in use.
    AlreadyExists {
        /// The ID of the channel that already exists.
        channel_id: ChannelId,
    },

    /// An internal error occurred.
    ///
    /// This is used for unexpected errors that don't fit other categories.
    Internal {
        /// A description of the internal error.
        message: String,
    },

    /// A required feature is not supported.
    ///
    /// This occurs when trying to use a protocol method that requires a feature
    /// that wasn't negotiated during handshake.
    FeatureNotSupported {
        /// The ID of the channel.
        channel_id: ChannelId,
        /// The name of the required feature.
        feature: String,
        /// The protocol method that requires the feature.
        method: String,
    },

    /// An operation timed out.
    ///
    /// This occurs when a send or receive operation does not complete within
    /// the specified timeout duration. This helps prevent deadlocks by ensuring
    /// operations don't block indefinitely.
    Timeout {
        /// The ID of the channel.
        channel_id: ChannelId,
        /// The operation that timed out ("send" or "recv").
        operation: String,
    },
}

impl ChannelError {
    /// Returns true if this error is recoverable.
    ///
    /// Recoverable errors are temporary and the operation can be retried.
    /// Non-recoverable errors indicate a permanent failure.
    #[must_use]
    pub const fn is_recoverable(&self) -> bool {
        matches!(self, Self::Full { .. } | Self::Timeout { .. })
    }

    /// Returns true if this error indicates the channel is closed.
    #[must_use]
    pub const fn is_closed(&self) -> bool {
        matches!(self, Self::Closed { .. })
    }

    /// Returns true if this error indicates a timeout.
    #[must_use]
    pub const fn is_timeout(&self) -> bool {
        matches!(self, Self::Timeout { .. })
    }

    /// Returns the channel ID associated with this error, if any.
    #[must_use]
    pub const fn channel_id(&self) -> Option<ChannelId> {
        match self {
            Self::Closed { channel_id }
            | Self::NotFound { channel_id }
            | Self::OutOfOrder { channel_id, .. }
            | Self::Full { channel_id, .. }
            | Self::AlreadyExists { channel_id }
            | Self::FeatureNotSupported { channel_id, .. }
            | Self::Timeout { channel_id, .. } => Some(*channel_id),
            Self::Internal { .. } => None,
        }
    }
}

impl fmt::Display for ChannelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed { channel_id } => {
                write!(f, "Channel {} is closed", channel_id)
            }
            Self::NotFound { channel_id } => {
                write!(f, "Channel {} not found", channel_id)
            }
            Self::OutOfOrder {
                channel_id,
                expected,
                received,
            } => {
                write!(
                    f,
                    "Message out of order on channel {}: expected sequence {}, got {}",
                    channel_id, expected, received
                )
            }
            Self::Full {
                channel_id,
                buffer_size,
            } => {
                write!(
                    f,
                    "Channel {} is full (buffer size: {})",
                    channel_id, buffer_size
                )
            }
            Self::AlreadyExists { channel_id } => {
                write!(f, "Channel {} already exists", channel_id)
            }
            Self::FeatureNotSupported {
                channel_id,
                feature,
                method,
            } => {
                write!(
                    f,
                    "Feature '{}' required by method '{}' is not supported on channel {}",
                    feature, method, channel_id
                )
            }
            Self::Timeout {
                channel_id,
                operation,
            } => {
                write!(
                    f,
                    "Operation '{}' timed out on channel {}",
                    operation, channel_id
                )
            }
            Self::Internal { message } => {
                write!(f, "Internal channel error: {}", message)
            }
        }
    }
}

impl std::error::Error for ChannelError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_is_recoverable() {
        let full = ChannelError::Full {
            channel_id: ChannelId::from(1),
            buffer_size: 100,
        };
        assert!(full.is_recoverable());

        let closed = ChannelError::Closed {
            channel_id: ChannelId::from(1),
        };
        assert!(!closed.is_recoverable());
    }

    #[test]
    fn test_error_is_closed() {
        let closed = ChannelError::Closed {
            channel_id: ChannelId::from(1),
        };
        assert!(closed.is_closed());

        let not_found = ChannelError::NotFound {
            channel_id: ChannelId::from(1),
        };
        assert!(!not_found.is_closed());
    }

    #[test]
    fn test_error_channel_id() {
        let closed = ChannelError::Closed {
            channel_id: ChannelId::from(42),
        };
        assert_eq!(closed.channel_id(), Some(ChannelId::from(42)));

        let internal = ChannelError::Internal {
            message: "test".to_string(),
        };
        assert_eq!(internal.channel_id(), None);
    }

    #[test]
    fn test_error_display() {
        let closed = ChannelError::Closed {
            channel_id: ChannelId::from(1),
        };
        assert_eq!(format!("{}", closed), "Channel Channel(1) is closed");

        let out_of_order = ChannelError::OutOfOrder {
            channel_id: ChannelId::from(1),
            expected: 5,
            received: 7,
        };
        assert!(format!("{}", out_of_order).contains("out of order"));
    }
}

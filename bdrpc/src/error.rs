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

//! Top-level error types for BDRPC.
//!
//! This module implements the three-layer error hierarchy defined in ADR-004:
//!
//! 1. **Transport Layer**: Connection-level failures ([`TransportError`])
//! 2. **Channel Layer**: Protocol-level failures ([`ChannelError`])
//! 3. **Application Layer**: User-defined errors (boxed trait objects)
//!
//! The [`BdrpcError`] enum composes these layers and provides a unified
//! error type for the entire framework.
//!
//! # Error Handling Strategy
//!
//! Each error layer has a specific handling strategy:
//!
//! - **Transport errors** → Close transport, trigger reconnection
//! - **Channel errors** → Close channel, keep transport alive
//! - **Application errors** → Propagate to caller, keep channel open
//!
//! # Examples
//!
//! ```rust
//! use bdrpc::BdrpcError;
//! use bdrpc::transport::TransportError;
//! use bdrpc::channel::ChannelError;
//!
//! // Transport error
//! let transport_err = TransportError::Closed;
//! let bdrpc_err: BdrpcError = transport_err.into();
//! assert!(bdrpc_err.is_transport_error());
//!
//! // Channel error
//! let channel_err = ChannelError::Closed {
//!     channel_id: bdrpc::ChannelId::from(1),
//! };
//! let bdrpc_err: BdrpcError = channel_err.into();
//! assert!(bdrpc_err.is_channel_error());
//! ```

use crate::channel::ChannelError;
use crate::transport::TransportError;
use std::error::Error as StdError;
use std::fmt;

/// Top-level error type for BDRPC operations.
///
/// This enum implements the three-layer error hierarchy from ADR-004,
/// distinguishing between transport failures, channel failures, and
/// application-specific errors.
///
/// # Error Layers
///
/// 1. **Transport**: Network-level failures affecting the entire connection
/// 2. **Channel**: Protocol-level failures affecting a specific channel
/// 3. **Application**: User-defined errors from service implementations
///
/// # Recovery Strategies
///
/// Each error type has an associated recovery strategy:
///
/// - **Transport errors**: Close all channels, attempt reconnection
/// - **Channel errors**: Close affected channel, keep transport alive
/// - **Application errors**: Return to caller, no framework action
///
/// # Examples
///
/// ```rust
/// use bdrpc::BdrpcError;
/// use bdrpc::transport::TransportError;
///
/// fn handle_error(error: BdrpcError) {
///     match error {
///         BdrpcError::Transport(e) => {
///             eprintln!("Transport failed: {}", e);
///             // Trigger reconnection
///         }
///         BdrpcError::Channel(e) => {
///             eprintln!("Channel failed: {}", e);
///             // Close channel
///         }
///         BdrpcError::Application(e) => {
///             eprintln!("Application error: {}", e);
///             // Propagate to caller
///         }
///     }
/// }
/// ```
#[derive(Debug)]
pub enum BdrpcError {
    /// A transport-layer error occurred.
    ///
    /// Transport errors indicate failures in the underlying network connection.
    /// These errors typically require closing the transport and all associated
    /// channels, and may trigger reconnection attempts.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::BdrpcError;
    /// use bdrpc::transport::TransportError;
    ///
    /// let error = BdrpcError::Transport(TransportError::Closed);
    /// assert!(error.is_transport_error());
    /// assert!(error.should_close_transport());
    /// ```
    Transport(TransportError),

    /// A channel-layer error occurred.
    ///
    /// Channel errors indicate failures in protocol handling or message
    /// processing. These errors typically require closing the affected channel
    /// but leave the transport and other channels operational.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::{BdrpcError, ChannelError, ChannelId};
    ///
    /// let error = BdrpcError::Channel(ChannelError::Closed {
    ///     channel_id: ChannelId::from(1),
    /// });
    /// assert!(error.is_channel_error());
    /// assert!(!error.should_close_transport());
    /// ```
    Channel(ChannelError),

    /// An application-layer error occurred.
    ///
    /// Application errors are user-defined errors from service implementations.
    /// The framework does not handle these errors; they are simply propagated
    /// to the caller. The channel and transport remain operational.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::BdrpcError;
    /// use std::io;
    ///
    /// let app_error = io::Error::new(io::ErrorKind::Other, "custom error");
    /// let error = BdrpcError::Application(Box::new(app_error));
    /// assert!(error.is_application_error());
    /// assert!(!error.should_close_transport());
    /// ```
    Application(Box<dyn StdError + Send + Sync>),
}

impl BdrpcError {
    /// Returns `true` if this is a transport error.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::BdrpcError;
    /// use bdrpc::transport::TransportError;
    ///
    /// let error = BdrpcError::Transport(TransportError::Closed);
    /// assert!(error.is_transport_error());
    /// ```
    #[must_use]
    pub const fn is_transport_error(&self) -> bool {
        matches!(self, Self::Transport(_))
    }

    /// Returns `true` if this is a channel error.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::{BdrpcError, ChannelError, ChannelId};
    ///
    /// let error = BdrpcError::Channel(ChannelError::Closed {
    ///     channel_id: ChannelId::from(1),
    /// });
    /// assert!(error.is_channel_error());
    /// ```
    #[must_use]
    pub const fn is_channel_error(&self) -> bool {
        matches!(self, Self::Channel(_))
    }

    /// Returns `true` if this is an application error.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::BdrpcError;
    /// use std::io;
    ///
    /// let app_error = io::Error::new(io::ErrorKind::Other, "test");
    /// let error = BdrpcError::Application(Box::new(app_error));
    /// assert!(error.is_application_error());
    /// ```
    #[must_use]
    pub const fn is_application_error(&self) -> bool {
        matches!(self, Self::Application(_))
    }

    /// Returns `true` if this error is potentially recoverable.
    ///
    /// Recoverable errors may succeed if retried. This includes:
    /// - Transient transport errors (connection failures, timeouts)
    /// - Recoverable channel errors (backpressure)
    ///
    /// Application errors are considered non-recoverable at the framework level.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::BdrpcError;
    /// use bdrpc::transport::TransportError;
    /// use std::time::Duration;
    ///
    /// let timeout = BdrpcError::Transport(TransportError::Timeout {
    ///     duration: Duration::from_secs(30),
    /// });
    /// assert!(timeout.is_recoverable());
    ///
    /// let closed = BdrpcError::Transport(TransportError::Closed);
    /// assert!(!closed.is_recoverable());
    /// ```
    #[must_use]
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::Transport(e) => e.is_recoverable(),
            Self::Channel(e) => e.is_recoverable(),
            Self::Application(_) => false,
        }
    }

    /// Returns `true` if this error should cause the transport to close.
    ///
    /// Transport errors typically require closing the transport, while
    /// channel and application errors do not.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::BdrpcError;
    /// use bdrpc::transport::TransportError;
    ///
    /// let error = BdrpcError::Transport(TransportError::ConnectionLost {
    ///     reason: "peer closed".to_string(),
    ///     source: None,
    /// });
    /// assert!(error.should_close_transport());
    /// ```
    #[must_use]
    pub fn should_close_transport(&self) -> bool {
        match self {
            Self::Transport(e) => e.should_close_transport(),
            Self::Channel(_) | Self::Application(_) => false,
        }
    }

    /// Returns `true` if this error should cause the channel to close.
    ///
    /// Most channel errors require closing the channel, while transport
    /// errors close all channels and application errors don't close channels.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::{BdrpcError, ChannelError, ChannelId};
    ///
    /// let error = BdrpcError::Channel(ChannelError::Closed {
    ///     channel_id: ChannelId::from(1),
    /// });
    /// assert!(error.should_close_channel());
    /// ```
    #[must_use]
    pub fn should_close_channel(&self) -> bool {
        match self {
            Self::Transport(_) => true, // Transport errors close all channels
            Self::Channel(e) => !e.is_recoverable(), // Close channel unless recoverable
            Self::Application(_) => false, // Application errors don't close channels
        }
    }
}

impl fmt::Display for BdrpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(e) => write!(f, "transport error: {}", e),
            Self::Channel(e) => write!(f, "channel error: {}", e),
            Self::Application(e) => write!(f, "application error: {}", e),
        }
    }
}

impl StdError for BdrpcError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Transport(e) => Some(e),
            Self::Channel(e) => Some(e),
            Self::Application(e) => Some(e.as_ref()),
        }
    }
}

impl From<TransportError> for BdrpcError {
    fn from(error: TransportError) -> Self {
        Self::Transport(error)
    }
}

impl From<ChannelError> for BdrpcError {
    fn from(error: ChannelError) -> Self {
        Self::Channel(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChannelId;
    use std::io;

    #[test]
    fn test_is_transport_error() {
        let error = BdrpcError::Transport(TransportError::Closed);
        assert!(error.is_transport_error());
        assert!(!error.is_channel_error());
        assert!(!error.is_application_error());
    }

    #[test]
    fn test_is_channel_error() {
        let error = BdrpcError::Channel(ChannelError::Closed {
            channel_id: ChannelId::from(1),
        });
        assert!(!error.is_transport_error());
        assert!(error.is_channel_error());
        assert!(!error.is_application_error());
    }

    #[test]
    fn test_is_application_error() {
        let app_error = io::Error::new(io::ErrorKind::Other, "test");
        let error = BdrpcError::Application(Box::new(app_error));
        assert!(!error.is_transport_error());
        assert!(!error.is_channel_error());
        assert!(error.is_application_error());
    }

    #[test]
    fn test_is_recoverable() {
        // Recoverable transport error
        let timeout = BdrpcError::Transport(TransportError::Timeout {
            duration: std::time::Duration::from_secs(30),
        });
        assert!(timeout.is_recoverable());

        // Non-recoverable transport error
        let closed = BdrpcError::Transport(TransportError::Closed);
        assert!(!closed.is_recoverable());

        // Recoverable channel error
        let full = BdrpcError::Channel(ChannelError::Full {
            channel_id: ChannelId::from(1),
            buffer_size: 100,
        });
        assert!(full.is_recoverable());

        // Non-recoverable channel error
        let channel_closed = BdrpcError::Channel(ChannelError::Closed {
            channel_id: ChannelId::from(1),
        });
        assert!(!channel_closed.is_recoverable());

        // Application errors are not recoverable at framework level
        let app_error = io::Error::new(io::ErrorKind::Other, "test");
        let error = BdrpcError::Application(Box::new(app_error));
        assert!(!error.is_recoverable());
    }

    #[test]
    fn test_should_close_transport() {
        // Transport errors should close transport
        let error = BdrpcError::Transport(TransportError::ConnectionLost {
            reason: "peer closed".to_string(),
            source: None,
        });
        assert!(error.should_close_transport());

        // Channel errors should not close transport
        let error = BdrpcError::Channel(ChannelError::Closed {
            channel_id: ChannelId::from(1),
        });
        assert!(!error.should_close_transport());

        // Application errors should not close transport
        let app_error = io::Error::new(io::ErrorKind::Other, "test");
        let error = BdrpcError::Application(Box::new(app_error));
        assert!(!error.should_close_transport());
    }

    #[test]
    fn test_should_close_channel() {
        // Transport errors close all channels
        let error = BdrpcError::Transport(TransportError::Closed);
        assert!(error.should_close_channel());

        // Non-recoverable channel errors close the channel
        let error = BdrpcError::Channel(ChannelError::Closed {
            channel_id: ChannelId::from(1),
        });
        assert!(error.should_close_channel());

        // Recoverable channel errors don't close the channel
        let error = BdrpcError::Channel(ChannelError::Full {
            channel_id: ChannelId::from(1),
            buffer_size: 100,
        });
        assert!(!error.should_close_channel());

        // Application errors don't close channels
        let app_error = io::Error::new(io::ErrorKind::Other, "test");
        let error = BdrpcError::Application(Box::new(app_error));
        assert!(!error.should_close_channel());
    }

    #[test]
    fn test_from_transport_error() {
        let transport_err = TransportError::Closed;
        let bdrpc_err: BdrpcError = transport_err.into();
        assert!(bdrpc_err.is_transport_error());
    }

    #[test]
    fn test_from_channel_error() {
        let channel_err = ChannelError::Closed {
            channel_id: ChannelId::from(1),
        };
        let bdrpc_err: BdrpcError = channel_err.into();
        assert!(bdrpc_err.is_channel_error());
    }

    #[test]
    fn test_display() {
        let error = BdrpcError::Transport(TransportError::Closed);
        assert!(error.to_string().contains("transport error"));

        let error = BdrpcError::Channel(ChannelError::Closed {
            channel_id: ChannelId::from(1),
        });
        assert!(error.to_string().contains("channel error"));

        let app_error = io::Error::new(io::ErrorKind::Other, "test");
        let error = BdrpcError::Application(Box::new(app_error));
        assert!(error.to_string().contains("application error"));
    }

    #[test]
    fn test_error_source() {
        let error = BdrpcError::Transport(TransportError::Closed);
        assert!(error.source().is_some());

        let error = BdrpcError::Channel(ChannelError::Closed {
            channel_id: ChannelId::from(1),
        });
        assert!(error.source().is_some());

        let app_error = io::Error::new(io::ErrorKind::Other, "test");
        let error = BdrpcError::Application(Box::new(app_error));
        assert!(error.source().is_some());
    }
}

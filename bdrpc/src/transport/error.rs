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

//! Transport layer error types.
//!
//! This module defines the error hierarchy for the transport layer, following
//! ADR-004 (Error Handling Hierarchy). Transport errors are the lowest level
//! in the error hierarchy and represent failures in the underlying network
//! communication.
//!
//! # Error Categories
//!
//! Transport errors are categorized into:
//!
//! - **Connection errors**: Failed to establish or lost connection
//! - **I/O errors**: Read/write failures
//! - **Configuration errors**: Invalid transport configuration
//! - **Timeout errors**: Operation exceeded time limit
//!
//! # Recovery Strategy
//!
//! According to ADR-004, transport errors should trigger:
//! - Close the affected transport
//! - Notify higher layers
//! - Attempt strategy (if configured)

use std::io;
use thiserror::Error;

/// Errors that can occur in the transport layer.
///
/// Transport errors represent failures in the underlying network communication.
/// These are the lowest level errors in BDRPC's error hierarchy and typically
/// result in closing the affected transport.
///
/// # Examples
///
/// ```rust
/// use bdrpc::transport::TransportError;
/// use std::io;
///
/// // Connection failed
/// let error = TransportError::ConnectionFailed {
///     address: "127.0.0.1:8080".to_string(),
///     source: io::Error::new(io::ErrorKind::ConnectionRefused, "connection refused"),
/// };
///
/// // Check if error is recoverable
/// if error.is_recoverable() {
///     println!("Can retry connection");
/// }
/// ```
#[derive(Debug, Error)]
pub enum TransportError {
    /// Failed to establish a connection to the remote endpoint.
    ///
    /// This error occurs during connection establishment and may be retried
    /// with a strategy strategy.
    #[error("failed to connect to {address}: {source}")]
    ConnectionFailed {
        /// The address that failed to connect
        address: String,
        /// The underlying I/O error
        #[source]
        source: io::Error,
    },

    /// Connection was lost during operation.
    ///
    /// This error occurs when an established connection is unexpectedly closed
    /// or becomes unusable. It triggers strategy if configured.
    #[error("connection lost: {reason}")]
    ConnectionLost {
        /// Description of why the connection was lost
        reason: String,
        /// The underlying I/O error, if available
        #[source]
        source: Option<io::Error>,
    },

    /// Failed to read types from the transport.
    ///
    /// This error occurs during read operations and may indicate a transient
    /// network issue or a permanent connection failure.
    #[error("read failed: {source}")]
    ReadFailed {
        /// The underlying I/O error
        #[source]
        source: io::Error,
    },

    /// Failed to write types to the transport.
    ///
    /// This error occurs during write operations and may indicate backpressure,
    /// a transient network issue, or a permanent connection failure.
    #[error("write failed: {source}")]
    WriteFailed {
        /// The underlying I/O error
        #[source]
        source: io::Error,
    },

    /// Operation timed out.
    ///
    /// This error occurs when an operation exceeds its configured timeout.
    /// It may be retried depending on the operation type.
    #[error("operation timed out after {duration:?}")]
    Timeout {
        /// The duration that was exceeded
        duration: std::time::Duration,
    },

    /// Invalid transport configuration.
    ///
    /// This error occurs when the transport is configured with invalid
    /// parameters. It is not recoverable and indicates a programming error.
    #[error("invalid configuration: {reason}")]
    InvalidConfiguration {
        /// Description of the configuration error
        reason: String,
    },

    /// Transport is already closed.
    ///
    /// This error occurs when attempting to use a transport that has been
    /// explicitly closed. It is not recoverable.
    #[error("transport is closed")]
    Closed,

    /// Transport is not connected.
    ///
    /// This error occurs when attempting to use a transport that has not
    /// been connected yet. It may be recoverable by establishing a connection.
    #[error("transport is not connected")]
    NotConnected,

    /// Failed to bind to the specified address.
    ///
    /// This error occurs when a server transport cannot bind to the requested
    /// address, typically due to the port being in use or insufficient permissions.
    #[error("failed to bind to {address}: {source}")]
    BindFailed {
        /// The address that failed to bind
        address: String,
        /// The underlying I/O error
        #[source]
        source: io::Error,
    },

    /// An unexpected I/O error occurred.
    ///
    /// This is a catch-all for I/O errors that don't fit into other categories.
    #[error("I/O error: {source}")]
    Io {
        /// The underlying I/O error
        #[source]
        source: io::Error,
    },

    /// WebSocket-specific error occurred.
    ///
    /// This error occurs during WebSocket operations such as handshake,
    /// frame processing, or protocol violations.
    #[cfg(feature = "websocket")]
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    /// WebSocket handshake failed.
    ///
    /// This error occurs when the WebSocket handshake cannot be completed,
    /// typically due to protocol mismatch or invalid headers.
    #[cfg(feature = "websocket")]
    #[error("WebSocket handshake failed: {reason}")]
    WebSocketHandshakeFailed {
        /// Description of why the handshake failed
        reason: String,
    },

    /// QUIC connection error occurred.
    ///
    /// This error occurs during QUIC connection establishment or operation,
    /// such as connection timeout, protocol violations, or stream errors.
    #[cfg(feature = "quic")]
    #[error("QUIC connection error: {0}")]
    QuicConnection(#[from] quinn::ConnectionError),

    /// QUIC stream error occurred.
    ///
    /// This error occurs during QUIC stream operations such as reading,
    /// writing, or stream state management.
    #[cfg(feature = "quic")]
    #[error("QUIC stream write error: {0}")]
    QuicWriteError(#[from] quinn::WriteError),

    /// QUIC stream read error occurred.
    #[cfg(feature = "quic")]
    #[error("QUIC stream read error: {0}")]
    QuicReadError(#[from] quinn::ReadError),

    /// QUIC endpoint configuration error.
    ///
    /// This error occurs when the QUIC endpoint cannot be configured properly,
    /// typically due to invalid TLS configuration or certificate issues.
    #[cfg(feature = "quic")]
    #[error("QUIC endpoint error: {reason}")]
    QuicEndpointError {
        /// Description of the endpoint error
        reason: String,
    },
}

impl TransportError {
    /// Returns `true` if this error is potentially recoverable through strategy.
    ///
    /// Recoverable errors include:
    /// - Connection failures (can retry)
    /// - Connection lost (can reconnect)
    /// - Timeouts (can retry)
    /// - Transient I/O errors
    ///
    /// Non-recoverable errors include:
    /// - Invalid configuration
    /// - Closed transport
    /// - Bind failures
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::transport::TransportError;
    /// use std::io;
    ///
    /// let error = TransportError::ConnectionFailed {
    ///     address: "127.0.0.1:8080".to_string(),
    ///     source: io::Error::new(io::ErrorKind::ConnectionRefused, "refused"),
    /// };
    ///
    /// assert!(error.is_recoverable());
    /// ```
    pub fn is_recoverable(&self) -> bool {
        match self {
            // Recoverable errors
            TransportError::ConnectionFailed { .. }
            | TransportError::ConnectionLost { .. }
            | TransportError::Timeout { .. }
            | TransportError::NotConnected => true,

            // Check if I/O error is transient
            TransportError::ReadFailed { source } | TransportError::WriteFailed { source } => {
                matches!(
                    source.kind(),
                    io::ErrorKind::Interrupted
                        | io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                )
            }

            TransportError::Io { source } => matches!(
                source.kind(),
                io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ),

            // WebSocket errors - some are recoverable
            #[cfg(feature = "websocket")]
            TransportError::WebSocket(e) => {
                use tokio_tungstenite::tungstenite::Error as WsError;
                matches!(
                    e,
                    WsError::Io(_) | WsError::ConnectionClosed | WsError::AlreadyClosed
                )
            }

            #[cfg(feature = "websocket")]
            TransportError::WebSocketHandshakeFailed { .. } => true,

            // QUIC errors - connection errors are recoverable
            #[cfg(feature = "quic")]
            TransportError::QuicConnection(_) => true,

            #[cfg(feature = "quic")]
            TransportError::QuicWriteError(_) | TransportError::QuicReadError(_) => true,

            #[cfg(feature = "quic")]
            TransportError::QuicEndpointError { .. } => false,

            // Non-recoverable errors
            TransportError::InvalidConfiguration { .. }
            | TransportError::Closed
            | TransportError::BindFailed { .. } => false,
        }
    }

    /// Returns `true` if this error indicates the transport should be closed.
    ///
    /// Most transport errors should result in closing the transport, except
    /// for transient errors that may be retried.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::transport::TransportError;
    ///
    /// let error = TransportError::Closed;
    /// assert!(error.should_close_transport());
    /// ```
    pub fn should_close_transport(&self) -> bool {
        match self {
            // These errors mean the transport is already closed or unusable
            TransportError::ConnectionLost { .. }
            | TransportError::Closed
            | TransportError::InvalidConfiguration { .. } => true,

            // Connection failures happen before the transport is established
            TransportError::ConnectionFailed { .. }
            | TransportError::NotConnected
            | TransportError::BindFailed { .. } => false,

            // I/O errors should close the transport unless they're transient
            TransportError::ReadFailed { source }
            | TransportError::WriteFailed { source }
            | TransportError::Io { source } => !matches!(
                source.kind(),
                io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
            ),

            // Timeouts may or may not require closing, but we err on the side of caution
            TransportError::Timeout { .. } => true,

            // WebSocket errors should close the transport
            #[cfg(feature = "websocket")]
            TransportError::WebSocket(_) | TransportError::WebSocketHandshakeFailed { .. } => true,

            // QUIC errors should close the transport
            #[cfg(feature = "quic")]
            TransportError::QuicConnection(_)
            | TransportError::QuicWriteError(_)
            | TransportError::QuicReadError(_)
            | TransportError::QuicEndpointError { .. } => true,
        }
    }

    /// Create a connection failed error for testing.
    ///
    /// This is a convenience method for creating test instances.
    #[cfg(test)]
    pub fn connection_failed(address: impl Into<String>) -> Self {
        TransportError::ConnectionFailed {
            address: address.into(),
            source: io::Error::new(io::ErrorKind::ConnectionRefused, "connection refused"),
        }
    }

    /// Create a connection lost error for testing.
    ///
    /// This is a convenience method for creating test instances.
    #[cfg(test)]
    pub fn connection_lost(reason: impl Into<String>) -> Self {
        TransportError::ConnectionLost {
            reason: reason.into(),
            source: None,
        }
    }

    /// Create an invalid configuration error for testing.
    ///
    /// This is a convenience method for creating test instances.
    #[cfg(test)]
    pub fn invalid_configuration(reason: impl Into<String>) -> Self {
        TransportError::InvalidConfiguration {
            reason: reason.into(),
        }
    }
}

impl From<io::Error> for TransportError {
    fn from(error: io::Error) -> Self {
        TransportError::Io { source: error }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_failed_is_recoverable() {
        let error = TransportError::ConnectionFailed {
            address: "127.0.0.1:8080".to_string(),
            source: io::Error::new(io::ErrorKind::ConnectionRefused, "refused"),
        };
        assert!(error.is_recoverable());
        assert!(!error.should_close_transport());
    }

    #[test]
    fn test_connection_lost_is_recoverable() {
        let error = TransportError::ConnectionLost {
            reason: "peer closed".to_string(),
            source: None,
        };
        assert!(error.is_recoverable());
        assert!(error.should_close_transport());
    }

    #[test]
    fn test_invalid_configuration_not_recoverable() {
        let error = TransportError::InvalidConfiguration {
            reason: "invalid port".to_string(),
        };
        assert!(!error.is_recoverable());
        assert!(error.should_close_transport());
    }

    #[test]
    fn test_timeout_is_recoverable() {
        let error = TransportError::Timeout {
            duration: std::time::Duration::from_secs(30),
        };
        assert!(error.is_recoverable());
        assert!(error.should_close_transport());
    }

    #[test]
    fn test_transient_io_error_is_recoverable() {
        let error = TransportError::ReadFailed {
            source: io::Error::new(io::ErrorKind::Interrupted, "interrupted"),
        };
        assert!(error.is_recoverable());
        assert!(!error.should_close_transport());
    }

    #[test]
    fn test_permanent_io_error_not_recoverable() {
        let error = TransportError::ReadFailed {
            source: io::Error::new(io::ErrorKind::BrokenPipe, "broken pipe"),
        };
        assert!(!error.is_recoverable());
        assert!(error.should_close_transport());
    }
}

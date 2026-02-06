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

//! Error types for the endpoint layer.

use crate::channel::ChannelError;
use crate::endpoint::ProtocolDirection;
use crate::transport::TransportError;
use std::fmt;

/// Errors that can occur in the endpoint layer.
///
/// This follows the three-layer error hierarchy from ADR-004:
/// - **Transient**: Temporary failures that may succeed on retry
/// - **Protocol**: Violations of protocol rules or incompatibilities
/// - **Fatal**: Unrecoverable errors requiring connection termination
#[derive(Debug)]
pub enum EndpointError {
    /// A transport-layer error occurred.
    Transport(TransportError),

    /// A channel-layer error occurred.
    Channel(ChannelError),

    /// Protocol is not registered with this endpoint.
    ProtocolNotRegistered {
        /// The protocol name that was not found.
        protocol: String,
    },

    /// Protocol is already registered with this endpoint.
    ProtocolAlreadyRegistered {
        /// The protocol name that is already registered.
        protocol: String,
    },

    /// Protocol direction is not supported for this operation.
    DirectionNotSupported {
        /// The protocol name.
        protocol: String,
        /// The operation that was attempted ("call" or "respond").
        operation: String,
        /// The direction this endpoint supports.
        our_direction: ProtocolDirection,
    },

    /// Incompatible protocol directions between endpoints.
    IncompatibleDirection {
        /// The protocol name.
        protocol: String,
        /// Our endpoint's direction.
        our_direction: ProtocolDirection,
        /// The peer endpoint's direction.
        peer_direction: ProtocolDirection,
    },

    /// Handshake failed.
    HandshakeFailed {
        /// Reason for the handshake failure.
        reason: String,
    },

    /// Protocol version negotiation failed.
    VersionNegotiationFailed {
        /// The protocol name.
        protocol: String,
        /// Versions we support.
        our_versions: Vec<u32>,
        /// Versions the peer supports.
        peer_versions: Vec<u32>,
    },

    /// No common protocol version found.
    NoCommonVersion {
        /// The protocol name.
        protocol: String,
        /// Versions we support.
        our_versions: Vec<u32>,
        /// Versions the peer supports.
        peer_versions: Vec<u32>,
    },

    /// Required feature is not supported.
    FeatureNotSupported {
        /// The protocol name.
        protocol: String,
        /// The required feature.
        feature: String,
    },

    /// Serializer mismatch between endpoints.
    SerializerMismatch {
        /// Our serializer name.
        ours: String,
        /// Peer's serializer name.
        theirs: String,
    },

    /// Connection not found.
    ConnectionNotFound {
        /// The connection identifier.
        connection_id: String,
    },

    /// Handler not registered for protocol.
    HandlerNotRegistered {
        /// The protocol name.
        protocol: String,
    },

    /// Invalid configuration.
    InvalidConfiguration {
        /// Description of the configuration error.
        reason: String,
    },

    /// I/O error.
    Io(std::io::Error),

    /// Serialization error.
    Serialization(String),
}

impl fmt::Display for EndpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(err) => write!(f, "transport error: {}", err),
            Self::Channel(err) => write!(f, "channel error: {}", err),
            Self::ProtocolNotRegistered { protocol } => {
                write!(f, "protocol '{}' is not registered", protocol)
            }
            Self::ProtocolAlreadyRegistered { protocol } => {
                write!(f, "protocol '{}' is already registered", protocol)
            }
            Self::DirectionNotSupported {
                protocol,
                operation,
                our_direction,
            } => {
                write!(
                    f,
                    "cannot {} protocol '{}': endpoint is {} only",
                    operation, protocol, our_direction
                )
            }
            Self::IncompatibleDirection {
                protocol,
                our_direction,
                peer_direction,
            } => {
                write!(
                    f,
                    "incompatible directions for protocol '{}': we are {}, peer is {}",
                    protocol, our_direction, peer_direction
                )
            }
            Self::HandshakeFailed { reason } => {
                write!(f, "handshake failed: {}", reason)
            }
            Self::VersionNegotiationFailed {
                protocol,
                our_versions,
                peer_versions,
            } => {
                write!(
                    f,
                    "version negotiation failed for protocol '{}': we support {:?}, peer supports {:?}",
                    protocol, our_versions, peer_versions
                )
            }
            Self::NoCommonVersion {
                protocol,
                our_versions,
                peer_versions,
            } => {
                write!(
                    f,
                    "no common version for protocol '{}': we support {:?}, peer supports {:?}",
                    protocol, our_versions, peer_versions
                )
            }
            Self::FeatureNotSupported { protocol, feature } => {
                write!(
                    f,
                    "feature '{}' not supported for protocol '{}'",
                    feature, protocol
                )
            }
            Self::SerializerMismatch { ours, theirs } => {
                write!(
                    f,
                    "serializer mismatch: we use '{}', peer uses '{}'",
                    ours, theirs
                )
            }
            Self::ConnectionNotFound { connection_id } => {
                write!(f, "connection '{}' not found", connection_id)
            }
            Self::HandlerNotRegistered { protocol } => {
                write!(f, "no handler registered for protocol '{}'", protocol)
            }
            Self::InvalidConfiguration { reason } => {
                write!(f, "invalid configuration: {}", reason)
            }
            Self::Io(err) => write!(f, "I/O error: {}", err),
            Self::Serialization(err) => write!(f, "serialization error: {}", err),
        }
    }
}

impl std::error::Error for EndpointError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transport(err) => Some(err),
            Self::Channel(err) => Some(err),
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<TransportError> for EndpointError {
    fn from(err: TransportError) -> Self {
        Self::Transport(err)
    }
}

impl From<ChannelError> for EndpointError {
    fn from(err: ChannelError) -> Self {
        Self::Channel(err)
    }
}

impl From<std::io::Error> for EndpointError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = EndpointError::ProtocolNotRegistered {
            protocol: "TestProtocol".to_string(),
        };
        assert_eq!(err.to_string(), "protocol 'TestProtocol' is not registered");

        let err = EndpointError::DirectionNotSupported {
            protocol: "TestProtocol".to_string(),
            operation: "call".to_string(),
            our_direction: ProtocolDirection::RespondOnly,
        };
        assert_eq!(
            err.to_string(),
            "cannot call protocol 'TestProtocol': endpoint is respond only only"
        );

        let err = EndpointError::IncompatibleDirection {
            protocol: "TestProtocol".to_string(),
            our_direction: ProtocolDirection::CallOnly,
            peer_direction: ProtocolDirection::CallOnly,
        };
        assert_eq!(
            err.to_string(),
            "incompatible directions for protocol 'TestProtocol': we are call only, peer is call only"
        );

        let err = EndpointError::NoCommonVersion {
            protocol: "TestProtocol".to_string(),
            our_versions: vec![1, 2],
            peer_versions: vec![3, 4],
        };
        assert_eq!(
            err.to_string(),
            "no common version for protocol 'TestProtocol': we support [1, 2], peer supports [3, 4]"
        );

        let err = EndpointError::FeatureNotSupported {
            protocol: "TestProtocol".to_string(),
            feature: "streaming".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "feature 'streaming' not supported for protocol 'TestProtocol'"
        );
    }

    #[test]
    fn test_error_from_transport() {
        let transport_err = TransportError::Closed;
        let endpoint_err: EndpointError = transport_err.into();
        assert!(matches!(endpoint_err, EndpointError::Transport(_)));
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::Other, "test error");
        let endpoint_err: EndpointError = io_err.into();
        assert!(matches!(endpoint_err, EndpointError::Io(_)));
    }
}

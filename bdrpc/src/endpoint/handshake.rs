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

//! Handshake protocol for endpoint capability negotiation.

use crate::endpoint::ProtocolDirection;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Messages exchanged during the handshake protocol.
///
/// The handshake allows endpoints to:
/// - Exchange protocol capabilities
/// - Negotiate serializers
/// - Negotiate protocol versions
/// - Validate direction compatibility
///
/// # Handshake Flow
///
/// 1. Client sends `Hello` with its capabilities
/// 2. Server responds with `Hello` with its capabilities
/// 3. Both sides negotiate compatible protocols
/// 4. Connection is ready for use
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HandshakeMessage {
    /// Initial hello message with endpoint capabilities.
    Hello {
        /// Endpoint identifier (optional).
        endpoint_id: Option<String>,
        /// Serializer name (e.g., "bincode", "json").
        serializer: String,
        /// Protocol capabilities.
        protocols: Vec<ProtocolCapability>,
        /// BDRPC version.
        bdrpc_version: String,
    },

    /// Acknowledgment of successful handshake.
    Ack {
        /// Negotiated protocols.
        negotiated: Vec<NegotiatedProtocol>,
    },

    /// Handshake error.
    Error {
        /// Error message.
        message: String,
    },
}

/// Metadata about a protocol that an endpoint supports.
///
/// This is exchanged during handshake to determine compatibility.
///
/// # Examples
///
/// ```rust
/// use bdrpc::endpoint::{ProtocolCapability, ProtocolDirection};
/// use std::collections::HashSet;
///
/// let capability = ProtocolCapability {
///     protocol_name: "UserService".to_string(),
///     supported_versions: vec![1, 2],
///     features: HashSet::from(["streaming".to_string()]),
///     direction: ProtocolDirection::Bidirectional,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolCapability {
    /// Protocol name (e.g., "UserService").
    pub protocol_name: String,

    /// Supported versions (e.g., [1, 2, 3]).
    pub supported_versions: Vec<u32>,

    /// Optional features (e.g., ["streaming", "compression"]).
    pub features: HashSet<String>,

    /// Direction support for this protocol.
    pub direction: ProtocolDirection,
}

impl ProtocolCapability {
    /// Creates a new protocol capability.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::{ProtocolCapability, ProtocolDirection};
    ///
    /// let capability = ProtocolCapability::new(
    ///     "UserService",
    ///     vec![1],
    ///     ProtocolDirection::Bidirectional,
    /// );
    /// ```
    pub fn new(
        protocol_name: impl Into<String>,
        supported_versions: Vec<u32>,
        direction: ProtocolDirection,
    ) -> Self {
        Self {
            protocol_name: protocol_name.into(),
            supported_versions,
            features: HashSet::new(),
            direction,
        }
    }

    /// Adds a feature to this capability.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::{ProtocolCapability, ProtocolDirection};
    ///
    /// let capability = ProtocolCapability::new("UserService", vec![1], ProtocolDirection::Bidirectional)
    ///     .with_feature("streaming");
    /// ```
    pub fn with_feature(mut self, feature: impl Into<String>) -> Self {
        self.features.insert(feature.into());
        self
    }

    /// Checks if this capability supports a specific version.
    pub fn supports_version(&self, version: u32) -> bool {
        self.supported_versions.contains(&version)
    }

    /// Checks if this capability has a specific feature.
    pub fn has_feature(&self, feature: &str) -> bool {
        self.features.contains(feature)
    }
}

/// Result of protocol negotiation between two endpoints.
///
/// This represents the agreed-upon protocol parameters after handshake.
///
/// # Examples
///
/// ```rust
/// use bdrpc::endpoint::NegotiatedProtocol;
/// use std::collections::HashSet;
///
/// let negotiated = NegotiatedProtocol {
///     name: "UserService".to_string(),
///     version: 2,
///     features: HashSet::from(["streaming".to_string()]),
///     we_can_call: true,
///     we_can_respond: true,
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegotiatedProtocol {
    /// Protocol name.
    pub name: String,

    /// Negotiated version.
    pub version: u32,

    /// Negotiated features (intersection of both endpoints).
    pub features: HashSet<String>,

    /// Whether we can call methods on this protocol.
    pub we_can_call: bool,

    /// Whether we can respond to methods on this protocol.
    pub we_can_respond: bool,
}

impl NegotiatedProtocol {
    /// Creates a new negotiated protocol.
    pub fn new(name: impl Into<String>, version: u32) -> Self {
        Self {
            name: name.into(),
            version,
            features: HashSet::new(),
            we_can_call: false,
            we_can_respond: false,
        }
    }

    /// Checks if we can use this protocol (either call or respond).
    pub fn is_usable(&self) -> bool {
        self.we_can_call || self.we_can_respond
    }

    /// Checks if this protocol is fully bidirectional.
    pub fn is_bidirectional(&self) -> bool {
        self.we_can_call && self.we_can_respond
    }
}

/// Negotiates protocol capabilities between two endpoints.
///
/// This function takes the capabilities from both endpoints and determines:
/// - Which protocols are compatible
/// - What version to use
/// - What features are available
/// - What directions are supported
///
/// # Examples
///
/// ```rust
/// use bdrpc::endpoint::{ProtocolCapability, ProtocolDirection, negotiate_protocols};
/// use std::collections::HashSet;
///
/// let our_caps = vec![
///     ProtocolCapability::new("UserService", vec![1, 2], ProtocolDirection::CallOnly),
/// ];
///
/// let peer_caps = vec![
///     ProtocolCapability::new("UserService", vec![2, 3], ProtocolDirection::RespondOnly),
/// ];
///
/// let negotiated = negotiate_protocols(&our_caps, &peer_caps);
/// assert_eq!(negotiated.len(), 1);
/// assert_eq!(negotiated[0].version, 2); // Highest common version
/// ```
pub fn negotiate_protocols(
    our_capabilities: &[ProtocolCapability],
    peer_capabilities: &[ProtocolCapability],
) -> Vec<NegotiatedProtocol> {
    let mut negotiated = Vec::new();

    // Build a map of peer capabilities for quick lookup
    let peer_map: HashMap<_, _> = peer_capabilities
        .iter()
        .map(|cap| (cap.protocol_name.as_str(), cap))
        .collect();

    for our_cap in our_capabilities {
        if let Some(peer_cap) = peer_map.get(our_cap.protocol_name.as_str()) {
            // Check direction compatibility
            if !our_cap.direction.is_compatible_with(&peer_cap.direction) {
                continue;
            }

            // Find highest common version
            if let Some(version) = negotiate_version(our_cap, peer_cap) {
                // Find common features
                let features: HashSet<_> = our_cap
                    .features
                    .intersection(&peer_cap.features)
                    .cloned()
                    .collect();

                // Determine what we can do
                let we_can_call = our_cap.direction.can_call() && peer_cap.direction.can_respond();
                let we_can_respond =
                    our_cap.direction.can_respond() && peer_cap.direction.can_call();

                negotiated.push(NegotiatedProtocol {
                    name: our_cap.protocol_name.clone(),
                    version,
                    features,
                    we_can_call,
                    we_can_respond,
                });
            }
        }
    }

    negotiated
}

/// Negotiates protocols with strict validation.
///
/// Unlike `negotiate_protocols`, this function returns an error if any protocol
/// cannot be negotiated, providing detailed information about what went wrong.
///
/// # Errors
///
/// Returns an error if:
/// - No common version exists for a protocol
/// - Directions are incompatible
/// - A protocol is missing from the peer
///
/// # Examples
///
/// ```rust
/// use bdrpc::endpoint::{ProtocolCapability, ProtocolDirection};
///
/// let our_caps = vec![
///     ProtocolCapability::new("UserService", vec![1, 2], ProtocolDirection::CallOnly),
/// ];
///
/// let peer_caps = vec![
///     ProtocolCapability::new("UserService", vec![3, 4], ProtocolDirection::RespondOnly),
/// ];
///
/// // Protocols can be negotiated during handshake
/// // This example shows incompatible versions (no overlap)
/// ```
#[allow(dead_code)] // Reserved for strict protocol negotiation mode
pub fn negotiate_protocols_strict(
    our_capabilities: &[ProtocolCapability],
    peer_capabilities: &[ProtocolCapability],
) -> Result<Vec<NegotiatedProtocol>, String> {
    let mut negotiated = Vec::new();
    let mut errors = Vec::new();

    // Build a map of peer capabilities for quick lookup
    let peer_map: HashMap<_, _> = peer_capabilities
        .iter()
        .map(|cap| (cap.protocol_name.as_str(), cap))
        .collect();

    for our_cap in our_capabilities {
        match peer_map.get(our_cap.protocol_name.as_str()) {
            None => {
                errors.push(format!(
                    "Protocol '{}' not found in peer capabilities",
                    our_cap.protocol_name
                ));
            }
            Some(peer_cap) => {
                // Check direction compatibility
                if !our_cap.direction.is_compatible_with(&peer_cap.direction) {
                    errors.push(format!(
                        "Incompatible directions for protocol '{}': we are {}, peer is {}",
                        our_cap.protocol_name, our_cap.direction, peer_cap.direction
                    ));
                    continue;
                }

                // Find highest common version
                match negotiate_version(our_cap, peer_cap) {
                    None => {
                        errors.push(format!(
                            "No common version for protocol '{}': we support {:?}, peer supports {:?}",
                            our_cap.protocol_name,
                            our_cap.supported_versions,
                            peer_cap.supported_versions
                        ));
                    }
                    Some(version) => {
                        // Find common features
                        let features: HashSet<_> = our_cap
                            .features
                            .intersection(&peer_cap.features)
                            .cloned()
                            .collect();

                        // Determine what we can do
                        let we_can_call =
                            our_cap.direction.can_call() && peer_cap.direction.can_respond();
                        let we_can_respond =
                            our_cap.direction.can_respond() && peer_cap.direction.can_call();

                        negotiated.push(NegotiatedProtocol {
                            name: our_cap.protocol_name.clone(),
                            version,
                            features,
                            we_can_call,
                            we_can_respond,
                        });
                    }
                }
            }
        }
    }

    if !errors.is_empty() {
        Err(errors.join("; "))
    } else if negotiated.is_empty() {
        Err("No protocols could be negotiated".to_string())
    } else {
        Ok(negotiated)
    }
}

/// Negotiates the version to use for a protocol.
///
/// Returns the highest version supported by both endpoints, or None if
/// there's no common version.
fn negotiate_version(our_cap: &ProtocolCapability, peer_cap: &ProtocolCapability) -> Option<u32> {
    our_cap
        .supported_versions
        .iter()
        .filter(|v| peer_cap.supported_versions.contains(v))
        .max()
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_capability_new() {
        let cap = ProtocolCapability::new("TestProtocol", vec![1, 2], ProtocolDirection::CallOnly);
        assert_eq!(cap.protocol_name, "TestProtocol");
        assert_eq!(cap.supported_versions, vec![1, 2]);
        assert_eq!(cap.direction, ProtocolDirection::CallOnly);
        assert!(cap.features.is_empty());
    }

    #[test]
    fn test_protocol_capability_with_feature() {
        let cap =
            ProtocolCapability::new("TestProtocol", vec![1], ProtocolDirection::Bidirectional)
                .with_feature("streaming")
                .with_feature("compression");
        assert!(cap.has_feature("streaming"));
        assert!(cap.has_feature("compression"));
        assert!(!cap.has_feature("encryption"));
    }

    #[test]
    fn test_negotiate_version() {
        let our_cap = ProtocolCapability::new("Test", vec![1, 2, 3], ProtocolDirection::CallOnly);
        let peer_cap =
            ProtocolCapability::new("Test", vec![2, 3, 4], ProtocolDirection::RespondOnly);

        let version = negotiate_version(&our_cap, &peer_cap);
        assert_eq!(version, Some(3)); // Highest common version
    }

    #[test]
    fn test_negotiate_version_no_common() {
        let our_cap = ProtocolCapability::new("Test", vec![1, 2], ProtocolDirection::CallOnly);
        let peer_cap = ProtocolCapability::new("Test", vec![3, 4], ProtocolDirection::RespondOnly);

        let version = negotiate_version(&our_cap, &peer_cap);
        assert_eq!(version, None);
    }

    #[test]
    fn test_negotiate_protocols_compatible() {
        let our_caps = vec![ProtocolCapability::new(
            "UserService",
            vec![1, 2],
            ProtocolDirection::CallOnly,
        )];

        let peer_caps = vec![ProtocolCapability::new(
            "UserService",
            vec![2, 3],
            ProtocolDirection::RespondOnly,
        )];

        let negotiated = negotiate_protocols(&our_caps, &peer_caps);
        assert_eq!(negotiated.len(), 1);
        assert_eq!(negotiated[0].name, "UserService");
        assert_eq!(negotiated[0].version, 2);
        assert!(negotiated[0].we_can_call);
        assert!(!negotiated[0].we_can_respond);
    }

    #[test]
    fn test_negotiate_protocols_incompatible_direction() {
        let our_caps = vec![ProtocolCapability::new(
            "UserService",
            vec![1],
            ProtocolDirection::CallOnly,
        )];

        let peer_caps = vec![ProtocolCapability::new(
            "UserService",
            vec![1],
            ProtocolDirection::CallOnly,
        )];

        let negotiated = negotiate_protocols(&our_caps, &peer_caps);
        assert_eq!(negotiated.len(), 0); // Incompatible directions
    }

    #[test]
    fn test_negotiate_protocols_bidirectional() {
        let our_caps = vec![ProtocolCapability::new(
            "ChatService",
            vec![1],
            ProtocolDirection::Bidirectional,
        )];

        let peer_caps = vec![ProtocolCapability::new(
            "ChatService",
            vec![1],
            ProtocolDirection::Bidirectional,
        )];

        let negotiated = negotiate_protocols(&our_caps, &peer_caps);
        assert_eq!(negotiated.len(), 1);
        assert!(negotiated[0].we_can_call);
        assert!(negotiated[0].we_can_respond);
        assert!(negotiated[0].is_bidirectional());
    }

    #[test]
    fn test_negotiate_protocols_features() {
        let our_caps = vec![
            ProtocolCapability::new("Service", vec![1], ProtocolDirection::Bidirectional)
                .with_feature("streaming")
                .with_feature("compression"),
        ];

        let peer_caps = vec![
            ProtocolCapability::new("Service", vec![1], ProtocolDirection::Bidirectional)
                .with_feature("streaming")
                .with_feature("encryption"),
        ];

        let negotiated = negotiate_protocols(&our_caps, &peer_caps);
        assert_eq!(negotiated.len(), 1);
        assert!(negotiated[0].features.contains("streaming"));
        assert!(!negotiated[0].features.contains("compression"));
        assert!(!negotiated[0].features.contains("encryption"));
    }

    #[test]
    fn test_negotiate_protocols_strict_success() {
        let our_caps = vec![ProtocolCapability::new(
            "UserService",
            vec![1, 2],
            ProtocolDirection::CallOnly,
        )];

        let peer_caps = vec![ProtocolCapability::new(
            "UserService",
            vec![2, 3],
            ProtocolDirection::RespondOnly,
        )];

        let result = negotiate_protocols_strict(&our_caps, &peer_caps);
        assert!(result.is_ok());
        let negotiated = result.unwrap();
        assert_eq!(negotiated.len(), 1);
        assert_eq!(negotiated[0].version, 2);
    }

    #[test]
    fn test_negotiate_protocols_strict_no_common_version() {
        let our_caps = vec![ProtocolCapability::new(
            "UserService",
            vec![1, 2],
            ProtocolDirection::CallOnly,
        )];

        let peer_caps = vec![ProtocolCapability::new(
            "UserService",
            vec![3, 4],
            ProtocolDirection::RespondOnly,
        )];

        let result = negotiate_protocols_strict(&our_caps, &peer_caps);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("No common version"));
        assert!(err.contains("UserService"));
    }

    #[test]
    fn test_negotiate_protocols_strict_incompatible_direction() {
        let our_caps = vec![ProtocolCapability::new(
            "UserService",
            vec![1],
            ProtocolDirection::CallOnly,
        )];

        let peer_caps = vec![ProtocolCapability::new(
            "UserService",
            vec![1],
            ProtocolDirection::CallOnly,
        )];

        let result = negotiate_protocols_strict(&our_caps, &peer_caps);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Incompatible directions"));
    }

    #[test]
    fn test_negotiate_protocols_strict_protocol_not_found() {
        let our_caps = vec![ProtocolCapability::new(
            "UserService",
            vec![1],
            ProtocolDirection::CallOnly,
        )];

        let peer_caps = vec![ProtocolCapability::new(
            "OtherService",
            vec![1],
            ProtocolDirection::RespondOnly,
        )];

        let result = negotiate_protocols_strict(&our_caps, &peer_caps);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("not found in peer capabilities"));
    }

    #[test]
    fn test_negotiate_protocols_strict_multiple_errors() {
        let our_caps = vec![
            ProtocolCapability::new("Service1", vec![1, 2], ProtocolDirection::CallOnly),
            ProtocolCapability::new("Service2", vec![1], ProtocolDirection::CallOnly),
        ];

        let peer_caps = vec![
            ProtocolCapability::new("Service1", vec![3, 4], ProtocolDirection::RespondOnly),
            ProtocolCapability::new("Service2", vec![1], ProtocolDirection::CallOnly),
        ];

        let result = negotiate_protocols_strict(&our_caps, &peer_caps);
        assert!(result.is_err());
        let err = result.unwrap_err();
        // Should contain errors for both protocols
        assert!(err.contains("Service1"));
        assert!(err.contains("Service2"));
    }

    #[test]
    fn test_negotiate_protocols_strict_empty_capabilities() {
        let our_caps: Vec<ProtocolCapability> = vec![];
        let peer_caps: Vec<ProtocolCapability> = vec![];

        let result = negotiate_protocols_strict(&our_caps, &peer_caps);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("No protocols could be negotiated"));
    }

    #[test]
    fn test_handshake_message_serialization() {
        let msg = HandshakeMessage::Hello {
            endpoint_id: Some("test-endpoint".to_string()),
            serializer: "bincode".to_string(),
            protocols: vec![ProtocolCapability::new(
                "TestProtocol",
                vec![1],
                ProtocolDirection::Bidirectional,
            )],
            bdrpc_version: "0.1.0".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: HandshakeMessage = serde_json::from_str(&json).unwrap();

        match deserialized {
            HandshakeMessage::Hello {
                endpoint_id,
                serializer,
                ..
            } => {
                assert_eq!(endpoint_id, Some("test-endpoint".to_string()));
                assert_eq!(serializer, "bincode");
            }
            _ => panic!("Wrong message type"),
        }
    }
}

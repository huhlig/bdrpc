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

//! Protocol direction support (per ADR-008).

use serde::{Deserialize, Serialize};

/// Specifies which directions of a protocol an endpoint supports.
///
/// This enum allows endpoints to declare whether they can call methods,
/// respond to methods, or both. This enables:
///
/// - **Client-Server**: Client calls, server responds
/// - **Server Push**: Server calls, client responds
/// - **Peer-to-Peer**: Both sides can call and respond
/// - **Hybrid**: Mix of patterns on the same endpoint
///
/// # Examples
///
/// ```rust
/// use bdrpc::endpoint::ProtocolDirection;
///
/// // Client can only call methods
/// let client_dir = ProtocolDirection::CallOnly;
/// assert!(client_dir.can_call());
/// assert!(!client_dir.can_respond());
///
/// // Server can only respond to methods
/// let server_dir = ProtocolDirection::RespondOnly;
/// assert!(!server_dir.can_call());
/// assert!(server_dir.can_respond());
///
/// // Peer can do both
/// let peer_dir = ProtocolDirection::Bidirectional;
/// assert!(peer_dir.can_call());
/// assert!(peer_dir.can_respond());
///
/// // Check compatibility
/// assert!(client_dir.is_compatible_with(&server_dir));
/// assert!(server_dir.is_compatible_with(&client_dir));
/// assert!(peer_dir.is_compatible_with(&peer_dir));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProtocolDirection {
    /// Can only call methods (send requests, receive responses).
    ///
    /// Typical for clients in traditional RPC patterns.
    CallOnly,

    /// Can only respond to methods (receive requests, send responses).
    ///
    /// Typical for servers in traditional RPC patterns.
    RespondOnly,

    /// Can both call and respond (full bi-directional).
    ///
    /// Typical for peer-to-peer or hybrid patterns.
    Bidirectional,
}

impl ProtocolDirection {
    /// Check if this direction allows calling methods.
    ///
    /// Returns `true` for [`CallOnly`](Self::CallOnly) and
    /// [`Bidirectional`](Self::Bidirectional).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::ProtocolDirection;
    ///
    /// assert!(ProtocolDirection::CallOnly.can_call());
    /// assert!(!ProtocolDirection::RespondOnly.can_call());
    /// assert!(ProtocolDirection::Bidirectional.can_call());
    /// ```
    #[inline]
    pub fn can_call(&self) -> bool {
        matches!(self, Self::CallOnly | Self::Bidirectional)
    }

    /// Check if this direction allows responding to methods.
    ///
    /// Returns `true` for [`RespondOnly`](Self::RespondOnly) and
    /// [`Bidirectional`](Self::Bidirectional).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::ProtocolDirection;
    ///
    /// assert!(!ProtocolDirection::CallOnly.can_respond());
    /// assert!(ProtocolDirection::RespondOnly.can_respond());
    /// assert!(ProtocolDirection::Bidirectional.can_respond());
    /// ```
    #[inline]
    pub fn can_respond(&self) -> bool {
        matches!(self, Self::RespondOnly | Self::Bidirectional)
    }

    /// Check if this direction is compatible with another.
    ///
    /// Two directions are compatible if they can communicate:
    /// - One can call and the other can respond, OR
    /// - One can respond and the other can call
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::ProtocolDirection;
    ///
    /// let client = ProtocolDirection::CallOnly;
    /// let server = ProtocolDirection::RespondOnly;
    /// let peer = ProtocolDirection::Bidirectional;
    ///
    /// // Client and server are compatible
    /// assert!(client.is_compatible_with(&server));
    /// assert!(server.is_compatible_with(&client));
    ///
    /// // Peer is compatible with everyone
    /// assert!(peer.is_compatible_with(&client));
    /// assert!(peer.is_compatible_with(&server));
    /// assert!(peer.is_compatible_with(&peer));
    ///
    /// // CallOnly endpoints cannot talk to each other
    /// assert!(!client.is_compatible_with(&client));
    ///
    /// // RespondOnly endpoints cannot talk to each other
    /// assert!(!server.is_compatible_with(&server));
    /// ```
    #[inline]
    pub fn is_compatible_with(&self, other: &Self) -> bool {
        (self.can_call() && other.can_respond()) || (self.can_respond() && other.can_call())
    }

    /// Returns a human-readable description of this direction.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::endpoint::ProtocolDirection;
    ///
    /// assert_eq!(ProtocolDirection::CallOnly.description(), "call only");
    /// assert_eq!(ProtocolDirection::RespondOnly.description(), "respond only");
    /// assert_eq!(ProtocolDirection::Bidirectional.description(), "bidirectional");
    /// ```
    pub fn description(&self) -> &'static str {
        match self {
            Self::CallOnly => "call only",
            Self::RespondOnly => "respond only",
            Self::Bidirectional => "bidirectional",
        }
    }
}

impl Default for ProtocolDirection {
    /// Returns [`Bidirectional`](Self::Bidirectional) as the default.
    ///
    /// This provides the most flexible behavior and maintains backward
    /// compatibility with code that doesn't explicitly specify direction.
    fn default() -> Self {
        Self::Bidirectional
    }
}

impl std::fmt::Display for ProtocolDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.description())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_call() {
        assert!(ProtocolDirection::CallOnly.can_call());
        assert!(!ProtocolDirection::RespondOnly.can_call());
        assert!(ProtocolDirection::Bidirectional.can_call());
    }

    #[test]
    fn test_can_respond() {
        assert!(!ProtocolDirection::CallOnly.can_respond());
        assert!(ProtocolDirection::RespondOnly.can_respond());
        assert!(ProtocolDirection::Bidirectional.can_respond());
    }

    #[test]
    fn test_compatibility() {
        let call = ProtocolDirection::CallOnly;
        let respond = ProtocolDirection::RespondOnly;
        let bi = ProtocolDirection::Bidirectional;

        // CallOnly and RespondOnly are compatible
        assert!(call.is_compatible_with(&respond));
        assert!(respond.is_compatible_with(&call));

        // Bidirectional is compatible with everything
        assert!(bi.is_compatible_with(&call));
        assert!(bi.is_compatible_with(&respond));
        assert!(bi.is_compatible_with(&bi));
        assert!(call.is_compatible_with(&bi));
        assert!(respond.is_compatible_with(&bi));

        // Same unidirectional directions are not compatible
        assert!(!call.is_compatible_with(&call));
        assert!(!respond.is_compatible_with(&respond));
    }

    #[test]
    fn test_default() {
        assert_eq!(
            ProtocolDirection::default(),
            ProtocolDirection::Bidirectional
        );
    }

    #[test]
    fn test_display() {
        assert_eq!(ProtocolDirection::CallOnly.to_string(), "call only");
        assert_eq!(ProtocolDirection::RespondOnly.to_string(), "respond only");
        assert_eq!(
            ProtocolDirection::Bidirectional.to_string(),
            "bidirectional"
        );
    }

    #[test]
    fn test_serialization() {
        // Test that the enum can be serialized and deserialized
        let directions = vec![
            ProtocolDirection::CallOnly,
            ProtocolDirection::RespondOnly,
            ProtocolDirection::Bidirectional,
        ];

        for dir in directions {
            let json = serde_json::to_string(&dir).unwrap();
            let deserialized: ProtocolDirection = serde_json::from_str(&json).unwrap();
            assert_eq!(dir, deserialized);
        }
    }
}

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

use std::fmt;
use std::net::SocketAddr;

/// Unique identifier for a transport connection.
///
/// Transport IDs are used to track and manage individual transport instances
/// within the transport manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TransportId(u64);

impl TransportId {
    /// Creates a new transport ID.
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Returns the raw ID value.
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for TransportId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Transport({})", self.0)
    }
}

/// Metadata associated with a transport connection.
///
/// This provides information about the transport that can be used for
/// logging, metrics, and debugging.
#[derive(Debug, Clone)]
pub struct TransportMetadata {
    /// Unique identifier for this transport
    pub id: TransportId,

    /// Local address of the connection, if available
    pub local_addr: Option<SocketAddr>,

    /// Remote peer address, if available
    pub peer_addr: Option<SocketAddr>,

    /// Transport type (e.g., "tcp", "memory", "tls")
    pub transport_type: String,

    /// When the transport was created
    pub created_at: std::time::Instant,
}

impl TransportMetadata {
    /// Creates new transport metadata.
    pub fn new(id: TransportId, transport_type: impl Into<String>) -> Self {
        Self {
            id,
            local_addr: None,
            peer_addr: None,
            transport_type: transport_type.into(),
            created_at: std::time::Instant::now(),
        }
    }

    /// Sets the local address.
    pub fn with_local_addr(mut self, addr: SocketAddr) -> Self {
        self.local_addr = Some(addr);
        self
    }

    /// Sets the peer address.
    pub fn with_peer_addr(mut self, addr: SocketAddr) -> Self {
        self.peer_addr = Some(addr);
        self
    }

    /// Returns the age of this transport.
    pub fn age(&self) -> std::time::Duration {
        self.created_at.elapsed()
    }
}

/// Represents an active transport connection.
///
/// This struct tracks metadata about an active connection, including its
/// transport ID, type, and associated caller transport (if applicable).
///
/// # Examples
///
/// ```rust
/// use bdrpc::transport::{TransportConnection, TransportId, TransportType};
///
/// let connection = TransportConnection::new(
///     TransportId::new(1),
///     TransportType::Tcp,
///     Some("main".to_string()),
/// );
///
/// assert_eq!(connection.transport_id().as_u64(), 1);
/// assert_eq!(connection.transport_type(), TransportType::Tcp);
/// assert_eq!(connection.caller_name(), Some("main"));
/// ```
#[derive(Debug, Clone)]
pub struct TransportConnection {
    /// Unique identifier for this connection
    transport_id: TransportId,

    /// Type of transport protocol
    transport_type: crate::transport::TransportType,

    /// Name of the caller transport, if this is a client connection
    caller_name: Option<String>,

    /// When this connection was established
    connected_at: std::time::Instant,
}

impl TransportConnection {
    /// Creates a new transport connection.
    ///
    /// # Arguments
    ///
    /// * `transport_id` - Unique identifier for this connection
    /// * `transport_type` - Type of transport protocol
    /// * `caller_name` - Optional name of the caller transport (for client connections)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::transport::{TransportConnection, TransportId, TransportType};
    ///
    /// // Server connection (no caller name)
    /// let server_conn = TransportConnection::new(
    ///     TransportId::new(1),
    ///     TransportType::Tcp,
    ///     None,
    /// );
    ///
    /// // Client connection (with caller name)
    /// let client_conn = TransportConnection::new(
    ///     TransportId::new(2),
    ///     TransportType::Tcp,
    ///     Some("main".to_string()),
    /// );
    /// ```
    pub fn new(
        transport_id: TransportId,
        transport_type: crate::transport::TransportType,
        caller_name: Option<String>,
    ) -> Self {
        Self {
            transport_id,
            transport_type,
            caller_name,
            connected_at: std::time::Instant::now(),
        }
    }

    /// Returns the transport ID.
    pub fn transport_id(&self) -> TransportId {
        self.transport_id
    }

    /// Returns the transport type.
    pub fn transport_type(&self) -> crate::transport::TransportType {
        self.transport_type
    }

    /// Returns the caller name, if this is a client connection.
    pub fn caller_name(&self) -> Option<&str> {
        self.caller_name.as_deref()
    }

    /// Returns how long this connection has been active.
    pub fn uptime(&self) -> std::time::Duration {
        self.connected_at.elapsed()
    }

    /// Checks if this is a client connection (has a caller name).
    pub fn is_client(&self) -> bool {
        self.caller_name.is_some()
    }

    /// Checks if this is a server connection (no caller name).
    pub fn is_server(&self) -> bool {
        self.caller_name.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{TransportId, TransportMetadata, TransportType};

    #[test]
    fn test_transport_id_creation() {
        let id = TransportId::new(42);
        assert_eq!(id.as_u64(), 42);
    }

    #[test]
    fn test_transport_id_display() {
        let id = TransportId::new(123);
        assert_eq!(format!("{}", id), "Transport(123)");
    }

    #[test]
    fn test_transport_metadata_creation() {
        let id = TransportId::new(1);
        let metadata = TransportMetadata::new(id, "test");

        assert_eq!(metadata.id, id);
        assert_eq!(metadata.transport_type, "test");
        assert!(metadata.local_addr.is_none());
        assert!(metadata.peer_addr.is_none());
    }

    #[test]
    fn test_transport_metadata_with_addresses() {
        let id = TransportId::new(1);
        let local = "127.0.0.1:8080".parse().unwrap();
        let peer = "127.0.0.1:9090".parse().unwrap();

        let metadata = TransportMetadata::new(id, "test")
            .with_local_addr(local)
            .with_peer_addr(peer);

        assert_eq!(metadata.local_addr, Some(local));
        assert_eq!(metadata.peer_addr, Some(peer));
    }

    #[test]
    fn test_transport_metadata_age() {
        let id = TransportId::new(1);
        let metadata = TransportMetadata::new(id, "test");

        std::thread::sleep(std::time::Duration::from_millis(10));

        let age = metadata.age();
        assert!(age.as_millis() >= 10);
    }

    #[test]
    fn test_transport_connection_new() {
        let conn = TransportConnection::new(
            TransportId::new(1),
            TransportType::Tcp,
            Some("main".to_string()),
        );

        assert_eq!(conn.transport_id().as_u64(), 1);
        assert_eq!(conn.transport_type(), TransportType::Tcp);
        assert_eq!(conn.caller_name(), Some("main"));
        assert!(conn.is_client());
        assert!(!conn.is_server());
    }

    #[test]
    fn test_transport_connection_server() {
        let conn = TransportConnection::new(TransportId::new(2), TransportType::Tcp, None);

        assert_eq!(conn.caller_name(), None);
        assert!(!conn.is_client());
        assert!(conn.is_server());
    }

    #[test]
    fn test_transport_connection_uptime() {
        let conn = TransportConnection::new(TransportId::new(1), TransportType::Tcp, None);

        std::thread::sleep(std::time::Duration::from_millis(10));

        let uptime = conn.uptime();
        assert!(uptime.as_millis() >= 10);
    }
}

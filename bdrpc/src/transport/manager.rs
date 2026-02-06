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

//! Transport lifecycle management.
//!
//! This module provides the [`TransportManager`] which manages the lifecycle
//! of transport connections, assigns unique IDs, and tracks active transports.

use crate::transport::{TransportId, TransportMetadata};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;

/// Manages the lifecycle of transport connections.
///
/// The `TransportManager` is responsible for:
/// - Assigning unique transport IDs
/// - Tracking transport metadata
/// - Managing transport registration and removal
/// - Providing access to transport information
///
/// Note: The manager tracks metadata only. The caller is responsible for
/// managing the actual transport instances and their lifecycle.
///
/// # Example
///
/// ```rust
/// use bdrpc::transport::{TransportManager, TcpTransport, Transport};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let manager = TransportManager::new();
///
/// // Connect to a server
/// let transport = TcpTransport::connect("127.0.0.1:8080").await?;
///
/// // Register the transport's metadata
/// let metadata = transport.metadata().clone();
/// manager.register(metadata.clone()).await;
///
/// // Get transport metadata
/// if let Some(metadata) = manager.get_metadata(metadata.id).await {
///     println!("Transport {} connected to {:?}", metadata.id, metadata.peer_addr);
/// }
///
/// // Remove transport when done
/// manager.remove(metadata.id).await;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct TransportManager {
    /// Shared state
    inner: Arc<TransportManagerInner>,
}

struct TransportManagerInner {
    /// Next transport ID to assign
    next_id: AtomicU64,

    /// Active transport metadata
    transports: RwLock<HashMap<TransportId, TransportMetadata>>,
}

impl TransportManager {
    /// Creates a new transport manager.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::transport::TransportManager;
    ///
    /// let manager = TransportManager::new();
    /// ```
    pub fn new() -> Self {
        Self {
            inner: Arc::new(TransportManagerInner {
                next_id: AtomicU64::new(1),
                transports: RwLock::new(HashMap::new()),
            }),
        }
    }

    /// Generates a new unique transport ID.
    ///
    /// IDs are assigned sequentially starting from 1.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::transport::TransportManager;
    ///
    /// let manager = TransportManager::new();
    /// let id1 = manager.next_id();
    /// let id2 = manager.next_id();
    /// assert_ne!(id1, id2);
    /// ```
    pub fn next_id(&self) -> TransportId {
        let id = self.inner.next_id.fetch_add(1, Ordering::SeqCst);
        TransportId::new(id)
    }

    /// Registers transport metadata with the manager.
    ///
    /// This tracks the transport's metadata for lifecycle management.
    /// The caller is responsible for managing the actual transport instance.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::transport::{TransportManager, TcpTransport, Transport};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let transport = TcpTransport::connect("127.0.0.1:8080").await?;
    /// let metadata = transport.metadata().clone();
    /// manager.register(metadata.clone()).await;
    /// println!("Registered transport with ID: {}", metadata.id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn register(&self, metadata: TransportMetadata) {
        let id = metadata.id;
        self.inner.transports.write().await.insert(id, metadata);
    }

    /// Removes a transport from the manager.
    ///
    /// This removes the transport metadata from tracking. The caller is
    /// responsible for shutting down the actual transport instance.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::transport::{TransportManager, TcpTransport, Transport};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let transport = TcpTransport::connect("127.0.0.1:8080").await?;
    /// let metadata = transport.metadata().clone();
    /// manager.register(metadata.clone()).await;
    ///
    /// // Later, remove the transport
    /// manager.remove(metadata.id).await;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn remove(&self, id: TransportId) -> Option<TransportMetadata> {
        self.inner.transports.write().await.remove(&id)
    }

    /// Gets metadata for a transport.
    ///
    /// Returns `None` if the transport is not registered.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::transport::{TransportManager, TcpTransport, Transport};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let transport = TcpTransport::connect("127.0.0.1:8080").await?;
    /// let metadata = transport.metadata().clone();
    /// manager.register(metadata.clone()).await;
    ///
    /// if let Some(metadata) = manager.get_metadata(metadata.id).await {
    ///     println!("Transport type: {}", metadata.transport_type);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_metadata(&self, id: TransportId) -> Option<TransportMetadata> {
        self.inner.transports.read().await.get(&id).cloned()
    }

    /// Returns the number of active transports.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::transport::{TransportManager, TcpTransport, Transport};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// assert_eq!(manager.count().await, 0);
    ///
    /// let transport = TcpTransport::connect("127.0.0.1:8080").await?;
    /// let metadata = transport.metadata().clone();
    /// manager.register(metadata).await;
    /// assert_eq!(manager.count().await, 1);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn count(&self) -> usize {
        self.inner.transports.read().await.len()
    }

    /// Returns a list of all active transport IDs.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::transport::{TransportManager, TcpTransport, Transport};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let transport = TcpTransport::connect("127.0.0.1:8080").await?;
    /// let metadata = transport.metadata().clone();
    /// manager.register(metadata.clone()).await;
    ///
    /// let ids = manager.list_ids().await;
    /// assert!(ids.contains(&metadata.id));
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_ids(&self) -> Vec<TransportId> {
        self.inner.transports.read().await.keys().copied().collect()
    }

    /// Returns metadata for all active transports.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::transport::{TransportManager, TcpTransport, Transport};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let transport = TcpTransport::connect("127.0.0.1:8080").await?;
    /// let metadata = transport.metadata().clone();
    /// manager.register(metadata).await;
    ///
    /// for metadata in manager.list_metadata().await {
    ///     println!("Transport {}: {}", metadata.id, metadata.transport_type);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_metadata(&self) -> Vec<TransportMetadata> {
        self.inner
            .transports
            .read()
            .await
            .values()
            .cloned()
            .collect()
    }

    /// Removes all transport metadata from tracking.
    ///
    /// This clears all tracked transports. The caller is responsible for
    /// shutting down the actual transport instances.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::transport::{TransportManager, TcpTransport, Transport};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let transport = TcpTransport::connect("127.0.0.1:8080").await?;
    /// let metadata = transport.metadata().clone();
    /// manager.register(metadata).await;
    ///
    /// // Clear all transports
    /// manager.clear().await;
    /// assert_eq!(manager.count().await, 0);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn clear(&self) {
        self.inner.transports.write().await.clear();
    }

    /// Checks if a transport is registered.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::transport::{TransportManager, TcpTransport, Transport};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let transport = TcpTransport::connect("127.0.0.1:8080").await?;
    /// let metadata = transport.metadata().clone();
    /// manager.register(metadata.clone()).await;
    ///
    /// assert!(manager.contains(metadata.id).await);
    /// manager.remove(metadata.id).await;
    /// assert!(!manager.contains(metadata.id).await);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn contains(&self, id: TransportId) -> bool {
        self.inner.transports.read().await.contains_key(&id)
    }
}

impl Default for TransportManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{MemoryTransport, Transport};

    #[tokio::test]
    async fn test_next_id_unique() {
        let manager = TransportManager::new();
        let id1 = manager.next_id();
        let id2 = manager.next_id();
        let id3 = manager.next_id();

        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);
    }

    #[tokio::test]
    async fn test_register_and_get_metadata() {
        let manager = TransportManager::new();
        let (transport1, _transport2) = MemoryTransport::pair_default();

        let metadata = transport1.metadata().clone();
        let id = metadata.id;
        manager.register(metadata).await;

        let retrieved = manager.get_metadata(id).await;
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, id);
        assert_eq!(retrieved.transport_type, "memory");
    }

    #[tokio::test]
    async fn test_remove_transport() {
        let manager = TransportManager::new();
        let (transport1, _transport2) = MemoryTransport::pair_default();

        let metadata = transport1.metadata().clone();
        let id = metadata.id;
        manager.register(metadata).await;
        assert!(manager.contains(id).await);

        let removed = manager.remove(id).await;
        assert!(removed.is_some());
        assert!(!manager.contains(id).await);
    }

    #[tokio::test]
    async fn test_count() {
        let manager = TransportManager::new();
        assert_eq!(manager.count().await, 0);

        let (transport1, _) = MemoryTransport::pair_default();
        let (transport2, _) = MemoryTransport::pair_default();

        manager.register(transport1.metadata().clone()).await;
        assert_eq!(manager.count().await, 1);

        manager.register(transport2.metadata().clone()).await;
        assert_eq!(manager.count().await, 2);
    }

    #[tokio::test]
    async fn test_list_ids() {
        let manager = TransportManager::new();
        let (transport1, _) = MemoryTransport::pair_default();
        let (transport2, _) = MemoryTransport::pair_default();

        let id1 = transport1.metadata().id;
        let id2 = transport2.metadata().id;

        manager.register(transport1.metadata().clone()).await;
        manager.register(transport2.metadata().clone()).await;

        let ids = manager.list_ids().await;
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&id1));
        assert!(ids.contains(&id2));
    }

    #[tokio::test]
    async fn test_list_metadata() {
        let manager = TransportManager::new();
        let (transport1, _) = MemoryTransport::pair_default();
        let (transport2, _) = MemoryTransport::pair_default();

        manager.register(transport1.metadata().clone()).await;
        manager.register(transport2.metadata().clone()).await;

        let metadata_list = manager.list_metadata().await;
        assert_eq!(metadata_list.len(), 2);
        assert!(metadata_list.iter().all(|m| m.transport_type == "memory"));
    }

    #[tokio::test]
    async fn test_clear() {
        let manager = TransportManager::new();
        let (transport1, _) = MemoryTransport::pair_default();
        let (transport2, _) = MemoryTransport::pair_default();

        manager.register(transport1.metadata().clone()).await;
        manager.register(transport2.metadata().clone()).await;
        assert_eq!(manager.count().await, 2);

        manager.clear().await;
        assert_eq!(manager.count().await, 0);
    }

    #[tokio::test]
    async fn test_contains() {
        let manager = TransportManager::new();
        let (transport1, _) = MemoryTransport::pair_default();

        let id = transport1.metadata().id;
        manager.register(transport1.metadata().clone()).await;
        assert!(manager.contains(id).await);

        let fake_id = TransportId::new(99999);
        assert!(!manager.contains(fake_id).await);
    }

    #[tokio::test]
    async fn test_multiple_managers() {
        let manager1 = TransportManager::new();
        let manager2 = TransportManager::new();

        let (transport1, _) = MemoryTransport::pair_default();
        let (transport2, _) = MemoryTransport::pair_default();

        let id1 = transport1.metadata().id;
        let id2 = transport2.metadata().id;

        manager1.register(transport1.metadata().clone()).await;
        manager2.register(transport2.metadata().clone()).await;

        assert!(manager1.contains(id1).await);
        assert!(!manager1.contains(id2).await);
        assert!(manager2.contains(id2).await);
        assert!(!manager2.contains(id1).await);
    }

    #[tokio::test]
    async fn test_clone_manager() {
        let manager1 = TransportManager::new();
        let manager2 = manager1.clone();

        let (transport1, _) = MemoryTransport::pair_default();
        let id = transport1.metadata().id;
        manager1.register(transport1.metadata().clone()).await;

        // Both managers should see the same transport
        assert!(manager1.contains(id).await);
        assert!(manager2.contains(id).await);
        assert_eq!(manager1.count().await, manager2.count().await);
    }
}

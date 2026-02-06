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

//! Channel manager for multiplexing multiple channels.

use super::{Channel, ChannelError, ChannelId, ChannelReceiver, ChannelSender, Protocol};
use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Manages multiple channels and routes messages to the correct channel.
///
/// The channel manager provides:
/// - **Channel registration**: Create and register new channels
/// - **Message routing**: Route incoming messages to the correct channel by ID
/// - **Lifecycle management**: Track active channels and clean up closed ones
/// - **Multiplexing**: Multiple channels share the manager
///
/// # Example
///
/// ```rust,no_run
/// use bdrpc::channel::{ChannelManager, ChannelId, Protocol};
///
/// #[derive(Debug, Clone)]
/// enum MyProtocol { Ping }
/// impl Protocol for MyProtocol {
///     fn method_name(&self) -> &'static str { "ping" }
///     fn is_request(&self) -> bool { true }
/// }
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let manager = ChannelManager::new();
///
/// // Create a channel
/// let channel_id = ChannelId::new();
/// let sender = manager.create_channel::<MyProtocol>(channel_id, 100).await?;
///
/// // Send a message
/// sender.send(MyProtocol::Ping).await?;
///
/// // Get the channel to receive
/// let mut receiver = manager.get_receiver::<MyProtocol>(channel_id).await?;
/// let msg = receiver.recv().await.unwrap();
/// # Ok(())
/// # }
/// ```
pub struct ChannelManager {
    /// Map of channel ID to channel sender (type-erased).
    channels: Arc<RwLock<HashMap<ChannelId, Box<dyn ChannelHandler>>>>,
}

impl std::fmt::Debug for ChannelManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelManager")
            .field("channels", &"<channels>")
            .finish()
    }
}

/// Type-erased channel handler trait.
///
/// This allows storing channels of different protocol types in the same
/// collection. Each channel handler can route messages to its specific
/// protocol type.
trait ChannelHandler: Send + Sync + Any {
    /// Returns the channel ID.
    #[allow(dead_code)] // Reserved for future use
    fn id(&self) -> ChannelId;

    /// Checks if the channel is closed.
    fn is_closed(&self) -> bool;

    /// Returns self as Any for downcasting.
    fn as_any(&self) -> &dyn Any;

    /// Returns self as mutable Any for downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Concrete implementation of ChannelHandler for a specific protocol.
struct TypedChannelHandler<P: Protocol> {
    #[allow(dead_code)] // Used for debugging and future features
    id: ChannelId,
    sender: ChannelSender<P>,
    receiver: Option<ChannelReceiver<P>>,
}

impl<P: Protocol> ChannelHandler for TypedChannelHandler<P> {
    fn id(&self) -> ChannelId {
        self.id
    }

    fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl ChannelManager {
    /// Creates a new channel manager.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::channel::ChannelManager;
    ///
    /// let manager = ChannelManager::new();
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Creates a new channel with the given ID and buffer size.
    ///
    /// The channel is automatically registered with the manager. Returns
    /// the sender half of the channel. Use [`get_receiver`] to get the
    /// receiver half.
    ///
    /// # Errors
    ///
    /// Returns [`ChannelError::AlreadyExists`] if a channel with this ID
    /// already exists.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use bdrpc::channel::{ChannelManager, ChannelId, Protocol};
    /// # #[derive(Debug, Clone)]
    /// # enum MyProtocol { Ping }
    /// # impl Protocol for MyProtocol {
    /// #     fn method_name(&self) -> &'static str { "ping" }
    /// #     fn is_request(&self) -> bool { true }
    /// # }
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = ChannelManager::new();
    /// let sender = manager.create_channel::<MyProtocol>(ChannelId::new(), 100).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_channel<P: Protocol>(
        &self,
        id: ChannelId,
        buffer_size: usize,
    ) -> Result<ChannelSender<P>, ChannelError> {
        let mut channels = self.channels.write().await;

        // Check if channel already exists
        if channels.contains_key(&id) {
            return Err(ChannelError::AlreadyExists { channel_id: id });
        }

        // Create the channel
        let (sender, receiver) = Channel::new_in_memory(id, buffer_size);

        // Store the sender and receiver in the manager
        let handler = Box::new(TypedChannelHandler {
            id,
            sender: sender.clone(),
            receiver: Some(receiver),
        });
        channels.insert(id, handler);

        Ok(sender)
    }

    /// Gets the sender for an existing channel.
    ///
    /// # Errors
    ///
    /// Returns [`ChannelError::NotFound`] if the channel doesn't exist.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use bdrpc::channel::{ChannelManager, ChannelId, Protocol};
    /// # #[derive(Debug, Clone)]
    /// # enum MyProtocol { Ping }
    /// # impl Protocol for MyProtocol {
    /// #     fn method_name(&self) -> &'static str { "ping" }
    /// #     fn is_request(&self) -> bool { true }
    /// # }
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = ChannelManager::new();
    /// let id = ChannelId::new();
    /// manager.create_channel::<MyProtocol>(id, 100).await?;
    ///
    /// let sender = manager.get_sender::<MyProtocol>(id).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_sender<P: Protocol>(
        &self,
        id: ChannelId,
    ) -> Result<ChannelSender<P>, ChannelError> {
        let channels = self.channels.read().await;

        channels
            .get(&id)
            .and_then(|handler| {
                // Downcast to the specific protocol type
                handler
                    .as_any()
                    .downcast_ref::<TypedChannelHandler<P>>()
                    .map(|h| h.sender.clone())
            })
            .ok_or(ChannelError::NotFound { channel_id: id })
    }

    /// Gets the receiver for an existing channel.
    ///
    /// This can only be called once per channel, as it takes ownership of the receiver.
    ///
    /// # Errors
    ///
    /// Returns [`ChannelError::NotFound`] if the channel doesn't exist or the receiver
    /// has already been taken.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use bdrpc::channel::{ChannelManager, ChannelId, Protocol};
    /// # #[derive(Debug, Clone)]
    /// # enum MyProtocol { Ping }
    /// # impl Protocol for MyProtocol {
    /// #     fn method_name(&self) -> &'static str { "ping" }
    /// #     fn is_request(&self) -> bool { true }
    /// # }
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = ChannelManager::new();
    /// let id = ChannelId::new();
    /// manager.create_channel::<MyProtocol>(id, 100).await?;
    ///
    /// let receiver = manager.get_receiver::<MyProtocol>(id).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_receiver<P: Protocol>(
        &self,
        id: ChannelId,
    ) -> Result<ChannelReceiver<P>, ChannelError> {
        let mut channels = self.channels.write().await;

        channels
            .get_mut(&id)
            .and_then(|handler| {
                // Downcast to the specific protocol type
                handler
                    .as_any_mut()
                    .downcast_mut::<TypedChannelHandler<P>>()
                    .and_then(|h| h.receiver.take())
            })
            .ok_or(ChannelError::NotFound { channel_id: id })
    }

    /// Removes a channel from the manager.
    ///
    /// This closes the channel and removes it from the manager. Any pending
    /// messages will still be delivered.
    ///
    /// # Errors
    ///
    /// Returns [`ChannelError::NotFound`] if the channel doesn't exist.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use bdrpc::channel::{ChannelManager, ChannelId, Protocol};
    /// # #[derive(Debug, Clone)]
    /// # enum MyProtocol { Ping }
    /// # impl Protocol for MyProtocol {
    /// #     fn method_name(&self) -> &'static str { "ping" }
    /// #     fn is_request(&self) -> bool { true }
    /// # }
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = ChannelManager::new();
    /// let id = ChannelId::new();
    /// manager.create_channel::<MyProtocol>(id, 100).await?;
    ///
    /// manager.remove_channel(id).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn remove_channel(&self, id: ChannelId) -> Result<(), ChannelError> {
        let mut channels = self.channels.write().await;

        channels
            .remove(&id)
            .ok_or(ChannelError::NotFound { channel_id: id })?;

        Ok(())
    }

    /// Returns true if a channel with the given ID exists.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use bdrpc::channel::{ChannelManager, ChannelId, Protocol};
    /// # #[derive(Debug, Clone)]
    /// # enum MyProtocol { Ping }
    /// # impl Protocol for MyProtocol {
    /// #     fn method_name(&self) -> &'static str { "ping" }
    /// #     fn is_request(&self) -> bool { true }
    /// # }
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = ChannelManager::new();
    /// let id = ChannelId::new();
    ///
    /// assert!(!manager.has_channel(id).await);
    /// manager.create_channel::<MyProtocol>(id, 100).await?;
    /// assert!(manager.has_channel(id).await);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn has_channel(&self, id: ChannelId) -> bool {
        let channels = self.channels.read().await;
        channels.contains_key(&id)
    }

    /// Returns the number of active channels.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use bdrpc::channel::{ChannelManager, ChannelId, Protocol};
    /// # #[derive(Debug, Clone)]
    /// # enum MyProtocol { Ping }
    /// # impl Protocol for MyProtocol {
    /// #     fn method_name(&self) -> &'static str { "ping" }
    /// #     fn is_request(&self) -> bool { true }
    /// # }
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = ChannelManager::new();
    /// assert_eq!(manager.channel_count().await, 0);
    ///
    /// manager.create_channel::<MyProtocol>(ChannelId::new(), 100).await?;
    /// assert_eq!(manager.channel_count().await, 1);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn channel_count(&self) -> usize {
        let channels = self.channels.read().await;
        channels.len()
    }

    /// Returns a list of all active channel IDs.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use bdrpc::channel::{ChannelManager, ChannelId, Protocol};
    /// # #[derive(Debug, Clone)]
    /// # enum MyProtocol { Ping }
    /// # impl Protocol for MyProtocol {
    /// #     fn method_name(&self) -> &'static str { "ping" }
    /// #     fn is_request(&self) -> bool { true }
    /// # }
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = ChannelManager::new();
    /// let id1 = ChannelId::new();
    /// let id2 = ChannelId::new();
    ///
    /// manager.create_channel::<MyProtocol>(id1, 100).await?;
    /// manager.create_channel::<MyProtocol>(id2, 100).await?;
    ///
    /// let ids = manager.channel_ids().await;
    /// assert_eq!(ids.len(), 2);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn channel_ids(&self) -> Vec<ChannelId> {
        let channels = self.channels.read().await;
        channels.keys().copied().collect()
    }

    /// Removes all closed channels from the manager.
    ///
    /// This is useful for cleaning up channels whose receivers have been
    /// dropped. Returns the number of channels removed.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use bdrpc::channel::{ChannelManager, ChannelId};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = ChannelManager::new();
    /// // ... create and use channels ...
    /// let removed = manager.cleanup_closed_channels().await;
    /// println!("Removed {} closed channels", removed);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn cleanup_closed_channels(&self) -> usize {
        let mut channels = self.channels.write().await;
        let initial_count = channels.len();

        channels.retain(|_, handler| !handler.is_closed());

        initial_count - channels.len()
    }
}

impl Default for ChannelManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Simple test protocol
    #[derive(Debug, Clone, PartialEq, Eq)]
    enum TestProtocol {
        #[allow(dead_code)] // Test protocol variant
        Message(String),
    }

    impl Protocol for TestProtocol {
        fn method_name(&self) -> &'static str {
            "test"
        }

        fn is_request(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn test_manager_create_channel() {
        let manager = ChannelManager::new();
        let id = ChannelId::new();

        let sender = manager
            .create_channel::<TestProtocol>(id, 10)
            .await
            .unwrap();
        assert_eq!(sender.id(), id);
    }

    #[tokio::test]
    async fn test_manager_duplicate_channel() {
        let manager = ChannelManager::new();
        let id = ChannelId::new();

        manager
            .create_channel::<TestProtocol>(id, 10)
            .await
            .unwrap();

        let result = manager.create_channel::<TestProtocol>(id, 10).await;
        assert!(matches!(result, Err(ChannelError::AlreadyExists { .. })));
    }

    #[tokio::test]
    async fn test_manager_get_sender() {
        let manager = ChannelManager::new();
        let id = ChannelId::new();

        manager
            .create_channel::<TestProtocol>(id, 10)
            .await
            .unwrap();

        let sender = manager.get_sender::<TestProtocol>(id).await.unwrap();
        assert_eq!(sender.id(), id);
    }

    #[tokio::test]
    async fn test_manager_get_nonexistent_sender() {
        let manager = ChannelManager::new();
        let id = ChannelId::new();

        let result = manager.get_sender::<TestProtocol>(id).await;
        assert!(matches!(result, Err(ChannelError::NotFound { .. })));
    }

    #[tokio::test]
    async fn test_manager_remove_channel() {
        let manager = ChannelManager::new();
        let id = ChannelId::new();

        manager
            .create_channel::<TestProtocol>(id, 10)
            .await
            .unwrap();
        assert!(manager.has_channel(id).await);

        manager.remove_channel(id).await.unwrap();
        assert!(!manager.has_channel(id).await);
    }

    #[tokio::test]
    async fn test_manager_channel_count() {
        let manager = ChannelManager::new();
        assert_eq!(manager.channel_count().await, 0);

        let id1 = ChannelId::new();
        let id2 = ChannelId::new();

        manager
            .create_channel::<TestProtocol>(id1, 10)
            .await
            .unwrap();
        assert_eq!(manager.channel_count().await, 1);

        manager
            .create_channel::<TestProtocol>(id2, 10)
            .await
            .unwrap();
        assert_eq!(manager.channel_count().await, 2);

        manager.remove_channel(id1).await.unwrap();
        assert_eq!(manager.channel_count().await, 1);
    }

    #[tokio::test]
    async fn test_manager_channel_ids() {
        let manager = ChannelManager::new();
        let id1 = ChannelId::new();
        let id2 = ChannelId::new();

        manager
            .create_channel::<TestProtocol>(id1, 10)
            .await
            .unwrap();
        manager
            .create_channel::<TestProtocol>(id2, 10)
            .await
            .unwrap();

        let ids = manager.channel_ids().await;
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&id1));
        assert!(ids.contains(&id2));
    }

    #[tokio::test]
    async fn test_manager_cleanup_closed_channels() {
        let manager = ChannelManager::new();
        let id1 = ChannelId::new();
        let id2 = ChannelId::new();

        let (sender1, mut receiver1) = Channel::<TestProtocol>::new_in_memory(id1, 10);
        let (sender2, _receiver2) = Channel::<TestProtocol>::new_in_memory(id2, 10);

        // Manually insert channels
        {
            let mut channels = manager.channels.write().await;
            channels.insert(
                id1,
                Box::new(TypedChannelHandler {
                    id: id1,
                    sender: sender1,
                    receiver: None, // Already consumed
                }),
            );
            channels.insert(
                id2,
                Box::new(TypedChannelHandler {
                    id: id2,
                    sender: sender2,
                    receiver: None,
                }),
            );
        }

        assert_eq!(manager.channel_count().await, 2);

        // Close receiver1 to close channel1
        receiver1.close();
        drop(receiver1);

        // Cleanup should remove channel1
        let removed = manager.cleanup_closed_channels().await;
        assert_eq!(removed, 1);
        assert_eq!(manager.channel_count().await, 1);
    }
}

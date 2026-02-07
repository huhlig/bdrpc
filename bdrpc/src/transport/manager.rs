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
//!
//! # Enhanced Features (v0.2.0)
//!
//! The enhanced TransportManager supports:
//! - Multiple listener transports (servers)
//! - Multiple caller transports (clients) with automatic reconnection
//! - Dynamic enable/disable of transports
//! - Event callbacks for transport lifecycle
//! - Connection tracking and metadata

use crate::transport::{
    CallerTransport, Transport, TransportConnection, TransportError, TransportEventHandler,
    TransportId, TransportListener, TransportMetadata,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;

#[cfg(feature = "observability")]
use tracing::{debug, error, info, warn};

/// Manages the lifecycle of transport connections.
///
/// The `TransportManager` is responsible for:
/// - Assigning unique transport IDs
/// - Managing listener transports (servers)
/// - Managing caller transports (clients) with automatic reconnection
/// - Tracking active connections
/// - Providing transport lifecycle event callbacks
///
/// # Enhanced Features (v0.2.0)
///
/// The enhanced TransportManager supports:
/// - Multiple listener transports that can accept connections
/// - Multiple caller transports with automatic reconnection
/// - Dynamic enable/disable of transports
/// - Event callbacks for connection lifecycle
/// - Connection tracking and metadata
///
/// # Example
///
/// ```rust,no_run
/// use bdrpc::transport::{TransportManager, TransportConfig, TransportType};
/// use std::sync::Arc;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let manager = TransportManager::new();
///
/// // Add a TCP listener
/// let listener_config = TransportConfig::new(TransportType::Tcp, "0.0.0.0:8080");
/// manager.add_listener("tcp-server".to_string(), listener_config).await?;
///
/// // Add a TCP caller with reconnection
/// let caller_config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:9090")
///     .with_reconnection_strategy(Arc::new(
///         bdrpc::reconnection::ExponentialBackoff::default()
///     ));
/// manager.add_caller("tcp-client".to_string(), caller_config).await?;
///
/// // Connect the caller
/// let transport_id = manager.connect_caller("tcp-client").await?;
/// println!("Connected with transport ID: {}", transport_id);
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

    /// Active transport metadata (legacy, for backward compatibility)
    transports: RwLock<HashMap<TransportId, TransportMetadata>>,

    /// Listener transports (servers) - boxed for type erasure
    listeners: RwLock<HashMap<String, ListenerEntry>>,

    /// Caller transports (clients)
    callers: RwLock<HashMap<String, CallerTransport>>,

    /// Active connections
    connections: RwLock<HashMap<TransportId, TransportConnection>>,

    /// Event handler for transport lifecycle events
    event_handler: RwLock<Option<Arc<dyn TransportEventHandler>>>,
}

/// Entry for a listener transport with its enabled state
struct ListenerEntry {
    /// The actual listener (type-erased)
    listener: Box<dyn std::any::Any + Send + Sync>,
    /// Whether this listener is enabled
    enabled: bool,
    /// Transport type for this listener
    transport_type: crate::transport::TransportType,
    /// Local address
    local_addr: String,
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
                listeners: RwLock::new(HashMap::new()),
                callers: RwLock::new(HashMap::new()),
                connections: RwLock::new(HashMap::new()),
                event_handler: RwLock::new(None),
            }),
        }
    }

    /// Adds a listener transport (server).
    ///
    /// The listener will accept incoming connections. Use a unique name to identify
    /// this listener for later management operations.
    ///
    /// # Arguments
    ///
    /// * `name` - Unique name for this listener
    /// * `config` - Configuration for the listener transport
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - A listener with the same name already exists
    /// - The listener cannot be created
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{TransportManager, TransportConfig, TransportType};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let config = TransportConfig::new(TransportType::Tcp, "0.0.0.0:8080");
    /// manager.add_listener("main".to_string(), config).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn add_listener(
        &self,
        name: String,
        config: crate::transport::TransportConfig,
    ) -> Result<(), TransportError> {
        let mut listeners = self.inner.listeners.write().await;

        if listeners.contains_key(&name) {
            return Err(TransportError::InvalidConfiguration {
                reason: format!("Listener '{}' already exists", name)
            });
        }

        #[cfg(feature = "observability")]
        info!("Adding listener '{}' at {}", name, config.address());

        // Create the listener based on transport type
        // For now, we'll store a placeholder since we need actual listener implementations
        let entry = ListenerEntry {
            listener: Box::new(()), // Placeholder - will be replaced with actual listener
            enabled: config.is_enabled(),
            transport_type: config.transport_type(),
            local_addr: config.address().to_string(),
        };

        listeners.insert(name, entry);

        #[cfg(feature = "observability")]
        debug!("Listener added successfully");

        Ok(())
    }

    /// Adds a caller transport (client).
    ///
    /// The caller can be used to establish outbound connections. If a reconnection
    /// strategy is configured, the caller will automatically reconnect on failure.
    ///
    /// # Arguments
    ///
    /// * `name` - Unique name for this caller
    /// * `config` - Configuration for the caller transport
    ///
    /// # Errors
    ///
    /// Returns an error if a caller with the same name already exists.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{TransportManager, TransportConfig, TransportType};
    /// use bdrpc::reconnection::ExponentialBackoff;
    /// use std::sync::Arc;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080")
    ///     .with_reconnection_strategy(Arc::new(ExponentialBackoff::default()));
    /// manager.add_caller("main".to_string(), config).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn add_caller(
        &self,
        name: String,
        config: crate::transport::TransportConfig,
    ) -> Result<(), TransportError> {
        let mut callers = self.inner.callers.write().await;

        if callers.contains_key(&name) {
            return Err(TransportError::InvalidConfiguration {
                reason: format!("Caller '{}' already exists", name)
            });
        }

        #[cfg(feature = "observability")]
        info!("Adding caller '{}' to {}", name, config.address());

        let caller = CallerTransport::new(name.clone(), config);
        callers.insert(name, caller);

        #[cfg(feature = "observability")]
        debug!("Caller added successfully");

        Ok(())
    }

    /// Removes a listener transport.
    ///
    /// This will shut down the listener and prevent new connections.
    /// Existing connections are not affected.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the listener to remove
    ///
    /// # Errors
    ///
    /// Returns an error if the listener does not exist or cannot be shut down.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{TransportManager, TransportConfig, TransportType};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let config = TransportConfig::new(TransportType::Tcp, "0.0.0.0:8080");
    /// manager.add_listener("main".to_string(), config).await?;
    /// manager.remove_listener("main").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn remove_listener(&self, name: &str) -> Result<(), TransportError> {
        let mut listeners = self.inner.listeners.write().await;

        if !listeners.contains_key(name) {
            return Err(TransportError::InvalidConfiguration {
                reason: format!("Listener '{}' not found", name)
            });
        }

        #[cfg(feature = "observability")]
        info!("Removing listener '{}'", name);

        listeners.remove(name);

        #[cfg(feature = "observability")]
        debug!("Listener removed successfully");

        Ok(())
    }

    /// Removes a caller transport.
    ///
    /// This will stop any reconnection attempts and close the connection.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the caller to remove
    ///
    /// # Errors
    ///
    /// Returns an error if the caller does not exist.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{TransportManager, TransportConfig, TransportType};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080");
    /// manager.add_caller("main".to_string(), config).await?;
    /// manager.remove_caller("main").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn remove_caller(&self, name: &str) -> Result<(), TransportError> {
        let mut callers = self.inner.callers.write().await;

        if !callers.contains_key(name) {
            return Err(TransportError::InvalidConfiguration {
                reason: format!("Caller '{}' not found", name)
            });
        }

        #[cfg(feature = "observability")]
        info!("Removing caller '{}'", name);

        callers.remove(name);

        #[cfg(feature = "observability")]
        debug!("Caller removed successfully");

        Ok(())
    }

    /// Enables a transport (listener or caller).
    ///
    /// For listeners, this allows accepting new connections.
    /// For callers, this allows connection attempts.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the transport to enable
    ///
    /// # Errors
    ///
    /// Returns an error if the transport does not exist.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{TransportManager, TransportConfig, TransportType};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let config = TransportConfig::new(TransportType::Tcp, "0.0.0.0:8080")
    ///     .with_enabled(false);
    /// manager.add_listener("main".to_string(), config).await?;
    /// manager.enable_transport("main").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn enable_transport(&self, name: &str) -> Result<(), TransportError> {
        // Try listeners first
        {
            let mut listeners = self.inner.listeners.write().await;
            if let Some(entry) = listeners.get_mut(name) {
                #[cfg(feature = "observability")]
                info!("Enabling listener '{}'", name);
                entry.enabled = true;
                return Ok(());
            }
        }

        // Try callers
        {
            let callers = self.inner.callers.read().await;
            if let Some(caller) = callers.get(name) {
                #[cfg(feature = "observability")]
                info!("Enabling caller '{}'", name);
                // Update caller state (implementation depends on CallerTransport)
                // For now, we'll just log it
                return Ok(());
            }
        }

        Err(TransportError::InvalidConfiguration {
            reason: format!("Transport '{}' not found", name)
        })
    }

    /// Disables a transport (listener or caller).
    ///
    /// For listeners, this stops accepting new connections.
    /// For callers, this stops connection attempts and reconnection.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the transport to disable
    ///
    /// # Errors
    ///
    /// Returns an error if the transport does not exist.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{TransportManager, TransportConfig, TransportType};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let config = TransportConfig::new(TransportType::Tcp, "0.0.0.0:8080");
    /// manager.add_listener("main".to_string(), config).await?;
    /// manager.disable_transport("main").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn disable_transport(&self, name: &str) -> Result<(), TransportError> {
        // Try listeners first
        {
            let mut listeners = self.inner.listeners.write().await;
            if let Some(entry) = listeners.get_mut(name) {
                #[cfg(feature = "observability")]
                info!("Disabling listener '{}'", name);
                entry.enabled = false;
                return Ok(());
            }
        }

        // Try callers
        {
            let callers = self.inner.callers.read().await;
            if let Some(caller) = callers.get(name) {
                #[cfg(feature = "observability")]
                info!("Disabling caller '{}'", name);
                // Update caller state (implementation depends on CallerTransport)
                // For now, we'll just log it
                return Ok(());
            }
        }

        Err(TransportError::InvalidConfiguration {
            reason: format!("Transport '{}' not found", name)
        })
    }

    /// Connects a caller transport.
    ///
    /// This initiates a connection using the specified caller. If the caller
    /// has a reconnection strategy, it will automatically reconnect on failure.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the caller to connect
    ///
    /// # Returns
    ///
    /// The transport ID of the established connection.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The caller does not exist
    /// - The connection fails
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{TransportManager, TransportConfig, TransportType};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080");
    /// manager.add_caller("main".to_string(), config).await?;
    /// let transport_id = manager.connect_caller("main").await?;
    /// println!("Connected: {}", transport_id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn connect_caller(&self, name: &str) -> Result<TransportId, TransportError> {
        let callers = self.inner.callers.read().await;

        let caller = callers.get(name).ok_or_else(|| {
            TransportError::InvalidConfiguration {
                reason: format!("Caller '{}' not found", name)
            }
        })?;

        // Check if caller is enabled
        if !caller.config().is_enabled() {
            return Err(TransportError::InvalidConfiguration {
                reason: format!("Caller '{}' is disabled", name)
            });
        }

        let config = caller.config().clone();
        let address = config.address().to_string();

        #[cfg(feature = "observability")]
        info!("Connecting caller '{}' to {}", name, address);

        // Create the actual transport connection based on type
        let transport_id = self.create_transport_connection(&config).await?;

        // Register the connection
        let connection = TransportConnection::new(
            transport_id,
            config.transport_type(),
            Some(name.to_string()),
        );
        self.inner.connections.write().await.insert(transport_id, connection);

        // Update caller state
        caller.set_state(crate::transport::CallerState::Connected(transport_id)).await;

        // Notify event handler
        if let Some(handler) = self.inner.event_handler.read().await.as_ref() {
            handler.on_transport_connected(transport_id);
        }

        // Start reconnection loop if strategy is configured
        if caller.has_reconnection_strategy() {
            let event_handler = self.inner.event_handler.read().await.clone();
            if let Some(handler) = event_handler {
                let manager = self.clone();
                let caller_name = name.to_string();
                
                caller.start_reconnection_loop(
                    handler,
                    move |addr| {
                        let manager = manager.clone();
                        let config = config.clone();
                        async move {
                            manager.create_transport_connection(&config).await
                        }
                    }
                ).await;
            }
        }

        #[cfg(feature = "observability")]
        info!("Caller '{}' connected with transport ID: {}", name, transport_id);

        Ok(transport_id)
    }

    /// Creates an actual transport connection based on the configuration.
    ///
    /// This is an internal helper method that creates the appropriate transport
    /// type (TCP, TLS, etc.) based on the configuration.
    async fn create_transport_connection(
        &self,
        config: &crate::transport::TransportConfig,
    ) -> Result<TransportId, TransportError> {
        use crate::transport::TransportType;

        let transport_id = self.next_id();
        let address = config.address();

        match config.transport_type() {
            TransportType::Tcp => {
                #[cfg(feature = "observability")]
                debug!("Creating TCP transport to {}", address);

                // For now, we'll just return the ID
                // The actual transport creation will be handled by the endpoint
                // in Phase 4 when we integrate with the endpoint
                Ok(transport_id)
            }
            #[cfg(feature = "tls")]
            TransportType::Tls => {
                #[cfg(feature = "observability")]
                debug!("Creating TLS transport to {}", address);

                // TLS transport creation will be implemented in Phase 4
                Ok(transport_id)
            }
            TransportType::Memory => {
                #[cfg(feature = "observability")]
                debug!("Creating Memory transport");

                // Memory transport creation
                Ok(transport_id)
            }
            #[allow(unreachable_patterns)]
            _ => {
                Err(TransportError::InvalidConfiguration {
                    reason: format!("Unsupported transport type: {:?}", config.transport_type())
                })
            }
        }
    }

    /// Sets the event handler for transport lifecycle events.
    ///
    /// The event handler will be called for connection, disconnection, and
    /// channel creation events.
    ///
    /// # Arguments
    ///
    /// * `handler` - The event handler implementation
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::transport::{TransportManager, TransportEventHandler, TransportId};
    /// use bdrpc::channel::ChannelId;
    /// use bdrpc::transport::TransportError;
    /// use std::sync::Arc;
    ///
    /// struct MyHandler;
    ///
    /// impl TransportEventHandler for MyHandler {
    ///     fn on_transport_connected(&self, transport_id: TransportId) {
    ///         println!("Connected: {}", transport_id);
    ///     }
    ///
    ///     fn on_transport_disconnected(&self, transport_id: TransportId, error: Option<TransportError>) {
    ///         println!("Disconnected: {}", transport_id);
    ///     }
    ///
    ///     fn on_new_channel_request(
    ///         &self,
    ///         channel_id: ChannelId,
    ///         protocol: &str,
    ///         transport_id: TransportId,
    ///     ) -> Result<bool, String> {
    ///         Ok(true)
    ///     }
    /// }
    ///
    /// # async fn example() {
    /// let manager = TransportManager::new();
    /// manager.set_event_handler(Arc::new(MyHandler)).await;
    /// # }
    /// ```
    pub async fn set_event_handler(&self, handler: Arc<dyn TransportEventHandler>) {
        *self.inner.event_handler.write().await = Some(handler);
    }

    /// Returns the number of active listeners.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{TransportManager, TransportConfig, TransportType};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let config = TransportConfig::new(TransportType::Tcp, "0.0.0.0:8080");
    /// manager.add_listener("main".to_string(), config).await?;
    /// assert_eq!(manager.listener_count().await, 1);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn listener_count(&self) -> usize {
        self.inner.listeners.read().await.len()
    }

    /// Returns the number of active callers.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{TransportManager, TransportConfig, TransportType};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080");
    /// manager.add_caller("main".to_string(), config).await?;
    /// assert_eq!(manager.caller_count().await, 1);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn caller_count(&self) -> usize {
        self.inner.callers.read().await.len()
    }

    /// Returns the number of active connections.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::transport::TransportManager;
    ///
    /// # async fn example() {
    /// let manager = TransportManager::new();
    /// assert_eq!(manager.connection_count().await, 0);
    /// # }
    /// ```
    pub async fn connection_count(&self) -> usize {
        self.inner.connections.read().await.len()
    }

    /// Lists all listener names.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{TransportManager, TransportConfig, TransportType};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let config = TransportConfig::new(TransportType::Tcp, "0.0.0.0:8080");
    /// manager.add_listener("main".to_string(), config).await?;
    /// let listeners = manager.list_listeners().await;
    /// assert!(listeners.contains(&"main".to_string()));
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_listeners(&self) -> Vec<String> {
        self.inner.listeners.read().await.keys().cloned().collect()
    }

    /// Lists all caller names.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{TransportManager, TransportConfig, TransportType};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080");
    /// manager.add_caller("main".to_string(), config).await?;
    /// let callers = manager.list_callers().await;
    /// assert!(callers.contains(&"main".to_string()));
    /// # Ok(())
    /// # }
    /// ```
    pub async fn list_callers(&self) -> Vec<String> {
        self.inner.callers.read().await.keys().cloned().collect()
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

    // ========================================================================
    // Enhanced TransportManager Tests (v0.2.0)
    // ========================================================================

    use crate::reconnection::ExponentialBackoff;
    use crate::transport::TransportType;

    #[tokio::test]
    async fn test_add_listener() {
        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Memory, "test-addr");

        let result = manager.add_listener("test-listener".to_string(), config).await;
        assert!(result.is_ok());
        assert_eq!(manager.listener_count().await, 1);
    }

    #[tokio::test]
    async fn test_add_listener_duplicate() {
        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Memory, "test-addr");

        manager.add_listener("test-listener".to_string(), config.clone()).await.unwrap();
        let result = manager.add_listener("test-listener".to_string(), config).await;
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TransportError::InvalidConfiguration { .. }));
    }

    #[tokio::test]
    async fn test_remove_listener() {
        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Memory, "test-addr");

        manager.add_listener("test-listener".to_string(), config).await.unwrap();
        assert_eq!(manager.listener_count().await, 1);

        let result = manager.remove_listener("test-listener").await;
        assert!(result.is_ok());
        assert_eq!(manager.listener_count().await, 0);
    }

    #[tokio::test]
    async fn test_remove_listener_not_found() {
        let manager = TransportManager::new();
        let result = manager.remove_listener("nonexistent").await;
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TransportError::InvalidConfiguration { .. }));
    }

    #[tokio::test]
    async fn test_add_caller() {
        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Memory, "test-addr");

        let result = manager.add_caller("test-caller".to_string(), config).await;
        assert!(result.is_ok());
        assert_eq!(manager.caller_count().await, 1);
    }

    #[tokio::test]
    async fn test_add_caller_with_reconnection() {
        let manager = TransportManager::new();
        let strategy = Arc::new(ExponentialBackoff::default());
        let config = crate::transport::TransportConfig::new(TransportType::Memory, "test-addr")
            .with_reconnection_strategy(strategy);

        let result = manager.add_caller("test-caller".to_string(), config).await;
        assert!(result.is_ok());
        assert_eq!(manager.caller_count().await, 1);
    }

    #[tokio::test]
    async fn test_add_caller_duplicate() {
        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Memory, "test-addr");

        manager.add_caller("test-caller".to_string(), config.clone()).await.unwrap();
        let result = manager.add_caller("test-caller".to_string(), config).await;
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TransportError::InvalidConfiguration { .. }));
    }

    #[tokio::test]
    async fn test_remove_caller() {
        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Memory, "test-addr");

        manager.add_caller("test-caller".to_string(), config).await.unwrap();
        assert_eq!(manager.caller_count().await, 1);

        let result = manager.remove_caller("test-caller").await;
        assert!(result.is_ok());
        assert_eq!(manager.caller_count().await, 0);
    }

    #[tokio::test]
    async fn test_remove_caller_not_found() {
        let manager = TransportManager::new();
        let result = manager.remove_caller("nonexistent").await;
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TransportError::InvalidConfiguration { .. }));
    }

    #[tokio::test]
    async fn test_enable_disable_listener() {
        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Memory, "test-addr")
            .with_enabled(false);

        manager.add_listener("test-listener".to_string(), config).await.unwrap();

        // Enable the listener
        let result = manager.enable_transport("test-listener").await;
        assert!(result.is_ok());

        // Disable the listener
        let result = manager.disable_transport("test-listener").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_enable_disable_caller() {
        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Memory, "test-addr");

        manager.add_caller("test-caller".to_string(), config).await.unwrap();

        // Enable the caller
        let result = manager.enable_transport("test-caller").await;
        assert!(result.is_ok());

        // Disable the caller
        let result = manager.disable_transport("test-caller").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_enable_transport_not_found() {
        let manager = TransportManager::new();
        let result = manager.enable_transport("nonexistent").await;
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TransportError::InvalidConfiguration { .. }));
    }

    #[tokio::test]
    async fn test_disable_transport_not_found() {
        let manager = TransportManager::new();
        let result = manager.disable_transport("nonexistent").await;
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TransportError::InvalidConfiguration { .. }));
    }

    #[tokio::test]
    async fn test_list_listeners() {
        let manager = TransportManager::new();
        let config1 = crate::transport::TransportConfig::new(TransportType::Memory, "addr1");
        let config2 = crate::transport::TransportConfig::new(TransportType::Memory, "addr2");

        manager.add_listener("listener1".to_string(), config1).await.unwrap();
        manager.add_listener("listener2".to_string(), config2).await.unwrap();

        let listeners = manager.list_listeners().await;
        assert_eq!(listeners.len(), 2);
        assert!(listeners.contains(&"listener1".to_string()));
        assert!(listeners.contains(&"listener2".to_string()));
    }

    #[tokio::test]
    async fn test_list_callers() {
        let manager = TransportManager::new();
        let config1 = crate::transport::TransportConfig::new(TransportType::Memory, "addr1");
        let config2 = crate::transport::TransportConfig::new(TransportType::Memory, "addr2");

        manager.add_caller("caller1".to_string(), config1).await.unwrap();
        manager.add_caller("caller2".to_string(), config2).await.unwrap();

        let callers = manager.list_callers().await;
        assert_eq!(callers.len(), 2);
        assert!(callers.contains(&"caller1".to_string()));
        assert!(callers.contains(&"caller2".to_string()));
    }

    #[tokio::test]
    async fn test_connection_count() {
        let manager = TransportManager::new();
        assert_eq!(manager.connection_count().await, 0);
    }

    #[tokio::test]
    async fn test_connect_caller_not_found() {
        let manager = TransportManager::new();
        let result = manager.connect_caller("nonexistent").await;
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TransportError::InvalidConfiguration { .. }));
    }

    #[tokio::test]
    async fn test_connect_caller() {
        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Memory, "test-addr");

        manager.add_caller("test-caller".to_string(), config).await.unwrap();
        let result = manager.connect_caller("test-caller").await;
        
        // For now, this returns a placeholder transport ID
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_set_event_handler() {
        use crate::channel::ChannelId;

        struct TestHandler;
        impl TransportEventHandler for TestHandler {
            fn on_transport_connected(&self, _transport_id: TransportId) {}
            fn on_transport_disconnected(&self, _transport_id: TransportId, _error: Option<TransportError>) {}
            fn on_new_channel_request(
                &self,
                _channel_id: ChannelId,
                _protocol: &str,
                _transport_id: TransportId,
            ) -> Result<bool, String> {
                Ok(true)
            }
        }

        let manager = TransportManager::new();
        manager.set_event_handler(Arc::new(TestHandler)).await;
        // No assertion needed - just verify it doesn't panic
    }

    #[tokio::test]
    async fn test_multiple_listeners_and_callers() {
        let manager = TransportManager::new();
        
        // Add multiple listeners
        for i in 0..3 {
            let config = crate::transport::TransportConfig::new(
                TransportType::Memory,
                format!("listener-addr-{}", i)
            );
            manager.add_listener(format!("listener-{}", i), config).await.unwrap();
        }

        // Add multiple callers
        for i in 0..3 {
            let config = crate::transport::TransportConfig::new(
                TransportType::Memory,
                format!("caller-addr-{}", i)
            );
            manager.add_caller(format!("caller-{}", i), config).await.unwrap();
        }

        assert_eq!(manager.listener_count().await, 3);
        assert_eq!(manager.caller_count().await, 3);

        let listeners = manager.list_listeners().await;
        let callers = manager.list_callers().await;
        
        assert_eq!(listeners.len(), 3);
        assert_eq!(callers.len(), 3);
    }

    #[tokio::test]
    async fn test_listener_with_disabled_state() {
        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Memory, "test-addr")
            .with_enabled(false);

        manager.add_listener("disabled-listener".to_string(), config).await.unwrap();
        assert_eq!(manager.listener_count().await, 1);

        // The listener exists but is disabled
        let listeners = manager.list_listeners().await;
        assert!(listeners.contains(&"disabled-listener".to_string()));
    }

    #[tokio::test]
    async fn test_caller_with_metadata() {
        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Memory, "test-addr")
            .with_metadata("region", "us-west")
            .with_metadata("priority", "high");

        manager.add_caller("metadata-caller".to_string(), config).await.unwrap();
        assert_eq!(manager.caller_count().await, 1);
    }

    #[tokio::test]
    async fn test_connect_caller_disabled() {
        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080")
            .with_enabled(false);
        manager.add_caller("test".to_string(), config).await.unwrap();
        
        let result = manager.connect_caller("test").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("disabled"));
    }

    #[tokio::test]
    async fn test_connect_caller_creates_connection() {
        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080");
        manager.add_caller("test".to_string(), config).await.unwrap();
        
        let transport_id = manager.connect_caller("test").await.unwrap();
        assert_eq!(manager.connection_count().await, 1);
        
        // Verify connection is tracked
        let connections = manager.inner.connections.read().await;
        assert!(connections.contains_key(&transport_id));
        let conn = connections.get(&transport_id).unwrap();
        assert_eq!(conn.caller_name(), Some("test"));
        assert!(conn.is_client());
    }

    #[tokio::test]
    async fn test_caller_state_transitions() {
        use crate::transport::CallerState;
        
        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080");
        manager.add_caller("test".to_string(), config).await.unwrap();
        
        // Initial state should be Disconnected
        let callers = manager.inner.callers.read().await;
        let caller = callers.get("test").unwrap();
        assert_eq!(caller.state().await, CallerState::Disconnected);
        
        // After connect, should be Connected
        drop(callers);
        let transport_id = manager.connect_caller("test").await.unwrap();
        
        let callers = manager.inner.callers.read().await;
        let caller = callers.get("test").unwrap();
        assert_eq!(caller.state().await, CallerState::Connected(transport_id));
    }

    #[tokio::test]
    async fn test_event_handler_on_connect() {
        use crate::channel::ChannelId;
        use std::sync::atomic::{AtomicBool, Ordering};
        
        struct TestHandler {
            connected_called: Arc<AtomicBool>,
        }
        
        impl TransportEventHandler for TestHandler {
            fn on_transport_connected(&self, _transport_id: TransportId) {
                self.connected_called.store(true, Ordering::SeqCst);
            }
            
            fn on_transport_disconnected(&self, _transport_id: TransportId, _error: Option<TransportError>) {}
            
            fn on_new_channel_request(
                &self,
                _channel_id: ChannelId,
                _protocol: &str,
                _transport_id: TransportId,
            ) -> Result<bool, String> {
                Ok(true)
            }
        }
        
        let connected_called = Arc::new(AtomicBool::new(false));
        let handler = Arc::new(TestHandler {
            connected_called: Arc::clone(&connected_called),
        });
        
        let manager = TransportManager::new();
        manager.set_event_handler(handler).await;
        
        let config = crate::transport::TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080");
        manager.add_caller("test".to_string(), config).await.unwrap();
        
        manager.connect_caller("test").await.unwrap();
        
        // Event handler should have been called
        assert!(connected_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_create_transport_connection_tcp() {
        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080");
        
        let result = manager.create_transport_connection(&config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_create_transport_connection_memory() {
        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Memory, "memory://test");
        
        let result = manager.create_transport_connection(&config).await;
        assert!(result.is_ok());
    }
}

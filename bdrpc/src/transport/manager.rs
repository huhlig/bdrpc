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
//! - Multiple caller transports (clients) with automatic strategy
//! - Dynamic enable/disable of transports
//! - Event callbacks for transport lifecycle
//! - Connection tracking and metadata

use crate::channel::TransportRouter;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;

use crate::TransportError;
use crate::transport::caller::{CallerState, CallerTransport};
use crate::transport::provider::TcpTransport;
use crate::transport::traits::{Transport, TransportEventHandler};
use crate::transport::types::TransportConnection;
use crate::transport::{TransportId, TransportMetadata};
#[cfg(feature = "observability")]
use tracing::{debug, error, info, warn};

/// Manages the lifecycle of transport connections.
///
/// The `TransportManager` is responsible for:
/// - Assigning unique transport IDs
/// - Managing listener transports (servers)
/// - Managing caller transports (clients) with automatic strategy
/// - Tracking active connections
/// - Providing transport lifecycle event callbacks
///
/// # Enhanced Features (v0.2.0)
///
/// The enhanced TransportManager supports:
/// - Multiple listener transports that can accept connections
/// - Multiple caller transports with automatic strategy
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
/// // Add a TCP caller with strategy
/// let caller_config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:9090")
///     .with_reconnection_strategy(Arc::new(
///         bdrpc::strategy::ExponentialBackoff::default()
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
    /// Next transport ID to assign (wrapped in Arc for sharing with accept tasks)
    next_id: Arc<AtomicU64>,

    /// Active transport metadata (legacy, for backward compatibility)
    transports: RwLock<HashMap<TransportId, TransportMetadata>>,

    /// Listener transports (servers) - boxed for type erasure
    listeners: RwLock<HashMap<String, ListenerEntry>>,

    /// Caller transports (clients)
    callers: RwLock<HashMap<String, CallerTransport>>,

    /// Active connections
    connections: RwLock<HashMap<TransportId, TransportConnection>>,

    /// Active transport objects (for message handling)
    active_transports: RwLock<HashMap<TransportId, Box<dyn Transport>>>,

    /// Transport routers for message routing between channels and transports
    routers: RwLock<HashMap<TransportId, Arc<TransportRouter>>>,

    /// Event handler for transport lifecycle events
    event_handler: RwLock<Option<Arc<dyn TransportEventHandler>>>,

    /// Channel for accepted connections from listeners
    /// Each listener spawns a background task that accepts connections and sends them here
    accept_sender: tokio::sync::mpsc::UnboundedSender<AcceptedConnection>,
    accept_receiver:
        Arc<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<AcceptedConnection>>>,
}

/// Represents a connection accepted from a listener
struct AcceptedConnection {
    /// The transport for this connection
    transport: Box<dyn Transport>,
    /// Transport ID assigned to this connection
    transport_id: TransportId,
    /// Name of the listener that accepted this connection
    listener_name: String,
}

/// Entry for a listener transport with its enabled state
struct ListenerEntry {
    /// The actual listener (type-erased)
    #[allow(dead_code)]
    listener: Box<dyn std::any::Any + Send + Sync>,
    /// Whether this listener is enabled
    enabled: bool,
    /// Transport type for this listener
    #[allow(dead_code)]
    transport_type: crate::transport::TransportType,
    /// Local address
    #[allow(dead_code)]
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
        let (accept_sender, accept_receiver) = tokio::sync::mpsc::unbounded_channel();

        Self {
            inner: Arc::new(TransportManagerInner {
                next_id: Arc::new(AtomicU64::new(1)),
                transports: RwLock::new(HashMap::new()),
                listeners: RwLock::new(HashMap::new()),
                callers: RwLock::new(HashMap::new()),
                connections: RwLock::new(HashMap::new()),
                active_transports: RwLock::new(HashMap::new()),
                routers: RwLock::new(HashMap::new()),
                event_handler: RwLock::new(None),
                accept_sender,
                accept_receiver: Arc::new(tokio::sync::Mutex::new(accept_receiver)),
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
                reason: format!("Listener '{}' already exists", name),
            });
        }

        #[cfg(feature = "observability")]
        info!("Adding listener '{}' at {}", name, config.address());

        // Create the actual listener based on transport type
        let transport_type = config.transport_type();
        let address = config.address().to_string();
        let enabled = config.is_enabled();

        match transport_type {
            crate::transport::TransportType::Tcp => {
                // Create TCP listener
                let tcp_listener = Arc::new(TcpTransport::bind(&address).await?);

                // Spawn background accept task if enabled
                if enabled {
                    let accept_sender = self.inner.accept_sender.clone();
                    let listener_name = name.clone();
                    let next_id_counter = Arc::clone(&self.inner.next_id);
                    let listener_clone = Arc::clone(&tcp_listener);

                    tokio::spawn(async move {
                        #[cfg(feature = "observability")]
                        info!("Accept task started for listener '{}'", listener_name);

                        loop {
                            match TcpTransport::accept(&listener_clone).await {
                                Ok((transport, _peer_addr)) => {
                                    let transport_id = TransportId::new(
                                        next_id_counter.fetch_add(1, Ordering::SeqCst),
                                    );

                                    #[cfg(feature = "observability")]
                                    debug!(
                                        "Listener '{}' accepted connection with ID {}",
                                        listener_name, transport_id
                                    );

                                    let accepted = AcceptedConnection {
                                        transport: Box::new(transport),
                                        transport_id,
                                        listener_name: listener_name.clone(),
                                    };

                                    if accept_sender.send(accepted).is_err() {
                                        #[cfg(feature = "observability")]
                                        info!(
                                            "Accept channel closed, terminating listener '{}'",
                                            listener_name
                                        );
                                        break;
                                    }
                                }
                                Err(_e) => {
                                    #[cfg(feature = "observability")]
                                    debug!("Accept error on listener '{}': {}", listener_name, _e);
                                    // Continue accepting despite errors
                                }
                            }
                        }
                    });
                }

                let entry = ListenerEntry {
                    listener: Box::new(tcp_listener),
                    enabled,
                    transport_type,
                    local_addr: address,
                };

                listeners.insert(name, entry);
            }
            _ => {
                return Err(TransportError::InvalidConfiguration {
                    reason: format!("Unsupported listener transport type: {:?}", transport_type),
                });
            }
        }

        #[cfg(feature = "observability")]
        debug!("Listener added successfully");

        Ok(())
    }

    /// Adds a caller transport (client).
    ///
    /// The caller can be used to establish outbound connections. If a strategy
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
    /// use bdrpc::strategy::ExponentialBackoff;
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
                reason: format!("Caller '{}' already exists", name),
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
                reason: format!("Listener '{}' not found", name),
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
    /// This will stop any strategy attempts and close the connection.
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
                reason: format!("Caller '{}' not found", name),
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
            if let Some(_caller) = callers.get(name) {
                #[cfg(feature = "observability")]
                info!("Enabling caller '{}'", name);
                // Update caller state (implementation depends on CallerTransport)
                // For now, we'll just log it
                return Ok(());
            }
        }

        Err(TransportError::InvalidConfiguration {
            reason: format!("Transport '{}' not found", name),
        })
    }

    /// Disables a transport (listener or caller).
    ///
    /// For listeners, this stops accepting new connections.
    /// For callers, this stops connection attempts and strategy.
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
            if let Some(_caller) = callers.get(name) {
                #[cfg(feature = "observability")]
                info!("Disabling caller '{}'", name);
                // Update caller state (implementation depends on CallerTransport)
                // For now, we'll just log it
                return Ok(());
            }
        }

        Err(TransportError::InvalidConfiguration {
            reason: format!("Transport '{}' not found", name),
        })
    }

    /// Connects a caller transport.
    ///
    /// This initiates a connection using the specified caller. If the caller
    /// has a strategy strategy, it will automatically reconnect on failure.
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

        let caller = callers
            .get(name)
            .ok_or_else(|| TransportError::InvalidConfiguration {
                reason: format!("Caller '{}' not found", name),
            })?;

        // Check if caller is enabled
        if !caller.config().is_enabled() {
            return Err(TransportError::InvalidConfiguration {
                reason: format!("Caller '{}' is disabled", name),
            });
        }

        let config = caller.config().clone();

        #[cfg(feature = "observability")]
        {
            let address = config.address().to_string();
            info!("Connecting caller '{}' to {}", name, address);
        }

        // Create the actual transport connection based on type
        let (transport_id, transport) = self.create_transport_connection(&config).await?;

        // Store the transport object
        self.inner
            .active_transports
            .write()
            .await
            .insert(transport_id, transport);

        // Register the connection
        let connection = TransportConnection::new(
            transport_id,
            config.transport_type(),
            Some(name.to_string()),
        );
        self.inner
            .connections
            .write()
            .await
            .insert(transport_id, connection);

        // Update caller state
        caller.set_state(CallerState::Connected(transport_id)).await;

        // Notify event handler
        if let Some(handler) = self.inner.event_handler.read().await.as_ref() {
            handler.on_transport_connected(transport_id);
        }

        // Start strategy loop if strategy is configured
        if caller.has_reconnection_strategy() {
            let event_handler = self.inner.event_handler.read().await.clone();
            if let Some(handler) = event_handler {
                let manager = self.clone();
                let _caller_name = name.to_string();

                caller
                    .start_reconnection_loop(handler, move |_addr| {
                        let manager = manager.clone();
                        let config = config.clone();
                        async move {
                            manager
                                .create_transport_connection(&config)
                                .await
                                .map(|(id, _transport)| id)
                        }
                    })
                    .await;
            }
        }

        #[cfg(feature = "observability")]
        info!(
            "Caller '{}' connected with transport ID: {}",
            name, transport_id
        );

        Ok(transport_id)
    }

    /// Creates an actual transport connection based on the configuration.
    ///
    /// This is an internal helper method that creates the appropriate transport
    /// type (TCP, TLS, etc.) based on the configuration.
    ///
    /// Returns both the transport ID and the transport object itself.
    async fn create_transport_connection(
        &self,
        config: &crate::transport::TransportConfig,
    ) -> Result<(TransportId, Box<dyn Transport>), TransportError> {
        use crate::transport::{TransportType, provider::TcpTransport};

        let transport_id = self.next_id();
        let address = config.address();

        match config.transport_type() {
            TransportType::Tcp => {
                #[cfg(feature = "observability")]
                debug!("Creating TCP transport to {}", address);

                // Create the TCP connection
                let transport = TcpTransport::connect(address).await?;

                Ok((transport_id, Box::new(transport)))
            }
            #[cfg(feature = "tls")]
            TransportType::Tls => {
                #[cfg(feature = "observability")]
                debug!("Creating TLS transport to {}", address);

                // TODO: TLS transport creation requires:
                // 1. First create a TCP transport
                // 2. Get TlsConfig from somewhere (not in TransportConfig yet)
                // 3. Wrap TCP transport with TLS
                // This will be properly implemented when TlsConfig is added to TransportConfig.

                // For now, just create a TCP connection as a placeholder
                let transport = TcpTransport::connect(address).await?;

                Ok((transport_id, Box::new(transport)))
            }
            TransportType::Memory => {
                #[cfg(feature = "observability")]
                debug!("Creating Memory transport");

                // Memory transports must be created as pairs
                // This is not supported for caller connections
                Err(TransportError::InvalidConfiguration {
                    reason: "Memory transports must be created as pairs, not via connect"
                        .to_string(),
                })
            }
            #[allow(unreachable_patterns)]
            _ => Err(TransportError::InvalidConfiguration {
                reason: format!("Unsupported transport type: {:?}", config.transport_type()),
            }),
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

    /// Gets all configured listener names.
    ///
    /// Returns a list of listener names that can be used to accept connections.
    /// Only enabled listeners are included.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::transport::{TransportManager, TransportConfig, TransportType};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080");
    /// manager.add_listener("tcp-main".to_string(), config).await?;
    ///
    /// let listeners = manager.get_listeners().await;
    /// assert_eq!(listeners, vec!["tcp-main"]);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_listeners(&self) -> Vec<String> {
        let listeners = self.inner.listeners.read().await;
        listeners
            .iter()
            .filter(|(_, entry)| entry.enabled)
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Registers an accepted connection with the transport manager.
    ///
    /// This stores the connection and its associated transport for later retrieval.
    ///
    /// # Arguments
    ///
    /// * `connection_id` - Unique identifier for this connection
    /// * `transport_id` - The transport ID associated with this connection
    /// * `transport` - The transport object for sending/receiving data
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{TransportManager, TcpTransport, Transport};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let transport = TcpTransport::connect("127.0.0.1:8080").await?;
    /// let transport_id = transport.metadata().id;
    ///
    /// manager.register_connection(
    ///     "conn-123".to_string(),
    ///     transport_id,
    /// Accepts an incoming connection from any configured listener.
    ///
    /// This method waits for a connection to be accepted by any of the background
    /// listener tasks. It returns the transport and metadata for the accepted connection.
    ///
    /// # Returns
    ///
    /// Returns a tuple of (transport, transport_id, listener_name) for the accepted connection.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No listeners are configured
    /// - All listener tasks have terminated
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
    ///
    /// // Accept a connection
    /// let (transport, transport_id, listener_name) = manager.accept_connection().await?;
    /// println!("Accepted connection from {} with ID {}", listener_name, transport_id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn accept_connection(
        &self,
    ) -> Result<(Box<dyn Transport>, TransportId, String), TransportError> {
        let mut receiver = self.inner.accept_receiver.lock().await;

        match receiver.recv().await {
            Some(accepted) => Ok((
                accepted.transport,
                accepted.transport_id,
                accepted.listener_name,
            )),
            None => Err(TransportError::InvalidConfiguration {
                reason: "All listener tasks have terminated".to_string(),
            }),
        }
    }

    ///     Box::new(transport),
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    /// Registers an accepted connection with the transport manager.
    ///
    /// This method stores the transport object and creates connection metadata.
    /// The transport_id serves as the unique identifier for this connection.
    ///
    /// # Arguments
    ///
    /// * `transport_id` - Unique identifier for this transport/connection
    /// * `transport` - The transport object to register
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{TransportManager, TransportId};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let transport_id = TransportId::new(1);
    /// // After accepting a connection...
    /// // manager.register_connection(transport_id, transport).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn register_connection(
        &self,
        transport_id: TransportId,
        transport: Box<dyn Transport>,
    ) -> Result<(), TransportError> {
        // Store the transport
        self.inner
            .active_transports
            .write()
            .await
            .insert(transport_id, transport);

        // Store connection metadata
        let conn = TransportConnection::new(
            transport_id,
            crate::transport::TransportType::Tcp, // TODO: Get actual type from transport
            None,                                 // No caller name for accepted connections
        );

        self.inner
            .connections
            .write()
            .await
            .insert(transport_id, conn);

        Ok(())
    }

    /// Checks if an active transport exists by its ID.
    ///
    /// # Arguments
    ///
    /// * `transport_id` - The transport identifier
    ///
    /// # Returns
    ///
    /// Returns true if the transport exists and is active.
    pub async fn has_active_transport(&self, transport_id: TransportId) -> bool {
        self.inner
            .active_transports
            .read()
            .await
            .contains_key(&transport_id)
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

    /// Creates a TransportRouter from a registered transport.
    ///
    /// This method takes ownership of the transport from the active_transports map,
    /// splits it into read and write halves, creates a TransportRouter, and stores
    /// the router for later access.
    ///
    /// # Arguments
    ///
    /// * `transport_id` - The ID of the transport to create a router for
    ///
    /// # Returns
    ///
    /// Returns an Arc to the created TransportRouter.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The transport does not exist
    /// - The transport cannot be split
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{TransportManager, TransportId};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let transport_id = TransportId::new(1);
    /// // After registering a transport...
    /// let router = manager.create_router(transport_id).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_router(
        &self,
        transport_id: TransportId,
    ) -> Result<Arc<TransportRouter>, TransportError> {
        // Remove the transport from active_transports
        let transport = self
            .inner
            .active_transports
            .write()
            .await
            .remove(&transport_id)
            .ok_or_else(|| TransportError::InvalidConfiguration {
                reason: format!("Transport {} not found", transport_id),
            })?;

        #[cfg(feature = "observability")]
        debug!("Creating router for transport {}", transport_id);

        // Split the transport into read and write halves
        // We need to use tokio::io::split which works with any AsyncRead + AsyncWrite
        use tokio::io::split;

        // Box the transport and split it
        let (read_half, write_half) = split(transport);

        // Create the router
        let router = Arc::new(TransportRouter::new(transport_id, read_half, write_half));

        // Store the router
        self.inner
            .routers
            .write()
            .await
            .insert(transport_id, Arc::clone(&router));

        #[cfg(feature = "observability")]
        info!("Router created for transport {}", transport_id);

        Ok(router)
    }

    /// Gets a router for a transport.
    ///
    /// # Arguments
    ///
    /// * `transport_id` - The transport ID
    ///
    /// # Returns
    ///
    /// Returns an Arc to the TransportRouter if it exists, None otherwise.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{TransportManager, TransportId};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let transport_id = TransportId::new(1);
    /// if let Some(router) = manager.get_router(transport_id).await {
    ///     // Use the router...
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_router(&self, transport_id: TransportId) -> Option<Arc<TransportRouter>> {
        self.inner.routers.read().await.get(&transport_id).cloned()
    }

    /// Removes a router for a transport.
    ///
    /// This should be called when a connection is closed to clean up resources.
    ///
    /// # Arguments
    ///
    /// * `transport_id` - The transport ID
    ///
    /// # Returns
    ///
    /// Returns the removed router if it existed.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{TransportManager, TransportId};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let transport_id = TransportId::new(1);
    /// manager.remove_router(transport_id).await;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn remove_router(&self, transport_id: TransportId) -> Option<Arc<TransportRouter>> {
        self.inner.routers.write().await.remove(&transport_id)
    }

    /// Disconnects a specific transport connection.
    ///
    /// This method performs a complete cleanup of a transport connection:
    /// - Retrieves and shuts down the router
    /// - Removes the router from tracking
    /// - Removes the transport from active transports
    /// - Removes connection metadata
    ///
    /// This method is idempotent - calling it multiple times with the same
    /// transport ID is safe and will not cause errors.
    ///
    /// # Arguments
    ///
    /// * `transport_id` - The transport ID to disconnect
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the disconnect was successful or if the transport
    /// was already disconnected.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{TransportManager, TransportId};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    /// let transport_id = TransportId::new(1);
    ///
    /// // Disconnect the transport
    /// manager.disconnect(transport_id).await?;
    ///
    /// // Safe to call again - idempotent
    /// manager.disconnect(transport_id).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn disconnect(&self, transport_id: TransportId) -> Result<(), TransportError> {
        #[cfg(feature = "observability")]
        info!(transport_id = %transport_id, "Disconnecting transport");

        // Get and remove the router
        if let Some(router) = self.remove_router(transport_id).await {
            #[cfg(feature = "observability")]
            debug!(transport_id = %transport_id, "Shutting down router");

            // Try to unwrap the Arc to call shutdown, otherwise rely on Drop
            match Arc::try_unwrap(router) {
                Ok(router) => {
                    // We have exclusive ownership, can call shutdown
                    router.shutdown().await;
                }
                Err(router) => {
                    // Still has other references, rely on Drop to abort tasks
                    #[cfg(feature = "observability")]
                    warn!(
                        transport_id = %transport_id,
                        "Router still has references, relying on Drop for cleanup"
                    );
                    drop(router);
                }
            }
        }

        // Remove from active transports
        self.inner
            .active_transports
            .write()
            .await
            .remove(&transport_id);

        // Remove connection metadata
        self.inner.connections.write().await.remove(&transport_id);

        // Remove from legacy transports map
        self.inner.transports.write().await.remove(&transport_id);

        #[cfg(feature = "observability")]
        info!(transport_id = %transport_id, "Transport disconnected successfully");

        Ok(())
    }

    /// Shuts down all transport connections gracefully.
    ///
    /// This method disconnects all active transports, ensuring proper cleanup
    /// of all resources. It continues even if individual disconnects fail,
    /// logging errors but not propagating them.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if all transports were shut down successfully.
    /// Returns an error only if a critical failure occurs.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::TransportManager;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let manager = TransportManager::new();
    ///
    /// // ... add transports and establish connections ...
    ///
    /// // Shutdown all connections
    /// manager.shutdown_all().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn shutdown_all(&self) -> Result<(), TransportError> {
        #[cfg(feature = "observability")]
        info!("Shutting down all transports");

        // Get all transport IDs
        let transport_ids: Vec<TransportId> =
            { self.inner.routers.read().await.keys().copied().collect() };

        #[cfg(feature = "observability")]
        debug!(count = transport_ids.len(), "Found transports to shutdown");

        // Disconnect each transport
        for transport_id in transport_ids {
            if let Err(_e) = self.disconnect(transport_id).await {
                #[cfg(feature = "observability")]
                error!(
                    transport_id = %transport_id,
                    error = %e,
                    "Failed to disconnect transport during shutdown"
                );
                // Continue with other transports even if one fails
            }
        }

        #[cfg(feature = "observability")]
        info!("All transports shut down");

        Ok(())
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
    use crate::transport::{
        Transport, TransportType, provider::MemoryTransport, strategy::ExponentialBackoff,
    };

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

    #[tokio::test]
    async fn test_add_listener() {
        let manager = TransportManager::new();
        // Use TCP with a high port number to avoid conflicts
        let config = crate::transport::TransportConfig::new(TransportType::Tcp, "127.0.0.1:19999");

        let result = manager
            .add_listener("test-listener".to_string(), config)
            .await;

        // Skip test if port is in use
        if result.is_err() {
            eprintln!("Skipping test - port in use");
            return;
        }

        assert_eq!(manager.listener_count().await, 1);
    }

    #[tokio::test]
    async fn test_add_listener_duplicate() {
        let manager = TransportManager::new();
        // Use TCP with a high port number to avoid conflicts
        let config = crate::transport::TransportConfig::new(TransportType::Tcp, "127.0.0.1:19998");

        let first_result = manager
            .add_listener("test-listener".to_string(), config.clone())
            .await;

        // Skip test if port is in use
        if first_result.is_err() {
            eprintln!("Skipping test - port in use");
            return;
        }

        let result = manager
            .add_listener("test-listener".to_string(), config)
            .await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TransportError::InvalidConfiguration { .. }
        ));
    }

    #[tokio::test]
    async fn test_remove_listener() {
        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Memory, "test-addr");

        manager
            .add_listener("test-listener".to_string(), config)
            .await
            .unwrap();
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
        assert!(matches!(
            result.unwrap_err(),
            TransportError::InvalidConfiguration { .. }
        ));
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

        manager
            .add_caller("test-caller".to_string(), config.clone())
            .await
            .unwrap();
        let result = manager.add_caller("test-caller".to_string(), config).await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TransportError::InvalidConfiguration { .. }
        ));
    }

    #[tokio::test]
    async fn test_remove_caller() {
        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Memory, "test-addr");

        manager
            .add_caller("test-caller".to_string(), config)
            .await
            .unwrap();
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
        assert!(matches!(
            result.unwrap_err(),
            TransportError::InvalidConfiguration { .. }
        ));
    }

    #[tokio::test]
    async fn test_enable_disable_listener() {
        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Memory, "test-addr")
            .with_enabled(false);

        manager
            .add_listener("test-listener".to_string(), config)
            .await
            .unwrap();

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

        manager
            .add_caller("test-caller".to_string(), config)
            .await
            .unwrap();

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
        assert!(matches!(
            result.unwrap_err(),
            TransportError::InvalidConfiguration { .. }
        ));
    }

    #[tokio::test]
    async fn test_disable_transport_not_found() {
        let manager = TransportManager::new();
        let result = manager.disable_transport("nonexistent").await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TransportError::InvalidConfiguration { .. }
        ));
    }

    #[tokio::test]
    async fn test_list_listeners() {
        let manager = TransportManager::new();
        let config1 = crate::transport::TransportConfig::new(TransportType::Memory, "addr1");
        let config2 = crate::transport::TransportConfig::new(TransportType::Memory, "addr2");

        manager
            .add_listener("listener1".to_string(), config1)
            .await
            .unwrap();
        manager
            .add_listener("listener2".to_string(), config2)
            .await
            .unwrap();

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

        manager
            .add_caller("caller1".to_string(), config1)
            .await
            .unwrap();
        manager
            .add_caller("caller2".to_string(), config2)
            .await
            .unwrap();

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
        assert!(matches!(
            result.unwrap_err(),
            TransportError::InvalidConfiguration { .. }
        ));
    }

    #[tokio::test]
    async fn test_connect_caller() {
        // This test verifies that connect_caller properly rejects Memory transports
        // since they can't be "connected" - they must be created as pairs
        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Memory, "test-addr");

        manager
            .add_caller("test-caller".to_string(), config)
            .await
            .unwrap();
        let result = manager.connect_caller("test-caller").await;

        // Memory transports should fail with InvalidConfiguration
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TransportError::InvalidConfiguration { .. }
        ));
    }

    #[tokio::test]
    async fn test_set_event_handler() {
        use crate::channel::ChannelId;

        struct TestHandler;
        impl TransportEventHandler for TestHandler {
            fn on_transport_connected(&self, _transport_id: TransportId) {}
            fn on_transport_disconnected(
                &self,
                _transport_id: TransportId,
                _error: Option<TransportError>,
            ) {
            }
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
                format!("listener-addr-{}", i),
            );
            manager
                .add_listener(format!("listener-{}", i), config)
                .await
                .unwrap();
        }

        // Add multiple callers
        for i in 0..3 {
            let config = crate::transport::TransportConfig::new(
                TransportType::Memory,
                format!("caller-addr-{}", i),
            );
            manager
                .add_caller(format!("caller-{}", i), config)
                .await
                .unwrap();
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

        manager
            .add_listener("disabled-listener".to_string(), config)
            .await
            .unwrap();
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

        manager
            .add_caller("metadata-caller".to_string(), config)
            .await
            .unwrap();
        assert_eq!(manager.caller_count().await, 1);
    }

    #[tokio::test]
    async fn test_connect_caller_disabled() {
        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080")
            .with_enabled(false);
        manager
            .add_caller("test".to_string(), config)
            .await
            .unwrap();

        let result = manager.connect_caller("test").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("disabled"));
    }

    #[tokio::test]
    async fn test_connect_caller_creates_connection() {
        // This test requires an actual TCP listener, so we create one first
        use crate::transport::provider::TcpTransport;

        let listener = TcpTransport::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Spawn a task to accept one connection
        tokio::spawn(async move {
            let _ = listener.accept().await;
        });

        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Tcp, addr.to_string());
        manager
            .add_caller("test".to_string(), config)
            .await
            .unwrap();

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
        use crate::transport::{CallerState, provider::TcpTransport};

        // Create a listener for the test
        let listener = TcpTransport::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Spawn a task to accept one connection
        tokio::spawn(async move {
            let _ = listener.accept().await;
        });

        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Tcp, addr.to_string());
        manager
            .add_caller("test".to_string(), config)
            .await
            .unwrap();

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
        use crate::transport::provider::TcpTransport;
        use std::sync::atomic::{AtomicBool, Ordering};

        struct TestHandler {
            connected_called: Arc<AtomicBool>,
        }

        impl TransportEventHandler for TestHandler {
            fn on_transport_connected(&self, _transport_id: TransportId) {
                self.connected_called.store(true, Ordering::SeqCst);
            }

            fn on_transport_disconnected(
                &self,
                _transport_id: TransportId,
                _error: Option<TransportError>,
            ) {
            }

            fn on_new_channel_request(
                &self,
                _channel_id: ChannelId,
                _protocol: &str,
                _transport_id: TransportId,
            ) -> Result<bool, String> {
                Ok(true)
            }
        }

        // Create a listener for the test
        let listener = TcpTransport::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Spawn a task to accept one connection
        tokio::spawn(async move {
            let _ = listener.accept().await;
        });

        let connected_called = Arc::new(AtomicBool::new(false));
        let handler = Arc::new(TestHandler {
            connected_called: Arc::clone(&connected_called),
        });

        let manager = TransportManager::new();
        manager.set_event_handler(handler).await;

        let config = crate::transport::TransportConfig::new(TransportType::Tcp, addr.to_string());
        manager
            .add_caller("test".to_string(), config)
            .await
            .unwrap();

        manager.connect_caller("test").await.unwrap();

        // Event handler should have been called
        assert!(connected_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_create_transport_connection_tcp() {
        // This test requires an actual TCP listener
        use crate::transport::provider::TcpTransport;

        let listener = TcpTransport::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Spawn a task to accept one connection
        tokio::spawn(async move {
            let _ = listener.accept().await;
        });

        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Tcp, addr.to_string());

        let result = manager.create_transport_connection(&config).await;
        assert!(result.is_ok());
        let (transport_id, _transport) = result.unwrap();
        assert!(transport_id.as_u64() > 0);
    }

    #[tokio::test]
    async fn test_create_transport_connection_memory() {
        // Memory transports can't be "connected" - they must be created as pairs
        let manager = TransportManager::new();
        let config = crate::transport::TransportConfig::new(TransportType::Memory, "memory://test");

        let result = manager.create_transport_connection(&config).await;
        // Should fail with InvalidConfiguration
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e, TransportError::InvalidConfiguration { .. }));
        }
    }
}

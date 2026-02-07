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

//! Enhanced transport types for the Transport Manager v0.2.0.
//!
//! This module contains new types and traits for the enhanced Transport Manager
//! that will support multiple transport types, automatic reconnection, and
//! dynamic transport management.
//!
//! **Status:** Phase 1 - Foundation & Design
//! **Target:** v0.2.0

use crate::channel::ChannelId;
use crate::reconnection::ReconnectionStrategy;
use crate::transport::{Transport, TransportError, TransportId};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

/// Event handler for transport lifecycle events.
///
/// This trait allows the endpoint to respond to transport events such as
/// connections, disconnections, and channel creation requests.
///
/// # Examples
///
/// ```rust
/// use bdrpc::transport::{TransportEventHandler, TransportId};
/// use bdrpc::channel::ChannelId;
/// use bdrpc::transport::TransportError;
///
/// struct MyEventHandler;
///
/// impl TransportEventHandler for MyEventHandler {
///     fn on_transport_connected(&self, transport_id: TransportId) {
///         println!("Transport {} connected", transport_id);
///     }
///
///     fn on_transport_disconnected(
///         &self,
///         transport_id: TransportId,
///         error: Option<TransportError>,
///     ) {
///         if let Some(err) = error {
///             eprintln!("Transport {} disconnected with error: {}", transport_id, err);
///         } else {
///             println!("Transport {} disconnected gracefully", transport_id);
///         }
///     }
///
///     fn on_new_channel_request(
///         &self,
///         channel_id: ChannelId,
///         protocol: &str,
///         transport_id: TransportId,
///     ) -> Result<bool, String> {
///         println!(
///             "Channel {} request for protocol '{}' on transport {}",
///             channel_id, protocol, transport_id
///         );
///         Ok(true) // Accept the channel
///     }
/// }
/// ```
pub trait TransportEventHandler: Send + Sync {
    /// Called when a transport successfully connects.
    ///
    /// This is invoked for both listener (server) and caller (client) transports
    /// after the connection is established.
    fn on_transport_connected(&self, transport_id: TransportId);

    /// Called when a transport disconnects.
    ///
    /// The `error` parameter contains the error that caused the disconnection,
    /// or `None` if the disconnection was graceful.
    fn on_transport_disconnected(&self, transport_id: TransportId, error: Option<TransportError>);

    /// Called when a new channel creation is requested.
    ///
    /// Returns `Ok(true)` to accept the channel, `Ok(false)` to reject it,
    /// or `Err(reason)` to reject with a specific error message.
    fn on_new_channel_request(
        &self,
        channel_id: ChannelId,
        protocol: &str,
        transport_id: TransportId,
    ) -> Result<bool, String>;
}

/// Trait for transport listeners (servers).
///
/// A transport listener accepts incoming connections. The actual transport type
/// is determined by the concrete implementation.
///
/// # Note
///
/// This trait is designed to be implemented by specific transport types
/// (e.g., `TcpListener`, `TlsListener`) rather than used as a trait object.
/// The accepted transport type is determined by the implementation.
///
/// # Examples
///
/// ```rust,no_run
/// use bdrpc::transport::TransportListener;
///
/// # async fn example<L: TransportListener>(listener: &L) -> Result<(), Box<dyn std::error::Error>> {
/// // Get listener address
/// let addr = listener.local_addr()?;
/// println!("Listening on {}", addr);
/// # Ok(())
/// # }
/// ```
#[async_trait::async_trait]
pub trait TransportListener: Send + Sync {
    /// The type of transport this listener produces
    type Transport: Transport;

    /// Accepts a new incoming connection.
    ///
    /// This method blocks until a new connection is available.
    async fn accept(&self) -> Result<Self::Transport, TransportError>;

    /// Returns the local address this listener is bound to.
    fn local_addr(&self) -> Result<String, TransportError>;

    /// Gracefully shuts down the listener.
    ///
    /// After calling this method, no new connections will be accepted.
    async fn shutdown(&self) -> Result<(), TransportError>;
}

/// State of a caller transport (client connection).
///
/// This tracks the current state of a client transport, including connection
/// status and reconnection attempts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallerState {
    /// Transport is disconnected and not attempting to reconnect
    Disconnected,

    /// Transport is currently attempting to connect
    Connecting,

    /// Transport is connected with the given transport ID
    Connected(TransportId),

    /// Transport is attempting to reconnect after a failure
    Reconnecting {
        /// Number of reconnection attempts made
        attempt: u32,
        /// The error that caused the disconnection
        last_error: String,
    },

    /// Transport is disabled and will not attempt to connect
    Disabled,
}

impl std::fmt::Display for CallerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disconnected => write!(f, "Disconnected"),
            Self::Connecting => write!(f, "Connecting"),
            Self::Connected(id) => write!(f, "Connected({})", id),
            Self::Reconnecting { attempt, .. } => write!(f, "Reconnecting(attempt {})", attempt),
            Self::Disabled => write!(f, "Disabled"),
        }
    }
}

/// A caller transport (client) with automatic reconnection support.
///
/// This struct manages a client transport connection, including automatic
/// reconnection using a configurable reconnection strategy.
///
/// # Examples
///
/// ```rust
/// use bdrpc::transport::{CallerTransport, TransportConfig, TransportType};
/// use bdrpc::reconnection::ExponentialBackoff;
/// use std::sync::Arc;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080")
///     .with_reconnection_strategy(Arc::new(ExponentialBackoff::default()));
///
/// let caller = CallerTransport::new("main".to_string(), config);
/// # Ok(())
/// # }
/// ```
pub struct CallerTransport {
    /// Name of this caller transport
    name: String,

    /// Configuration for this transport
    config: crate::transport::TransportConfig,

    /// Current connection state
    state: Arc<RwLock<CallerState>>,

    /// Handle to the reconnection task, if running
    reconnection_task: Arc<RwLock<Option<JoinHandle<()>>>>,
}

impl CallerTransport {
    /// Creates a new caller transport.
    ///
    /// # Arguments
    ///
    /// * `name` - Unique name for this caller transport
    /// * `config` - Configuration including address and reconnection strategy
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::transport::{CallerTransport, TransportConfig, TransportType};
    ///
    /// let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080");
    /// let caller = CallerTransport::new("main".to_string(), config);
    /// ```
    pub fn new(name: String, config: crate::transport::TransportConfig) -> Self {
        Self {
            name,
            config,
            state: Arc::new(RwLock::new(CallerState::Disconnected)),
            reconnection_task: Arc::new(RwLock::new(None)),
        }
    }

    /// Returns the name of this caller transport.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the configuration for this transport.
    pub fn config(&self) -> &crate::transport::TransportConfig {
        &self.config
    }

    /// Returns the current state of this transport.
    pub async fn state(&self) -> CallerState {
        self.state.read().await.clone()
    }

    /// Sets the state of this transport.
    pub async fn set_state(&self, state: CallerState) {
        *self.state.write().await = state;
    }

    /// Checks if this transport has a reconnection strategy configured.
    pub fn has_reconnection_strategy(&self) -> bool {
        self.config.reconnection_strategy().is_some()
    }

    /// Returns the reconnection strategy, if configured.
    pub fn reconnection_strategy(&self) -> Option<&Arc<dyn ReconnectionStrategy>> {
        self.config.reconnection_strategy()
    }

    /// Starts the automatic reconnection loop for this caller transport.
    ///
    /// This spawns a background task that will continuously attempt to reconnect
    /// using the configured reconnection strategy. The task will run until
    /// `stop_reconnection_loop()` is called or the transport is disabled.
    ///
    /// # Arguments
    ///
    /// * `event_handler` - Handler to notify of connection events
    /// * `connect_fn` - Async function that attempts to establish a connection
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::transport::{CallerTransport, TransportConfig, TransportType, TransportEventHandler, TransportId};
    /// use bdrpc::channel::ChannelId;
    /// use bdrpc::transport::TransportError;
    /// use std::sync::Arc;
    ///
    /// struct MyHandler;
    /// impl TransportEventHandler for MyHandler {
    ///     fn on_transport_connected(&self, _id: TransportId) {}
    ///     fn on_transport_disconnected(&self, _id: TransportId, _err: Option<TransportError>) {}
    ///     fn on_new_channel_request(&self, _cid: ChannelId, _proto: &str, _tid: TransportId) -> Result<bool, String> { Ok(true) }
    /// }
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080");
    /// let caller = CallerTransport::new("main".to_string(), config);
    /// let handler = Arc::new(MyHandler);
    ///
    /// caller.start_reconnection_loop(
    ///     handler,
    ///     |_addr| async { Ok(TransportId::new(1)) }
    /// ).await;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn start_reconnection_loop<F, Fut>(
        &self,
        event_handler: Arc<dyn TransportEventHandler>,
        connect_fn: F,
    ) where
        F: Fn(String) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<TransportId, TransportError>> + Send,
    {
        // Stop any existing reconnection loop
        self.stop_reconnection_loop().await;

        let name = self.name.clone();
        let config = self.config.clone();
        let state = Arc::clone(&self.state);
        let reconnection_task = Arc::clone(&self.reconnection_task);

        let strategy = match self.config.reconnection_strategy() {
            Some(s) => Arc::clone(s),
            None => {
                #[cfg(feature = "observability")]
                warn!("Caller '{}' has no reconnection strategy configured", name);
                return;
            }
        };

        #[cfg(feature = "observability")]
        info!("Starting reconnection loop for caller '{}'", name);

        let task = tokio::spawn(async move {
            let mut attempt = 0u32;
            let connect_fn = Arc::new(connect_fn);

            loop {
                // Check if we should continue
                let current_state = state.read().await.clone();
                match current_state {
                    CallerState::Disabled => {
                        #[cfg(feature = "observability")]
                        info!("Caller '{}' is disabled, stopping reconnection loop", name);
                        break;
                    }
                    CallerState::Connected(_) => {
                        #[cfg(feature = "observability")]
                        debug!("Caller '{}' is already connected, waiting for disconnection", name);
                        // Wait a bit before checking again
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        continue;
                    }
                    _ => {}
                }

                // Update state to connecting
                *state.write().await = CallerState::Connecting;

                #[cfg(feature = "observability")]
                info!("Caller '{}' attempting connection (attempt {})", name, attempt + 1);

                // Attempt to connect
                let connect_result = connect_fn(config.address().to_string()).await;

                match connect_result {
                    Ok(transport_id) => {
                        #[cfg(feature = "observability")]
                        info!("Caller '{}' connected successfully with transport ID {}", name, transport_id);

                        // Update state
                        *state.write().await = CallerState::Connected(transport_id);

                        // Notify strategy and handler
                        strategy.on_connected();
                        event_handler.on_transport_connected(transport_id);

                        // Reset attempt counter
                        attempt = 0;

                        // Wait for disconnection
                        loop {
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                            let current = state.read().await.clone();
                            if !matches!(current, CallerState::Connected(_)) {
                                break;
                            }
                        }
                    }
                    Err(error) => {
                        #[cfg(feature = "observability")]
                        warn!("Caller '{}' connection failed (attempt {}): {}", name, attempt + 1, error);

                        // Notify strategy
                        strategy.on_disconnected(&error);

                        // Check if we should retry
                        if !strategy.should_reconnect(attempt, &error).await {
                            #[cfg(feature = "observability")]
                            error!("Caller '{}' giving up after {} attempts", name, attempt + 1);

                            *state.write().await = CallerState::Disconnected;
                            break;
                        }

                        // Update state to reconnecting
                        *state.write().await = CallerState::Reconnecting {
                            attempt: attempt + 1,
                            last_error: error.to_string(),
                        };

                        // Calculate delay
                        let delay = strategy.next_delay(attempt).await;

                        #[cfg(feature = "observability")]
                        debug!("Caller '{}' will retry in {:?}", name, delay);

                        // Wait before retrying
                        tokio::time::sleep(delay).await;

                        attempt += 1;
                    }
                }
            }

            #[cfg(feature = "observability")]
            info!("Reconnection loop stopped for caller '{}'", name);
        });

        *reconnection_task.write().await = Some(task);
    }

    /// Stops the automatic reconnection loop.
    ///
    /// This aborts the background reconnection task if it's running.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::transport::{CallerTransport, TransportConfig, TransportType};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080");
    /// let caller = CallerTransport::new("main".to_string(), config);
    ///
    /// // Later, stop the reconnection loop
    /// caller.stop_reconnection_loop().await;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn stop_reconnection_loop(&self) {
        let mut task_guard = self.reconnection_task.write().await;
        if let Some(task) = task_guard.take() {
            #[cfg(feature = "observability")]
            info!("Stopping reconnection loop for caller '{}'", self.name);

            task.abort();
        }
    }

    /// Checks if the reconnection loop is currently running.
    pub async fn is_reconnection_loop_running(&self) -> bool {
        self.reconnection_task.read().await.is_some()
    }
}

impl std::fmt::Debug for CallerTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallerTransport")
            .field("name", &self.name)
            .field("config", &self.config)
            .field("has_reconnection_strategy", &self.has_reconnection_strategy())
            .finish()
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
    use crate::reconnection::ExponentialBackoff;
    use crate::transport::TransportType;

    #[test]
    fn test_caller_state_display() {
        assert_eq!(format!("{}", CallerState::Disconnected), "Disconnected");
        assert_eq!(format!("{}", CallerState::Connecting), "Connecting");
        assert_eq!(
            format!("{}", CallerState::Connected(TransportId::new(42))),
            "Connected(Transport(42))"
        );
        assert_eq!(
            format!(
                "{}",
                CallerState::Reconnecting {
                    attempt: 3,
                    last_error: "timeout".to_string()
                }
            ),
            "Reconnecting(attempt 3)"
        );
        assert_eq!(format!("{}", CallerState::Disabled), "Disabled");
    }

    #[test]
    fn test_caller_transport_new() {
        let config = crate::transport::TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080");
        let caller = CallerTransport::new("test".to_string(), config);

        assert_eq!(caller.name(), "test");
        assert_eq!(caller.config().address(), "127.0.0.1:8080");
        assert!(!caller.has_reconnection_strategy());
    }

    #[test]
    fn test_caller_transport_with_reconnection() {
        let strategy = Arc::new(ExponentialBackoff::default());
        let config = crate::transport::TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080")
            .with_reconnection_strategy(strategy);
        let caller = CallerTransport::new("test".to_string(), config);

        assert!(caller.has_reconnection_strategy());
        assert!(caller.reconnection_strategy().is_some());
    }

    #[tokio::test]
    async fn test_caller_transport_state() {
        let config = crate::transport::TransportConfig::new(TransportType::Tcp, "127.0.0.1:8080");
        let caller = CallerTransport::new("test".to_string(), config);

        assert_eq!(caller.state().await, CallerState::Disconnected);

        caller.set_state(CallerState::Connecting).await;
        assert_eq!(caller.state().await, CallerState::Connecting);

        let transport_id = TransportId::new(1);
        caller.set_state(CallerState::Connected(transport_id)).await;
        assert_eq!(caller.state().await, CallerState::Connected(transport_id));
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

// Made with Bob
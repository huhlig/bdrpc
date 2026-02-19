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

use crate::TransportError;
use crate::transport::strategy::ReconnectionStrategy;
use crate::transport::{TransportEventHandler, TransportId};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

/// State of a caller transport (client connection).
///
/// This tracks the current state of a client transport, including connection
/// status and strategy attempts.
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
        /// Number of strategy attempts made
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

/// A caller transport (client) with automatic strategy support.
///
/// This struct manages a client transport connection, including automatic
/// strategy using a configurable strategy strategy.
///
/// # Examples
///
/// ```rust
/// use bdrpc::transport::{CallerTransport, TransportConfig, TransportType};
/// use bdrpc::strategy::ExponentialBackoff;
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

    /// Handle to the strategy task, if running
    reconnection_task: Arc<RwLock<Option<JoinHandle<()>>>>,
}

impl CallerTransport {
    /// Creates a new caller transport.
    ///
    /// # Arguments
    ///
    /// * `name` - Unique name for this caller transport
    /// * `config` - Configuration including address and strategy strategy
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

    /// Checks if this transport has a strategy strategy configured.
    pub fn has_reconnection_strategy(&self) -> bool {
        self.config.reconnection_strategy().is_some()
    }

    /// Returns the strategy strategy, if configured.
    pub fn reconnection_strategy(&self) -> Option<&Arc<dyn ReconnectionStrategy>> {
        self.config.reconnection_strategy()
    }

    /// Starts the automatic strategy loop for this caller transport.
    ///
    /// This spawns a background task that will continuously attempt to reconnect
    /// using the configured strategy strategy. The task will run until
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
        // Stop any existing strategy loop
        self.stop_reconnection_loop().await;

        let config = self.config.clone();
        let state = Arc::clone(&self.state);
        let reconnection_task = Arc::clone(&self.reconnection_task);

        let strategy = match self.config.reconnection_strategy() {
            Some(s) => Arc::clone(s),
            None => {
                #[cfg(feature = "observability")]
                tracing::warn!("Caller '{}' has no strategy strategy configured", self.name);
                return;
            }
        };

        #[cfg(feature = "observability")]
        tracing::info!("Starting strategy loop for caller '{}'", &self.name);

        #[allow(unused_variables)]
        let name = self.name.clone();
        let task = tokio::spawn(async move {
            let mut attempt = 0u32;
            let connect_fn = Arc::new(connect_fn);

            loop {
                // Check if we should continue
                let current_state = state.read().await.clone();
                match current_state {
                    CallerState::Disabled => {
                        #[cfg(feature = "observability")]
                        tracing::info!("Caller '{}' is disabled, stopping strategy loop", name);
                        break;
                    }
                    CallerState::Connected(_) => {
                        #[cfg(feature = "observability")]
                        tracing::debug!(
                            "Caller '{}' is already connected, waiting for disconnection",
                            name
                        );
                        // Wait a bit before checking again
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        continue;
                    }
                    _ => {}
                }

                // Update state to connecting
                *state.write().await = CallerState::Connecting;

                #[cfg(feature = "observability")]
                tracing::info!(
                    "Caller '{}' attempting connection (attempt {})",
                    name,
                    attempt + 1
                );

                // Attempt to connect
                let connect_result = connect_fn(config.address().to_string()).await;

                match connect_result {
                    Ok(transport_id) => {
                        #[cfg(feature = "observability")]
                        tracing::info!(
                            "Caller '{}' connected successfully with transport ID {}",
                            name,
                            transport_id
                        );

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
                        tracing::warn!(
                            "Caller '{}' connection failed (attempt {}): {}",
                            name,
                            attempt + 1,
                            error
                        );

                        // Notify strategy
                        strategy.on_disconnected(&error);

                        // Check if we should retry
                        if !strategy.should_reconnect(attempt, &error).await {
                            #[cfg(feature = "observability")]
                            tracing::error!(
                                "Caller '{}' giving up after {} attempts",
                                name,
                                attempt + 1
                            );

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
                        tracing::debug!("Caller '{}' will retry in {:?}", name, delay);

                        // Wait before retrying
                        tokio::time::sleep(delay).await;

                        attempt += 1;
                    }
                }
            }

            #[cfg(feature = "observability")]
            tracing::info!("Reconnection loop stopped for caller '{}'", name);
        });

        *reconnection_task.write().await = Some(task);
    }

    /// Stops the automatic strategy loop.
    ///
    /// This aborts the background strategy task if it's running.
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
    /// // Later, stop the strategy loop
    /// caller.stop_reconnection_loop().await;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn stop_reconnection_loop(&self) {
        let mut task_guard = self.reconnection_task.write().await;
        if let Some(task) = task_guard.take() {
            #[cfg(feature = "observability")]
            tracing::info!("Stopping strategy loop for caller '{}'", self.name);

            task.abort();
        }
    }

    /// Checks if the strategy loop is currently running.
    pub async fn is_reconnection_loop_running(&self) -> bool {
        self.reconnection_task.read().await.is_some()
    }
}

impl std::fmt::Debug for CallerTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallerTransport")
            .field("name", &self.name)
            .field("config", &self.config)
            .field(
                "has_reconnection_strategy",
                &self.has_reconnection_strategy(),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::TransportId;
    use crate::transport::TransportType;
    use crate::transport::strategy::ExponentialBackoff;

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
}

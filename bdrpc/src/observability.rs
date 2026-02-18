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

//! Observability support for BDRPC.
//!
//! This module provides comprehensive observability features for monitoring, debugging,
//! and operating BDRPC applications in production environments.
//!
//! # Overview
//!
//! The observability module consists of several key components:
//!
//! - **[`HealthCheck`]**: Liveness and readiness probes for container orchestration
//! - **[`ErrorMetrics`]**: Error tracking by category and layer
//! - **[`TransportMetrics`]**: Connection lifecycle and types transfer metrics
//! - **[`ChannelMetrics`]**: Message flow and latency metrics
//! - **[`CorrelationId`]**: Distributed tracing support
//! - **[`ErrorObserver`]**: Custom error callbacks
//! - **[`log_error`]**: Structured logging integration
//!
//! # Health Checks
//!
//! Health checks enable monitoring systems to determine endpoint status, supporting
//! both liveness (is it alive?) and readiness (can it accept requests?) probes.
//!
//! ## Basic Health Monitoring
//!
//! ```rust
//! use bdrpc::observability::{HealthCheck, HealthStatus};
//!
//! let health = HealthCheck::new();
//! assert!(health.is_alive());
//! assert!(health.is_ready());
//! assert_eq!(health.status(), HealthStatus::Healthy);
//! ```
//!
//! ## Degraded State
//!
//! Mark the endpoint as degraded when experiencing issues but still operational:
//!
//! ```rust
//! use bdrpc::observability::{HealthCheck, HealthStatus};
//!
//! let health = HealthCheck::new();
//! health.set_degraded("High latency detected");
//!
//! assert_eq!(health.status(), HealthStatus::Degraded);
//! assert!(health.is_alive()); // Still alive
//! assert!(health.is_ready()); // Still ready
//! ```
//!
//! ## Health Reports
//!
//! Get detailed health information for monitoring dashboards:
//!
//! ```rust
//! use bdrpc::observability::HealthCheck;
//!
//! let health = HealthCheck::new();
//! let report = health.report();
//! println!("{}", report); // Human-readable format
//! ```
//!
//! ## Kubernetes Integration
//!
//! ```rust,no_run
//! use bdrpc::observability::HealthCheck;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let health = Arc::new(HealthCheck::new());
//!
//! // Liveness probe endpoint
//! let health_clone = health.clone();
//! tokio::spawn(async move {
//!     // HTTP endpoint at /health/live
//!     // Returns 200 if health_clone.is_alive()
//! });
//!
//! // Readiness probe endpoint
//! let health_clone = health.clone();
//! tokio::spawn(async move {
//!     // HTTP endpoint at /health/ready
//!     // Returns 200 if health_clone.is_ready()
//! });
//! # Ok(())
//! # }
//! ```
//!
//! # Metrics
//!
//! BDRPC provides comprehensive metrics for all major components. Metrics are
//! collected using atomic counters for thread-safe operation.
//!
//! ## Error Metrics
//!
//! Track errors by category, layer, and recoverability:
//!
//! ```rust
//! use bdrpc::observability::ErrorMetrics;
//! use bdrpc::BdrpcError;
//! use bdrpc::transport::TransportError;
//! use bdrpc::channel::ChannelError;
//! use bdrpc::ChannelId;
//!
//! let metrics = ErrorMetrics::new();
//!
//! // Record transport error
//! let error = BdrpcError::Transport(TransportError::Closed);
//! metrics.record_error(&error);
//! assert_eq!(metrics.transport_errors(), 1);
//! assert_eq!(metrics.transport_closures(), 1);
//!
//! // Record channel error
//! let error = BdrpcError::Channel(ChannelError::Closed {
//!     channel_id: ChannelId::from(1),
//! });
//! metrics.record_error(&error);
//! assert_eq!(metrics.channel_errors(), 1);
//!
//! // Check totals
//! assert_eq!(metrics.total_errors(), 2);
//! ```
//!
//! ## Transport Metrics
//!
//! Monitor connection lifecycle and types transfer:
//!
//! ```rust
//! use bdrpc::observability::TransportMetrics;
//!
//! let metrics = TransportMetrics::new();
//!
//! // Track connections
//! metrics.record_connection_opened();
//! assert_eq!(metrics.active_connections(), 1);
//!
//! // Track types transfer
//! metrics.record_bytes_sent(1024);
//! metrics.record_bytes_received(512);
//! assert_eq!(metrics.total_bytes_sent(), 1024);
//! assert_eq!(metrics.total_bytes_received(), 512);
//!
//! // Track errors
//! metrics.record_connection_error();
//! assert_eq!(metrics.total_connection_errors(), 1);
//! ```
//!
//! ## Channel Metrics
//!
//! Monitor message flow and latency:
//!
//! ```rust
//! use bdrpc::observability::ChannelMetrics;
//! use std::time::Duration;
//!
//! let metrics = ChannelMetrics::new();
//!
//! // Track channels
//! metrics.record_channel_created();
//! assert_eq!(metrics.active_channels(), 1);
//!
//! // Track messages
//! metrics.record_message_sent();
//! metrics.record_message_received();
//! assert_eq!(metrics.total_messages_sent(), 1);
//! assert_eq!(metrics.total_messages_received(), 1);
//!
//! // Track latency
//! metrics.record_latency(Duration::from_millis(10));
//! assert_eq!(metrics.average_latency_us(), 10000);
//! ```
//!
//! ## Metrics Integration
//!
//! When the `observability` feature is enabled, all metrics are automatically
//! exported to the `metrics` crate for integration with observability systems:
//!
//! ```toml
//! [dependencies]
//! bdrpc = { version = "0.1", features = ["observability"] }
//! metrics = "0.21"
//! metrics-exporter-prometheus = "0.12"
//! ```
//!
//! ```rust,ignore
//! use metrics_exporter_prometheus::PrometheusBuilder;
//!
//! // Set up Prometheus exporter
//! let builder = PrometheusBuilder::new();
//! builder.install().expect("failed to install Prometheus recorder");
//!
//! // BDRPC metrics will now be exported at /metrics
//! // - bdrpc.transport.connections.opened
//! // - bdrpc.transport.connections.active
//! // - bdrpc.transport.bytes.sent
//! // - bdrpc.channel.messages.sent
//! // - bdrpc.channel.latency.us
//! // - bdrpc.errors.transport
//! // - bdrpc.errors.channel
//! // - etc.
//! ```
//!
//! # Distributed Tracing
//!
//! Use correlation IDs to track requests across multiple services:
//!
//! ## Basic Correlation
//!
//! ```rust
//! use bdrpc::observability::CorrelationId;
//!
//! // Generate a new correlation ID
//! let correlation_id = CorrelationId::new();
//! println!("Request ID: {}", correlation_id);
//!
//! // Parse from string (e.g., from HTTP header)
//! let parsed = CorrelationId::from_string(
//!     "550e8400-e29b-41d4-a716-446655440000"
//! ).unwrap();
//! ```
//!
//! ## Correlation Context
//!
//! Propagate correlation IDs through async tasks:
//!
//! ```rust
//! use bdrpc::observability::{CorrelationId, CorrelationContext};
//!
//! # async fn example() {
//! let correlation_id = CorrelationId::new();
//!
//! // Execute with correlation ID
//! CorrelationContext::with(correlation_id, async {
//!     // Correlation ID is available here
//!     if let Some(id) = CorrelationContext::get().await {
//!         println!("Current correlation ID: {}", id);
//!     }
//! }).await;
//! # }
//! ```
//!
//! ## Tracing Integration
//!
//! ```rust,ignore
//! use bdrpc::observability::CorrelationId;
//! use tracing::info;
//!
//! let correlation_id = CorrelationId::new();
//!
//! // Include in structured logs
//! info!(
//!     correlation_id = %correlation_id,
//!     "Processing request"
//! );
//! ```
//!
//! # Error Callbacks
//!
//! Register custom callbacks to be notified when errors occur:
//!
//! ## Basic Error Handling
//!
//! ```rust
//! use bdrpc::observability::ErrorObserver;
//! use bdrpc::BdrpcError;
//! use bdrpc::transport::TransportError;
//!
//! let observer = ErrorObserver::new();
//!
//! observer.on_error(|error| {
//!     eprintln!("Error occurred: {}", error);
//! });
//!
//! // Trigger callback
//! let error = BdrpcError::Transport(TransportError::Closed);
//! observer.notify(&error);
//! ```
//!
//! ## Multiple Callbacks
//!
//! ```rust
//! use bdrpc::observability::ErrorObserver;
//! use std::sync::Arc;
//! use std::sync::atomic::{AtomicU64, Ordering};
//!
//! let observer = ErrorObserver::new();
//! let error_count = Arc::new(AtomicU64::new(0));
//!
//! // Callback 1: Count errors
//! let count_clone = error_count.clone();
//! observer.on_error(move |_| {
//!     count_clone.fetch_add(1, Ordering::Relaxed);
//! });
//!
//! // Callback 2: Log errors
//! observer.on_error(|error| {
//!     eprintln!("Error: {}", error);
//! });
//!
//! // Callback 3: Alert on critical errors
//! observer.on_error(|error| {
//!     if !error.is_recoverable() {
//!         // Send alert
//!     }
//! });
//! ```
//!
//! ## Integration with Metrics
//!
//! ```rust
//! use bdrpc::observability::{ErrorObserver, ErrorMetrics};
//! use std::sync::Arc;
//!
//! let observer = ErrorObserver::new();
//! let metrics = Arc::new(ErrorMetrics::new());
//!
//! let metrics_clone = metrics.clone();
//! observer.on_error(move |error| {
//!     metrics_clone.record_error(error);
//! });
//! ```
//!
//! # Structured Logging
//!
//! When the `tracing` feature is enabled, errors are automatically logged with
//! structured context:
//!
//! ```rust,ignore
//! use bdrpc::observability::log_error;
//! use bdrpc::BdrpcError;
//! use bdrpc::transport::TransportError;
//!
//! let error = BdrpcError::Transport(TransportError::Closed);
//! log_error(&error);
//!
//! // Logs at ERROR level with structured fields:
//! // - error: the error message
//! // - recoverable: whether the error is recoverable
//! // - should_close_transport: whether transport should close
//! // - should_close_channel: whether channel should close
//! ```
//!
//! ## Tracing Setup
//!
//! ```rust,ignore
//! use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
//!
//! tracing_subscriber::registry()
//!     .with(tracing_subscriber::fmt::layer())
//!     .with(tracing_subscriber::EnvFilter::from_default_env())
//!     .init();
//!
//! // Now all BDRPC operations will be traced
//! ```
//!
//! # Complete Observability Setup
//!
//! Here's a complete example integrating all observability features:
//!
//! ```rust,no_run
//! use bdrpc::observability::{
//!     HealthCheck, ErrorMetrics, TransportMetrics, ChannelMetrics,
//!     ErrorObserver, CorrelationId,
//! };
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Set up health checks
//! let health = Arc::new(HealthCheck::new());
//!
//! // Set up metrics
//! let error_metrics = Arc::new(ErrorMetrics::new());
//! let transport_metrics = Arc::new(TransportMetrics::new());
//! let channel_metrics = Arc::new(ChannelMetrics::new());
//!
//! // Set up error observer
//! let observer = ErrorObserver::new();
//! let error_metrics_clone = error_metrics.clone();
//! let health_clone = health.clone();
//!
//! observer.on_error(move |error| {
//!     // Record metrics
//!     error_metrics_clone.record_error(error);
//!
//!     // Update health status
//!     if !error.is_recoverable() {
//!         health_clone.set_unhealthy(&format!("Critical error: {}", error));
//!     }
//! });
//!
//! // Generate correlation ID for this request
//! let correlation_id = CorrelationId::new();
//!
//! // Use in your application...
//! # Ok(())
//! # }
//! ```
//!
//! # Performance Considerations
//!
//! ## Metrics Overhead
//!
//! - Metrics use atomic operations (minimal overhead)
//! - No locks required for recording metrics
//! - Suitable for high-throughput scenarios
//!
//! ## Feature Flags
//!
//! - Disable `observability` feature to remove metrics integration overhead
//! - Disable `tracing` feature to remove logging overhead
//! - Core metrics still available without features
//!
//! ## Best Practices
//!
//! 1. **Sample high-frequency events**: Don't log every message in high-throughput scenarios
//! 2. **Use correlation IDs**: Essential for debugging distributed systems
//! 3. **Monitor health checks**: Integrate with orchestration systems
//! 4. **Set up alerts**: Use metrics to trigger alerts on critical errors
//! 5. **Regular metric resets**: Reset counters periodically if needed
//!
//! # See Also
//!
//! - [`transport`](crate::transport) - Transport layer with built-in metrics
//! - [`channel`](crate::channel) - Channel layer with built-in metrics
//! - [`error`](crate::error) - Error types with observability support

mod correlation;
mod health;
mod metrics;

pub use correlation::{CorrelationContext, CorrelationId};
pub use health::{HealthCheck, HealthReport, HealthStatus};
pub use metrics::{ChannelMetrics, TransportMetrics};

use crate::BdrpcError;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Metrics for tracking errors across all layers.
///
/// This structure maintains atomic counters for different error categories,
/// allowing thread-safe error tracking without locks.
///
/// # Examples
///
/// ```rust
/// use bdrpc::observability::ErrorMetrics;
/// use bdrpc::BdrpcError;
/// use bdrpc::transport::TransportError;
/// use bdrpc::channel::ChannelError;
/// use bdrpc::ChannelId;
///
/// let metrics = ErrorMetrics::new();
///
/// // Record transport error
/// let error = BdrpcError::Transport(TransportError::Closed);
/// metrics.record_error(&error);
/// assert_eq!(metrics.transport_errors(), 1);
///
/// // Record channel error
/// let error = BdrpcError::Channel(ChannelError::Closed {
///     channel_id: ChannelId::from(1),
/// });
/// metrics.record_error(&error);
/// assert_eq!(metrics.channel_errors(), 1);
///
/// // Check totals
/// assert_eq!(metrics.total_errors(), 2);
/// assert_eq!(metrics.recoverable_errors(), 0);
/// ```
#[derive(Debug, Default)]
pub struct ErrorMetrics {
    /// Total number of transport errors
    transport_errors: AtomicU64,
    /// Total number of channel errors
    channel_errors: AtomicU64,
    /// Total number of application errors
    application_errors: AtomicU64,
    /// Total number of recoverable errors
    recoverable_errors: AtomicU64,
    /// Total number of errors that closed the transport
    transport_closures: AtomicU64,
    /// Total number of errors that closed a channel
    channel_closures: AtomicU64,
}

impl ErrorMetrics {
    /// Creates a new error metrics tracker.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::ErrorMetrics;
    ///
    /// let metrics = ErrorMetrics::new();
    /// assert_eq!(metrics.total_errors(), 0);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records an error and updates the relevant counters.
    ///
    /// This method is thread-safe and can be called from multiple threads
    /// simultaneously.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::ErrorMetrics;
    /// use bdrpc::BdrpcError;
    /// use bdrpc::transport::TransportError;
    ///
    /// let metrics = ErrorMetrics::new();
    /// let error = BdrpcError::Transport(TransportError::Closed);
    /// metrics.record_error(&error);
    ///
    /// assert_eq!(metrics.transport_errors(), 1);
    /// assert_eq!(metrics.transport_closures(), 1);
    /// ```
    pub fn record_error(&self, error: &BdrpcError) {
        // Increment layer-specific counter
        match error {
            BdrpcError::Transport(_) => {
                self.transport_errors.fetch_add(1, Ordering::Relaxed);
                #[cfg(feature = "observability")]
                ::metrics::counter!("bdrpc.errors.transport").increment(1);
            }
            BdrpcError::Channel(_) => {
                self.channel_errors.fetch_add(1, Ordering::Relaxed);
                #[cfg(feature = "observability")]
                ::metrics::counter!("bdrpc.errors.channel").increment(1);
            }
            BdrpcError::Application(_) => {
                self.application_errors.fetch_add(1, Ordering::Relaxed);
                #[cfg(feature = "observability")]
                ::metrics::counter!("bdrpc.errors.application").increment(1);
            }
        }

        // Track recoverability
        if error.is_recoverable() {
            self.recoverable_errors.fetch_add(1, Ordering::Relaxed);
            #[cfg(feature = "observability")]
            ::metrics::counter!("bdrpc.errors.recoverable").increment(1);
        }

        // Track closures
        if error.should_close_transport() {
            self.transport_closures.fetch_add(1, Ordering::Relaxed);
            #[cfg(feature = "observability")]
            ::metrics::counter!("bdrpc.closures.transport").increment(1);
        }
        if error.should_close_channel() {
            self.channel_closures.fetch_add(1, Ordering::Relaxed);
            #[cfg(feature = "observability")]
            ::metrics::counter!("bdrpc.closures.channel").increment(1);
        }

        // Record total errors
        #[cfg(feature = "observability")]
        ::metrics::counter!("bdrpc.errors.total").increment(1);
    }

    /// Returns the total number of transport errors.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::ErrorMetrics;
    /// use bdrpc::BdrpcError;
    /// use bdrpc::transport::TransportError;
    ///
    /// let metrics = ErrorMetrics::new();
    /// metrics.record_error(&BdrpcError::Transport(TransportError::Closed));
    /// assert_eq!(metrics.transport_errors(), 1);
    /// ```
    #[must_use]
    pub fn transport_errors(&self) -> u64 {
        self.transport_errors.load(Ordering::Relaxed)
    }

    /// Returns the total number of channel errors.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::ErrorMetrics;
    /// use bdrpc::BdrpcError;
    /// use bdrpc::channel::ChannelError;
    /// use bdrpc::ChannelId;
    ///
    /// let metrics = ErrorMetrics::new();
    /// metrics.record_error(&BdrpcError::Channel(ChannelError::Closed {
    ///     channel_id: ChannelId::from(1),
    /// }));
    /// assert_eq!(metrics.channel_errors(), 1);
    /// ```
    #[must_use]
    pub fn channel_errors(&self) -> u64 {
        self.channel_errors.load(Ordering::Relaxed)
    }

    /// Returns the total number of application errors.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::ErrorMetrics;
    /// use bdrpc::BdrpcError;
    /// use std::io;
    ///
    /// let metrics = ErrorMetrics::new();
    /// let app_error = io::Error::new(io::ErrorKind::Other, "test");
    /// metrics.record_error(&BdrpcError::Application(Box::new(app_error)));
    /// assert_eq!(metrics.application_errors(), 1);
    /// ```
    #[must_use]
    pub fn application_errors(&self) -> u64 {
        self.application_errors.load(Ordering::Relaxed)
    }

    /// Returns the total number of errors across all layers.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::ErrorMetrics;
    /// use bdrpc::BdrpcError;
    /// use bdrpc::transport::TransportError;
    /// use bdrpc::channel::ChannelError;
    /// use bdrpc::ChannelId;
    ///
    /// let metrics = ErrorMetrics::new();
    /// metrics.record_error(&BdrpcError::Transport(TransportError::Closed));
    /// metrics.record_error(&BdrpcError::Channel(ChannelError::Closed {
    ///     channel_id: ChannelId::from(1),
    /// }));
    /// assert_eq!(metrics.total_errors(), 2);
    /// ```
    #[must_use]
    pub fn total_errors(&self) -> u64 {
        self.transport_errors() + self.channel_errors() + self.application_errors()
    }

    /// Returns the total number of recoverable errors.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::ErrorMetrics;
    /// use bdrpc::BdrpcError;
    /// use bdrpc::channel::ChannelError;
    /// use bdrpc::ChannelId;
    ///
    /// let metrics = ErrorMetrics::new();
    /// metrics.record_error(&BdrpcError::Channel(ChannelError::Full {
    ///     channel_id: ChannelId::from(1),
    ///     buffer_size: 100,
    /// }));
    /// assert_eq!(metrics.recoverable_errors(), 1);
    /// ```
    #[must_use]
    pub fn recoverable_errors(&self) -> u64 {
        self.recoverable_errors.load(Ordering::Relaxed)
    }

    /// Returns the number of errors that caused transport closures.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::ErrorMetrics;
    /// use bdrpc::BdrpcError;
    /// use bdrpc::transport::TransportError;
    ///
    /// let metrics = ErrorMetrics::new();
    /// metrics.record_error(&BdrpcError::Transport(TransportError::Closed));
    /// assert_eq!(metrics.transport_closures(), 1);
    /// ```
    #[must_use]
    pub fn transport_closures(&self) -> u64 {
        self.transport_closures.load(Ordering::Relaxed)
    }

    /// Returns the number of errors that caused channel closures.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::ErrorMetrics;
    /// use bdrpc::BdrpcError;
    /// use bdrpc::channel::ChannelError;
    /// use bdrpc::ChannelId;
    ///
    /// let metrics = ErrorMetrics::new();
    /// metrics.record_error(&BdrpcError::Channel(ChannelError::Closed {
    ///     channel_id: ChannelId::from(1),
    /// }));
    /// assert_eq!(metrics.channel_closures(), 1);
    /// ```
    #[must_use]
    pub fn channel_closures(&self) -> u64 {
        self.channel_closures.load(Ordering::Relaxed)
    }

    /// Resets all counters to zero.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::ErrorMetrics;
    /// use bdrpc::BdrpcError;
    /// use bdrpc::transport::TransportError;
    ///
    /// let metrics = ErrorMetrics::new();
    /// metrics.record_error(&BdrpcError::Transport(TransportError::Closed));
    /// assert_eq!(metrics.total_errors(), 1);
    ///
    /// metrics.reset();
    /// assert_eq!(metrics.total_errors(), 0);
    /// ```
    pub fn reset(&self) {
        self.transport_errors.store(0, Ordering::Relaxed);
        self.channel_errors.store(0, Ordering::Relaxed);
        self.application_errors.store(0, Ordering::Relaxed);
        self.recoverable_errors.store(0, Ordering::Relaxed);
        self.transport_closures.store(0, Ordering::Relaxed);
        self.channel_closures.store(0, Ordering::Relaxed);
    }
}

/// Type alias for error callback functions.
pub type ErrorCallback = Box<dyn Fn(&BdrpcError) + Send + Sync>;

/// Observer for error events.
///
/// This type allows registering callbacks that are invoked when errors occur,
/// enabling custom error handling, logging, and monitoring.
///
/// # Examples
///
/// ```rust
/// use bdrpc::observability::ErrorObserver;
/// use bdrpc::BdrpcError;
/// use bdrpc::transport::TransportError;
/// use std::sync::Arc;
/// use std::sync::atomic::{AtomicU64, Ordering};
///
/// let observer = ErrorObserver::new();
/// let counter = Arc::new(AtomicU64::new(0));
/// let counter_clone = counter.clone();
///
/// observer.on_error(move |_error| {
///     counter_clone.fetch_add(1, Ordering::Relaxed);
/// });
///
/// let error = BdrpcError::Transport(TransportError::Closed);
/// observer.notify(&error);
///
/// assert_eq!(counter.load(Ordering::Relaxed), 1);
/// ```
#[derive(Clone)]
pub struct ErrorObserver {
    callbacks: Arc<Mutex<Vec<Arc<ErrorCallback>>>>,
}

impl ErrorObserver {
    /// Creates a new error observer.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::ErrorObserver;
    ///
    /// let observer = ErrorObserver::new();
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            callbacks: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Registers a callback to be invoked when errors occur.
    ///
    /// The callback receives a reference to the error and can perform
    /// any action such as logging, metrics collection, or alerting.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::ErrorObserver;
    ///
    /// let observer = ErrorObserver::new();
    /// observer.on_error(|error| {
    ///     eprintln!("Error: {}", error);
    /// });
    /// ```
    pub fn on_error<F>(&self, callback: F)
    where
        F: Fn(&BdrpcError) + Send + Sync + 'static,
    {
        let mut callbacks = self.callbacks.lock().unwrap();
        callbacks.push(Arc::new(Box::new(callback)));
    }

    /// Notifies all registered callbacks about an error.
    ///
    /// This method is called internally by BDRPC when errors occur.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::ErrorObserver;
    /// use bdrpc::BdrpcError;
    /// use bdrpc::transport::TransportError;
    ///
    /// let observer = ErrorObserver::new();
    /// observer.on_error(|error| {
    ///     println!("Notified: {}", error);
    /// });
    ///
    /// let error = BdrpcError::Transport(TransportError::Closed);
    /// observer.notify(&error);
    /// ```
    pub fn notify(&self, error: &BdrpcError) {
        let callbacks = self.callbacks.lock().unwrap();
        for callback in callbacks.iter() {
            callback(error);
        }
    }

    /// Clears all registered callbacks.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::ErrorObserver;
    ///
    /// let observer = ErrorObserver::new();
    /// observer.on_error(|_| {});
    /// observer.clear();
    /// ```
    pub fn clear(&self) {
        let mut callbacks = self.callbacks.lock().unwrap();
        callbacks.clear();
    }
}

impl Default for ErrorObserver {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ErrorObserver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let callbacks = self.callbacks.lock().unwrap();
        f.debug_struct("ErrorObserver")
            .field("callback_count", &callbacks.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChannelId;
    use crate::channel::ChannelError;
    use crate::transport::TransportError;
    use std::io;
    use std::sync::atomic::AtomicU64;

    #[test]
    fn test_metrics_new() {
        let metrics = ErrorMetrics::new();
        assert_eq!(metrics.total_errors(), 0);
        assert_eq!(metrics.transport_errors(), 0);
        assert_eq!(metrics.channel_errors(), 0);
        assert_eq!(metrics.application_errors(), 0);
    }

    #[test]
    fn test_metrics_record_transport_error() {
        let metrics = ErrorMetrics::new();
        let error = BdrpcError::Transport(TransportError::Closed);
        metrics.record_error(&error);

        assert_eq!(metrics.transport_errors(), 1);
        assert_eq!(metrics.total_errors(), 1);
        assert_eq!(metrics.transport_closures(), 1);
    }

    #[test]
    fn test_metrics_record_channel_error() {
        let metrics = ErrorMetrics::new();
        let error = BdrpcError::Channel(ChannelError::Closed {
            channel_id: ChannelId::from(1),
        });
        metrics.record_error(&error);

        assert_eq!(metrics.channel_errors(), 1);
        assert_eq!(metrics.total_errors(), 1);
        assert_eq!(metrics.channel_closures(), 1);
    }

    #[test]
    fn test_metrics_record_application_error() {
        let metrics = ErrorMetrics::new();
        let app_error = io::Error::other("test");
        let error = BdrpcError::Application(Box::new(app_error));
        metrics.record_error(&error);

        assert_eq!(metrics.application_errors(), 1);
        assert_eq!(metrics.total_errors(), 1);
    }

    #[test]
    fn test_metrics_recoverable_errors() {
        let metrics = ErrorMetrics::new();

        // Recoverable error
        let error = BdrpcError::Channel(ChannelError::Full {
            channel_id: ChannelId::from(1),
            buffer_size: 100,
        });
        metrics.record_error(&error);
        assert_eq!(metrics.recoverable_errors(), 1);

        // Non-recoverable error
        let error = BdrpcError::Transport(TransportError::Closed);
        metrics.record_error(&error);
        assert_eq!(metrics.recoverable_errors(), 1); // Still 1
    }

    #[test]
    fn test_metrics_reset() {
        let metrics = ErrorMetrics::new();
        let error = BdrpcError::Transport(TransportError::Closed);
        metrics.record_error(&error);
        assert_eq!(metrics.total_errors(), 1);

        metrics.reset();
        assert_eq!(metrics.total_errors(), 0);
        assert_eq!(metrics.transport_errors(), 0);
    }

    #[test]
    fn test_observer_new() {
        let observer = ErrorObserver::new();
        assert_eq!(
            format!("{:?}", observer),
            "ErrorObserver { callback_count: 0 }"
        );
    }

    #[test]
    fn test_observer_on_error() {
        let observer = ErrorObserver::new();
        let counter = Arc::new(AtomicU64::new(0));
        let counter_clone = counter.clone();

        observer.on_error(move |_error| {
            counter_clone.fetch_add(1, Ordering::Relaxed);
        });

        let error = BdrpcError::Transport(TransportError::Closed);
        observer.notify(&error);

        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_observer_multiple_callbacks() {
        let observer = ErrorObserver::new();
        let counter1 = Arc::new(AtomicU64::new(0));
        let counter2 = Arc::new(AtomicU64::new(0));

        let c1 = counter1.clone();
        observer.on_error(move |_| {
            c1.fetch_add(1, Ordering::Relaxed);
        });

        let c2 = counter2.clone();
        observer.on_error(move |_| {
            c2.fetch_add(1, Ordering::Relaxed);
        });

        let error = BdrpcError::Transport(TransportError::Closed);
        observer.notify(&error);

        assert_eq!(counter1.load(Ordering::Relaxed), 1);
        assert_eq!(counter2.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_observer_clear() {
        let observer = ErrorObserver::new();
        let counter = Arc::new(AtomicU64::new(0));
        let counter_clone = counter.clone();

        observer.on_error(move |_| {
            counter_clone.fetch_add(1, Ordering::Relaxed);
        });

        observer.clear();

        let error = BdrpcError::Transport(TransportError::Closed);
        observer.notify(&error);

        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_observer_clone() {
        let observer = ErrorObserver::new();
        let counter = Arc::new(AtomicU64::new(0));
        let counter_clone = counter.clone();

        observer.on_error(move |_| {
            counter_clone.fetch_add(1, Ordering::Relaxed);
        });

        let observer_clone = observer.clone();
        let error = BdrpcError::Transport(TransportError::Closed);
        observer_clone.notify(&error);

        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }
}

/// Logs an error with structured context using the tracing framework.
///
/// This function is only available when the `tracing` feature is enabled.
/// It automatically determines the appropriate log level based on the error type:
///
/// - Transport errors: ERROR level
/// - Channel errors: WARN level (unless closed, then ERROR)
/// - Application errors: INFO level (framework doesn't control these)
///
/// # Examples
///
/// ```rust,ignore
/// use bdrpc::observability::log_error;
/// use bdrpc::BdrpcError;
/// use bdrpc::transport::TransportError;
///
/// let error = BdrpcError::Transport(TransportError::Closed);
/// log_error(&error);
/// ```
#[cfg(feature = "tracing")]
pub fn log_error(error: &BdrpcError) {
    match error {
        BdrpcError::Transport(e) => {
            tracing::error!(
                error = %e,
                recoverable = error.is_recoverable(),
                should_close_transport = error.should_close_transport(),
                "Transport error occurred"
            );
        }
        BdrpcError::Channel(e) => {
            // Log channel errors at ERROR level if closed, WARN otherwise
            if e.is_closed() {
                tracing::error!(
                    error = %e,
                    channel_id = ?e.channel_id(),
                    recoverable = error.is_recoverable(),
                    should_close_channel = error.should_close_channel(),
                    "Channel error occurred (closed)"
                );
            } else {
                tracing::warn!(
                    error = %e,
                    channel_id = ?e.channel_id(),
                    recoverable = error.is_recoverable(),
                    should_close_channel = error.should_close_channel(),
                    "Channel error occurred"
                );
            }
        }
        BdrpcError::Application(e) => {
            tracing::info!(
                error = %e,
                "Application error occurred"
            );
        }
    }
}

/// Logs an error with structured context (no-op when tracing is disabled).
///
/// This is a no-op stub when the `tracing` feature is not enabled.
#[cfg(not(feature = "tracing"))]
#[inline]
pub fn log_error(_error: &BdrpcError) {
    // No-op when tracing is disabled
}

#[cfg(all(test, feature = "tracing"))]
mod tracing_tests {
    use super::*;
    use crate::ChannelId;
    use crate::channel::ChannelError;
    use crate::transport::TransportError;
    use std::io;

    #[test]
    fn test_log_transport_error() {
        // This test just ensures the function compiles and runs
        let error = BdrpcError::Transport(TransportError::Closed);
        log_error(&error);
    }

    #[test]
    fn test_log_channel_error() {
        let error = BdrpcError::Channel(ChannelError::Closed {
            channel_id: ChannelId::from(1),
        });
        log_error(&error);
    }

    #[test]
    fn test_log_application_error() {
        let app_error = io::Error::other("test");
        let error = BdrpcError::Application(Box::new(app_error));
        log_error(&error);
    }
}

#[cfg(all(test, not(feature = "tracing")))]
mod no_tracing_tests {
    use super::*;
    use crate::transport::TransportError;

    #[test]
    fn test_log_error_noop() {
        // Ensure the no-op version compiles
        let error = BdrpcError::Transport(TransportError::Closed);
        log_error(&error);
    }
}

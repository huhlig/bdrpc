//! Reconnection strategy traits and types.
//!
//! This module defines the pluggable reconnection strategy pattern that allows
//! different reconnection behaviors for different use cases.

use crate::transport::TransportError;
use async_trait::async_trait;
use std::time::Duration;

/// A strategy for handling reconnection attempts after connection failures.
///
/// This trait allows pluggable reconnection behavior, enabling different strategies
/// for different use cases (exponential backoff, fixed delay, circuit breaker, etc.).
///
/// # Examples
///
/// ```
/// use bdrpc::reconnection::{ReconnectionStrategy, ExponentialBackoff};
/// use std::time::Duration;
///
/// let strategy = ExponentialBackoff::builder()
///     .initial_delay(Duration::from_millis(100))
///     .max_delay(Duration::from_secs(30))
///     .build();
/// ```
#[async_trait]
pub trait ReconnectionStrategy: Send + Sync {
    /// Determine if reconnection should be attempted.
    ///
    /// # Arguments
    ///
    /// * `attempt` - The current attempt number (0-indexed)
    /// * `last_error` - The error that caused the disconnection
    ///
    /// # Returns
    ///
    /// `true` if reconnection should be attempted, `false` to give up
    async fn should_reconnect(&self, attempt: u32, last_error: &TransportError) -> bool;

    /// Calculate the delay before the next reconnection attempt.
    ///
    /// # Arguments
    ///
    /// * `attempt` - The current attempt number (0-indexed)
    ///
    /// # Returns
    ///
    /// The duration to wait before the next attempt
    async fn next_delay(&self, attempt: u32) -> Duration;

    /// Called when a connection is successfully established.
    ///
    /// This allows the strategy to reset internal state or update metrics.
    fn on_connected(&self);

    /// Called when a connection attempt fails.
    ///
    /// # Arguments
    ///
    /// * `error` - The error that caused the failure
    fn on_disconnected(&self, error: &TransportError);

    /// Reset the strategy's internal state.
    ///
    /// This is useful for manual intervention or after a successful connection
    /// that later disconnects.
    fn reset(&self);

    /// Get a human-readable name for this strategy.
    ///
    /// Used for logging and debugging.
    fn name(&self) -> &str;
}

/// Metadata about reconnection attempts.
///
/// This is used for observability and debugging.
#[derive(Debug, Clone, Default)]
pub struct ReconnectionMetrics {
    /// Total number of reconnection attempts
    pub total_attempts: u64,
    /// Number of successful reconnections
    pub successful_reconnections: u64,
    /// Number of failed reconnections
    pub failed_reconnections: u64,
    /// Current consecutive failures
    pub consecutive_failures: u32,
    /// Last error encountered
    pub last_error: Option<String>,
}

impl ReconnectionMetrics {
    /// Create a new metrics instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a reconnection attempt.
    pub fn record_attempt(&mut self) {
        self.total_attempts += 1;
    }

    /// Record a successful reconnection.
    pub fn record_success(&mut self) {
        self.successful_reconnections += 1;
        self.consecutive_failures = 0;
        self.last_error = None;
    }

    /// Record a failed reconnection.
    pub fn record_failure(&mut self, error: &TransportError) {
        self.failed_reconnections += 1;
        self.consecutive_failures += 1;
        self.last_error = Some(error.to_string());
    }

    /// Reset all metrics.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_new() {
        let metrics = ReconnectionMetrics::new();
        assert_eq!(metrics.total_attempts, 0);
        assert_eq!(metrics.successful_reconnections, 0);
        assert_eq!(metrics.failed_reconnections, 0);
        assert_eq!(metrics.consecutive_failures, 0);
        assert!(metrics.last_error.is_none());
    }

    #[test]
    fn test_metrics_record_attempt() {
        let mut metrics = ReconnectionMetrics::new();
        metrics.record_attempt();
        assert_eq!(metrics.total_attempts, 1);
    }

    #[test]
    fn test_metrics_record_success() {
        let mut metrics = ReconnectionMetrics::new();
        metrics.consecutive_failures = 5;
        metrics.last_error = Some("error".to_string());

        metrics.record_success();

        assert_eq!(metrics.successful_reconnections, 1);
        assert_eq!(metrics.consecutive_failures, 0);
        assert!(metrics.last_error.is_none());
    }

    #[test]
    fn test_metrics_record_failure() {
        let mut metrics = ReconnectionMetrics::new();
        let error = TransportError::connection_failed("test");

        metrics.record_failure(&error);

        assert_eq!(metrics.failed_reconnections, 1);
        assert_eq!(metrics.consecutive_failures, 1);
        assert!(metrics.last_error.is_some());
    }

    #[test]
    fn test_metrics_reset() {
        let mut metrics = ReconnectionMetrics::new();
        metrics.total_attempts = 10;
        metrics.successful_reconnections = 5;
        metrics.failed_reconnections = 5;
        metrics.consecutive_failures = 3;
        metrics.last_error = Some("error".to_string());

        metrics.reset();

        assert_eq!(metrics.total_attempts, 0);
        assert_eq!(metrics.successful_reconnections, 0);
        assert_eq!(metrics.failed_reconnections, 0);
        assert_eq!(metrics.consecutive_failures, 0);
        assert!(metrics.last_error.is_none());
    }
}

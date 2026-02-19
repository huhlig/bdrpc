//! Fixed delay strategy strategy.
//!
//! This strategy uses a constant delay between strategy attempts.

use super::traits::{ReconnectionMetrics, ReconnectionStrategy};
use crate::transport::TransportError;
use async_trait::async_trait;
use std::sync::Mutex;
use std::time::Duration;

/// Fixed delay strategy strategy.
///
/// This strategy waits a constant amount of time between strategy attempts.
/// It's useful for predictable retry patterns.
///
/// # Examples
///
/// ```
/// use bdrpc::strategy::FixedDelay;
/// use std::time::Duration;
///
/// // Default configuration (1 second delay)
/// let strategy = FixedDelay::default();
///
/// // Custom configuration
/// let strategy = FixedDelay::builder()
///     .delay(Duration::from_secs(5))
///     .max_attempts(Some(10))
///     .build();
/// ```
#[derive(Debug)]
pub struct FixedDelay {
    /// Delay between strategy attempts
    delay: Duration,
    /// Maximum number of attempts (None = unlimited)
    max_attempts: Option<u32>,
    /// Internal metrics
    metrics: Mutex<ReconnectionMetrics>,
}

impl Default for FixedDelay {
    fn default() -> Self {
        Self {
            delay: Duration::from_secs(1),
            max_attempts: None,
            metrics: Mutex::new(ReconnectionMetrics::new()),
        }
    }
}

impl FixedDelay {
    /// Create a new builder for configuring fixed delay.
    pub fn builder() -> FixedDelayBuilder {
        FixedDelayBuilder::default()
    }

    /// Create a new fixed delay strategy with the given delay.
    pub fn new(delay: Duration) -> Self {
        Self {
            delay,
            max_attempts: None,
            metrics: Mutex::new(ReconnectionMetrics::new()),
        }
    }

    /// Get the current metrics.
    pub fn metrics(&self) -> ReconnectionMetrics {
        self.metrics.lock().unwrap().clone()
    }

    /// Check if the error is non-recoverable.
    fn is_non_recoverable_error(error: &TransportError) -> bool {
        matches!(error, TransportError::InvalidConfiguration { .. })
    }
}

#[async_trait]
impl ReconnectionStrategy for FixedDelay {
    async fn should_reconnect(&self, attempt: u32, last_error: &TransportError) -> bool {
        // Don't reconnect for non-recoverable errors
        if Self::is_non_recoverable_error(last_error) {
            return false;
        }

        // Check attempt limit
        if let Some(max) = self.max_attempts {
            if attempt >= max {
                return false;
            }
        }

        true
    }

    async fn next_delay(&self, _attempt: u32) -> Duration {
        self.delay
    }

    fn on_connected(&self) {
        let mut metrics = self.metrics.lock().unwrap();
        metrics.record_success();
    }

    fn on_disconnected(&self, error: &TransportError) {
        let mut metrics = self.metrics.lock().unwrap();
        metrics.record_failure(error);
    }

    fn reset(&self) {
        let mut metrics = self.metrics.lock().unwrap();
        metrics.reset();
    }

    fn name(&self) -> &str {
        "FixedDelay"
    }
}

/// Builder for configuring fixed delay strategy.
#[derive(Debug)]
pub struct FixedDelayBuilder {
    delay: Duration,
    max_attempts: Option<u32>,
}

impl Default for FixedDelayBuilder {
    fn default() -> Self {
        Self {
            delay: Duration::from_secs(1),
            max_attempts: None,
        }
    }
}

impl FixedDelayBuilder {
    /// Set the delay between strategy attempts.
    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// Set the maximum number of attempts.
    pub fn max_attempts(mut self, max: Option<u32>) -> Self {
        self.max_attempts = max;
        self
    }

    /// Build the fixed delay strategy.
    pub fn build(self) -> FixedDelay {
        FixedDelay {
            delay: self.delay,
            max_attempts: self.max_attempts,
            metrics: Mutex::new(ReconnectionMetrics::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default() {
        let strategy = FixedDelay::default();
        assert_eq!(strategy.delay, Duration::from_secs(1));
        assert!(strategy.max_attempts.is_none());
    }

    #[test]
    fn test_new() {
        let strategy = FixedDelay::new(Duration::from_secs(5));
        assert_eq!(strategy.delay, Duration::from_secs(5));
        assert!(strategy.max_attempts.is_none());
    }

    #[test]
    fn test_builder() {
        let strategy = FixedDelay::builder()
            .delay(Duration::from_secs(3))
            .max_attempts(Some(10))
            .build();

        assert_eq!(strategy.delay, Duration::from_secs(3));
        assert_eq!(strategy.max_attempts, Some(10));
    }

    #[tokio::test]
    async fn test_should_reconnect_within_limit() {
        let strategy = FixedDelay::builder().max_attempts(Some(5)).build();

        let error = TransportError::connection_failed("test");

        assert!(strategy.should_reconnect(0, &error).await);
        assert!(strategy.should_reconnect(4, &error).await);
        assert!(!strategy.should_reconnect(5, &error).await);
    }

    #[tokio::test]
    async fn test_should_reconnect_unlimited() {
        let strategy = FixedDelay::default();
        let error = TransportError::connection_failed("test");

        assert!(strategy.should_reconnect(0, &error).await);
        assert!(strategy.should_reconnect(100, &error).await);
    }

    #[tokio::test]
    async fn test_should_not_reconnect_invalid_config() {
        let strategy = FixedDelay::default();
        let error = TransportError::invalid_configuration("bad config");

        assert!(!strategy.should_reconnect(0, &error).await);
    }

    #[tokio::test]
    async fn test_next_delay_constant() {
        let strategy = FixedDelay::new(Duration::from_secs(5));

        let delay0 = strategy.next_delay(0).await;
        let delay1 = strategy.next_delay(1).await;
        let delay10 = strategy.next_delay(10).await;

        assert_eq!(delay0, Duration::from_secs(5));
        assert_eq!(delay1, Duration::from_secs(5));
        assert_eq!(delay10, Duration::from_secs(5));
    }

    #[test]
    fn test_on_connected() {
        let strategy = FixedDelay::default();
        let error = TransportError::connection_failed("test");

        strategy.on_disconnected(&error);
        strategy.on_disconnected(&error);

        let metrics = strategy.metrics();
        assert_eq!(metrics.consecutive_failures, 2);

        strategy.on_connected();

        let metrics = strategy.metrics();
        assert_eq!(metrics.consecutive_failures, 0);
        assert_eq!(metrics.successful_reconnections, 1);
    }

    #[test]
    fn test_on_disconnected() {
        let strategy = FixedDelay::default();
        let error = TransportError::connection_failed("test");

        strategy.on_disconnected(&error);

        let metrics = strategy.metrics();
        assert_eq!(metrics.failed_reconnections, 1);
        assert_eq!(metrics.consecutive_failures, 1);
        assert!(metrics.last_error.is_some());
    }

    #[test]
    fn test_reset() {
        let strategy = FixedDelay::default();
        let error = TransportError::connection_failed("test");

        strategy.on_disconnected(&error);
        strategy.reset();

        let metrics = strategy.metrics();
        assert_eq!(metrics.failed_reconnections, 0);
        assert_eq!(metrics.consecutive_failures, 0);
    }

    #[test]
    fn test_name() {
        let strategy = FixedDelay::default();
        assert_eq!(strategy.name(), "FixedDelay");
    }
}

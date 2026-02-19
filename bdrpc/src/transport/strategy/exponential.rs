//! Exponential backoff strategy strategy.
//!
//! This strategy implements exponential backoff with optional jitter to prevent
//! thundering herd problems.

use super::traits::{ReconnectionMetrics, ReconnectionStrategy};
use crate::transport::TransportError;
use async_trait::async_trait;
use std::sync::Mutex;
use std::time::Duration;

/// Exponential backoff strategy strategy.
///
/// This strategy increases the delay between strategy attempts exponentially,
/// with optional jitter to prevent thundering herd problems.
///
/// # Examples
///
/// ```
/// use bdrpc::strategy::ExponentialBackoff;
/// use std::time::Duration;
///
/// // Default configuration
/// let strategy = ExponentialBackoff::default();
///
/// // Custom configuration
/// let strategy = ExponentialBackoff::builder()
///     .initial_delay(Duration::from_millis(100))
///     .max_delay(Duration::from_secs(30))
///     .multiplier(2.0)
///     .jitter(true)
///     .max_attempts(Some(10))
///     .build();
/// ```
#[derive(Debug)]
pub struct ExponentialBackoff {
    /// Initial delay before first retry
    initial_delay: Duration,
    /// Maximum delay between retries
    max_delay: Duration,
    /// Multiplier for exponential growth
    multiplier: f64,
    /// Whether to add jitter to delays
    jitter: bool,
    /// Maximum number of attempts (None = unlimited)
    max_attempts: Option<u32>,
    /// Internal metrics
    metrics: Mutex<ReconnectionMetrics>,
}

impl Default for ExponentialBackoff {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(60),
            multiplier: 2.0,
            jitter: true,
            max_attempts: None,
            metrics: Mutex::new(ReconnectionMetrics::new()),
        }
    }
}

impl ExponentialBackoff {
    /// Create a new builder for configuring exponential backoff.
    pub fn builder() -> ExponentialBackoffBuilder {
        ExponentialBackoffBuilder::default()
    }

    /// Get the current metrics.
    pub fn metrics(&self) -> ReconnectionMetrics {
        self.metrics.lock().unwrap().clone()
    }

    /// Calculate delay with optional jitter.
    fn calculate_delay(&self, attempt: u32) -> Duration {
        // Calculate base delay: initial_delay * multiplier^attempt
        let base_delay_ms =
            self.initial_delay.as_millis() as f64 * self.multiplier.powi(attempt as i32);

        let base_delay = Duration::from_millis(base_delay_ms as u64);
        let capped_delay = base_delay.min(self.max_delay);

        if self.jitter {
            // Add jitter: random value between 0 and delay
            let jitter_ms = (rand::random::<f64>() * capped_delay.as_millis() as f64) as u64;
            Duration::from_millis(jitter_ms)
        } else {
            capped_delay
        }
    }

    /// Check if the error is non-recoverable.
    fn is_non_recoverable_error(error: &TransportError) -> bool {
        // Don't reconnect for certain error types
        matches!(error, TransportError::InvalidConfiguration { .. })
    }
}

#[async_trait]
impl ReconnectionStrategy for ExponentialBackoff {
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

    async fn next_delay(&self, attempt: u32) -> Duration {
        self.calculate_delay(attempt)
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
        "ExponentialBackoff"
    }
}

/// Builder for configuring exponential backoff strategy.
#[derive(Debug)]
pub struct ExponentialBackoffBuilder {
    initial_delay: Duration,
    max_delay: Duration,
    multiplier: f64,
    jitter: bool,
    max_attempts: Option<u32>,
}

impl Default for ExponentialBackoffBuilder {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(60),
            multiplier: 2.0,
            jitter: true,
            max_attempts: None,
        }
    }
}

impl ExponentialBackoffBuilder {
    /// Set the initial delay before the first retry.
    pub fn initial_delay(mut self, delay: Duration) -> Self {
        self.initial_delay = delay;
        self
    }

    /// Set the maximum delay between retries.
    pub fn max_delay(mut self, delay: Duration) -> Self {
        self.max_delay = delay;
        self
    }

    /// Set the multiplier for exponential growth.
    pub fn multiplier(mut self, multiplier: f64) -> Self {
        self.multiplier = multiplier;
        self
    }

    /// Enable or disable jitter.
    pub fn jitter(mut self, jitter: bool) -> Self {
        self.jitter = jitter;
        self
    }

    /// Set the maximum number of attempts.
    pub fn max_attempts(mut self, max: Option<u32>) -> Self {
        self.max_attempts = max;
        self
    }

    /// Build the exponential backoff strategy.
    pub fn build(self) -> ExponentialBackoff {
        ExponentialBackoff {
            initial_delay: self.initial_delay,
            max_delay: self.max_delay,
            multiplier: self.multiplier,
            jitter: self.jitter,
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
        let strategy = ExponentialBackoff::default();
        assert_eq!(strategy.initial_delay, Duration::from_millis(100));
        assert_eq!(strategy.max_delay, Duration::from_secs(60));
        assert_eq!(strategy.multiplier, 2.0);
        assert!(strategy.jitter);
        assert!(strategy.max_attempts.is_none());
    }

    #[test]
    fn test_builder() {
        let strategy = ExponentialBackoff::builder()
            .initial_delay(Duration::from_millis(50))
            .max_delay(Duration::from_secs(30))
            .multiplier(3.0)
            .jitter(false)
            .max_attempts(Some(5))
            .build();

        assert_eq!(strategy.initial_delay, Duration::from_millis(50));
        assert_eq!(strategy.max_delay, Duration::from_secs(30));
        assert_eq!(strategy.multiplier, 3.0);
        assert!(!strategy.jitter);
        assert_eq!(strategy.max_attempts, Some(5));
    }

    #[tokio::test]
    async fn test_should_reconnect_within_limit() {
        let strategy = ExponentialBackoff::builder().max_attempts(Some(5)).build();

        let error = TransportError::connection_failed("test");

        assert!(strategy.should_reconnect(0, &error).await);
        assert!(strategy.should_reconnect(4, &error).await);
        assert!(!strategy.should_reconnect(5, &error).await);
    }

    #[tokio::test]
    async fn test_should_reconnect_unlimited() {
        let strategy = ExponentialBackoff::default();
        let error = TransportError::connection_failed("test");

        assert!(strategy.should_reconnect(0, &error).await);
        assert!(strategy.should_reconnect(100, &error).await);
    }

    #[tokio::test]
    async fn test_should_not_reconnect_invalid_config() {
        let strategy = ExponentialBackoff::default();
        let error = TransportError::invalid_configuration("bad config");

        assert!(!strategy.should_reconnect(0, &error).await);
    }

    #[tokio::test]
    async fn test_next_delay_exponential_growth() {
        let strategy = ExponentialBackoff::builder()
            .initial_delay(Duration::from_millis(100))
            .multiplier(2.0)
            .jitter(false)
            .max_delay(Duration::from_secs(10))
            .build();

        let delay0 = strategy.next_delay(0).await;
        let delay1 = strategy.next_delay(1).await;
        let delay2 = strategy.next_delay(2).await;

        assert_eq!(delay0, Duration::from_millis(100));
        assert_eq!(delay1, Duration::from_millis(200));
        assert_eq!(delay2, Duration::from_millis(400));
    }

    #[tokio::test]
    async fn test_next_delay_capped() {
        let strategy = ExponentialBackoff::builder()
            .initial_delay(Duration::from_millis(100))
            .multiplier(2.0)
            .jitter(false)
            .max_delay(Duration::from_millis(500))
            .build();

        let delay10 = strategy.next_delay(10).await;
        assert_eq!(delay10, Duration::from_millis(500));
    }

    #[test]
    fn test_on_connected() {
        let strategy = ExponentialBackoff::default();
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
        let strategy = ExponentialBackoff::default();
        let error = TransportError::connection_failed("test");

        strategy.on_disconnected(&error);

        let metrics = strategy.metrics();
        assert_eq!(metrics.failed_reconnections, 1);
        assert_eq!(metrics.consecutive_failures, 1);
        assert!(metrics.last_error.is_some());
    }

    #[test]
    fn test_reset() {
        let strategy = ExponentialBackoff::default();
        let error = TransportError::connection_failed("test");

        strategy.on_disconnected(&error);
        strategy.reset();

        let metrics = strategy.metrics();
        assert_eq!(metrics.failed_reconnections, 0);
        assert_eq!(metrics.consecutive_failures, 0);
    }

    #[test]
    fn test_name() {
        let strategy = ExponentialBackoff::default();
        assert_eq!(strategy.name(), "ExponentialBackoff");
    }
}

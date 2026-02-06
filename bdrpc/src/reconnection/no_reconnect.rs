//! No reconnection strategy.
//!
//! This strategy immediately gives up on connection failures without attempting
//! to reconnect. Useful for testing or one-shot connections.

use super::traits::{ReconnectionMetrics, ReconnectionStrategy};
use crate::transport::TransportError;
use async_trait::async_trait;
use std::sync::Mutex;
use std::time::Duration;

/// No reconnection strategy.
///
/// This strategy never attempts to reconnect after a connection failure.
/// It's useful for:
/// - Testing scenarios where reconnection should not occur
/// - One-shot connections that should fail immediately
/// - Situations where manual intervention is required
///
/// # Examples
///
/// ```
/// use bdrpc::reconnection::NoReconnect;
///
/// let strategy = NoReconnect::new();
/// ```
#[derive(Debug, Default)]
pub struct NoReconnect {
    /// Internal metrics
    metrics: Mutex<ReconnectionMetrics>,
}

impl NoReconnect {
    /// Create a new no-reconnect strategy.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the current metrics.
    pub fn metrics(&self) -> ReconnectionMetrics {
        self.metrics.lock().unwrap().clone()
    }
}

#[async_trait]
impl ReconnectionStrategy for NoReconnect {
    async fn should_reconnect(&self, _attempt: u32, _last_error: &TransportError) -> bool {
        // Never reconnect
        false
    }

    async fn next_delay(&self, _attempt: u32) -> Duration {
        // This should never be called since should_reconnect always returns false
        Duration::from_secs(0)
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
        "NoReconnect"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let strategy = NoReconnect::new();
        let metrics = strategy.metrics();
        assert_eq!(metrics.total_attempts, 0);
    }

    #[test]
    fn test_default() {
        let strategy = NoReconnect::default();
        let metrics = strategy.metrics();
        assert_eq!(metrics.total_attempts, 0);
    }

    #[tokio::test]
    async fn test_should_never_reconnect() {
        let strategy = NoReconnect::new();
        let error = TransportError::connection_failed("test");

        assert!(!strategy.should_reconnect(0, &error).await);
        assert!(!strategy.should_reconnect(1, &error).await);
        assert!(!strategy.should_reconnect(100, &error).await);
    }

    #[tokio::test]
    async fn test_should_not_reconnect_any_error() {
        let strategy = NoReconnect::new();

        let error1 = TransportError::connection_failed("test");
        let error2 = TransportError::connection_lost("test");
        let error3 = TransportError::invalid_configuration("test");

        assert!(!strategy.should_reconnect(0, &error1).await);
        assert!(!strategy.should_reconnect(0, &error2).await);
        assert!(!strategy.should_reconnect(0, &error3).await);
    }

    #[tokio::test]
    async fn test_next_delay() {
        let strategy = NoReconnect::new();
        let delay = strategy.next_delay(0).await;
        assert_eq!(delay, Duration::from_secs(0));
    }

    #[test]
    fn test_on_connected() {
        let strategy = NoReconnect::new();

        strategy.on_connected();

        let metrics = strategy.metrics();
        assert_eq!(metrics.successful_reconnections, 1);
    }

    #[test]
    fn test_on_disconnected() {
        let strategy = NoReconnect::new();
        let error = TransportError::connection_failed("test");

        strategy.on_disconnected(&error);

        let metrics = strategy.metrics();
        assert_eq!(metrics.failed_reconnections, 1);
        assert_eq!(metrics.consecutive_failures, 1);
        assert!(metrics.last_error.is_some());
    }

    #[test]
    fn test_reset() {
        let strategy = NoReconnect::new();
        let error = TransportError::connection_failed("test");

        strategy.on_disconnected(&error);
        strategy.reset();

        let metrics = strategy.metrics();
        assert_eq!(metrics.failed_reconnections, 0);
        assert_eq!(metrics.consecutive_failures, 0);
    }

    #[test]
    fn test_name() {
        let strategy = NoReconnect::new();
        assert_eq!(strategy.name(), "NoReconnect");
    }
}

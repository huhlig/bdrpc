//! Circuit breaker reconnection strategy.
//!
//! This strategy implements the circuit breaker pattern to prevent repeated
//! connection attempts when the remote service is unavailable.

use super::traits::{ReconnectionMetrics, ReconnectionStrategy};
use crate::transport::TransportError;
use async_trait::async_trait;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Circuit breaker state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Circuit is closed, allowing connection attempts
    Closed,
    /// Circuit is open, blocking connection attempts
    Open,
    /// Circuit is half-open, allowing limited connection attempts
    HalfOpen,
}

/// Circuit breaker reconnection strategy.
///
/// This strategy implements the circuit breaker pattern:
/// - **Closed**: Normal operation, attempts reconnection
/// - **Open**: After threshold failures, stops attempting for a timeout period
/// - **Half-Open**: After timeout, allows limited attempts to test recovery
///
/// # Examples
///
/// ```
/// use bdrpc::reconnection::CircuitBreaker;
/// use std::time::Duration;
///
/// // Default configuration
/// let strategy = CircuitBreaker::default();
///
/// // Custom configuration
/// let strategy = CircuitBreaker::builder()
///     .failure_threshold(5)
///     .timeout(Duration::from_secs(30))
///     .half_open_attempts(3)
///     .build();
/// ```
#[derive(Debug)]
pub struct CircuitBreaker {
    /// Number of failures before opening circuit
    failure_threshold: u32,
    /// Time to wait before entering half-open state
    timeout: Duration,
    /// Number of attempts allowed in half-open state
    half_open_attempts: u32,
    /// Internal state
    state: Mutex<CircuitBreakerState>,
    /// Internal metrics
    metrics: Mutex<ReconnectionMetrics>,
}

#[derive(Debug)]
struct CircuitBreakerState {
    /// Current circuit state
    state: CircuitState,
    /// Number of consecutive failures
    consecutive_failures: u32,
    /// Number of attempts in half-open state
    half_open_attempt_count: u32,
    /// Time when circuit was opened
    opened_at: Option<Instant>,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            timeout: Duration::from_secs(60),
            half_open_attempts: 3,
            state: Mutex::new(CircuitBreakerState {
                state: CircuitState::Closed,
                consecutive_failures: 0,
                half_open_attempt_count: 0,
                opened_at: None,
            }),
            metrics: Mutex::new(ReconnectionMetrics::new()),
        }
    }
}

impl CircuitBreaker {
    /// Create a new builder for configuring circuit breaker.
    pub fn builder() -> CircuitBreakerBuilder {
        CircuitBreakerBuilder::default()
    }

    /// Get the current circuit state.
    pub fn state(&self) -> CircuitState {
        self.state.lock().unwrap().state
    }

    /// Get the current metrics.
    pub fn metrics(&self) -> ReconnectionMetrics {
        self.metrics.lock().unwrap().clone()
    }

    /// Manually reset the circuit breaker to closed state.
    pub fn close_circuit(&self) {
        let mut state = self.state.lock().unwrap();
        state.state = CircuitState::Closed;
        state.consecutive_failures = 0;
        state.half_open_attempt_count = 0;
        state.opened_at = None;
    }

    /// Check if the error is non-recoverable.
    fn is_non_recoverable_error(error: &TransportError) -> bool {
        matches!(error, TransportError::InvalidConfiguration { .. })
    }

    /// Update circuit state based on current conditions.
    fn update_state(&self, state: &mut CircuitBreakerState) {
        match state.state {
            CircuitState::Open => {
                // Check if timeout has elapsed
                if let Some(opened_at) = state.opened_at {
                    if opened_at.elapsed() >= self.timeout {
                        state.state = CircuitState::HalfOpen;
                        state.half_open_attempt_count = 0;
                    }
                }
            }
            CircuitState::Closed => {
                // Check if we should open the circuit
                if state.consecutive_failures >= self.failure_threshold {
                    state.state = CircuitState::Open;
                    state.opened_at = Some(Instant::now());
                }
            }
            CircuitState::HalfOpen => {
                // Half-open state is managed by should_reconnect
            }
        }
    }
}

#[async_trait]
impl ReconnectionStrategy for CircuitBreaker {
    async fn should_reconnect(&self, _attempt: u32, last_error: &TransportError) -> bool {
        // Don't reconnect for non-recoverable errors
        if Self::is_non_recoverable_error(last_error) {
            return false;
        }

        let mut state = self.state.lock().unwrap();
        self.update_state(&mut state);

        match state.state {
            CircuitState::Closed => true,
            CircuitState::Open => false,
            CircuitState::HalfOpen => {
                if state.half_open_attempt_count < self.half_open_attempts {
                    state.half_open_attempt_count += 1;
                    true
                } else {
                    // Exceeded half-open attempts, go back to open
                    state.state = CircuitState::Open;
                    state.opened_at = Some(Instant::now());
                    false
                }
            }
        }
    }

    async fn next_delay(&self, _attempt: u32) -> Duration {
        // Use a short delay for circuit breaker
        Duration::from_millis(100)
    }

    fn on_connected(&self) {
        let mut state = self.state.lock().unwrap();
        state.state = CircuitState::Closed;
        state.consecutive_failures = 0;
        state.half_open_attempt_count = 0;
        state.opened_at = None;

        let mut metrics = self.metrics.lock().unwrap();
        metrics.record_success();
    }

    fn on_disconnected(&self, error: &TransportError) {
        let mut state = self.state.lock().unwrap();
        state.consecutive_failures += 1;
        self.update_state(&mut state);

        let mut metrics = self.metrics.lock().unwrap();
        metrics.record_failure(error);
    }

    fn reset(&self) {
        self.close_circuit();
        let mut metrics = self.metrics.lock().unwrap();
        metrics.reset();
    }

    fn name(&self) -> &str {
        "CircuitBreaker"
    }
}

/// Builder for configuring circuit breaker strategy.
#[derive(Debug)]
pub struct CircuitBreakerBuilder {
    failure_threshold: u32,
    timeout: Duration,
    half_open_attempts: u32,
}

impl Default for CircuitBreakerBuilder {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            timeout: Duration::from_secs(60),
            half_open_attempts: 3,
        }
    }
}

impl CircuitBreakerBuilder {
    /// Set the failure threshold before opening the circuit.
    pub fn failure_threshold(mut self, threshold: u32) -> Self {
        self.failure_threshold = threshold;
        self
    }

    /// Set the timeout before entering half-open state.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the number of attempts allowed in half-open state.
    pub fn half_open_attempts(mut self, attempts: u32) -> Self {
        self.half_open_attempts = attempts;
        self
    }

    /// Build the circuit breaker strategy.
    pub fn build(self) -> CircuitBreaker {
        CircuitBreaker {
            failure_threshold: self.failure_threshold,
            timeout: self.timeout,
            half_open_attempts: self.half_open_attempts,
            state: Mutex::new(CircuitBreakerState {
                state: CircuitState::Closed,
                consecutive_failures: 0,
                half_open_attempt_count: 0,
                opened_at: None,
            }),
            metrics: Mutex::new(ReconnectionMetrics::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default() {
        let strategy = CircuitBreaker::default();
        assert_eq!(strategy.failure_threshold, 5);
        assert_eq!(strategy.timeout, Duration::from_secs(60));
        assert_eq!(strategy.half_open_attempts, 3);
        assert_eq!(strategy.state(), CircuitState::Closed);
    }

    #[test]
    fn test_builder() {
        let strategy = CircuitBreaker::builder()
            .failure_threshold(10)
            .timeout(Duration::from_secs(30))
            .half_open_attempts(5)
            .build();

        assert_eq!(strategy.failure_threshold, 10);
        assert_eq!(strategy.timeout, Duration::from_secs(30));
        assert_eq!(strategy.half_open_attempts, 5);
    }

    #[tokio::test]
    async fn test_circuit_opens_after_threshold() {
        let strategy = CircuitBreaker::builder().failure_threshold(3).build();

        let error = TransportError::connection_failed("test");

        // First 3 failures should keep circuit closed
        assert_eq!(strategy.state(), CircuitState::Closed);
        assert!(strategy.should_reconnect(0, &error).await);
        strategy.on_disconnected(&error);

        assert_eq!(strategy.state(), CircuitState::Closed);
        assert!(strategy.should_reconnect(1, &error).await);
        strategy.on_disconnected(&error);

        assert_eq!(strategy.state(), CircuitState::Closed);
        assert!(strategy.should_reconnect(2, &error).await);
        strategy.on_disconnected(&error);

        // After threshold, circuit should open
        assert_eq!(strategy.state(), CircuitState::Open);
        assert!(!strategy.should_reconnect(3, &error).await);
    }

    #[tokio::test]
    async fn test_circuit_half_open_after_timeout() {
        let strategy = CircuitBreaker::builder()
            .failure_threshold(2)
            .timeout(Duration::from_millis(100))
            .half_open_attempts(2)
            .build();

        let error = TransportError::connection_failed("test");

        // Trigger circuit open
        strategy.on_disconnected(&error);
        strategy.on_disconnected(&error);
        assert_eq!(strategy.state(), CircuitState::Open);

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should transition to half-open
        assert!(strategy.should_reconnect(0, &error).await);
        assert_eq!(strategy.state(), CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn test_half_open_limited_attempts() {
        let strategy = CircuitBreaker::builder()
            .failure_threshold(1)
            .timeout(Duration::from_millis(10))
            .half_open_attempts(2)
            .build();

        let error = TransportError::connection_failed("test");

        // Open circuit
        strategy.on_disconnected(&error);
        assert_eq!(strategy.state(), CircuitState::Open);

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Half-open allows limited attempts
        assert!(strategy.should_reconnect(0, &error).await);
        assert!(strategy.should_reconnect(1, &error).await);
        assert!(!strategy.should_reconnect(2, &error).await);
        assert_eq!(strategy.state(), CircuitState::Open);
    }

    #[test]
    fn test_on_connected_closes_circuit() {
        let strategy = CircuitBreaker::builder().failure_threshold(2).build();

        let error = TransportError::connection_failed("test");

        // Open circuit
        strategy.on_disconnected(&error);
        strategy.on_disconnected(&error);
        assert_eq!(strategy.state(), CircuitState::Open);

        // Successful connection should close circuit
        strategy.on_connected();
        assert_eq!(strategy.state(), CircuitState::Closed);

        let metrics = strategy.metrics();
        assert_eq!(metrics.consecutive_failures, 0);
    }

    #[test]
    fn test_close_circuit_manual() {
        let strategy = CircuitBreaker::builder().failure_threshold(2).build();

        let error = TransportError::connection_failed("test");

        // Open circuit
        strategy.on_disconnected(&error);
        strategy.on_disconnected(&error);
        assert_eq!(strategy.state(), CircuitState::Open);

        // Manual close
        strategy.close_circuit();
        assert_eq!(strategy.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_should_not_reconnect_invalid_config() {
        let strategy = CircuitBreaker::default();
        let error = TransportError::invalid_configuration("bad config");

        assert!(!strategy.should_reconnect(0, &error).await);
    }

    #[test]
    fn test_reset() {
        let strategy = CircuitBreaker::builder().failure_threshold(2).build();

        let error = TransportError::connection_failed("test");

        // Open circuit
        strategy.on_disconnected(&error);
        strategy.on_disconnected(&error);
        assert_eq!(strategy.state(), CircuitState::Open);

        // Reset
        strategy.reset();
        assert_eq!(strategy.state(), CircuitState::Closed);

        let metrics = strategy.metrics();
        assert_eq!(metrics.failed_reconnections, 0);
    }

    #[test]
    fn test_name() {
        let strategy = CircuitBreaker::default();
        assert_eq!(strategy.name(), "CircuitBreaker");
    }
}

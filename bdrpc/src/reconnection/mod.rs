//! Reconnection strategies for handling connection failures.
//!
//! This module provides pluggable reconnection strategies that determine how
//! an endpoint should behave when a connection fails. Different strategies
//! are appropriate for different use cases.
//!
//! # Available Strategies
//!
//! - [`ExponentialBackoff`]: Increases delay exponentially with optional jitter (default)
//! - [`FixedDelay`]: Uses a constant delay between attempts
//! - [`CircuitBreaker`]: Implements the circuit breaker pattern
//! - [`NoReconnect`]: Never attempts to reconnect
//!
//! # Examples
//!
//! ## Using Exponential Backoff
//!
//! ```
//! use bdrpc::reconnection::ExponentialBackoff;
//! use std::time::Duration;
//!
//! let strategy = ExponentialBackoff::builder()
//!     .initial_delay(Duration::from_millis(100))
//!     .max_delay(Duration::from_secs(30))
//!     .multiplier(2.0)
//!     .jitter(true)
//!     .max_attempts(Some(10))
//!     .build();
//! ```
//!
//! ## Using Fixed Delay
//!
//! ```
//! use bdrpc::reconnection::FixedDelay;
//! use std::time::Duration;
//!
//! let strategy = FixedDelay::new(Duration::from_secs(5));
//! ```
//!
//! ## Using Circuit Breaker
//!
//! ```
//! use bdrpc::reconnection::CircuitBreaker;
//! use std::time::Duration;
//!
//! let strategy = CircuitBreaker::builder()
//!     .failure_threshold(5)
//!     .timeout(Duration::from_secs(60))
//!     .half_open_attempts(3)
//!     .build();
//! ```
//!
//! ## Disabling Reconnection
//!
//! ```
//! use bdrpc::reconnection::NoReconnect;
//!
//! let strategy = NoReconnect::new();
//! ```

mod circuit_breaker;
mod exponential;
mod fixed;
mod no_reconnect;
mod traits;

pub use circuit_breaker::{CircuitBreaker, CircuitBreakerBuilder, CircuitState};
pub use exponential::{ExponentialBackoff, ExponentialBackoffBuilder};
pub use fixed::{FixedDelay, FixedDelayBuilder};
pub use no_reconnect::NoReconnect;
pub use traits::{ReconnectionMetrics, ReconnectionStrategy};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::TransportError;

    #[tokio::test]
    async fn test_all_strategies_implement_trait() {
        let strategies: Vec<Box<dyn ReconnectionStrategy>> = vec![
            Box::new(ExponentialBackoff::default()),
            Box::new(FixedDelay::default()),
            Box::new(CircuitBreaker::default()),
            Box::new(NoReconnect::new()),
        ];

        let error = TransportError::connection_failed("test");

        for strategy in strategies {
            // All strategies should have a name
            assert!(!strategy.name().is_empty());

            // All strategies should handle callbacks
            strategy.on_disconnected(&error);
            strategy.on_connected();
            strategy.reset();
        }
    }

    #[tokio::test]
    async fn test_strategy_names_unique() {
        let exp = ExponentialBackoff::default();
        let fixed = FixedDelay::default();
        let circuit = CircuitBreaker::default();
        let no_reconnect = NoReconnect::new();

        let names = [
            exp.name(),
            fixed.name(),
            circuit.name(),
            no_reconnect.name(),
        ];

        // All names should be unique
        for (i, name1) in names.iter().enumerate() {
            for (j, name2) in names.iter().enumerate() {
                if i != j {
                    assert_ne!(name1, name2);
                }
            }
        }
    }
}

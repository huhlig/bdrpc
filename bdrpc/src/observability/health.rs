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

//! Health check support for BDRPC endpoints.
//!
//! This module provides health check functionality for monitoring endpoint status,
//! including liveness and readiness probes commonly used in container orchestration
//! systems like Kubernetes.
//!
//! # Overview
//!
//! Health checks allow monitoring systems to determine:
//! - **Liveness**: Is the endpoint alive and responsive?
//! - **Readiness**: Is the endpoint ready to accept requests?
//!
//! # Health Status
//!
//! An endpoint can be in one of three states:
//! - [`HealthStatus::Healthy`]: Fully operational
//! - [`HealthStatus::Degraded`]: Operational but with issues
//! - [`HealthStatus::Unhealthy`]: Not operational
//!
//! # Examples
//!
//! ## Basic Health Check
//!
//! ```rust
//! use bdrpc::observability::{HealthStatus, HealthCheck};
//!
//! let health = HealthCheck::new();
//! assert_eq!(health.status(), HealthStatus::Healthy);
//! assert!(health.is_alive());
//! assert!(health.is_ready());
//! ```
//!
//! ## Degraded State
//!
//! ```rust
//! use bdrpc::observability::{HealthStatus, HealthCheck};
//!
//! let mut health = HealthCheck::new();
//! health.set_degraded("High latency detected");
//!
//! assert_eq!(health.status(), HealthStatus::Degraded);
//! assert!(health.is_alive()); // Still alive
//! assert!(health.is_ready()); // Still ready
//! assert_eq!(health.message(), Some("High latency detected".to_string()));
//! ```
//!
//! ## Unhealthy State
//!
//! ```rust
//! use bdrpc::observability::{HealthStatus, HealthCheck};
//!
//! let mut health = HealthCheck::new();
//! health.set_unhealthy("Transport disconnected");
//!
//! assert_eq!(health.status(), HealthStatus::Unhealthy);
//! assert!(!health.is_alive());
//! assert!(!health.is_ready());
//! ```

use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Health status of an endpoint.
///
/// Represents the current operational state of an endpoint.
///
/// # Examples
///
/// ```rust
/// use bdrpc::observability::HealthStatus;
///
/// let status = HealthStatus::Healthy;
/// assert!(status.is_healthy());
/// assert!(status.is_operational());
///
/// let status = HealthStatus::Degraded;
/// assert!(!status.is_healthy());
/// assert!(status.is_operational());
///
/// let status = HealthStatus::Unhealthy;
/// assert!(!status.is_healthy());
/// assert!(!status.is_operational());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Endpoint is fully operational
    Healthy,
    /// Endpoint is operational but experiencing issues
    Degraded,
    /// Endpoint is not operational
    Unhealthy,
}

impl HealthStatus {
    /// Returns `true` if the status is [`Healthy`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::HealthStatus;
    ///
    /// assert!(HealthStatus::Healthy.is_healthy());
    /// assert!(!HealthStatus::Degraded.is_healthy());
    /// assert!(!HealthStatus::Unhealthy.is_healthy());
    /// ```
    #[must_use]
    pub const fn is_healthy(self) -> bool {
        matches!(self, Self::Healthy)
    }

    /// Returns `true` if the status is [`Degraded`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::HealthStatus;
    ///
    /// assert!(!HealthStatus::Healthy.is_degraded());
    /// assert!(HealthStatus::Degraded.is_degraded());
    /// assert!(!HealthStatus::Unhealthy.is_degraded());
    /// ```
    #[must_use]
    pub const fn is_degraded(self) -> bool {
        matches!(self, Self::Degraded)
    }

    /// Returns `true` if the status is [`Unhealthy`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::HealthStatus;
    ///
    /// assert!(!HealthStatus::Healthy.is_unhealthy());
    /// assert!(!HealthStatus::Degraded.is_unhealthy());
    /// assert!(HealthStatus::Unhealthy.is_unhealthy());
    /// ```
    #[must_use]
    pub const fn is_unhealthy(self) -> bool {
        matches!(self, Self::Unhealthy)
    }

    /// Returns `true` if the endpoint is operational (Healthy or Degraded).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::HealthStatus;
    ///
    /// assert!(HealthStatus::Healthy.is_operational());
    /// assert!(HealthStatus::Degraded.is_operational());
    /// assert!(!HealthStatus::Unhealthy.is_operational());
    /// ```
    #[must_use]
    pub const fn is_operational(self) -> bool {
        matches!(self, Self::Healthy | Self::Degraded)
    }
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Healthy => write!(f, "healthy"),
            Self::Degraded => write!(f, "degraded"),
            Self::Unhealthy => write!(f, "unhealthy"),
        }
    }
}

/// Internal state for health checks.
#[derive(Debug, Clone)]
struct HealthState {
    /// Current health status
    status: HealthStatus,
    /// Optional message describing the current state
    message: Option<String>,
    /// Timestamp of last status change
    last_change: Instant,
    /// Timestamp of last health check
    last_check: Instant,
}

impl Default for HealthState {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            status: HealthStatus::Healthy,
            message: None,
            last_change: now,
            last_check: now,
        }
    }
}

/// Health check for an endpoint.
///
/// Tracks the health status of an endpoint and provides liveness and readiness probes.
///
/// # Thread Safety
///
/// `HealthCheck` is thread-safe and can be shared across multiple threads using `Arc`.
///
/// # Examples
///
/// ```rust
/// use bdrpc::observability::{HealthCheck, HealthStatus};
/// use std::time::Duration;
///
/// let health = HealthCheck::new();
///
/// // Check initial state
/// assert_eq!(health.status(), HealthStatus::Healthy);
/// assert!(health.is_alive());
/// assert!(health.is_ready());
///
/// // Mark as degraded
/// health.set_degraded("High latency");
/// assert_eq!(health.status(), HealthStatus::Degraded);
/// assert!(health.is_alive());
///
/// // Mark as unhealthy
/// health.set_unhealthy("Connection lost");
/// assert_eq!(health.status(), HealthStatus::Unhealthy);
/// assert!(!health.is_alive());
///
/// // Recover
/// health.set_healthy();
/// assert_eq!(health.status(), HealthStatus::Healthy);
/// ```
#[derive(Debug, Clone)]
pub struct HealthCheck {
    state: Arc<RwLock<HealthState>>,
}

impl HealthCheck {
    /// Creates a new health check in the healthy state.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::{HealthCheck, HealthStatus};
    ///
    /// let health = HealthCheck::new();
    /// assert_eq!(health.status(), HealthStatus::Healthy);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(HealthState::default())),
        }
    }

    /// Returns the current health status.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::{HealthCheck, HealthStatus};
    ///
    /// let health = HealthCheck::new();
    /// assert_eq!(health.status(), HealthStatus::Healthy);
    /// ```
    #[must_use]
    pub fn status(&self) -> HealthStatus {
        let state = self.state.read().unwrap();
        state.status
    }

    /// Returns the current status message, if any.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::HealthCheck;
    ///
    /// let health = HealthCheck::new();
    /// assert_eq!(health.message(), None);
    ///
    /// health.set_degraded("High latency");
    /// assert_eq!(health.message(), Some("High latency".to_string()));
    /// ```
    #[must_use]
    pub fn message(&self) -> Option<String> {
        let state = self.state.read().unwrap();
        state.message.clone()
    }

    /// Returns the duration since the last status change.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::HealthCheck;
    /// use std::time::Duration;
    ///
    /// let health = HealthCheck::new();
    /// let duration = health.time_since_change();
    /// assert!(duration < Duration::from_secs(1));
    /// ```
    #[must_use]
    pub fn time_since_change(&self) -> Duration {
        let state = self.state.read().unwrap();
        state.last_change.elapsed()
    }

    /// Returns the duration since the last health check.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::HealthCheck;
    /// use std::time::Duration;
    ///
    /// let health = HealthCheck::new();
    /// let duration = health.time_since_check();
    /// assert!(duration < Duration::from_secs(1));
    /// ```
    #[must_use]
    pub fn time_since_check(&self) -> Duration {
        let state = self.state.read().unwrap();
        state.last_check.elapsed()
    }

    /// Sets the health status to healthy.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::{HealthCheck, HealthStatus};
    ///
    /// let health = HealthCheck::new();
    /// health.set_unhealthy("Error");
    /// health.set_healthy();
    /// assert_eq!(health.status(), HealthStatus::Healthy);
    /// assert_eq!(health.message(), None);
    /// ```
    pub fn set_healthy(&self) {
        self.set_status(HealthStatus::Healthy, None);
    }

    /// Sets the health status to degraded with an optional message.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::{HealthCheck, HealthStatus};
    ///
    /// let health = HealthCheck::new();
    /// health.set_degraded("High latency detected");
    /// assert_eq!(health.status(), HealthStatus::Degraded);
    /// assert_eq!(health.message(), Some("High latency detected".to_string()));
    /// ```
    pub fn set_degraded(&self, message: impl Into<String>) {
        self.set_status(HealthStatus::Degraded, Some(message.into()));
    }

    /// Sets the health status to unhealthy with an optional message.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::{HealthCheck, HealthStatus};
    ///
    /// let health = HealthCheck::new();
    /// health.set_unhealthy("Connection lost");
    /// assert_eq!(health.status(), HealthStatus::Unhealthy);
    /// assert_eq!(health.message(), Some("Connection lost".to_string()));
    /// ```
    pub fn set_unhealthy(&self, message: impl Into<String>) {
        self.set_status(HealthStatus::Unhealthy, Some(message.into()));
    }

    /// Sets the health status and message.
    fn set_status(&self, status: HealthStatus, message: Option<String>) {
        let mut state = self.state.write().unwrap();
        let changed = state.status != status;
        state.status = status;
        state.message = message;
        if changed {
            state.last_change = Instant::now();
        }
        state.last_check = Instant::now();

        #[cfg(feature = "observability")]
        {
            ::metrics::gauge!("bdrpc.health.status").set(match status {
                HealthStatus::Healthy => 2.0,
                HealthStatus::Degraded => 1.0,
                HealthStatus::Unhealthy => 0.0,
            });
        }
    }

    /// Performs a liveness check.
    ///
    /// Returns `true` if the endpoint is alive (Healthy or Degraded).
    /// Returns `false` if the endpoint is Unhealthy.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::HealthCheck;
    ///
    /// let health = HealthCheck::new();
    /// assert!(health.is_alive());
    ///
    /// health.set_degraded("Issues");
    /// assert!(health.is_alive());
    ///
    /// health.set_unhealthy("Failed");
    /// assert!(!health.is_alive());
    /// ```
    #[must_use]
    pub fn is_alive(&self) -> bool {
        let mut state = self.state.write().unwrap();
        state.last_check = Instant::now();
        state.status.is_operational()
    }

    /// Performs a readiness check.
    ///
    /// Returns `true` if the endpoint is ready to accept requests (Healthy or Degraded).
    /// Returns `false` if the endpoint is Unhealthy.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::HealthCheck;
    ///
    /// let health = HealthCheck::new();
    /// assert!(health.is_ready());
    ///
    /// health.set_degraded("Slow");
    /// assert!(health.is_ready());
    ///
    /// health.set_unhealthy("Down");
    /// assert!(!health.is_ready());
    /// ```
    #[must_use]
    pub fn is_ready(&self) -> bool {
        let mut state = self.state.write().unwrap();
        state.last_check = Instant::now();
        state.status.is_operational()
    }

    /// Returns a detailed health report.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::HealthCheck;
    ///
    /// let health = HealthCheck::new();
    /// let report = health.report();
    /// assert_eq!(report.status, bdrpc::observability::HealthStatus::Healthy);
    /// assert_eq!(report.message, None);
    /// ```
    #[must_use]
    pub fn report(&self) -> HealthReport {
        let state = self.state.read().unwrap();
        HealthReport {
            status: state.status,
            message: state.message.clone(),
            time_since_change: state.last_change.elapsed(),
            time_since_check: state.last_check.elapsed(),
        }
    }
}

impl Default for HealthCheck {
    fn default() -> Self {
        Self::new()
    }
}

/// Detailed health report.
///
/// Contains comprehensive information about the current health status.
///
/// # Examples
///
/// ```rust
/// use bdrpc::observability::{HealthCheck, HealthStatus};
///
/// let health = HealthCheck::new();
/// health.set_degraded("High latency");
///
/// let report = health.report();
/// assert_eq!(report.status, HealthStatus::Degraded);
/// assert_eq!(report.message, Some("High latency".to_string()));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    /// Current health status
    pub status: HealthStatus,
    /// Optional status message
    pub message: Option<String>,
    /// Time since last status change
    #[serde(skip)]
    pub time_since_change: Duration,
    /// Time since last health check
    #[serde(skip)]
    pub time_since_check: Duration,
}

impl HealthReport {
    /// Returns `true` if the endpoint is healthy.
    #[must_use]
    pub const fn is_healthy(&self) -> bool {
        self.status.is_healthy()
    }

    /// Returns `true` if the endpoint is operational.
    #[must_use]
    pub const fn is_operational(&self) -> bool {
        self.status.is_operational()
    }
}

impl std::fmt::Display for HealthReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "status={}", self.status)?;
        if let Some(msg) = &self.message {
            write!(f, ", message=\"{}\"", msg)?;
        }
        write!(
            f,
            ", changed={:.1}s ago, checked={:.1}s ago",
            self.time_since_change.as_secs_f64(),
            self.time_since_check.as_secs_f64()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_health_status_methods() {
        assert!(HealthStatus::Healthy.is_healthy());
        assert!(!HealthStatus::Healthy.is_degraded());
        assert!(!HealthStatus::Healthy.is_unhealthy());
        assert!(HealthStatus::Healthy.is_operational());

        assert!(!HealthStatus::Degraded.is_healthy());
        assert!(HealthStatus::Degraded.is_degraded());
        assert!(!HealthStatus::Degraded.is_unhealthy());
        assert!(HealthStatus::Degraded.is_operational());

        assert!(!HealthStatus::Unhealthy.is_healthy());
        assert!(!HealthStatus::Unhealthy.is_degraded());
        assert!(HealthStatus::Unhealthy.is_unhealthy());
        assert!(!HealthStatus::Unhealthy.is_operational());
    }

    #[test]
    fn test_health_status_display() {
        assert_eq!(HealthStatus::Healthy.to_string(), "healthy");
        assert_eq!(HealthStatus::Degraded.to_string(), "degraded");
        assert_eq!(HealthStatus::Unhealthy.to_string(), "unhealthy");
    }

    #[test]
    fn test_health_check_new() {
        let health = HealthCheck::new();
        assert_eq!(health.status(), HealthStatus::Healthy);
        assert_eq!(health.message(), None);
        assert!(health.is_alive());
        assert!(health.is_ready());
    }

    #[test]
    fn test_health_check_set_healthy() {
        let health = HealthCheck::new();
        health.set_unhealthy("Error");
        health.set_healthy();
        assert_eq!(health.status(), HealthStatus::Healthy);
        assert_eq!(health.message(), None);
    }

    #[test]
    fn test_health_check_set_degraded() {
        let health = HealthCheck::new();
        health.set_degraded("High latency");
        assert_eq!(health.status(), HealthStatus::Degraded);
        assert_eq!(health.message(), Some("High latency".to_string()));
        assert!(health.is_alive());
        assert!(health.is_ready());
    }

    #[test]
    fn test_health_check_set_unhealthy() {
        let health = HealthCheck::new();
        health.set_unhealthy("Connection lost");
        assert_eq!(health.status(), HealthStatus::Unhealthy);
        assert_eq!(health.message(), Some("Connection lost".to_string()));
        assert!(!health.is_alive());
        assert!(!health.is_ready());
    }

    #[test]
    fn test_health_check_time_tracking() {
        let health = HealthCheck::new();
        thread::sleep(Duration::from_millis(10));

        let duration = health.time_since_change();
        assert!(duration >= Duration::from_millis(10));
        assert!(duration < Duration::from_secs(1));
    }

    #[test]
    fn test_health_check_report() {
        let health = HealthCheck::new();
        health.set_degraded("Test message");

        let report = health.report();
        assert_eq!(report.status, HealthStatus::Degraded);
        assert_eq!(report.message, Some("Test message".to_string()));
        assert!(report.is_operational());
        assert!(!report.is_healthy());
    }

    #[test]
    fn test_health_report_display() {
        let health = HealthCheck::new();
        health.set_degraded("High latency");
        thread::sleep(Duration::from_millis(10));

        let report = health.report();
        let display = report.to_string();
        assert!(display.contains("status=degraded"));
        assert!(display.contains("message=\"High latency\""));
        assert!(display.contains("changed="));
        assert!(display.contains("checked="));
    }

    #[test]
    fn test_health_check_clone() {
        let health = HealthCheck::new();
        health.set_degraded("Original");

        let cloned = health.clone();
        assert_eq!(cloned.status(), HealthStatus::Degraded);
        assert_eq!(cloned.message(), Some("Original".to_string()));

        // Changes to clone affect original (shared state)
        cloned.set_healthy();
        assert_eq!(health.status(), HealthStatus::Healthy);
    }

    #[test]
    fn test_health_check_thread_safety() {
        let health = HealthCheck::new();
        let health_clone = health.clone();

        let handle = thread::spawn(move || {
            health_clone.set_degraded("Thread message");
        });

        handle.join().unwrap();
        assert_eq!(health.status(), HealthStatus::Degraded);
        assert_eq!(health.message(), Some("Thread message".to_string()));
    }

    #[test]
    fn test_liveness_probe() {
        let health = HealthCheck::new();
        assert!(health.is_alive());

        health.set_degraded("Issues");
        assert!(health.is_alive());

        health.set_unhealthy("Failed");
        assert!(!health.is_alive());
    }

    #[test]
    fn test_readiness_probe() {
        let health = HealthCheck::new();
        assert!(health.is_ready());

        health.set_degraded("Slow");
        assert!(health.is_ready());

        health.set_unhealthy("Down");
        assert!(!health.is_ready());
    }
}

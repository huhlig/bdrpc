//! Backpressure strategy traits and types.
//!
//! This module defines the core abstractions for flow control in BDRPC.
//! Backpressure is applied per-channel to prevent memory exhaustion and
//! provide fair resource allocation.

use crate::channel::ChannelId;
use std::fmt;

/// Metrics collected by backpressure strategies.
///
/// These metrics provide visibility into flow control behavior and can be
/// used for monitoring and alerting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackpressureMetrics {
    /// Current number of messages in the queue
    pub queue_depth: usize,

    /// Maximum capacity of the queue
    pub capacity: usize,

    /// Total number of messages sent
    pub messages_sent: u64,

    /// Total number of messages received
    pub messages_received: u64,

    /// Total time spent waiting for capacity (milliseconds)
    pub wait_time_ms: u64,
}

impl BackpressureMetrics {
    /// Create new metrics with zero values.
    pub fn new(capacity: usize) -> Self {
        Self {
            queue_depth: 0,
            capacity,
            messages_sent: 0,
            messages_received: 0,
            wait_time_ms: 0,
        }
    }

    /// Calculate the current utilization as a percentage (0-100).
    pub fn utilization_percent(&self) -> f64 {
        if self.capacity == 0 {
            0.0
        } else {
            (self.queue_depth as f64 / self.capacity as f64) * 100.0
        }
    }

    /// Check if the queue is at capacity.
    pub fn is_full(&self) -> bool {
        self.queue_depth >= self.capacity
    }

    /// Check if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.queue_depth == 0
    }
}

/// Strategy for applying backpressure to channel sends.
///
/// Backpressure strategies control the flow of messages through channels,
/// preventing memory exhaustion and ensuring fair resource allocation.
/// Different strategies are appropriate for different use cases.
///
/// # Examples
///
/// ```rust
/// use bdrpc::backpressure::{BackpressureStrategy, BoundedQueue};
/// use bdrpc::channel::ChannelId;
/// use std::sync::Arc;
///
/// # async fn example() {
/// let strategy: Arc<dyn BackpressureStrategy> = Arc::new(BoundedQueue::new(100));
/// let channel_id = ChannelId::new();
///
/// // Check if we can send
/// if strategy.should_send(&channel_id, 50).await {
///     // Send message
///     strategy.on_message_sent(&channel_id);
/// } else {
///     // Wait for capacity
///     strategy.wait_for_capacity(&channel_id).await;
/// }
/// # }
/// ```
#[async_trait::async_trait]
pub trait BackpressureStrategy: Send + Sync + fmt::Debug {
    /// Check if a message can be sent immediately.
    ///
    /// # Arguments
    ///
    /// * `channel_id` - The channel attempting to send
    /// * `queue_depth` - Current number of messages in the queue
    ///
    /// # Returns
    ///
    /// `true` if the message can be sent without blocking, `false` otherwise.
    async fn should_send(&self, channel_id: &ChannelId, queue_depth: usize) -> bool;

    /// Wait for capacity to become available.
    ///
    /// This method blocks until the strategy determines that a message can be sent.
    /// It should be called after `should_send()` returns `false`.
    ///
    /// # Arguments
    ///
    /// * `channel_id` - The channel waiting for capacity
    async fn wait_for_capacity(&self, channel_id: &ChannelId);

    /// Notify that a message was sent.
    ///
    /// This allows the strategy to update its internal state and metrics.
    ///
    /// # Arguments
    ///
    /// * `channel_id` - The channel that sent the message
    fn on_message_sent(&self, channel_id: &ChannelId);

    /// Notify that a message was received (frees capacity).
    ///
    /// This allows the strategy to update its internal state and potentially
    /// wake waiting senders.
    ///
    /// # Arguments
    ///
    /// * `channel_id` - The channel that received the message
    fn on_message_received(&self, channel_id: &ChannelId);

    /// Get current capacity metrics.
    ///
    /// # Returns
    ///
    /// Current metrics for monitoring and debugging.
    fn metrics(&self) -> BackpressureMetrics;

    /// Get a human-readable name for this strategy.
    ///
    /// Used for logging and debugging.
    fn name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_new() {
        let metrics = BackpressureMetrics::new(100);
        assert_eq!(metrics.capacity, 100);
        assert_eq!(metrics.queue_depth, 0);
        assert_eq!(metrics.messages_sent, 0);
        assert_eq!(metrics.messages_received, 0);
        assert_eq!(metrics.wait_time_ms, 0);
    }

    #[test]
    fn test_metrics_utilization() {
        let mut metrics = BackpressureMetrics::new(100);
        assert_eq!(metrics.utilization_percent(), 0.0);

        metrics.queue_depth = 50;
        assert_eq!(metrics.utilization_percent(), 50.0);

        metrics.queue_depth = 100;
        assert_eq!(metrics.utilization_percent(), 100.0);
    }

    #[test]
    fn test_metrics_is_full() {
        let mut metrics = BackpressureMetrics::new(100);
        assert!(!metrics.is_full());

        metrics.queue_depth = 99;
        assert!(!metrics.is_full());

        metrics.queue_depth = 100;
        assert!(metrics.is_full());

        metrics.queue_depth = 101;
        assert!(metrics.is_full());
    }

    #[test]
    fn test_metrics_is_empty() {
        let mut metrics = BackpressureMetrics::new(100);
        assert!(metrics.is_empty());

        metrics.queue_depth = 1;
        assert!(!metrics.is_empty());
    }
}

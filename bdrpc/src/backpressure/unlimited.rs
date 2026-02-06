//! Unlimited backpressure strategy.
//!
//! This strategy provides no backpressure at all. It's useful for testing
//! and trusted local connections, but can cause memory exhaustion in production.

use super::traits::{BackpressureMetrics, BackpressureStrategy};
use crate::channel::ChannelId;
use std::sync::atomic::{AtomicU64, Ordering};

/// Unlimited backpressure strategy.
///
/// This strategy never blocks sends and provides no flow control.
/// It's useful for testing scenarios and trusted local connections,
/// but should be used with caution in production as it can lead to
/// memory exhaustion.
///
/// # Warning
///
/// This strategy can cause unbounded memory growth if the receiver
/// cannot keep up with the sender. Use only in controlled environments.
///
/// # Examples
///
/// ```rust
/// use bdrpc::backpressure::{BackpressureStrategy, Unlimited};
/// use bdrpc::channel::ChannelId;
///
/// # async fn example() {
/// // Create an unlimited strategy (no backpressure)
/// let strategy = Unlimited::new();
/// let channel_id = ChannelId::new();
///
/// // Always returns true
/// assert!(strategy.should_send(&channel_id, 1000).await);
/// assert!(strategy.should_send(&channel_id, 10000).await);
///
/// // Never blocks
/// strategy.wait_for_capacity(&channel_id).await; // Returns immediately
/// # }
/// ```
#[derive(Debug, Default)]
pub struct Unlimited {
    /// Total messages sent
    messages_sent: AtomicU64,

    /// Total messages received
    messages_received: AtomicU64,
}

impl Unlimited {
    /// Create a new unlimited strategy.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::backpressure::Unlimited;
    ///
    /// let strategy = Unlimited::new();
    /// ```
    pub fn new() -> Self {
        Self {
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
        }
    }
}

#[async_trait::async_trait]
impl BackpressureStrategy for Unlimited {
    async fn should_send(&self, _channel_id: &ChannelId, _queue_depth: usize) -> bool {
        // Always allow sending
        true
    }

    async fn wait_for_capacity(&self, _channel_id: &ChannelId) {
        // Never wait - return immediately
    }

    fn on_message_sent(&self, _channel_id: &ChannelId) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
    }

    fn on_message_received(&self, _channel_id: &ChannelId) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
    }

    fn metrics(&self) -> BackpressureMetrics {
        let sent = self.messages_sent.load(Ordering::Relaxed);
        let received = self.messages_received.load(Ordering::Relaxed);

        BackpressureMetrics {
            queue_depth: sent.saturating_sub(received) as usize,
            capacity: usize::MAX,
            messages_sent: sent,
            messages_received: received,
            wait_time_ms: 0,
        }
    }

    fn name(&self) -> &str {
        "Unlimited"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let strategy = Unlimited::new();
        let metrics = strategy.metrics();
        assert_eq!(metrics.messages_sent, 0);
        assert_eq!(metrics.messages_received, 0);
    }

    #[test]
    fn test_default() {
        let strategy = Unlimited::default();
        let metrics = strategy.metrics();
        assert_eq!(metrics.messages_sent, 0);
        assert_eq!(metrics.messages_received, 0);
    }

    #[tokio::test]
    async fn test_should_send_always_true() {
        let strategy = Unlimited::new();
        let channel_id = ChannelId::new();

        // Should always return true regardless of queue depth
        assert!(strategy.should_send(&channel_id, 0).await);
        assert!(strategy.should_send(&channel_id, 100).await);
        assert!(strategy.should_send(&channel_id, 10000).await);
        assert!(strategy.should_send(&channel_id, usize::MAX).await);
    }

    #[tokio::test]
    async fn test_wait_for_capacity_immediate() {
        let strategy = Unlimited::new();
        let channel_id = ChannelId::new();

        // Should return immediately without blocking
        let start = std::time::Instant::now();
        strategy.wait_for_capacity(&channel_id).await;
        let elapsed = start.elapsed();

        // Should complete in less than 1ms
        assert!(elapsed.as_millis() < 1);
    }

    #[tokio::test]
    async fn test_send_receive_tracking() {
        let strategy = Unlimited::new();
        let channel_id = ChannelId::new();

        // Send some messages
        for _ in 0..100 {
            strategy.on_message_sent(&channel_id);
        }

        let metrics = strategy.metrics();
        assert_eq!(metrics.messages_sent, 100);
        assert_eq!(metrics.messages_received, 0);
        assert_eq!(metrics.queue_depth, 100);

        // Receive some messages
        for _ in 0..60 {
            strategy.on_message_received(&channel_id);
        }

        let metrics = strategy.metrics();
        assert_eq!(metrics.messages_sent, 100);
        assert_eq!(metrics.messages_received, 60);
        assert_eq!(metrics.queue_depth, 40);
    }

    #[tokio::test]
    async fn test_metrics() {
        let strategy = Unlimited::new();
        let channel_id = ChannelId::new();

        let metrics = strategy.metrics();
        assert_eq!(metrics.capacity, usize::MAX);
        assert_eq!(metrics.queue_depth, 0);
        assert_eq!(metrics.messages_sent, 0);
        assert_eq!(metrics.messages_received, 0);
        assert_eq!(metrics.wait_time_ms, 0);

        // Send and receive
        strategy.on_message_sent(&channel_id);
        strategy.on_message_sent(&channel_id);
        strategy.on_message_received(&channel_id);

        let metrics = strategy.metrics();
        assert_eq!(metrics.messages_sent, 2);
        assert_eq!(metrics.messages_received, 1);
        assert_eq!(metrics.queue_depth, 1);
        assert_eq!(metrics.wait_time_ms, 0);
    }

    #[test]
    fn test_name() {
        let strategy = Unlimited::new();
        assert_eq!(strategy.name(), "Unlimited");
    }
}

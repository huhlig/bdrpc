//! Bounded queue backpressure strategy.
//!
//! This is the default and most commonly used backpressure strategy.
//! It provides simple bounded channel semantics with blocking when full.

use super::traits::{BackpressureMetrics, BackpressureStrategy};
use crate::channel::ChannelId;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;
use tokio::sync::Notify;

/// Bounded queue backpressure strategy.
///
/// This strategy maintains a simple bounded queue with a fixed capacity.
/// When the queue is full, senders block until space becomes available.
/// This is the default strategy and works well for most use cases.
///
/// # Examples
///
/// ```rust
/// use bdrpc::backpressure::{BackpressureStrategy, BoundedQueue};
/// use bdrpc::channel::ChannelId;
///
/// # async fn example() {
/// // Create a bounded queue with capacity of 100 messages
/// let strategy = BoundedQueue::new(100);
/// let channel_id = ChannelId::new();
///
/// // Check if we can send
/// if strategy.should_send(&channel_id, 50).await {
///     println!("Can send immediately");
///     strategy.on_message_sent(&channel_id);
/// } else {
///     println!("Queue full, waiting...");
///     strategy.wait_for_capacity(&channel_id).await;
///     strategy.on_message_sent(&channel_id);
/// }
///
/// // Receive a message to free capacity
/// strategy.on_message_received(&channel_id);
/// # }
/// ```
#[derive(Debug)]
pub struct BoundedQueue {
    /// Maximum capacity of the queue
    capacity: usize,

    /// Current number of messages in the queue
    current_depth: AtomicUsize,

    /// Total messages sent
    messages_sent: AtomicU64,

    /// Total messages received
    messages_received: AtomicU64,

    /// Total wait time in milliseconds
    wait_time_ms: AtomicU64,

    /// Notifier for waiting senders
    notify: Arc<Notify>,
}

impl BoundedQueue {
    /// Create a new bounded queue strategy with the specified capacity.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of messages that can be queued
    ///
    /// # Panics
    ///
    /// Panics if capacity is 0.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::backpressure::BoundedQueue;
    ///
    /// let strategy = BoundedQueue::new(100);
    /// ```
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "Capacity must be greater than 0");

        Self {
            capacity,
            current_depth: AtomicUsize::new(0),
            messages_sent: AtomicU64::new(0),
            messages_received: AtomicU64::new(0),
            wait_time_ms: AtomicU64::new(0),
            notify: Arc::new(Notify::new()),
        }
    }

    /// Get the current queue depth.
    pub fn queue_depth(&self) -> usize {
        self.current_depth.load(Ordering::Relaxed)
    }

    /// Get the capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

#[async_trait::async_trait]
impl BackpressureStrategy for BoundedQueue {
    async fn should_send(&self, _channel_id: &ChannelId, queue_depth: usize) -> bool {
        queue_depth < self.capacity
    }

    async fn wait_for_capacity(&self, _channel_id: &ChannelId) {
        let start = Instant::now();

        loop {
            // Check if there's capacity now
            let depth = self.current_depth.load(Ordering::Acquire);
            if depth < self.capacity {
                // Record wait time
                let elapsed = start.elapsed().as_millis() as u64;
                self.wait_time_ms.fetch_add(elapsed, Ordering::Relaxed);
                return;
            }

            // Wait for notification
            self.notify.notified().await;
        }
    }

    fn on_message_sent(&self, _channel_id: &ChannelId) {
        self.current_depth.fetch_add(1, Ordering::Release);
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
    }

    fn on_message_received(&self, _channel_id: &ChannelId) {
        let prev = self.current_depth.fetch_sub(1, Ordering::Release);
        self.messages_received.fetch_add(1, Ordering::Relaxed);

        // If we were at capacity, notify waiting senders
        if prev >= self.capacity {
            self.notify.notify_one();
        }
    }

    fn metrics(&self) -> BackpressureMetrics {
        BackpressureMetrics {
            queue_depth: self.current_depth.load(Ordering::Relaxed),
            capacity: self.capacity,
            messages_sent: self.messages_sent.load(Ordering::Relaxed),
            messages_received: self.messages_received.load(Ordering::Relaxed),
            wait_time_ms: self.wait_time_ms.load(Ordering::Relaxed),
        }
    }

    fn name(&self) -> &str {
        "BoundedQueue"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let strategy = BoundedQueue::new(100);
        assert_eq!(strategy.capacity(), 100);
        assert_eq!(strategy.queue_depth(), 0);
    }

    #[test]
    #[should_panic(expected = "Capacity must be greater than 0")]
    fn test_new_zero_capacity() {
        BoundedQueue::new(0);
    }

    #[tokio::test]
    async fn test_should_send() {
        let strategy = BoundedQueue::new(10);
        let channel_id = ChannelId::new();

        // Should be able to send when under capacity
        assert!(strategy.should_send(&channel_id, 0).await);
        assert!(strategy.should_send(&channel_id, 5).await);
        assert!(strategy.should_send(&channel_id, 9).await);

        // Should not be able to send at capacity
        assert!(!strategy.should_send(&channel_id, 10).await);
        assert!(!strategy.should_send(&channel_id, 11).await);
    }

    #[tokio::test]
    async fn test_send_receive_cycle() {
        let strategy = BoundedQueue::new(10);
        let channel_id = ChannelId::new();

        // Send some messages
        for _ in 0..5 {
            strategy.on_message_sent(&channel_id);
        }

        assert_eq!(strategy.queue_depth(), 5);
        let metrics = strategy.metrics();
        assert_eq!(metrics.messages_sent, 5);
        assert_eq!(metrics.messages_received, 0);

        // Receive some messages
        for _ in 0..3 {
            strategy.on_message_received(&channel_id);
        }

        assert_eq!(strategy.queue_depth(), 2);
        let metrics = strategy.metrics();
        assert_eq!(metrics.messages_sent, 5);
        assert_eq!(metrics.messages_received, 3);
    }

    #[tokio::test]
    async fn test_wait_for_capacity() {
        let strategy = Arc::new(BoundedQueue::new(2));
        let channel_id = ChannelId::new();

        // Fill the queue
        strategy.on_message_sent(&channel_id);
        strategy.on_message_sent(&channel_id);
        assert_eq!(strategy.queue_depth(), 2);

        // Spawn a task that waits for capacity
        let strategy_clone = strategy.clone();
        let channel_id_clone = channel_id;
        let waiter = tokio::spawn(async move {
            strategy_clone.wait_for_capacity(&channel_id_clone).await;
            strategy_clone.on_message_sent(&channel_id_clone);
        });

        // Give the waiter time to start waiting
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Free up capacity
        strategy.on_message_received(&channel_id);

        // Wait for the waiter to complete
        waiter.await.unwrap();

        // Should have 2 messages again (2 sent, 1 received, 1 sent)
        assert_eq!(strategy.queue_depth(), 2);
    }

    #[tokio::test]
    async fn test_metrics() {
        let strategy = BoundedQueue::new(100);
        let channel_id = ChannelId::new();

        let metrics = strategy.metrics();
        assert_eq!(metrics.capacity, 100);
        assert_eq!(metrics.queue_depth, 0);
        assert_eq!(metrics.messages_sent, 0);
        assert_eq!(metrics.messages_received, 0);

        // Send and receive some messages
        for _ in 0..10 {
            strategy.on_message_sent(&channel_id);
        }
        for _ in 0..5 {
            strategy.on_message_received(&channel_id);
        }

        let metrics = strategy.metrics();
        assert_eq!(metrics.queue_depth, 5);
        assert_eq!(metrics.messages_sent, 10);
        assert_eq!(metrics.messages_received, 5);
    }

    #[test]
    fn test_name() {
        let strategy = BoundedQueue::new(100);
        assert_eq!(strategy.name(), "BoundedQueue");
    }
}

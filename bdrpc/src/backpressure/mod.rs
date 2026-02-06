//! Backpressure and flow control strategies.
//!
//! This module provides pluggable backpressure strategies for controlling
//! the flow of messages through channels. Different strategies are appropriate
//! for different use cases.
//!
//! # Overview
//!
//! Backpressure is applied per-channel to prevent memory exhaustion and ensure
//! fair resource allocation. Each channel can have its own flow control strategy
//! based on its protocol's needs.
//!
//! # Built-in Strategies
//!
//! - [`BoundedQueue`]: Simple bounded channel with blocking (default)
//! - [`Unlimited`]: No backpressure (testing only)
//!
//! # Examples
//!
//! ```rust
//! use bdrpc::backpressure::{BackpressureStrategy, BoundedQueue};
//! use bdrpc::channel::ChannelId;
//! use std::sync::Arc;
//!
//! # async fn example() {
//! // Create a bounded queue strategy
//! let strategy: Arc<dyn BackpressureStrategy> = Arc::new(BoundedQueue::new(100));
//! let channel_id = ChannelId::new();
//!
//! // Check if we can send
//! if strategy.should_send(&channel_id, 50).await {
//!     // Send message
//!     strategy.on_message_sent(&channel_id);
//! } else {
//!     // Wait for capacity
//!     strategy.wait_for_capacity(&channel_id).await;
//!     strategy.on_message_sent(&channel_id);
//! }
//!
//! // Get metrics
//! let metrics = strategy.metrics();
//! println!("Queue depth: {}/{}", metrics.queue_depth, metrics.capacity);
//! # }
//! ```

mod bounded_queue;
mod traits;
mod unlimited;

pub use bounded_queue::BoundedQueue;
pub use traits::{BackpressureMetrics, BackpressureStrategy};
pub use unlimited::Unlimited;

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

//! Comprehensive metrics for BDRPC components.
//!
//! This module provides metrics tracking for transports, channels, and overall
//! system performance. Metrics are collected using atomic counters for thread-safe
//! operation and can be exported to the `metrics` crate when the `metrics` feature
//! is enabled.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Metrics for transport layer operations.
///
/// Tracks connection lifecycle, types transfer, and transport-level errors.
///
/// # Examples
///
/// ```rust
/// use bdrpc::observability::TransportMetrics;
///
/// let metrics = TransportMetrics::new();
/// metrics.record_connection_opened();
/// metrics.record_bytes_sent(1024);
/// metrics.record_bytes_received(512);
///
/// assert_eq!(metrics.active_connections(), 1);
/// assert_eq!(metrics.total_bytes_sent(), 1024);
/// assert_eq!(metrics.total_bytes_received(), 512);
/// ```
#[derive(Debug, Default)]
pub struct TransportMetrics {
    /// Total number of connections opened
    connections_opened: AtomicU64,
    /// Total number of connections closed
    connections_closed: AtomicU64,
    /// Total bytes sent across all transports
    bytes_sent: AtomicU64,
    /// Total bytes received across all transports
    bytes_received: AtomicU64,
    /// Total number of connection errors
    connection_errors: AtomicU64,
    /// Total number of read errors
    read_errors: AtomicU64,
    /// Total number of write errors
    write_errors: AtomicU64,
}

impl TransportMetrics {
    /// Creates a new transport metrics tracker.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::TransportMetrics;
    ///
    /// let metrics = TransportMetrics::new();
    /// assert_eq!(metrics.active_connections(), 0);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a new connection being opened.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::TransportMetrics;
    ///
    /// let metrics = TransportMetrics::new();
    /// metrics.record_connection_opened();
    /// assert_eq!(metrics.active_connections(), 1);
    /// ```
    pub fn record_connection_opened(&self) {
        self.connections_opened.fetch_add(1, Ordering::Relaxed);
        #[cfg(feature = "observability")]
        {
            metrics::counter!("bdrpc.transport.connections.opened").increment(1);
            metrics::gauge!("bdrpc.transport.connections.active").increment(1.0);
        }
    }

    /// Records a connection being closed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::TransportMetrics;
    ///
    /// let metrics = TransportMetrics::new();
    /// metrics.record_connection_opened();
    /// metrics.record_connection_closed();
    /// assert_eq!(metrics.active_connections(), 0);
    /// ```
    pub fn record_connection_closed(&self) {
        self.connections_closed.fetch_add(1, Ordering::Relaxed);
        #[cfg(feature = "observability")]
        {
            metrics::counter!("bdrpc.transport.connections.closed").increment(1);
            metrics::gauge!("bdrpc.transport.connections.active").decrement(1.0);
        }
    }

    /// Records bytes sent over a transport.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::TransportMetrics;
    ///
    /// let metrics = TransportMetrics::new();
    /// metrics.record_bytes_sent(1024);
    /// assert_eq!(metrics.total_bytes_sent(), 1024);
    /// ```
    pub fn record_bytes_sent(&self, bytes: u64) {
        self.bytes_sent.fetch_add(bytes, Ordering::Relaxed);
        #[cfg(feature = "observability")]
        metrics::counter!("bdrpc.transport.bytes.sent").increment(bytes);
    }

    /// Records bytes received over a transport.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::TransportMetrics;
    ///
    /// let metrics = TransportMetrics::new();
    /// metrics.record_bytes_received(512);
    /// assert_eq!(metrics.total_bytes_received(), 512);
    /// ```
    pub fn record_bytes_received(&self, bytes: u64) {
        self.bytes_received.fetch_add(bytes, Ordering::Relaxed);
        #[cfg(feature = "observability")]
        metrics::counter!("bdrpc.transport.bytes.received").increment(bytes);
    }

    /// Records a connection error.
    pub fn record_connection_error(&self) {
        self.connection_errors.fetch_add(1, Ordering::Relaxed);
        #[cfg(feature = "observability")]
        metrics::counter!("bdrpc.transport.errors.connection").increment(1);
    }

    /// Records a read error.
    pub fn record_read_error(&self) {
        self.read_errors.fetch_add(1, Ordering::Relaxed);
        #[cfg(feature = "observability")]
        metrics::counter!("bdrpc.transport.errors.read").increment(1);
    }

    /// Records a write error.
    pub fn record_write_error(&self) {
        self.write_errors.fetch_add(1, Ordering::Relaxed);
        #[cfg(feature = "observability")]
        metrics::counter!("bdrpc.transport.errors.write").increment(1);
    }

    /// Returns the number of currently active connections.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::TransportMetrics;
    ///
    /// let metrics = TransportMetrics::new();
    /// metrics.record_connection_opened();
    /// metrics.record_connection_opened();
    /// metrics.record_connection_closed();
    /// assert_eq!(metrics.active_connections(), 1);
    /// ```
    #[must_use]
    pub fn active_connections(&self) -> u64 {
        let opened = self.connections_opened.load(Ordering::Relaxed);
        let closed = self.connections_closed.load(Ordering::Relaxed);
        opened.saturating_sub(closed)
    }

    /// Returns the total number of connections opened.
    #[must_use]
    pub fn total_connections_opened(&self) -> u64 {
        self.connections_opened.load(Ordering::Relaxed)
    }

    /// Returns the total number of connections closed.
    #[must_use]
    pub fn total_connections_closed(&self) -> u64 {
        self.connections_closed.load(Ordering::Relaxed)
    }

    /// Returns the total bytes sent.
    #[must_use]
    pub fn total_bytes_sent(&self) -> u64 {
        self.bytes_sent.load(Ordering::Relaxed)
    }

    /// Returns the total bytes received.
    #[must_use]
    pub fn total_bytes_received(&self) -> u64 {
        self.bytes_received.load(Ordering::Relaxed)
    }

    /// Returns the total number of connection errors.
    #[must_use]
    pub fn total_connection_errors(&self) -> u64 {
        self.connection_errors.load(Ordering::Relaxed)
    }

    /// Returns the total number of read errors.
    #[must_use]
    pub fn total_read_errors(&self) -> u64 {
        self.read_errors.load(Ordering::Relaxed)
    }

    /// Returns the total number of write errors.
    #[must_use]
    pub fn total_write_errors(&self) -> u64 {
        self.write_errors.load(Ordering::Relaxed)
    }

    /// Resets all counters to zero.
    pub fn reset(&self) {
        self.connections_opened.store(0, Ordering::Relaxed);
        self.connections_closed.store(0, Ordering::Relaxed);
        self.bytes_sent.store(0, Ordering::Relaxed);
        self.bytes_received.store(0, Ordering::Relaxed);
        self.connection_errors.store(0, Ordering::Relaxed);
        self.read_errors.store(0, Ordering::Relaxed);
        self.write_errors.store(0, Ordering::Relaxed);
    }
}

/// Metrics for channel layer operations.
///
/// Tracks message flow, latency, and channel-level errors.
///
/// # Examples
///
/// ```rust
/// use bdrpc::observability::ChannelMetrics;
/// use std::time::Duration;
///
/// let metrics = ChannelMetrics::new();
/// metrics.record_channel_created();
/// metrics.record_message_sent();
/// metrics.record_message_received();
/// metrics.record_latency(Duration::from_millis(10));
///
/// assert_eq!(metrics.active_channels(), 1);
/// assert_eq!(metrics.total_messages_sent(), 1);
/// assert_eq!(metrics.total_messages_received(), 1);
/// ```
#[derive(Debug, Default)]
pub struct ChannelMetrics {
    /// Total number of channels created
    channels_created: AtomicU64,
    /// Total number of channels closed
    channels_closed: AtomicU64,
    /// Total messages sent across all channels
    messages_sent: AtomicU64,
    /// Total messages received across all channels
    messages_received: AtomicU64,
    /// Total number of send errors
    send_errors: AtomicU64,
    /// Total number of receive errors
    receive_errors: AtomicU64,
    /// Sum of all message latencies in microseconds
    total_latency_us: AtomicU64,
    /// Number of latency measurements
    latency_count: AtomicU64,
}

impl ChannelMetrics {
    /// Creates a new channel metrics tracker.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::ChannelMetrics;
    ///
    /// let metrics = ChannelMetrics::new();
    /// assert_eq!(metrics.active_channels(), 0);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a new channel being created.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::ChannelMetrics;
    ///
    /// let metrics = ChannelMetrics::new();
    /// metrics.record_channel_created();
    /// assert_eq!(metrics.active_channels(), 1);
    /// ```
    pub fn record_channel_created(&self) {
        self.channels_created.fetch_add(1, Ordering::Relaxed);
        #[cfg(feature = "observability")]
        {
            metrics::counter!("bdrpc.channel.channels.created").increment(1);
            metrics::gauge!("bdrpc.channel.channels.active").increment(1.0);
        }
    }

    /// Records a channel being closed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::ChannelMetrics;
    ///
    /// let metrics = ChannelMetrics::new();
    /// metrics.record_channel_created();
    /// metrics.record_channel_closed();
    /// assert_eq!(metrics.active_channels(), 0);
    /// ```
    pub fn record_channel_closed(&self) {
        self.channels_closed.fetch_add(1, Ordering::Relaxed);
        #[cfg(feature = "observability")]
        {
            metrics::counter!("bdrpc.channel.channels.closed").increment(1);
            metrics::gauge!("bdrpc.channel.channels.active").decrement(1.0);
        }
    }

    /// Records a message being sent.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::ChannelMetrics;
    ///
    /// let metrics = ChannelMetrics::new();
    /// metrics.record_message_sent();
    /// assert_eq!(metrics.total_messages_sent(), 1);
    /// ```
    pub fn record_message_sent(&self) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
        #[cfg(feature = "observability")]
        metrics::counter!("bdrpc.channel.messages.sent").increment(1);
    }

    /// Records a message being received.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::ChannelMetrics;
    ///
    /// let metrics = ChannelMetrics::new();
    /// metrics.record_message_received();
    /// assert_eq!(metrics.total_messages_received(), 1);
    /// ```
    pub fn record_message_received(&self) {
        self.messages_received.fetch_add(1, Ordering::Relaxed);
        #[cfg(feature = "observability")]
        metrics::counter!("bdrpc.channel.messages.received").increment(1);
    }

    /// Records a send error.
    pub fn record_send_error(&self) {
        self.send_errors.fetch_add(1, Ordering::Relaxed);
        #[cfg(feature = "observability")]
        metrics::counter!("bdrpc.channel.errors.send").increment(1);
    }

    /// Records a receive error.
    pub fn record_receive_error(&self) {
        self.receive_errors.fetch_add(1, Ordering::Relaxed);
        #[cfg(feature = "observability")]
        metrics::counter!("bdrpc.channel.errors.receive").increment(1);
    }

    /// Records message latency.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::ChannelMetrics;
    /// use std::time::Duration;
    ///
    /// let metrics = ChannelMetrics::new();
    /// metrics.record_latency(Duration::from_millis(10));
    /// metrics.record_latency(Duration::from_millis(20));
    /// assert_eq!(metrics.average_latency_us(), 15_000);
    /// ```
    pub fn record_latency(&self, latency: Duration) {
        let us = latency.as_micros() as u64;
        self.total_latency_us.fetch_add(us, Ordering::Relaxed);
        self.latency_count.fetch_add(1, Ordering::Relaxed);
        #[cfg(feature = "observability")]
        metrics::histogram!("bdrpc.channel.latency.us").record(us as f64);
    }

    /// Returns the number of currently active channels.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::ChannelMetrics;
    ///
    /// let metrics = ChannelMetrics::new();
    /// metrics.record_channel_created();
    /// metrics.record_channel_created();
    /// metrics.record_channel_closed();
    /// assert_eq!(metrics.active_channels(), 1);
    /// ```
    #[must_use]
    pub fn active_channels(&self) -> u64 {
        let created = self.channels_created.load(Ordering::Relaxed);
        let closed = self.channels_closed.load(Ordering::Relaxed);
        created.saturating_sub(closed)
    }

    /// Returns the total number of channels created.
    #[must_use]
    pub fn total_channels_created(&self) -> u64 {
        self.channels_created.load(Ordering::Relaxed)
    }

    /// Returns the total number of channels closed.
    #[must_use]
    pub fn total_channels_closed(&self) -> u64 {
        self.channels_closed.load(Ordering::Relaxed)
    }

    /// Returns the total messages sent.
    #[must_use]
    pub fn total_messages_sent(&self) -> u64 {
        self.messages_sent.load(Ordering::Relaxed)
    }

    /// Returns the total messages received.
    #[must_use]
    pub fn total_messages_received(&self) -> u64 {
        self.messages_received.load(Ordering::Relaxed)
    }

    /// Returns the total number of send errors.
    #[must_use]
    pub fn total_send_errors(&self) -> u64 {
        self.send_errors.load(Ordering::Relaxed)
    }

    /// Returns the total number of receive errors.
    #[must_use]
    pub fn total_receive_errors(&self) -> u64 {
        self.receive_errors.load(Ordering::Relaxed)
    }

    /// Returns the average message latency in microseconds.
    ///
    /// Returns 0 if no latency measurements have been recorded.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::ChannelMetrics;
    /// use std::time::Duration;
    ///
    /// let metrics = ChannelMetrics::new();
    /// metrics.record_latency(Duration::from_millis(10));
    /// metrics.record_latency(Duration::from_millis(20));
    /// assert_eq!(metrics.average_latency_us(), 15_000);
    /// ```
    #[must_use]
    pub fn average_latency_us(&self) -> u64 {
        let total = self.total_latency_us.load(Ordering::Relaxed);
        let count = self.latency_count.load(Ordering::Relaxed);
        if count == 0 { 0 } else { total / count }
    }

    /// Returns the average message latency as a Duration.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::ChannelMetrics;
    /// use std::time::Duration;
    ///
    /// let metrics = ChannelMetrics::new();
    /// metrics.record_latency(Duration::from_millis(10));
    /// assert_eq!(metrics.average_latency(), Duration::from_millis(10));
    /// ```
    #[must_use]
    pub fn average_latency(&self) -> Duration {
        Duration::from_micros(self.average_latency_us())
    }

    /// Resets all counters to zero.
    pub fn reset(&self) {
        self.channels_created.store(0, Ordering::Relaxed);
        self.channels_closed.store(0, Ordering::Relaxed);
        self.messages_sent.store(0, Ordering::Relaxed);
        self.messages_received.store(0, Ordering::Relaxed);
        self.send_errors.store(0, Ordering::Relaxed);
        self.receive_errors.store(0, Ordering::Relaxed);
        self.total_latency_us.store(0, Ordering::Relaxed);
        self.latency_count.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_metrics_new() {
        let metrics = TransportMetrics::new();
        assert_eq!(metrics.active_connections(), 0);
        assert_eq!(metrics.total_bytes_sent(), 0);
        assert_eq!(metrics.total_bytes_received(), 0);
    }

    #[test]
    fn test_transport_metrics_connections() {
        let metrics = TransportMetrics::new();
        metrics.record_connection_opened();
        metrics.record_connection_opened();
        assert_eq!(metrics.active_connections(), 2);
        assert_eq!(metrics.total_connections_opened(), 2);

        metrics.record_connection_closed();
        assert_eq!(metrics.active_connections(), 1);
        assert_eq!(metrics.total_connections_closed(), 1);
    }

    #[test]
    fn test_transport_metrics_bytes() {
        let metrics = TransportMetrics::new();
        metrics.record_bytes_sent(1024);
        metrics.record_bytes_sent(512);
        assert_eq!(metrics.total_bytes_sent(), 1536);

        metrics.record_bytes_received(256);
        assert_eq!(metrics.total_bytes_received(), 256);
    }

    #[test]
    fn test_transport_metrics_errors() {
        let metrics = TransportMetrics::new();
        metrics.record_connection_error();
        metrics.record_read_error();
        metrics.record_write_error();

        assert_eq!(metrics.total_connection_errors(), 1);
        assert_eq!(metrics.total_read_errors(), 1);
        assert_eq!(metrics.total_write_errors(), 1);
    }

    #[test]
    fn test_transport_metrics_reset() {
        let metrics = TransportMetrics::new();
        metrics.record_connection_opened();
        metrics.record_bytes_sent(1024);

        metrics.reset();
        assert_eq!(metrics.active_connections(), 0);
        assert_eq!(metrics.total_bytes_sent(), 0);
    }

    #[test]
    fn test_channel_metrics_new() {
        let metrics = ChannelMetrics::new();
        assert_eq!(metrics.active_channels(), 0);
        assert_eq!(metrics.total_messages_sent(), 0);
        assert_eq!(metrics.total_messages_received(), 0);
    }

    #[test]
    fn test_channel_metrics_channels() {
        let metrics = ChannelMetrics::new();
        metrics.record_channel_created();
        metrics.record_channel_created();
        assert_eq!(metrics.active_channels(), 2);
        assert_eq!(metrics.total_channels_created(), 2);

        metrics.record_channel_closed();
        assert_eq!(metrics.active_channels(), 1);
        assert_eq!(metrics.total_channels_closed(), 1);
    }

    #[test]
    fn test_channel_metrics_messages() {
        let metrics = ChannelMetrics::new();
        metrics.record_message_sent();
        metrics.record_message_sent();
        assert_eq!(metrics.total_messages_sent(), 2);

        metrics.record_message_received();
        assert_eq!(metrics.total_messages_received(), 1);
    }

    #[test]
    fn test_channel_metrics_errors() {
        let metrics = ChannelMetrics::new();
        metrics.record_send_error();
        metrics.record_receive_error();

        assert_eq!(metrics.total_send_errors(), 1);
        assert_eq!(metrics.total_receive_errors(), 1);
    }

    #[test]
    fn test_channel_metrics_latency() {
        let metrics = ChannelMetrics::new();
        metrics.record_latency(Duration::from_millis(10));
        metrics.record_latency(Duration::from_millis(20));

        assert_eq!(metrics.average_latency_us(), 15_000);
        assert_eq!(metrics.average_latency(), Duration::from_millis(15));
    }

    #[test]
    fn test_channel_metrics_latency_zero() {
        let metrics = ChannelMetrics::new();
        assert_eq!(metrics.average_latency_us(), 0);
        assert_eq!(metrics.average_latency(), Duration::ZERO);
    }

    #[test]
    fn test_channel_metrics_reset() {
        let metrics = ChannelMetrics::new();
        metrics.record_channel_created();
        metrics.record_message_sent();
        metrics.record_latency(Duration::from_millis(10));

        metrics.reset();
        assert_eq!(metrics.active_channels(), 0);
        assert_eq!(metrics.total_messages_sent(), 0);
        assert_eq!(metrics.average_latency_us(), 0);
    }
}

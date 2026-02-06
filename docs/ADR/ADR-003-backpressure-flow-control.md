# ADR-003: Backpressure and Flow Control

## Status
Accepted

## Context
Bi-directional RPC systems face unique backpressure challenges:
- Both sides can be producers and consumers simultaneously
- Different protocols have different throughput requirements
- Slow consumers can cause memory buildup
- Fast producers can overwhelm receivers
- Multiple channels share transport bandwidth

We need a flexible, pluggable approach to flow control that works at the channel level.

## Decision

### Core Principle
**Backpressure is applied per-channel, not per-transport.** Each channel can have its own flow control strategy based on its protocol's needs.

### Pluggable Strategy Pattern

```rust
#[async_trait]
trait BackpressureStrategy: Send + Sync {
    /// Check if a message can be sent immediately
    async fn should_send(&self, channel_id: &ChannelId, queue_depth: usize) -> bool;
    
    /// Wait for capacity to become available
    async fn wait_for_capacity(&self, channel_id: &ChannelId);
    
    /// Notify that a message was sent
    fn on_message_sent(&self, channel_id: &ChannelId);
    
    /// Notify that a message was received (frees capacity)
    fn on_message_received(&self, channel_id: &ChannelId);
    
    /// Get current capacity metrics
    fn metrics(&self) -> BackpressureMetrics;
}

struct BackpressureMetrics {
    queue_depth: usize,
    capacity: usize,
    messages_sent: u64,
    messages_received: u64,
    wait_time_ms: u64,
}
```

### Built-in Strategies

#### 1. BoundedQueue (Default)
```rust
struct BoundedQueue {
    capacity: usize,
    current_depth: AtomicUsize,
    notify: Notify,
}
```
- Simple bounded channel semantics
- Blocks when queue is full
- Wakes waiting senders when space available
- Good for most use cases

#### 2. TokenBucket
```rust
struct TokenBucket {
    capacity: usize,
    tokens: AtomicUsize,
    refill_rate: Duration,
    burst_size: usize,
}
```
- Rate limiting with burst capacity
- Tokens refill over time
- Allows bursts up to burst_size
- Good for rate-limited APIs

#### 3. SlidingWindow
```rust
struct SlidingWindow {
    window_size: usize,
    in_flight: AtomicUsize,
    pending_acks: DashMap<MessageId, Instant>,
}
```
- Credit-based flow control (like HTTP/2)
- Receiver grants credits
- Sender tracks in-flight messages
- Good for high-throughput scenarios

#### 4. AdaptiveWindow
```rust
struct AdaptiveWindow {
    min_window: usize,
    max_window: usize,
    current_window: AtomicUsize,
    rtt_estimator: RttEstimator,
    congestion_detector: CongestionDetector,
}
```
- Dynamically adjusts window based on RTT and throughput
- Increases window when network is healthy
- Decreases on congestion signals
- Good for variable network conditions

#### 5. Priority
```rust
struct Priority {
    high_capacity: usize,
    normal_capacity: usize,
    low_capacity: usize,
    queues: [BoundedQueue; 3],
}
```
- Different limits for different priorities
- High-priority messages bypass normal backpressure
- Good for systems with critical vs. best-effort traffic

#### 6. Unlimited
```rust
struct Unlimited;
```
- No backpressure (dangerous!)
- Useful for testing or trusted local connections
- Can cause memory exhaustion

### Integration with Channels

```rust
struct Channel<P: Protocol> {
    id: ChannelId,
    backpressure: Arc<dyn BackpressureStrategy>,
    sender: mpsc::Sender<P>,
    receiver: mpsc::Receiver<P>,
}

impl<P: Protocol> Channel<P> {
    async fn send(&self, message: P) -> Result<()> {
        // Check backpressure
        if !self.backpressure.should_send(&self.id, self.queue_depth()).await {
            self.backpressure.wait_for_capacity(&self.id).await;
        }
        
        // Send message
        self.sender.send(message).await?;
        self.backpressure.on_message_sent(&self.id);
        
        Ok(())
    }
    
    async fn recv(&self) -> Option<P> {
        let message = self.receiver.recv().await?;
        self.backpressure.on_message_received(&self.id);
        Some(message)
    }
}
```

### Channel Configuration

```rust
struct ChannelConfig {
    backpressure: Box<dyn BackpressureStrategy>,
    buffer_size: usize,
    // ... other config
}

impl Endpoint {
    fn create_channel<P: Protocol>(
        &self,
        id: ChannelId,
        config: ChannelConfig,
    ) -> Channel<P> {
        // Create channel with specified backpressure strategy
    }
}
```

## Consequences

### Positive
- **Flexibility**: Different strategies for different protocols
- **Safety**: Prevents memory exhaustion from unbounded queues
- **Performance**: Strategies can optimize for throughput or latency
- **Fairness**: Per-channel backpressure prevents one channel from starving others
- **Observability**: Metrics expose queue depths and wait times

### Negative
- **Complexity**: More configuration options for users
- **Tuning**: Optimal parameters may require experimentation
- **Overhead**: Backpressure checks add latency to send path

### Neutral
- **Transport-level backpressure**: TCP provides its own backpressure; our strategies work on top
- **Cross-channel fairness**: No built-in fairness across channels (could be added later)

## Alternatives Considered

### Transport-level Backpressure Only
Rely on TCP flow control. Rejected because:
- Doesn't work for in-memory transports
- Can't differentiate between channels
- No application-level control

### Fixed Buffer Sizes
Hard-code buffer sizes for all channels. Rejected because different protocols have different needs.

### Reactive Streams
Use reactive streams (like Rust's futures::Stream). Rejected because:
- More complex API
- Doesn't map cleanly to RPC semantics
- Harder to implement bi-directional flow

## Implementation Notes

### Deadlock Prevention
Bi-directional communication can deadlock if both sides fill their send buffers. Strategies must:
- Reserve capacity for responses
- Implement timeouts
- Provide deadlock detection

Example:
```rust
struct DeadlockSafe {
    request_capacity: usize,
    response_capacity: usize,
    // Always reserve space for responses
}
```

### Metrics and Monitoring
All strategies should expose:
- Current queue depth
- Capacity utilization
- Wait time distribution
- Dropped messages (if applicable)

### Graceful Degradation
When backpressure is applied:
- Log warnings at appropriate thresholds
- Expose metrics for alerting
- Consider shedding load (drop low-priority messages)

### Testing
Backpressure strategies must be testable:
```rust
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_bounded_queue_blocks() {
        let strategy = BoundedQueue::new(10);
        // Fill queue
        // Verify next send blocks
    }
}
```

## Open Questions

1. **Cross-channel fairness**: Should we implement fairness across channels on the same transport?
2. **Dynamic strategy switching**: Should strategies be changeable at runtime?
3. **Backpressure propagation**: Should backpressure on one channel affect others?
4. **Priority inversion**: How to handle priority inversion in Priority strategy?

## Related ADRs
- ADR-001: Core Architecture (defines Channel)
- ADR-007: Message Ordering Guarantees (ordering interacts with backpressure)

## References
- [HTTP/2 Flow Control](https://httpwg.org/specs/rfc7540.html#FlowControl)
- [TCP Congestion Control](https://datatracker.ietf.org/doc/html/rfc5681)
- [Reactive Streams Specification](https://www.reactive-streams.org/)
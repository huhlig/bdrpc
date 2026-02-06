# ADR-007: Message Ordering Guarantees

## Status
Accepted

## Context
In distributed systems, message ordering is critical for correctness but expensive to maintain globally. BDRPC needs clear ordering semantics that:
- Are easy to reason about
- Provide sufficient guarantees for most use cases
- Allow high performance through parallelism
- Make trade-offs explicit

## Decision

### Three-Level Ordering Model

We will provide different ordering guarantees at three levels:

1. **Within a Channel**: FIFO ordering (deterministic)
2. **Across Channels on Same Transport**: No ordering guarantees (non-deterministic)
3. **Across Different Transports**: No ordering guarantees (non-deterministic)

### Level 1: Within-Channel Ordering (FIFO)

**Guarantee**: Messages sent on a channel are received in the same order they were sent.

```rust
// Channel provides FIFO ordering
let channel = endpoint.create_channel::<MyProtocol>(channel_id).await?;

// These messages will arrive in order: A, B, C
channel.send(MessageA).await?;
channel.send(MessageB).await?;
channel.send(MessageC).await?;
```

**Implementation**:
```rust
struct Channel<P: Protocol> {
    id: ChannelId,
    // Single sender/receiver pair ensures FIFO
    sender: mpsc::Sender<P>,
    receiver: mpsc::Receiver<P>,
    sequence_number: AtomicU64,
}

impl<P: Protocol> Channel<P> {
    async fn send(&self, message: P) -> Result<()> {
        // Assign sequence number
        let seq = self.sequence_number.fetch_add(1, Ordering::SeqCst);
        
        // Wrap message with sequence number
        let envelope = Envelope {
            sequence: seq,
            payload: message,
        };
        
        // Send through FIFO channel
        self.sender.send(envelope).await?;
        Ok(())
    }
    
    async fn recv(&self) -> Option<P> {
        let envelope = self.receiver.recv().await?;
        
        // Verify sequence (detect reordering bugs)
        debug_assert_eq!(
            envelope.sequence,
            self.expected_sequence.fetch_add(1, Ordering::SeqCst)
        );
        
        Some(envelope.payload)
    }
}
```

**Rationale**:
- Simple mental model
- Matches TCP semantics
- Sufficient for most RPC use cases
- No head-of-line blocking between channels

### Level 2: Across Channels (No Ordering)

**Guarantee**: No ordering guarantees between messages on different channels, even if they share a transport.

```rust
let channel_a = endpoint.create_channel::<ProtocolA>(id_a).await?;
let channel_b = endpoint.create_channel::<ProtocolB>(id_b).await?;

// These may arrive in ANY order
channel_a.send(MessageA1).await?;
channel_b.send(MessageB1).await?;
channel_a.send(MessageA2).await?;
channel_b.send(MessageB2).await?;

// Possible orderings:
// A1, B1, A2, B2
// B1, A1, B2, A2
// A1, A2, B1, B2
// ... etc
```

**Implementation**:
```rust
struct Endpoint {
    // Each channel has independent queue
    channels: HashMap<ChannelId, Box<dyn ChannelHandler>>,
}

impl Endpoint {
    async fn multiplex_messages(&self, transport: &mut Transport) {
        // Process channels in parallel
        let mut tasks = FuturesUnordered::new();
        
        for (channel_id, channel) in &self.channels {
            tasks.push(async move {
                // Each channel processes independently
                channel.process_next_message().await
            });
        }
        
        // Messages from different channels can interleave
        while let Some(result) = tasks.next().await {
            // Handle result
        }
    }
}
```

**Rationale**:
- Enables parallelism
- No head-of-line blocking
- Different protocols have different performance characteristics
- Matches real-world network behavior

### Level 3: Across Transports (No Ordering)

**Guarantee**: No ordering guarantees between messages on different transports.

```rust
let transport_1 = endpoint.connect(addr1).await?;
let transport_2 = endpoint.connect(addr2).await?;

let channel_a = endpoint.create_channel_on_transport::<ProtocolA>(transport_1, id_a).await?;
let channel_b = endpoint.create_channel_on_transport::<ProtocolA>(transport_2, id_b).await?;

// No ordering guarantees, even though same protocol
channel_a.send(Message1).await?;
channel_b.send(Message2).await?;
```

**Rationale**:
- Different transports have different latencies
- Network paths may differ
- Allows load balancing across connections
- Matches distributed system reality

## Application-Level Ordering

If cross-channel or cross-transport ordering is needed, the application must implement it:

### Strategy 1: Sequence Numbers
```rust
#[derive(Serialize, Deserialize)]
struct OrderedMessage {
    global_sequence: u64,
    payload: Vec<u8>,
}

struct OrderedSender {
    channels: Vec<Channel<OrderedMessage>>,
    sequence: AtomicU64,
}

impl OrderedSender {
    async fn send(&self, payload: Vec<u8>) -> Result<()> {
        let seq = self.sequence.fetch_add(1, Ordering::SeqCst);
        let message = OrderedMessage {
            global_sequence: seq,
            payload,
        };
        
        // Send on any channel
        let channel = &self.channels[seq as usize % self.channels.len()];
        channel.send(message).await
    }
}

struct OrderedReceiver {
    channels: Vec<Channel<OrderedMessage>>,
    next_expected: AtomicU64,
    buffer: Mutex<BTreeMap<u64, Vec<u8>>>,
}

impl OrderedReceiver {
    async fn recv(&self) -> Vec<u8> {
        loop {
            // Try to get next expected message from buffer
            let next = self.next_expected.load(Ordering::SeqCst);
            if let Some(payload) = self.buffer.lock().await.remove(&next) {
                self.next_expected.fetch_add(1, Ordering::SeqCst);
                return payload;
            }
            
            // Receive from any channel
            let (seq, payload) = self.recv_any_channel().await;
            
            if seq == next {
                self.next_expected.fetch_add(1, Ordering::SeqCst);
                return payload;
            } else {
                // Buffer out-of-order message
                self.buffer.lock().await.insert(seq, payload);
            }
        }
    }
}
```

### Strategy 2: Causal Ordering (Vector Clocks)
```rust
#[derive(Serialize, Deserialize)]
struct CausalMessage {
    vector_clock: HashMap<String, u64>,
    payload: Vec<u8>,
}

struct CausalSender {
    endpoint_id: String,
    clock: Mutex<HashMap<String, u64>>,
}

impl CausalSender {
    async fn send(&self, channel: &Channel<CausalMessage>, payload: Vec<u8>) -> Result<()> {
        let mut clock = self.clock.lock().await;
        *clock.entry(self.endpoint_id.clone()).or_insert(0) += 1;
        
        let message = CausalMessage {
            vector_clock: clock.clone(),
            payload,
        };
        
        channel.send(message).await
    }
}
```

### Strategy 3: Single Channel
```rust
// Simplest: use one channel for ordered messages
let ordered_channel = endpoint.create_channel::<OrderedProtocol>(channel_id).await?;

// All messages on this channel are ordered
ordered_channel.send(Message1).await?;
ordered_channel.send(Message2).await?;
ordered_channel.send(Message3).await?;
```

## Ordering and Backpressure Interaction

Backpressure can affect perceived ordering:

```rust
// Channel A has backpressure, Channel B doesn't
channel_a.send(MessageA).await?;  // May block
channel_b.send(MessageB).await?;  // Sends immediately

// MessageB may arrive before MessageA due to backpressure
```

**Documentation**: This behavior must be clearly documented. Users relying on ordering must use a single channel or implement application-level ordering.

## Testing Ordering Guarantees

### Within-Channel Ordering Test
```rust
#[tokio::test]
async fn test_channel_fifo_ordering() {
    let (endpoint_a, endpoint_b) = create_connected_endpoints().await;
    let channel_a = endpoint_a.create_channel::<TestProtocol>(id).await.unwrap();
    let channel_b = endpoint_b.get_channel::<TestProtocol>(id).await.unwrap();
    
    // Send 1000 messages
    for i in 0..1000 {
        channel_a.send(TestMessage { seq: i }).await.unwrap();
    }
    
    // Receive and verify order
    for i in 0..1000 {
        let msg = channel_b.recv().await.unwrap();
        assert_eq!(msg.seq, i, "Message out of order");
    }
}
```

### Cross-Channel Non-Ordering Test
```rust
#[tokio::test]
async fn test_cross_channel_no_ordering() {
    let (endpoint_a, endpoint_b) = create_connected_endpoints().await;
    let channel_a1 = endpoint_a.create_channel::<TestProtocol>(id1).await.unwrap();
    let channel_a2 = endpoint_a.create_channel::<TestProtocol>(id2).await.unwrap();
    
    // Send interleaved messages
    channel_a1.send(TestMessage { channel: 1, seq: 0 }).await.unwrap();
    channel_a2.send(TestMessage { channel: 2, seq: 0 }).await.unwrap();
    channel_a1.send(TestMessage { channel: 1, seq: 1 }).await.unwrap();
    channel_a2.send(TestMessage { channel: 2, seq: 1 }).await.unwrap();
    
    // Receive all messages
    let mut received = vec![];
    for _ in 0..4 {
        // Receive from either channel
        let msg = receive_any_channel(&endpoint_b).await.unwrap();
        received.push((msg.channel, msg.seq));
    }
    
    // Verify within-channel ordering
    let channel_1_msgs: Vec<_> = received.iter()
        .filter(|(c, _)| *c == 1)
        .map(|(_, s)| *s)
        .collect();
    assert_eq!(channel_1_msgs, vec![0, 1]);
    
    let channel_2_msgs: Vec<_> = received.iter()
        .filter(|(c, _)| *c == 2)
        .map(|(_, s)| *s)
        .collect();
    assert_eq!(channel_2_msgs, vec![0, 1]);
    
    // But cross-channel order is not guaranteed
    // (test passes regardless of interleaving)
}
```

## Consequences

### Positive
- **Simple mental model**: FIFO within channel, no guarantees across
- **High performance**: Parallelism across channels
- **No head-of-line blocking**: Slow channel doesn't block fast channel
- **Matches reality**: Reflects actual network behavior
- **Flexibility**: Application can add ordering when needed

### Negative
- **Surprising behavior**: Users may expect global ordering
- **Application complexity**: Cross-channel ordering requires extra code
- **Debugging difficulty**: Non-deterministic ordering can hide bugs
- **Documentation burden**: Must clearly explain ordering model

### Neutral
- **Trade-off**: Performance vs. simplicity
- **Use case dependent**: Some apps need ordering, others don't

## Alternatives Considered

### Global Ordering
Maintain total order across all channels. Rejected because:
- Severe performance penalty
- Requires coordination
- Head-of-line blocking
- Doesn't match network reality

### Configurable Ordering
Let users choose ordering per channel. Rejected because:
- Adds complexity
- Hard to implement efficiently
- Most users don't need it
- Can be implemented at application level

### No Ordering Guarantees
Not even within-channel ordering. Rejected because:
- Too surprising
- Breaks most RPC use cases
- Doesn't match TCP semantics

## Implementation Notes

### Sequence Number Wrapping
```rust
// Use 64-bit sequence numbers to avoid wrapping
// At 1M messages/sec, takes 584,942 years to wrap
struct Channel {
    sequence_number: AtomicU64,
}
```

### Reordering Detection
```rust
#[cfg(debug_assertions)]
fn verify_sequence(&self, received: u64, expected: u64) {
    if received != expected {
        panic!(
            "Message reordering detected! Expected {}, got {}",
            expected, received
        );
    }
}
```

### Metrics
```rust
struct OrderingMetrics {
    messages_sent: Counter,
    messages_received: Counter,
    sequence_gaps: Counter,  // Out-of-order detection
    max_sequence_gap: Gauge,
}
```

## Documentation Requirements

The ordering model must be prominently documented:

1. **Quick Start Guide**: Explain FIFO within channel
2. **Advanced Guide**: Explain cross-channel non-ordering
3. **Examples**: Show application-level ordering patterns
4. **FAQ**: Address common ordering questions
5. **API Docs**: Document ordering guarantees on each method

## Related ADRs
- ADR-001: Core Architecture (defines Channel)
- ADR-003: Backpressure and Flow Control (interacts with ordering)
- ADR-004: Error Handling Hierarchy (ordering during errors)

## References
- [TCP Ordering Guarantees](https://datatracker.ietf.org/doc/html/rfc793)
- [QUIC Streams](https://datatracker.ietf.org/doc/html/rfc9000#section-2)
- [Lamport Timestamps](https://lamport.azurewebsites.net/pubs/time-clocks.pdf)
- [Vector Clocks](https://en.wikipedia.org/wiki/Vector_clock)
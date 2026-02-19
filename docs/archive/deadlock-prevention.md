# Deadlock Prevention in BDRPC

This document describes the built-in protections against deadlocks and common implementation issues in BDRPC.

## Overview

BDRPC includes several layers of protection to prevent deadlocks and help users avoid common pitfalls when implementing bi-directional RPC systems.

## Built-in Protections

### 1. Timeout Support

**Problem**: Operations that block indefinitely can cause deadlocks, especially in bi-directional communication where both sides may be waiting for each other.

**Solution**: Timeout methods for all blocking operations.

#### Send with Timeout

```rust
use std::time::Duration;
use bdrpc::channel::{Channel, ChannelId};

let (sender, _receiver) = Channel::<String>::new(ChannelId::new(), 100);

// Try to send with a 5 second timeout
match sender.send_timeout("Hello".to_string(), Duration::from_secs(5)).await {
    Ok(()) => println!("Message sent successfully"),
    Err(e) if e.is_timeout() => println!("Send timed out - channel may be full"),
    Err(e) => println!("Send failed: {}", e),
}
```

#### Receive with Timeout

```rust
use std::time::Duration;
use bdrpc::channel::{Channel, ChannelId};

let (_sender, mut receiver) = Channel::<String>::new(ChannelId::new(), 100);

// Try to receive with a 5 second timeout
match receiver.recv_timeout(Duration::from_secs(5)).await {
    Ok(message) => println!("Received: {}", message),
    Err(e) if e.is_timeout() => println!("Receive timed out - no message available"),
    Err(e) => println!("Receive failed: {}", e),
}
```

### 2. Non-Blocking Operations

**Problem**: Sometimes you want to check if an operation can succeed without blocking.

**Solution**: `try_send()` and `try_recv()` methods that return immediately.

#### Try Send

```rust
use bdrpc::channel::{Channel, ChannelId};

let (sender, _receiver) = Channel::<String>::new(ChannelId::new(), 100);

// Try to send without blocking
match sender.try_send("Hello".to_string()) {
    Ok(()) => println!("Message sent immediately"),
    Err(e) if e.is_recoverable() => println!("Channel full, try again later"),
    Err(e) => println!("Send failed: {}", e),
}
```

#### Try Receive

```rust
use bdrpc::channel::{Channel, ChannelId};

let (_sender, mut receiver) = Channel::<String>::new(ChannelId::new(), 100);

// Try to receive without blocking
match receiver.try_recv() {
    Some(message) => println!("Received: {}", message),
    None => println!("No message available right now"),
}
```

### 3. Channel State Inspection

**Problem**: Understanding channel state helps diagnose issues and make informed decisions.

**Solution**: Methods to inspect channel state.

```rust
use bdrpc::channel::{Channel, ChannelId};

let (sender, _receiver) = Channel::<String>::new(ChannelId::new(), 100);

// Check channel state
println!("Channel capacity: {}", sender.capacity());
println!("Channel closed: {}", sender.is_closed());

// Make decisions based on state
if sender.capacity() < 10 {
    println!("Warning: Channel is nearly full!");
}
```

### 4. Backpressure Strategy

**Problem**: Unbounded message sending can lead to memory exhaustion and system instability.

**Solution**: Built-in backpressure with configurable strategies.

```rust
use bdrpc::channel::{Channel, ChannelId};
use bdrpc::backpressure::{BoundedQueue, Unlimited};
use std::sync::Arc;

// Default: Bounded queue with backpressure
let (sender1, receiver1) = Channel::<String>::new(ChannelId::new(), 100);

// Custom: Unlimited (use with caution!)
let (sender2, receiver2) = Channel::<String>::with_backpressure(
    ChannelId::new(),
    100,
    Arc::new(Unlimited::new())
);
```

### 5. Error Classification

**Problem**: Not all errors are equal - some are recoverable, some indicate permanent failure.

**Solution**: Error methods to classify and handle errors appropriately.

```rust
use bdrpc::channel::ChannelError;

fn handle_error(error: ChannelError) {
    if error.is_recoverable() {
        println!("Temporary error, can retry: {}", error);
        // Implement retry logic
    } else if error.is_closed() {
        println!("Channel closed, cannot recover: {}", error);
        // Clean up and exit
    } else if error.is_timeout() {
        println!("Operation timed out: {}", error);
        // Decide whether to retry or fail
    } else {
        println!("Permanent error: {}", error);
        // Handle fatal error
    }
}
```

## Common Deadlock Scenarios and Solutions

### Scenario 1: Mutual Waiting

**Problem**: Both sides wait for a response from each other.

```rust
// ❌ BAD: Both sides waiting for each other
// Client:
sender.send(Request).await?;
let response = receiver.recv().await; // Blocks forever

// Server:
sender.send(Request).await?;  // Also waiting
let response = receiver.recv().await; // Deadlock!
```

**Solution**: Use timeouts or non-blocking operations.

```rust
// ✅ GOOD: Use timeout to detect deadlock
use std::time::Duration;

sender.send(Request).await?;
match receiver.recv_timeout(Duration::from_secs(30)).await {
    Ok(response) => handle_response(response),
    Err(e) if e.is_timeout() => {
        println!("Timeout waiting for response - possible deadlock");
        // Handle timeout appropriately
    }
    Err(e) => handle_error(e),
}
```

### Scenario 2: Full Channel Blocking

**Problem**: Sender blocks because channel is full, but receiver is also blocked.

```rust
// ❌ BAD: Can deadlock if channel fills up
for i in 0..1000 {
    sender.send(Message(i)).await?; // May block if channel full
}
```

**Solution**: Check capacity or use try_send.

```rust
// ✅ GOOD: Check capacity and handle backpressure
for i in 0..1000 {
    if sender.capacity() < 10 {
        println!("Warning: Channel nearly full, slowing down");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    match sender.send_timeout(Message(i), Duration::from_secs(5)).await {
        Ok(()) => {},
        Err(e) if e.is_timeout() => {
            println!("Send timeout - receiver may be stuck");
            break;
        }
        Err(e) => return Err(e.into()),
    }
}
```

### Scenario 3: Synchronous Operations in Async Context

**Problem**: Blocking operations in async code can prevent other tasks from running.

```rust
// ❌ BAD: Blocking in async context
async fn process_messages(mut receiver: ChannelReceiver<Message>) {
    loop {
        let msg = receiver.recv().await.unwrap(); // Blocks forever if no messages
        // Process message
    }
}
```

**Solution**: Use timeouts or select! to handle multiple futures.

```rust
// ✅ GOOD: Use timeout or select!
use tokio::time::{sleep, Duration};

async fn process_messages(mut receiver: ChannelReceiver<Message>) {
    loop {
        tokio::select! {
            result = receiver.recv_timeout(Duration::from_secs(30)) => {
                match result {
                    Ok(msg) => process_message(msg),
                    Err(e) if e.is_timeout() => {
                        println!("No messages for 30 seconds");
                        continue;
                    }
                    Err(e) => {
                        println!("Receiver error: {}", e);
                        break;
                    }
                }
            }
            _ = sleep(Duration::from_secs(60)) => {
                println!("Periodic health check");
            }
        }
    }
}
```

## Best Practices

### 1. Always Use Timeouts for Network Operations

```rust
// Set reasonable timeouts based on your use case
const SEND_TIMEOUT: Duration = Duration::from_secs(5);
const RECV_TIMEOUT: Duration = Duration::from_secs(30);

sender.send_timeout(message, SEND_TIMEOUT).await?;
receiver.recv_timeout(RECV_TIMEOUT).await?;
```

### 2. Spawn Separate Tasks for Send and Receive

```rust
// ✅ GOOD: Separate tasks prevent blocking each other
let sender_task = tokio::spawn(async move {
    for msg in messages {
        sender.send(msg).await?;
    }
    Ok::<_, Error>(())
});

let receiver_task = tokio::spawn(async move {
    while let Some(msg) = receiver.recv().await {
        process(msg);
    }
    Ok::<_, Error>(())
});

// Wait for both to complete
let (send_result, recv_result) = tokio::join!(sender_task, receiver_task);
```

### 3. Monitor Channel Health

```rust
use std::time::Duration;
use tokio::time::interval;

// Spawn a monitoring task
tokio::spawn(async move {
    let mut interval = interval(Duration::from_secs(10));
    loop {
        interval.tick().await;
        
        let capacity = sender.capacity();
        if capacity < 10 {
            println!("WARNING: Channel nearly full! Capacity: {}", capacity);
        }
        
        if sender.is_closed() {
            println!("Channel closed, stopping monitor");
            break;
        }
    }
});
```

### 4. Handle Errors Appropriately

```rust
match sender.send_timeout(message, Duration::from_secs(5)).await {
    Ok(()) => {},
    Err(e) if e.is_timeout() => {
        // Timeout - decide whether to retry or fail
        println!("Send timeout, retrying once...");
        sender.send_timeout(message, Duration::from_secs(10)).await?;
    }
    Err(e) if e.is_closed() => {
        // Channel closed - clean up and exit
        println!("Channel closed, stopping sender");
        return Ok(());
    }
    Err(e) => {
        // Other error - propagate
        return Err(e.into());
    }
}
```

### 5. Use Bounded Channels

```rust
// ✅ GOOD: Bounded channel with reasonable size
let (sender, receiver) = Channel::<Message>::new(ChannelId::new(), 100);

// ❌ AVOID: Unbounded channels can lead to memory issues
// let (sender, receiver) = Channel::<Message>::with_backpressure(
//     ChannelId::new(),
//     usize::MAX,
//     Arc::new(Unlimited::new())
// );
```

## Debugging Deadlocks

If you suspect a deadlock:

1. **Enable tracing**: Use the `observability` feature to see what's happening
2. **Check channel state**: Use `capacity()` and `is_closed()` methods
3. **Add timeouts**: Convert blocking operations to timeout-based ones
4. **Use tokio-console**: Monitor async tasks and their states
5. **Add logging**: Log before and after blocking operations

```rust
#[cfg(feature = "observability")]
use tracing::{info, warn};

info!("About to send message");
match sender.send_timeout(message, Duration::from_secs(5)).await {
    Ok(()) => info!("Message sent successfully"),
    Err(e) => warn!("Send failed: {}", e),
}
```

## Summary

BDRPC provides multiple layers of protection against deadlocks:

1. **Timeout methods** (`send_timeout`, `recv_timeout`) - Prevent indefinite blocking
2. **Non-blocking methods** (`try_send`, `try_recv`) - Check without blocking
3. **State inspection** (`capacity`, `is_closed`) - Monitor channel health
4. **Backpressure** - Prevent memory exhaustion
5. **Error classification** - Handle errors appropriately

By using these features and following best practices, you can build robust bi-directional RPC systems that avoid common deadlock scenarios.
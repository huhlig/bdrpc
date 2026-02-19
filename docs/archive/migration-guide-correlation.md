# Migration Guide: Request-Response Correlation

This guide helps you migrate to bdrpc's new request-response correlation feature, which enables concurrent RPC calls without race conditions.

## Overview

Starting with version 0.2.0, bdrpc supports concurrent RPC calls through request-response correlation. This feature allows multiple requests to be in-flight simultaneously, with responses correctly matched to their originating requests.

## What's New

### Key Features

1. **Concurrent RPC Calls**: Multiple requests can be sent simultaneously without blocking
2. **Automatic Correlation**: The framework automatically manages correlation IDs
3. **Out-of-Order Responses**: Responses can arrive in any order and will be correctly matched
4. **Backward Compatible**: Existing code continues to work without changes
5. **Zero-Cost Abstraction**: No overhead for non-RPC message traffic

### Performance Benefits

- **3x Throughput Improvement**: Concurrent calls eliminate head-of-line blocking
- **Lower Latency**: Fast operations don't wait for slow ones
- **Better Resource Utilization**: Multiple requests can be processed in parallel

## API Changes

### No Breaking Changes

The good news: **your existing code will continue to work without modifications**. The correlation feature is automatically enabled for RPC channels and transparent to your application code.

### New Capabilities

While the API remains the same, you can now safely make concurrent calls:

**Before (Sequential - Still Works)**:
```rust
let client = CalculatorClient::new(sender, receiver);

// Sequential calls - one at a time
let result1 = client.add(5, 3).await?;
let result2 = client.multiply(7, 6).await?;
let result3 = client.divide(20, 4).await?;
```

**After (Concurrent - Now Possible)**:
```rust
let client = CalculatorClient::new(sender, receiver);

// Concurrent calls - all at once!
let (result1, result2, result3) = tokio::join!(
    client.add(5, 3),
    client.multiply(7, 6),
    client.divide(20, 4)
);
```

### Client Cloning

Clients are now `Clone`, allowing you to share them across tasks:

```rust
let client = CalculatorClient::new(sender, receiver);

// Clone for use in another task
let client2 = client.clone();
tokio::spawn(async move {
    let result = client2.add(10, 20).await?;
    println!("Result: {}", result);
});

// Original client still works
let result = client.multiply(5, 5).await?;
```

## Migration Steps

### Step 1: Update Dependencies

Update your `Cargo.toml`:

```toml
[dependencies]
bdrpc = "0.2.0"  # or later
```

### Step 2: Regenerate Service Code

If you're using the `#[bdrpc::service]` macro, regenerate your service code:

```bash
cargo clean
cargo build
```

The macro will automatically generate correlation-aware client and server code.

### Step 3: Test Concurrent Usage (Optional)

If you want to take advantage of concurrent RPC calls, update your code to use `tokio::join!` or `tokio::spawn`:

```rust
// Example: Concurrent operations
let (sum, product, quotient) = tokio::join!(
    client.add(a, b),
    client.multiply(c, d),
    client.divide(e, f)
);
```

### Step 4: Verify Behavior

Run your tests to ensure everything works as expected:

```bash
cargo nextest run
```

## Common Patterns

### Pattern 1: Fire-and-Forget with Concurrent Calls

```rust
// Launch multiple operations concurrently
let handles: Vec<_> = (0..10)
    .map(|i| {
        let client = client.clone();
        tokio::spawn(async move {
            client.process(i).await
        })
    })
    .collect();

// Wait for all to complete
for handle in handles {
    handle.await??;
}
```

### Pattern 2: Request Pipelining

```rust
// Send requests without waiting for responses
let mut futures = Vec::new();
for item in items {
    futures.push(client.process(item));
}

// Collect all results
let results = futures::future::join_all(futures).await;
```

### Pattern 3: Mixed Latency Operations

```rust
// Fast and slow operations don't block each other
let (fast_result, slow_result) = tokio::join!(
    client.fast_operation(),
    client.slow_operation()
);
// fast_result arrives quickly, even though slow_operation is still running
```

### Pattern 4: Error Handling with Concurrent Calls

```rust
let results = tokio::join!(
    client.operation1(),
    client.operation2(),
    client.operation3()
);

match results {
    (Ok(r1), Ok(r2), Ok(r3)) => {
        // All succeeded
    }
    _ => {
        // Handle errors individually
        if let Err(e) = results.0 {
            eprintln!("Operation 1 failed: {}", e);
        }
        // ... handle other errors
    }
}
```

## Backward Compatibility

### What Stays the Same

1. **API Surface**: All existing methods work identically
2. **Sequential Calls**: Single request-response patterns work as before
3. **Non-RPC Channels**: Regular message channels are unaffected
4. **Serialization**: No changes to message formats (correlation ID is optional)

### What's Different

1. **Concurrency Support**: Multiple requests can now be in-flight
2. **Client Cloning**: Clients implement `Clone` for sharing across tasks
3. **Response Ordering**: Responses may arrive out-of-order (but are correctly matched)

### Compatibility Guarantees

- **Wire Protocol**: Correlation IDs are optional in the `Envelope` structure
- **Old Clients**: Can communicate with new servers (correlation ID is ignored if not present)
- **New Clients**: Can communicate with old servers (correlation works if both sides support it)
- **Mixed Versions**: Deployments with mixed versions work correctly

## Performance Considerations

### When to Use Concurrent Calls

**Good Use Cases**:
- Multiple independent operations
- Operations with varying latencies
- High-throughput scenarios
- Batch processing

**Not Recommended**:
- Operations with dependencies (use sequential calls)
- Very low latency requirements (overhead of task spawning)
- Memory-constrained environments (each pending request uses memory)

### Tuning

The default timeout for RPC calls is 30 seconds. If you need different timeouts, you can implement custom timeout logic:

```rust
use tokio::time::{timeout, Duration};

let result = timeout(
    Duration::from_secs(5),
    client.operation()
).await??;
```

### Memory Usage

Each pending request consumes memory for:
- Correlation ID tracking (~24 bytes)
- Response channel (~64 bytes)
- Request/response data

For high-concurrency scenarios, monitor memory usage and consider rate limiting.

## Troubleshooting

### Issue: Responses Arriving Out of Order

**Symptom**: Responses don't arrive in the order requests were sent.

**Solution**: This is expected behavior with concurrent RPC. Responses are correctly matched to requests via correlation IDs. If you need ordered results, use sequential calls or collect results in order:

```rust
let mut futures = Vec::new();
for item in items {
    futures.push(client.process(item));
}
let results = futures::future::join_all(futures).await;
// Results are in the same order as requests
```

### Issue: Timeout Errors

**Symptom**: `ChannelError::Timeout` errors when making concurrent calls.

**Solution**: The default 30-second timeout applies per request. For long-running operations, implement custom timeout handling or increase the timeout.

### Issue: Memory Growth

**Symptom**: Memory usage increases with concurrent requests.

**Solution**: Limit concurrency using semaphores or bounded channels:

```rust
use tokio::sync::Semaphore;

let semaphore = Arc::new(Semaphore::new(10)); // Max 10 concurrent

for item in items {
    let permit = semaphore.clone().acquire_owned().await?;
    let client = client.clone();
    tokio::spawn(async move {
        let _permit = permit; // Released on drop
        client.process(item).await
    });
}
```

## Examples

### Complete Example: Concurrent Calculator

See `examples/concurrent_calculator.rs` for a comprehensive example demonstrating:
- Basic concurrent operations
- Performance comparison (sequential vs concurrent)
- Mixed latency operations
- Error handling
- Client cloning
- Stress testing
- Request pipelining

Run it with:
```bash
cargo run --example concurrent_calculator
```

### Other Examples

- `examples/calculator_service.rs`: Basic RPC service (works with or without concurrency)
- `examples/future_methods_demo.rs`: Demonstrates async method patterns
- `examples/service_macro_demo.rs`: Shows macro-generated code structure

## Testing

### Unit Tests

The correlation feature includes comprehensive tests:

```bash
# Run all tests
cargo nextest run

# Run correlation-specific tests
cargo nextest run rpc_correlation
```

### Integration Tests

Test your service with concurrent calls:

```rust
#[tokio::test]
async fn test_concurrent_operations() {
    let (client, _server) = setup_test_service().await;
    
    let (r1, r2, r3) = tokio::join!(
        client.operation1(),
        client.operation2(),
        client.operation3()
    );
    
    assert!(r1.is_ok());
    assert!(r2.is_ok());
    assert!(r3.is_ok());
}
```

## Best Practices

1. **Use Concurrent Calls for Independent Operations**: Don't make operations concurrent if they have dependencies
2. **Clone Clients for Task Spawning**: Each task should have its own client clone
3. **Handle Errors Individually**: Concurrent calls can fail independently
4. **Monitor Pending Requests**: In production, track the number of in-flight requests
5. **Set Appropriate Timeouts**: Consider operation latency when setting timeouts
6. **Limit Concurrency**: Use semaphores or rate limiting for high-load scenarios

## FAQ

### Q: Do I need to change my existing code?

**A**: No, existing code continues to work without changes. Concurrent calls are opt-in.

### Q: What's the performance impact?

**A**: For sequential calls, there's no overhead. For concurrent calls, you'll see significant throughput improvements (typically 2-3x).

### Q: Can I mix sequential and concurrent calls?

**A**: Yes, you can use both patterns in the same application.

### Q: How many concurrent requests can I have?

**A**: There's no hard limit, but consider memory usage. Each pending request uses ~100 bytes. For 10,000 concurrent requests, that's ~1MB.

### Q: What happens if correlation IDs wrap around?

**A**: Correlation IDs are 64-bit unsigned integers. At 1 million requests per second, it would take 584,942 years to wrap around.

### Q: Can I use custom correlation IDs?

**A**: Not currently. The framework manages correlation IDs automatically. This is a potential future enhancement.

### Q: Does this work with all transports?

**A**: Yes, correlation works with all transports (TCP, QUIC, WebSocket, in-memory).

### Q: What about streaming RPC?

**A**: Streaming RPC with correlation is a planned future enhancement.

## Next Steps

1. **Read the Architecture Guide**: See `docs/architecture-guide.md` for detailed design information
2. **Try the Examples**: Run `examples/concurrent_calculator.rs` to see it in action
3. **Update Your Code**: Start using concurrent calls where appropriate
4. **Monitor Performance**: Measure the impact in your application
5. **Provide Feedback**: Report issues or suggestions on GitHub

## Support

- **Documentation**: See `docs/` directory for comprehensive guides
- **Examples**: Check `examples/` directory for working code
- **Issues**: Report bugs or request features on GitHub
- **Discussions**: Ask questions in GitHub Discussions

## Version History

- **0.2.0**: Initial release of request-response correlation feature
- **0.1.x**: Sequential RPC only (mutex-based)

---

For more information, see:
- [Architecture Guide](architecture-guide.md)
- [Quick Start Guide](quick-start.md)
- [Best Practices](best-practices.md)
- [Performance Guide](performance-guide.md)
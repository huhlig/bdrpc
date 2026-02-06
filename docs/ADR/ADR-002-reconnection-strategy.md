# ADR-002: Pluggable Reconnection Strategy

## Status
Accepted

## Context
In distributed systems, network connections fail. BDRPC needs a robust reconnection mechanism that:
- Handles transient failures gracefully
- Allows different strategies for different use cases
- Provides clear responsibility: clients reconnect, servers accept
- Avoids thundering herd problems

## Decision

### Core Principle
**Client-initiated connections are the client's responsibility to maintain.** When an endpoint connects as a client, it owns the reconnection logic.

### Pluggable Strategy Pattern
We will implement a trait-based reconnection strategy that can be swapped at runtime:

```rust
#[async_trait]
trait ReconnectionStrategy: Send + Sync {
    /// Determine if reconnection should be attempted
    async fn should_reconnect(&self, attempt: u32, last_error: &TransportError) -> bool;
    
    /// Calculate delay before next attempt
    async fn next_delay(&self, attempt: u32) -> Duration;
    
    /// Called when connection succeeds
    fn on_connected(&self);
    
    /// Called when connection fails
    fn on_disconnected(&self, error: &TransportError);
    
    /// Reset internal state (e.g., after manual intervention)
    fn reset(&self);
}
```

### Built-in Strategies

#### 1. ExponentialBackoff (Default)
```rust
struct ExponentialBackoff {
    initial_delay: Duration,
    max_delay: Duration,
    multiplier: f64,
    jitter: bool,
    max_attempts: Option<u32>,
}
```
- Starts with small delay, increases exponentially
- Adds jitter to prevent thundering herd
- Caps at maximum delay
- Optional attempt limit

#### 2. FixedDelay
```rust
struct FixedDelay {
    delay: Duration,
    max_attempts: Option<u32>,
}
```
- Simple fixed interval between attempts
- Useful for predictable retry patterns

#### 3. CircuitBreaker
```rust
struct CircuitBreaker {
    failure_threshold: u32,
    timeout: Duration,
    half_open_attempts: u32,
}
```
- Stops attempting after threshold failures
- Enters "open" state (no attempts)
- After timeout, enters "half-open" (limited attempts)
- Requires manual reset or successful connection to close

#### 4. NoReconnect
```rust
struct NoReconnect;
```
- Fails immediately on disconnect
- Useful for testing or one-shot connections

### Integration with Endpoint

```rust
struct ClientConfig {
    reconnection_strategy: Box<dyn ReconnectionStrategy>,
    // ... other config
}

impl Endpoint {
    async fn connect_with_strategy(
        &self,
        addr: SocketAddr,
        strategy: Box<dyn ReconnectionStrategy>,
    ) -> Result<TransportId> {
        // Initial connection attempt
        // On failure, delegate to strategy
    }
}
```

### Reconnection Loop

```rust
async fn reconnection_loop(
    endpoint: Arc<Endpoint>,
    addr: SocketAddr,
    strategy: Arc<dyn ReconnectionStrategy>,
) {
    let mut attempt = 0;
    
    loop {
        match endpoint.try_connect(addr).await {
            Ok(transport_id) => {
                strategy.on_connected();
                return; // Success
            }
            Err(error) => {
                strategy.on_disconnected(&error);
                
                if !strategy.should_reconnect(attempt, &error).await {
                    break; // Give up
                }
                
                let delay = strategy.next_delay(attempt).await;
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
        }
    }
}
```

## Consequences

### Positive
- **Flexibility**: Different strategies for different scenarios
- **Testability**: Easy to test with NoReconnect or custom strategies
- **Observability**: Callbacks allow metrics and logging
- **Safety**: Prevents infinite retry loops with configurable limits
- **Performance**: Exponential backoff with jitter prevents thundering herd

### Negative
- **Complexity**: More moving parts than a simple retry loop
- **Configuration**: Users must choose appropriate strategy
- **State management**: Strategies may need internal state (thread-safe)

### Neutral
- **Server responsibility**: Servers don't reconnect; they accept new connections
- **Channel recovery**: Channels must be re-established after reconnection

## Alternatives Considered

### Hard-coded Exponential Backoff
Simple but inflexible. Rejected because different use cases need different strategies.

### Retry Crate Integration
Use existing retry crates. Rejected because we need tight integration with transport lifecycle and custom error handling.

### Automatic Reconnection Always
Always reconnect with no configuration. Rejected because some scenarios (testing, one-shot requests) don't want reconnection.

## Implementation Notes

### Metrics Integration
Strategies should expose metrics:
- Reconnection attempts
- Success/failure rates
- Current backoff delay
- Circuit breaker state

### Error Classification
Some errors should not trigger reconnection:
- Authentication failures
- Protocol mismatches
- Explicit connection rejection

The strategy's `should_reconnect` method can inspect error types and decide accordingly.

### Graceful Shutdown
During shutdown, reconnection loops must be cancelled cleanly:
```rust
impl Endpoint {
    async fn shutdown(&self) {
        // Cancel all reconnection tasks
        // Wait for in-flight connections to complete
    }
}
```

## Related ADRs
- ADR-001: Core Architecture (defines Transport and Endpoint)
- ADR-004: Error Handling Hierarchy (defines TransportError)

## References
- [Exponential Backoff and Jitter](https://aws.amazon.com/blogs/architecture/exponential-backoff-and-jitter/)
- [Circuit Breaker Pattern](https://martinfowler.com/bliki/CircuitBreaker.html)
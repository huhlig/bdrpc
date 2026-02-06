# Error Recovery Strategy

This document describes the error recovery behaviors for BDRPC, implementing the strategy defined in ADR-004 (Error Handling Hierarchy).

## Overview

BDRPC implements a three-layer error hierarchy with specific recovery strategies for each layer:

1. **Transport Layer** → Close transport, trigger reconnection
2. **Channel Layer** → Close channel, keep transport alive
3. **Application Layer** → Propagate to caller, no framework action

## Transport Error Recovery

### Behavior

When a `TransportError` occurs:

1. **Close the Transport**: The affected transport is immediately closed
2. **Close All Channels**: All channels using this transport are closed
3. **Notify Listeners**: Error callbacks are invoked with the error details
4. **Trigger Reconnection**: If a reconnection strategy is configured, attempt to reconnect

### Recoverable Transport Errors

The following transport errors are considered recoverable and will trigger reconnection:

- `ConnectionFailed`: Failed to establish initial connection
- `ConnectionLost`: Connection was lost during operation
- `Timeout`: Operation exceeded time limit
- `NotConnected`: Transport not yet connected
- Transient I/O errors: `Interrupted`, `WouldBlock`, `TimedOut`

### Non-Recoverable Transport Errors

The following transport errors are not recoverable:

- `InvalidConfiguration`: Configuration error (programming bug)
- `Closed`: Transport explicitly closed by user
- `BindFailed`: Server failed to bind to address
- Permanent I/O errors: `BrokenPipe`, `ConnectionReset`, etc.

### Example

```rust
use bdrpc::transport::{TransportError, Transport};
use bdrpc::BdrpcError;

async fn handle_transport_error(error: BdrpcError) {
    if let BdrpcError::Transport(transport_err) = error {
        // Check if we should close the transport
        if transport_err.should_close_transport() {
            // Close transport and all channels
            // This is handled automatically by the framework
            eprintln!("Transport closed due to: {}", transport_err);
        }
        
        // Check if we can recover
        if transport_err.is_recoverable() {
            // Reconnection will be attempted automatically
            // if a reconnection strategy is configured
            eprintln!("Will attempt reconnection");
        } else {
            eprintln!("Permanent failure: {}", transport_err);
        }
    }
}
```

## Channel Error Recovery

### Behavior

When a `ChannelError` occurs:

1. **Evaluate Recoverability**: Check if the error is recoverable
2. **Close Channel (if needed)**: Non-recoverable errors close the channel
3. **Keep Transport Alive**: The transport and other channels remain operational
4. **Notify Listeners**: Error callbacks are invoked with the error details

### Recoverable Channel Errors

The following channel errors are recoverable:

- `Full`: Channel buffer is full (backpressure)
  - **Action**: Caller should wait and retry
  - **Channel State**: Remains open

### Non-Recoverable Channel Errors

The following channel errors are not recoverable and will close the channel:

- `Closed`: Channel is already closed
- `NotFound`: Channel does not exist
- `OutOfOrder`: Message ordering violation (protocol bug)
- `AlreadyExists`: Channel ID collision
- `Internal`: Unexpected internal error

### Example

```rust
use bdrpc::channel::{ChannelError, Channel};
use bdrpc::BdrpcError;

async fn handle_channel_error(error: BdrpcError) {
    if let BdrpcError::Channel(channel_err) = error {
        // Check if we should close the channel
        if error.should_close_channel() {
            // Channel will be closed automatically
            eprintln!("Channel {} closed due to: {}", 
                channel_err.channel_id().unwrap(), 
                channel_err);
        }
        
        // Check if we can recover
        if channel_err.is_recoverable() {
            // For Full errors, implement backpressure handling
            if let ChannelError::Full { channel_id, buffer_size } = channel_err {
                eprintln!("Channel {} full ({}), applying backpressure", 
                    channel_id, buffer_size);
                // Wait and retry
            }
        }
    }
}
```

## Application Error Recovery

### Behavior

When an application error occurs:

1. **Propagate to Caller**: Error is returned to the calling code
2. **No Framework Action**: BDRPC does not handle application errors
3. **Keep Channel Open**: The channel remains operational
4. **Keep Transport Open**: The transport remains operational

Application errors are user-defined errors from service implementations. The framework treats them as opaque and simply propagates them to the caller.

### Example

```rust
use bdrpc::BdrpcError;
use std::io;

async fn handle_application_error(error: BdrpcError) {
    if let BdrpcError::Application(app_err) = error {
        // Application errors are propagated to the caller
        // The framework does not take any action
        eprintln!("Application error: {}", app_err);
        
        // The caller is responsible for handling the error
        // Channel and transport remain operational
        
        // Example: retry logic, logging, metrics, etc.
    }
}
```

## Error Decision Tree

```
Error Occurs
    │
    ├─ TransportError?
    │   ├─ Yes → Close transport
    │   │       → Close all channels
    │   │       → Notify listeners
    │   │       → Is recoverable?
    │   │           ├─ Yes → Trigger reconnection
    │   │           └─ No → Permanent failure
    │   │
    ├─ ChannelError?
    │   ├─ Yes → Is recoverable?
    │   │       ├─ Yes → Keep channel open
    │   │       │       → Apply backpressure
    │   │       │       → Notify listeners
    │   │       └─ No → Close channel
    │   │               → Keep transport open
    │   │               → Notify listeners
    │   │
    └─ ApplicationError?
        └─ Yes → Propagate to caller
                → No framework action
                → Keep channel open
                → Keep transport open
```

## Error Observability

### Metrics

The framework tracks the following error metrics:

- **Transport errors**: Count by error type
- **Channel errors**: Count by error type and channel ID
- **Application errors**: Count (opaque)
- **Recovery attempts**: Count and success rate
- **Reconnection attempts**: Count and success rate

### Logging

All errors are logged with structured context:

```rust
// Transport error
error!(
    error = %transport_err,
    transport_id = %transport_id,
    "Transport error occurred"
);

// Channel error
error!(
    error = %channel_err,
    channel_id = %channel_id,
    transport_id = %transport_id,
    "Channel error occurred"
);

// Application error
error!(
    error = %app_err,
    channel_id = %channel_id,
    "Application error occurred"
);
```

### Error Callbacks

Users can register error callbacks to be notified of errors:

```rust
use bdrpc::{BdrpcError, ErrorCallback};

let callback: ErrorCallback = Box::new(|error: BdrpcError| {
    match error {
        BdrpcError::Transport(e) => {
            // Handle transport error
        }
        BdrpcError::Channel(e) => {
            // Handle channel error
        }
        BdrpcError::Application(e) => {
            // Handle application error
        }
    }
});

// Register callback with endpoint
endpoint.on_error(callback);
```

## Testing Error Recovery

### Unit Tests

Each error type has unit tests verifying:

- Error classification (transport/channel/application)
- Recoverability determination
- Close behavior (transport/channel)

### Integration Tests

Integration tests verify end-to-end error recovery:

- **Transport error recovery**: Verify reconnection after connection loss
- **Channel error recovery**: Verify backpressure handling
- **Application error propagation**: Verify errors reach caller

See `bdrpc/tests/error_recovery.rs` for complete integration tests.

## Best Practices

### For Library Users

1. **Handle Transport Errors**: Implement reconnection logic or use built-in strategies
2. **Handle Backpressure**: Implement retry logic for `ChannelError::Full`
3. **Log Errors**: Use error callbacks for observability
4. **Test Error Paths**: Verify your error handling logic

### For Library Developers

1. **Use Appropriate Error Types**: Choose the correct error layer
2. **Document Recovery Behavior**: Clearly document what happens on error
3. **Test Error Scenarios**: Write integration tests for error paths
4. **Provide Error Context**: Include relevant information in error messages

## References

- [ADR-004: Error Handling Hierarchy](adr/ADR-004-error-handling-hierarchy.md)
- [ADR-002: Reconnection Strategy](adr/ADR-002-reconnection-strategy.md)
- [ADR-003: Backpressure and Flow Control](adr/ADR-003-backpressure-flow-control.md)
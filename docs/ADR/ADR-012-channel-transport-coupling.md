# ADR-012: Channel-Transport Coupling

**Status**: Proposed  
**Date**: 2026-02-06  
**Deciders**: Development Team  
**Related**: ADR-001 (Core Architecture)

## Context

The current BDRPC architecture allows creating channels independently of transports using `Channel::new()` and `Channel::with_features()`. This has led to confusion in examples where channels are created but not properly connected to transports, resulting in "orphaned" channels that can send/receive messages in-memory but don't actually communicate over the network.

### Current Issues

1. **Confusion in Examples**: Examples like `chat_server.rs` and `calculator.rs` create standalone channels without transports, which doesn't demonstrate real network communication
2. **API Misuse**: Users can easily create channels thinking they're communicating over the network when they're just using in-memory channels
3. **Architectural Inconsistency**: The Endpoint layer is supposed to orchestrate transport + channels, but channels can bypass this entirely

### Current Architecture

```rust
// This creates an orphaned channel (no transport)
let (sender, receiver) = Channel::<MyProtocol>::new(channel_id, 100);

// Messages go through in-memory mpsc channel only
sender.send(msg).await?;  // No network I/O!
```

## Decision

We propose a **two-tier approach** to clarify the channel API:

### 1. Make Standalone Channels Explicit (Short-term)

Rename the current `Channel::new()` to `Channel::new_in_memory()` to make it clear these are for testing/in-process communication only:

```rust
// Explicit that this is in-memory only
let (sender, receiver) = Channel::<MyProtocol>::new_in_memory(channel_id, 100);
```

Add clear documentation warnings:

```rust
/// Creates an in-memory channel pair for testing or in-process communication.
///
/// ⚠️ **Warning**: This creates a standalone channel that is NOT connected to
/// any transport. Messages sent through this channel will only be received by
/// the paired receiver in the same process. For network communication, use
/// `Endpoint::create_channel()` instead.
///
/// # Use Cases
/// - Unit testing
/// - In-process communication
/// - Examples demonstrating channel API only
```

### 2. Endpoint-Managed Channels (Recommended)

The proper way to create channels should be through the Endpoint:

```rust
// Endpoint manages both transport and channels
let endpoint = Endpoint::new(serializer, config);
endpoint.connect("127.0.0.1:8080").await?;

// Channel is automatically wired to the transport
let channel = endpoint.create_channel::<MyProtocol>().await?;
channel.send(msg).await?;  // Goes over the network!
```

### 3. Update Examples

Update all examples to either:
- Use `Channel::new_in_memory()` with clear comments explaining it's for demonstration
- Use the full Endpoint stack for realistic network communication examples

## Consequences

### Positive

1. **Clearer API**: Users understand when they're using in-memory vs network channels
2. **Better Examples**: Examples clearly show the difference between testing and production usage
3. **Prevents Misuse**: Harder to accidentally create orphaned channels
4. **Maintains Flexibility**: Still allows in-memory channels for testing

### Negative

1. **Breaking Change**: Existing code using `Channel::new()` needs to be updated
2. **More Verbose**: In-memory channels require longer method name
3. **Migration Effort**: All examples and tests need updating

### Neutral

1. **Documentation Burden**: Need to clearly explain both approaches
2. **Learning Curve**: Users need to understand when to use each approach

## Implementation Plan

### Phase 1: Add New API (Non-breaking)
1. Add `Channel::new_in_memory()` method
2. Add `Channel::with_features_in_memory()` method
3. Add deprecation warnings to `Channel::new()` and `Channel::with_features()`
4. Update documentation with warnings

### Phase 2: Update Examples
1. Update `hello_world.rs` to use `new_in_memory()` with clear comments
2. Update `channel_basics.rs` to use `new_in_memory()` with clear comments
3. Update `advanced_channels.rs` to use `new_in_memory()` with clear comments
4. Update `chat_server.rs` to use `new_in_memory()` with clear comments
5. Update `calculator.rs` to use `new_in_memory()` with clear comments
6. Create new example `network_chat.rs` using full Endpoint stack

### Phase 3: Deprecation (v0.2.0)
1. Mark `Channel::new()` as deprecated
2. Mark `Channel::with_features()` as deprecated
3. Update all internal code to use new methods

### Phase 4: Removal (v1.0.0)
1. Remove deprecated methods
2. Make `new_in_memory()` the only way to create standalone channels

## Alternative Considered

### Alternative 1: Remove Standalone Channels Entirely
**Rejected**: Too restrictive. In-memory channels are valuable for testing and examples.

### Alternative 2: Make Channel Constructor Private
**Rejected**: Would break too much existing code and testing infrastructure.

### Alternative 3: Add Runtime Checks
**Rejected**: Would add overhead and complexity. Better to make it clear at compile time.

## References

- ADR-001: Core Architecture
- Issue: Examples creating orphaned channels
- Discussion: Channel-Transport coupling in examples

## Notes

This ADR addresses a usability issue discovered during example development. The current API is technically correct but leads to confusion about when network I/O actually occurs.

The proposed solution maintains backward compatibility while making the API more explicit and harder to misuse.
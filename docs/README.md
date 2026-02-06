# BDRPC Documentation

This directory contains comprehensive documentation for the BDRPC (Bi-Directional RPC) framework.

## Directory Structure

```
docs/
├── README.md                    # This file
├── quick-start.md               # Getting started guide
├── architecture-guide.md        # Deep dive into design
├── performance-guide.md         # Optimization and tuning
├── protocol-evolution.md        # Evolving protocols over time
├── migration-guide.md           # Upgrading between versions
├── error-recovery.md            # Error handling best practices
├── deadlock-prevention.md       # Avoiding deadlocks
├── streaming-pattern.md         # AsyncRead streaming pattern
├── adr/                         # Architecture Decision Records
│   ├── ADR-001-core-architecture.md
│   ├── ADR-002-reconnection-strategy.md
│   ├── ADR-003-backpressure-flow-control.md
│   ├── ADR-004-error-handling-hierarchy.md
│   ├── ADR-005-serialization-strategy.md
│   ├── ADR-006-protocol-evolution-versioning.md
│   ├── ADR-007-message-ordering-guarantees.md
│   ├── ADR-008-protocol-directionality.md
│   ├── ADR-009-frame-integrity-checking.md
│   ├── ADR-010-dynamic-channel-negotiation.md
│   └── ADR-011-large-transfer-streaming.md
└── dev/                         # Development documentation
    ├── implementation-plan.md   # Phased implementation roadmap
    ├── implementation-overview.md
    ├── implementation-phases-0-5.md
    ├── implementation-phases-6-10.md
    ├── implementation-phases-11-15.md
    ├── performance-baseline.md  # Benchmark results
    └── open-questions.md        # Unresolved design questions
```

## Architecture Decision Records (ADRs)

ADRs document the key architectural decisions made during the design of BDRPC. Each ADR follows a standard format:
- **Status**: Accepted, Proposed, Deprecated, or Superseded
- **Context**: The problem or situation
- **Decision**: What was decided
- **Consequences**: Positive, negative, and neutral outcomes
- **Alternatives Considered**: Other options that were evaluated

### Core ADRs

1. **[ADR-001: Core Architecture](adr/ADR-001-core-architecture.md)**
   - Three-layer model: Transport → Protocol → Channel
   - Foundation for the entire framework
   - **Read this first** to understand the system

2. **[ADR-002: Reconnection Strategy](adr/ADR-002-reconnection-strategy.md)**
   - Pluggable reconnection strategies
   - Client-initiated connection management
   - Built-in strategies: ExponentialBackoff, FixedDelay, CircuitBreaker, NoReconnect

3. **[ADR-003: Backpressure and Flow Control](adr/ADR-003-backpressure-flow-control.md)**
   - Per-channel backpressure strategies
   - Built-in strategies: BoundedQueue, TokenBucket, SlidingWindow, AdaptiveWindow, Priority
   - Prevents memory exhaustion and enables flow control

4. **[ADR-004: Error Handling Hierarchy](adr/ADR-004-error-handling-hierarchy.md)**
   - Three-layer error model: Transport, Channel, Application
   - Error recovery strategies per layer
   - Observability and metrics

5. **[ADR-005: Serialization Strategy](adr/ADR-005-serialization-strategy.md)**
   - Per-endpoint serialization choice
   - Support for serde (bincode, JSON) and rkyv
   - Serializer negotiation during handshake

6. **[ADR-006: Protocol Evolution and Versioning](adr/ADR-006-protocol-evolution-versioning.md)**
   - Version negotiation and feature flags
   - Backward compatibility strategies
   - Deprecation workflow

7. **[ADR-007: Message Ordering Guarantees](adr/ADR-007-message-ordering-guarantees.md)**
   - FIFO ordering within channels
   - No ordering across channels or transports
   - Application-level ordering patterns

8. **[ADR-008: Protocol Directionality and Endpoint Capabilities](adr/ADR-008-protocol-directionality.md)**
   - Protocol direction support (CallOnly, RespondOnly, Bidirectional)
   - Endpoint capability registration and negotiation
   - Direction compatibility validation

9. **[ADR-009: Frame Integrity Checking](adr/ADR-009-frame-integrity-checking.md)**
   - Frame-level integrity validation
   - Checksum and CRC strategies
   - Error detection and recovery

10. **[ADR-010: Dynamic Channel Negotiation](adr/ADR-010-dynamic-channel-negotiation.md)**
   - System channel for control messages (Channel ID 0)
   - Dynamic channel creation after transport establishment
   - Channel negotiation protocol and lifecycle management
   - Support for multiplexing gateway pattern

## User Guides

### [Quick Start Guide](quick-start.md)

Get started with BDRPC in minutes:
- Installation instructions
- First BDRPC application walkthrough
- Key concepts explained
- Common patterns and examples
- Error handling basics
- Configuration options
- Troubleshooting tips

### [Architecture Guide](architecture-guide.md)

Deep dive into BDRPC's design and structure:
- Layered architecture overview (Transport, Channel, Endpoint)
- Core components detailed explanation
- Data flow and message lifecycle
- Design principles and philosophy
- Key features and capabilities
- Performance characteristics
- Links to all relevant ADRs

### [Performance Guide](performance-guide.md)

Optimize your BDRPC applications:
- Performance baseline metrics (4M+ msg/s throughput, 2-4µs latency)
- Serialization format selection and optimization
- Buffer management strategies
- Channel configuration best practices
- Transport selection guidelines
- Batching strategies for high throughput
- Monitoring and profiling techniques
- Performance tuning checklist

### [Protocol Evolution Guide](protocol-evolution.md)

Evolve your protocols safely over time:
- Versioning strategy and semantic versioning
- Evolution patterns (adding messages, optional fields, deprecation)
- Version negotiation during handshake
- Backward and forward compatibility
- Protocol adapters and migration helpers
- Testing compatibility across versions
- Best practices for long-lived systems

### [Migration Guide](migration-guide.md)

Upgrade between BDRPC versions:
- Version support policy
- Migration paths for each version
- Common migration patterns
- Breaking changes documentation
- Troubleshooting upgrade issues
- Testing your migration
- Rollback strategies

### [Error Recovery Guide](error-recovery.md)

Handle errors effectively:
- Error hierarchy (Transport, Channel, Application)
- Recovery strategies per layer
- Retry patterns and backoff
- Circuit breaker usage
- Error metrics and observability
- Best practices for resilient systems

### [Deadlock Prevention Guide](deadlock-prevention.md)

Avoid deadlocks in your applications:
- Common deadlock scenarios
- Timeout-based prevention
- Channel design patterns
- Best practices for async code
- Debugging deadlock issues

### [Streaming Pattern Guide](streaming-pattern.md)

Stream large data efficiently using AsyncRead:
- Chunked protocol pattern for streaming
- Client-side AsyncRead integration
- Server-side chunk reassembly
- Memory-efficient transfers (no full buffering)
- Support for unknown sizes (true streaming)
- Usage examples (files, network, compression)
- Comparison with other approaches
- Implementation checklist

## Development Documentation

### [Post-v0.1.0 Roadmap](dev/post-v0.1.0-roadmap.md)

Future features and enhancements planned for BDRPC after the v0.1.0 release:

- Outstanding v0.1.0 items (debugging tools, fuzz testing, stress testing)
- New features (WebSocket, QUIC, authentication, service mesh)
- Ecosystem integration (gRPC compatibility, OpenTelemetry)
- Community building and governance

### [v0.1.0 Implementation Plan (Archived)](dev/archive/implementation-plan-v0.1.0.md)

The original 15-phase implementation roadmap that guided BDRPC to its initial release:

- **Phase 0**: Project Setup
- **Phase 1**: Core Transport Layer
- **Phase 2**: Serialization Layer
- **Phase 3**: Protocol Generation (proc macros)
- **Phase 4**: Channel Layer
- **Phase 5**: Endpoint
- **Phase 6**: Error Handling
- **Phase 7**: Reconnection Strategy
- **Phase 8**: Backpressure
- **Phase 9**: Protocol Evolution
- **Phase 10**: Advanced Transports (TLS, Compression)
- **Phase 11**: Observability
- **Phase 12**: Performance Optimization
- **Phase 13**: Documentation & Examples
- **Phase 14**: Testing & Hardening
- **Phase 15**: Release Preparation

Each phase includes:
- Clear goals and deliverables
- Detailed task breakdown
- Dependencies on previous phases
- Success criteria

### [Open Questions](dev/open-questions.md)

Unresolved design questions organized by category:

- **Architecture & Design**: Cross-channel fairness, dynamic strategy switching, version negotiation
- **Performance & Scalability**: Buffer pooling, zero-copy optimization, batch processing
- **Security & Reliability**: Authentication, rate limiting, circuit breakers
- **Observability**: Distributed tracing, metrics granularity
- **API Design**: Async traits, builder patterns, error ergonomics
- **Testing & Quality**: Fuzz testing, property-based testing
- **Ecosystem Integration**: gRPC compatibility, service mesh integration
- **Documentation & Community**: Documentation format, community platform

Questions are prioritized (High/Medium/Low) and linked to implementation phases.

## How to Use This Documentation

### For Contributors

1. **Start with ADR-001** to understand the core architecture
2. **Read relevant ADRs** for the area you're working on
3. **Check the implementation plan** to see what phase you're in
4. **Review open questions** that might affect your work
5. **Update ADRs** when making significant design changes

### For Users

1. **Read ADR-001** for system overview
2. **Browse other ADRs** to understand design decisions
3. **Check open questions** if you have feedback on design choices
4. **Refer to implementation plan** to see what's coming

### For Reviewers

1. **Review ADRs** for architectural soundness
2. **Evaluate implementation plan** for feasibility
3. **Provide input on open questions** based on your experience
4. **Suggest additional ADRs** for missing decisions

## Contributing to Documentation

### Adding a New ADR

1. Create a new file: `docs/adr/ADR-XXX-title.md`
2. Use the next sequential number
3. Follow the ADR template structure
4. Link to related ADRs
5. Update this README

### ADR Template

```markdown
# ADR-XXX: Title

## Status
[Proposed | Accepted | Deprecated | Superseded by ADR-YYY]

## Context
What is the issue we're addressing?

## Decision
What did we decide?

## Consequences
### Positive
- Good outcome 1
- Good outcome 2

### Negative
- Trade-off 1
- Trade-off 2

### Neutral
- Observation 1

## Alternatives Considered
### Option A
Why we didn't choose this

### Option B
Why we didn't choose this

## Implementation Notes
Technical details for implementers

## Related ADRs
- ADR-XXX: Related decision

## References
- External links
```

### Updating the Implementation Plan

- Keep task status current
- Add discovered tasks as needed
- Update time estimates based on progress
- Document blockers and risks

### Adding Open Questions

- Use clear, specific questions
- Provide context and options
- Link to relevant ADRs
- Prioritize appropriately
- Update when questions are resolved

## Documentation Principles

1. **Clarity**: Write for readers unfamiliar with the project
2. **Completeness**: Document the "why" not just the "what"
3. **Currency**: Keep docs up-to-date with code
4. **Traceability**: Link decisions to their rationale
5. **Accessibility**: Use clear language and examples

## Questions or Feedback?

- Open a GitHub issue for specific questions
- Start a GitHub discussion for broader topics
- Submit a PR to improve documentation
- Reach out to maintainers

---

**Last Updated**: 2026-02-06

**Documentation Version**: 0.1.0 (Pre-release)
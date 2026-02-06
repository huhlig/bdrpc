# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-02-06

### Summary
Initial release of BDRPC - a high-performance, bi-directional RPC framework for Rust. This release includes a complete implementation of the core framework with 367 passing tests, 83% code coverage, and comprehensive documentation.

### Added - Phase 14: Testing & Hardening (2026-02-06)
- **Code Coverage Analysis**
  - 83% overall coverage (regions), 82% line coverage
  - 367 tests passing (321 library + 46 integration)
  - Generated HTML coverage reports with cargo-llvm-cov
  - Identified coverage gaps for future improvement
- **Bug Fixes**
  - Fixed 26 tracing-related compilation errors
  - Fixed all 12 clippy warnings
  - Zero unsafe code, zero warnings
  - Clean compilation with all features enabled

### Added - Phase 13: Documentation & Examples (2026-02-05)
- **Complete API Documentation**
  - All 9 modules fully documented with examples
  - lib.rs with comprehensive quick start guide
  - Module-level documentation for all public APIs
- **User Guides** (2,551 lines total)
  - Quick Start Guide (368 lines)
  - Architecture Guide (476 lines)
  - Performance Guide (545 lines)
  - Protocol Evolution Guide (642 lines)
  - Migration Guide (520 lines)
  - Deadlock Prevention Guide
- **Examples** (6 working examples)
  - hello_world.rs - Full stack demonstration
  - channel_basics.rs - Channel-only usage
  - advanced_channels.rs - Channel management patterns
  - calculator.rs - Bi-directional RPC over TCP
  - chat_server.rs - Multiple concurrent clients
  - file_transfer.rs - Streaming large files with progress tracking

### Added - Phase 12: Performance Optimization (2026-02-05)
- **Benchmarking Infrastructure**
  - Throughput benchmarks (small, medium, large messages, batching)
  - Latency benchmarks (send, round-trip, serialization)
  - Performance baseline documentation
- **Buffer Pool Optimization**
  - Thread-safe buffer pooling with 7 size classes (256B to 1MB)
  - Lock-free design using parking_lot::Mutex
  - Zero-copy conversion via Into<Vec<u8>>
  - Automatic buffer return on drop
- **Performance Results** (exceeds all goals)
  - Batch throughput: 4.02M messages/second (goal: 1M)
  - Single message: 481K messages/second
  - Latency: 2-4 µs (goal: <100µs)
  - Channel creation: 173 ns overhead

### Added - Phase 11: Observability (2026-02-04)
- **Metrics System**
  - TransportMetrics: connections, bytes, errors
  - ChannelMetrics: messages, latency, errors
  - Integration with metrics crate (conditional)
  - 20 comprehensive metrics tests
- **Distributed Tracing**
  - Structured logging with tracing crate
  - Instrumented all transport, channel, and endpoint operations
  - CorrelationId type for request tracking
  - CorrelationContext for async-safe propagation
  - 14 correlation tests
- **Health Checks**
  - HealthStatus enum (Healthy, Degraded, Unhealthy)
  - HealthCheck type with thread-safe state management
  - Liveness and readiness probes
  - Kubernetes-compatible health endpoints
  - 13 health check tests

### Added - Phase 10: Advanced Transports (2026-02-04)
- **TLS Transport**
  - Secure TCP connections with rustls
  - Client and server TLS support
  - Certificate validation
  - 2 TLS tests
- **Compression Transport**
  - Gzip, Zstd, LZ4 compression algorithms
  - Configurable compression levels
  - Transparent compression/decompression
  - 14 compression tests

### Added - Phase 9: Protocol Evolution (2026-02-03)
- **Versioning System**
  - Semantic versioning for protocols
  - Version negotiation during handshake
  - Backward compatibility support
  - 12 versioning tests

### Added - Phase 8: Backpressure & Flow Control (2026-02-03)
- **Backpressure Strategies**
  - BoundedQueue: Configurable queue limits
  - Unlimited: No backpressure
  - Pluggable backpressure trait
  - 15 backpressure tests

### Added - Phase 7: Reconnection Strategies (2026-02-03)
- **Reconnection Implementations**
  - ExponentialBackoff: Exponential retry delays
  - FixedDelay: Constant retry intervals
  - CircuitBreaker: Failure threshold with half-open state
  - NoReconnect: Disable reconnection
  - 48 reconnection tests

### Added - Phase 6: Error Handling (2026-02-02)
- **Three-Layer Error Hierarchy**
  - TransportError: Network-level errors
  - ChannelError: Channel-level errors
  - ApplicationError: User-defined errors
  - Error classification (recoverable, should_close)
  - 26 error handling tests

### Added - Phase 5: Endpoint Layer (2026-02-02)
- **Endpoint Implementation**
  - Connection management
  - Protocol negotiation
  - Handshake protocol
  - Channel negotiation
  - 44 endpoint tests

### Added - Phase 4: Channel Layer (2026-02-01)
- **Channel System**
  - Type-safe message passing
  - FIFO ordering guarantees
  - Feature negotiation
  - Channel multiplexing
  - ChannelManager for lifecycle management
  - 35 channel tests

### Added - Phase 3: Protocol Generation (2026-01-31)
- **Procedural Macros**
  - #[derive(Protocol)] macro
  - Automatic method name generation
  - Protocol trait implementation
  - Backward compatibility support

### Added - Phase 2: Serialization Layer (2026-01-30)
- **Serialization Trait System**
  - Serializer trait for pluggable serialization backends
  - SerializationError and DeserializationError types
  - Comprehensive documentation with examples
- **Serializer Implementations**
  - PostcardSerializer: Compact binary format (default, no_std compatible)
  - JsonSerializer: Human-readable format with pretty-print support
  - RkyvSerializer: Placeholder for future zero-copy implementation
- **Message Framing**
  - Length-prefixed framing protocol (u32 big-endian + payload)
  - write_frame / read_frame for raw frame operations
  - write_message / read_message for serialized messages
  - 16MB maximum frame size for DoS protection
  - 13 comprehensive framing tests
- **Test Coverage**
  - 36 serialization tests (21 serializer + 13 framing + 2 error)

### Added - Phase 1: Transport Layer (2026-01-29)
- **Core Transport Abstractions**
  - Transport trait for pluggable transports
  - TcpTransport: Production TCP implementation
  - MemoryTransport: In-memory testing transport
  - TransportManager: Lifecycle management
  - 34 transport tests

### Added - Phase 0: Project Setup (2026-01-28)
- **Project Infrastructure**
  - Workspace with bdrpc and bdrpc-macros crates
  - Rust 2024 edition
  - Comprehensive Cargo.toml with feature flags
  - CI/CD setup
  - Development documentation structure

### Architecture Decision Records
- **ADR-001**: Core Architecture - Three-layer design (Transport, Channel, Application)
- **ADR-002**: Reconnection Strategy - Pluggable reconnection with circuit breakers
- **ADR-003**: Backpressure and Flow Control - Bounded queues and flow control
- **ADR-004**: Error Handling Hierarchy - Three-layer error classification
- **ADR-005**: Serialization Strategy - Pluggable serialization formats
- **ADR-006**: Protocol Evolution and Versioning - Semantic versioning for protocols
- **ADR-007**: Message Ordering Guarantees - FIFO ordering within channels
- **ADR-008**: Protocol Directionality - CallOnly, RespondOnly, Bidirectional
- **ADR-009**: Frame Integrity Checking - Transport-layer integrity by default
- **ADR-010**: Dynamic Channel Negotiation - Runtime channel creation
- **ADR-011**: Large Transfer Streaming - Chunked streaming for large data
- **ADR-012**: Channel-Transport Coupling - Loose coupling via managers

### Quality Metrics
- **Tests**: 367 passing (321 library + 46 integration)
- **Code Coverage**: 83% overall, 82% lines
- **Unsafe Code**: Zero unsafe blocks
- **Clippy Warnings**: Zero warnings
- **Documentation**: 100% of public APIs documented
- **Performance**: 4.02M msg/s throughput, 2-4µs latency

### Breaking Changes
None - this is the initial release.

### Known Limitations
- TLS transport has limited test coverage (requires certificate setup)
- Endpoint.rs has 63% coverage (complex async logic)
- Fuzz testing and stress testing deferred to post-v0.1.0
- WebSocket and QUIC transports planned for v0.2.0

### Migration Guide
Not applicable for initial release. See [Migration Guide](docs/migration-guide.md) for future version upgrades.

### Contributors
- Hans Uhlig (@huhlig) - Initial implementation and design

---

## [Unreleased]

### Planned for v0.2.0
- WebSocket transport
- QUIC transport
- Enhanced fuzz testing
- Stress testing suite
- Improved endpoint test coverage
- Additional compression algorithms (Snappy, Brotli)
- gRPC compatibility layer
- Schema registry
- Service mesh integration

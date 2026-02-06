# ADR-009: Frame Integrity Checking Strategy

**Status:** Accepted  
**Date:** 2026-02-05  
**Deciders:** Architecture Team  
**Related:** ADR-001 (Core Architecture), ADR-005 (Serialization Strategy)

## Context

The message framing layer needs to decide whether to include integrity checks (like CRC32 or checksums) in the frame format, or rely on the underlying transport's reliability guarantees.

### Current Frame Format
```
+------------------+------------------+
| Length (4 bytes) | Payload (N bytes) |
+------------------+------------------+
```

### Options Considered

1. **No integrity check** - Rely on transport layer
2. **Optional CRC32** - Add 4-byte checksum when enabled
3. **Always include CRC32** - Mandatory integrity check
4. **Cryptographic hash** - Use stronger hash like BLAKE3

## Decision

**We will rely on the underlying transport's reliability guarantees and NOT include integrity checks in the framing layer by default.**

However, we will provide an **optional** integrity checking mode that can be enabled per-endpoint for scenarios requiring additional protection.

### Rationale

#### 1. Transport Layer Guarantees

Most transports already provide strong integrity guarantees:

- **TCP**: Built-in checksums and reliable delivery
- **TLS/TCP**: Additional cryptographic integrity via MAC
- **QUIC**: Built-in integrity and encryption
- **In-memory**: No corruption possible

Adding redundant checks wastes CPU and bandwidth.

#### 2. Performance Impact

CRC32 computation adds overhead:
- ~1-2 GB/s throughput on modern CPUs
- For high-throughput scenarios (>100K msg/s), this is measurable
- Length-only framing is essentially zero-cost

#### 3. Layered Architecture

Following the principle of separation of concerns:
- **Transport layer**: Handles reliability and integrity
- **Framing layer**: Handles message boundaries
- **Serialization layer**: Handles data format

Each layer should focus on its responsibility.

#### 4. Flexibility for Special Cases

Some scenarios DO benefit from application-level integrity:
- Unreliable transports (UDP-based)
- Long-lived connections with potential memory corruption
- Debugging transport issues
- Compliance requirements

We address this with **optional** integrity checking.

### Implementation Strategy

#### Phase 2 (Current): No Integrity Checks
```rust
// Current simple framing
pub async fn write_frame<W>(writer: &mut W, payload: &[u8]) -> Result<()>
pub async fn read_frame<R>(reader: &mut R) -> Result<Vec<u8>>
```

#### Future Enhancement: Optional Integrity
```rust
pub struct FramingOptions {
    pub integrity_check: IntegrityCheck,
    pub max_frame_size: u32,
}

pub enum IntegrityCheck {
    None,           // Default: rely on transport
    Crc32,          // Fast checksum
    XxHash64,       // Faster, better distribution
    Blake3,         // Cryptographic (for untrusted transports)
}

// Enhanced frame format when integrity checking enabled:
// +--------+--------+---------+----------+
// | Length | CRC32  | Payload | Checksum |
// +--------+--------+---------+----------+
//   4 bytes  4 bytes  N bytes   4 bytes
```

## Consequences

### Positive

1. **Better performance**: No redundant integrity checks on reliable transports
2. **Simpler implementation**: Less code, fewer edge cases
3. **Lower latency**: Reduced per-message overhead
4. **Cleaner architecture**: Clear separation of concerns
5. **Flexibility**: Can add integrity checks later without breaking changes

### Negative

1. **No protection against bugs**: Transport bugs or memory corruption undetected
2. **Debugging harder**: Can't distinguish transport vs. application issues
3. **Compliance gaps**: Some industries require end-to-end integrity

### Mitigations

1. **Comprehensive testing**: Extensive transport layer tests
2. **Optional integrity mode**: Available when needed
3. **Serialization validation**: Postcard/JSON have built-in format validation
4. **Monitoring**: Track deserialization errors as proxy for corruption
5. **Documentation**: Clearly document when to enable integrity checks

## Examples

### Default Usage (No Integrity Check)
```rust
// Relies on TCP's built-in integrity
let mut stream = TcpTransport::connect("localhost:8080").await?;
write_frame(&mut stream, &data).await?;
```

### With Integrity Checking (Future)
```rust
// For unreliable transports or compliance
let options = FramingOptions {
    integrity_check: IntegrityCheck::Crc32,
    max_frame_size: MAX_FRAME_SIZE,
};
let framer = Framer::new(stream, options);
framer.write_frame(&data).await?;
```

### When to Enable Integrity Checks

**Enable when:**
- Using unreliable transports (UDP, custom protocols)
- Compliance requirements mandate end-to-end integrity
- Debugging suspected transport issues
- Long-lived connections (days/weeks)
- Untrusted network paths

**Don't enable when:**
- Using TCP/TLS (redundant)
- High-throughput scenarios (performance cost)
- In-memory transports (no corruption possible)
- Short-lived connections

## Related Decisions

- **ADR-001**: Established layered architecture
- **ADR-005**: Serialization strategy (Postcard has format validation)
- **Future ADR**: UDP transport will require integrity checks

## References

- [TCP Checksum](https://datatracker.ietf.org/doc/html/rfc793#section-3.1)
- [TLS Record Protocol](https://datatracker.ietf.org/doc/html/rfc8446#section-5.2)
- [End-to-End Arguments in System Design](https://web.mit.edu/Saltzer/www/publications/endtoend/endtoend.pdf)
- [CRC32 Performance](https://create.stephan-brumme.com/crc32/)

## Notes

This decision can be revisited if:
1. We observe corruption in production
2. Users request integrity checking
3. We add unreliable transport support
4. Compliance requirements change

The architecture supports adding integrity checks without breaking existing code.
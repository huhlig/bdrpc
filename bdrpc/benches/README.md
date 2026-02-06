# BDRPC Benchmarks

This directory contains performance benchmarks for BDRPC using the Criterion framework.

## Available Benchmarks

### Throughput Benchmarks (`throughput.rs`)

Measures messages per second for various scenarios:

- **Small messages** (100 bytes): Tests overhead with minimal payload
- **Medium messages** (1 KB): Typical message size for many applications
- **Large messages** (10 KB): Tests performance with larger payloads
- **Batch throughput**: Tests sending multiple messages (10, 100, 1000)
- **Channel creation**: Measures overhead of creating new channels

### Latency Benchmarks (`latency.rs`)

Measures message latency (p50, p95, p99 percentiles):

- **One-way send latency**: Time to send a message through a channel
- **Round-trip latency**: Time for request-response pattern
- **Channel creation latency**: Time to create a new channel
- **Serialization latency**: Time to serialize messages (JSON)
- **Deserialization latency**: Time to deserialize messages (JSON)

Tests multiple message sizes: 100 bytes, 1 KB, 10 KB

## Running Benchmarks

### Run All Benchmarks

```bash
cargo bench
```

### Run Specific Benchmark Suite

```bash
# Throughput benchmarks only
cargo bench --bench throughput

# Latency benchmarks only
cargo bench --bench latency
```

### Run Specific Benchmark

```bash
# Run only small message throughput
cargo bench --bench throughput -- channel_throughput_small

# Run only send latency
cargo bench --bench latency -- latency_send
```

### Save Baseline for Comparison

```bash
# Save current performance as baseline
cargo bench -- --save-baseline main

# Compare against baseline
cargo bench -- --baseline main
```

## Interpreting Results

Criterion provides detailed statistics including:

- **Mean**: Average time per operation
- **Std Dev**: Standard deviation
- **Median**: 50th percentile (p50)
- **MAD**: Median Absolute Deviation
- **p95/p99**: 95th and 99th percentiles

### Example Output

```
channel_throughput_small/send_recv_100_bytes
                        time:   [45.234 µs 45.891 µs 46.612 µs]
                        thrpt:  [21.45 Kelem/s 21.79 Kelem/s 22.10 Kelem/s]
```

This shows:
- Mean time: ~45.9 µs per message
- Throughput: ~21.79K messages/second
- Confidence interval: [45.234 µs, 46.612 µs]

## Performance Goals

Target performance metrics (to be established):

- **Throughput**: 
  - Small messages: > 50K msg/s
  - Medium messages: > 20K msg/s
  - Large messages: > 5K msg/s

- **Latency** (p99):
  - Small messages: < 100 µs
  - Medium messages: < 200 µs
  - Large messages: < 1 ms

## Profiling

For detailed profiling, use:

```bash
# CPU profiling with flamegraph
cargo bench --bench throughput -- --profile-time=10

# Memory profiling (requires additional tools)
# See: https://github.com/koute/memory-profiler
```

## Continuous Benchmarking

Benchmarks should be run:

1. **Before optimization**: Establish baseline
2. **After optimization**: Verify improvements
3. **Before releases**: Ensure no regressions
4. **On CI**: Track performance over time (optional)

## Notes

- Benchmarks use in-memory channels for consistent results
- Results may vary based on system load and hardware
- Run benchmarks on a quiet system for best accuracy
- Criterion automatically detects performance changes
- HTML reports are generated in `target/criterion/`

## Adding New Benchmarks

To add a new benchmark:

1. Create a new file in `bdrpc/benches/`
2. Use the Criterion framework
3. Follow existing patterns for consistency
4. Document the benchmark purpose
5. Update this README

Example:

```rust
use criterion::{criterion_group, criterion_main, Criterion};

fn my_benchmark(c: &mut Criterion) {
    c.bench_function("my_test", |b| {
        b.iter(|| {
            // Code to benchmark
        });
    });
}

criterion_group!(benches, my_benchmark);
criterion_main!(benches);
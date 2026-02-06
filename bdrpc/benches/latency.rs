//! Latency benchmarks for BDRPC
//!
//! Measures message latency (p50, p95, p99) for various scenarios:
//! - Round-trip latency
//! - One-way send latency
//! - Different message sizes

use bdrpc::channel::{Channel, ChannelId, Protocol};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::{Duration, Instant};

/// Simple message for latency testing
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LatencyMessage {
    id: u64,
    timestamp: u128, // Nanoseconds since epoch
    data: Vec<u8>,
}

impl LatencyMessage {
    fn new(id: u64, size: usize) -> Self {
        Self {
            id,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            data: vec![0u8; size],
        }
    }
}

impl Protocol for LatencyMessage {
    fn method_name(&self) -> &'static str {
        "latency_message"
    }

    fn is_request(&self) -> bool {
        true
    }
}

/// Benchmark one-way send latency
fn bench_send_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("latency_send");

    for size in [100, 1024, 10240].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}bytes", size)),
            size,
            |b, &size| {
                let rt = tokio::runtime::Runtime::new().unwrap();

                b.to_async(&rt).iter(|| async move {
                    // Create channel
                    let (sender, mut receiver) = Channel::<LatencyMessage>::with_features_in_memory(
                        ChannelId::new(),
                        100,
                        HashSet::new(),
                    );

                    // Spawn receiver task
                    let recv_handle = tokio::spawn(async move { receiver.recv().await.unwrap() });

                    // Measure send time
                    let start = Instant::now();
                    let msg = LatencyMessage::new(1, size);
                    sender.send(msg).await.unwrap();
                    let send_duration = start.elapsed();

                    // Wait for receiver
                    recv_handle.await.unwrap();

                    send_duration
                });
            },
        );
    }

    group.finish();
}

/// Benchmark round-trip latency
fn bench_roundtrip_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("latency_roundtrip");

    for size in [100, 1024, 10240].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}bytes", size)),
            size,
            |b, &size| {
                let rt = tokio::runtime::Runtime::new().unwrap();

                b.to_async(&rt).iter(|| async move {
                    // Create bidirectional channels
                    let (client_sender, mut client_receiver) =
                        Channel::<LatencyMessage>::with_features_in_memory(
                            ChannelId::new(),
                            100,
                            HashSet::new(),
                        );

                    let (server_sender, mut server_receiver) =
                        Channel::<LatencyMessage>::with_features_in_memory(
                            ChannelId::new(),
                            100,
                            HashSet::new(),
                        );

                    // Spawn echo server
                    let echo_handle = tokio::spawn(async move {
                        let msg = server_receiver.recv().await.unwrap();
                        server_sender.send(msg).await.unwrap();
                    });

                    // Measure round-trip time
                    let start = Instant::now();

                    // Send request
                    let msg = LatencyMessage::new(1, size);
                    client_sender.send(msg).await.unwrap();

                    // Wait for response
                    client_receiver.recv().await.unwrap();

                    let roundtrip_duration = start.elapsed();

                    // Wait for echo server
                    echo_handle.await.unwrap();

                    roundtrip_duration
                });
            },
        );
    }

    group.finish();
}

/// Benchmark channel creation latency
fn bench_channel_creation_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("latency_channel_creation");

    group.bench_function("create_channel", |b| {
        b.iter(|| {
            let start = Instant::now();
            let (_sender, _receiver) = Channel::<LatencyMessage>::with_features_in_memory(
                ChannelId::new(),
                100,
                HashSet::new(),
            );
            start.elapsed()
        });
    });

    group.finish();
}

/// Benchmark message serialization latency
fn bench_serialization_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("latency_serialization");

    for size in [100, 1024, 10240].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}bytes", size)),
            size,
            |b, &size| {
                b.iter(|| {
                    let msg = LatencyMessage::new(1, size);
                    let start = Instant::now();
                    let _serialized = serde_json::to_vec(&msg).unwrap();
                    start.elapsed()
                });
            },
        );
    }

    group.finish();
}

/// Benchmark message deserialization latency
fn bench_deserialization_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("latency_deserialization");

    for size in [100, 1024, 10240].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}bytes", size)),
            size,
            |b, &size| {
                // Pre-serialize message
                let msg = LatencyMessage::new(1, size);
                let serialized = serde_json::to_vec(&msg).unwrap();

                b.iter(|| {
                    let start = Instant::now();
                    let _deserialized: LatencyMessage =
                        serde_json::from_slice(&serialized).unwrap();
                    start.elapsed()
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_send_latency,
    bench_roundtrip_latency,
    bench_channel_creation_latency,
    bench_serialization_latency,
    bench_deserialization_latency
);
criterion_main!(benches);

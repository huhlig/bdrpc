//! Throughput benchmarks for BDRPC
//!
//! Measures messages per second for various scenarios:
//! - Small messages (100 bytes)
//! - Medium messages (1 KB)
//! - Large messages (10 KB)
//! - Batch processing

use bdrpc::channel::{Channel, ChannelId, Protocol};
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;

/// Small message payload (100 bytes)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SmallMessage {
    id: u64,
    data: Vec<u8>, // ~92 bytes for total of ~100 bytes
}

impl SmallMessage {
    fn new(id: u64) -> Self {
        Self {
            id,
            data: vec![0u8; 92],
        }
    }
}

impl Protocol for SmallMessage {
    fn method_name(&self) -> &'static str {
        "small_message"
    }

    fn is_request(&self) -> bool {
        true
    }
}

/// Medium message payload (1 KB)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MediumMessage {
    id: u64,
    data: Vec<u8>, // ~1016 bytes for total of ~1 KB
}

impl MediumMessage {
    fn new(id: u64) -> Self {
        Self {
            id,
            data: vec![0u8; 1016],
        }
    }
}

impl Protocol for MediumMessage {
    fn method_name(&self) -> &'static str {
        "medium_message"
    }

    fn is_request(&self) -> bool {
        true
    }
}

/// Large message payload (10 KB)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LargeMessage {
    id: u64,
    data: Vec<u8>, // ~10232 bytes for total of ~10 KB
}

impl LargeMessage {
    fn new(id: u64) -> Self {
        Self {
            id,
            data: vec![0u8; 10232],
        }
    }
}

impl Protocol for LargeMessage {
    fn method_name(&self) -> &'static str {
        "large_message"
    }

    fn is_request(&self) -> bool {
        true
    }
}

/// Benchmark sending small messages through a channel
fn bench_small_messages(c: &mut Criterion) {
    let mut group = c.benchmark_group("channel_throughput_small");
    group.throughput(Throughput::Elements(1));
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("send_recv_100_bytes", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();

        b.to_async(&rt).iter(|| async {
            // Create channel
            let (sender, mut receiver) = Channel::<SmallMessage>::with_features_in_memory(
                ChannelId::new(),
                100,
                HashSet::new(),
            );

            // Spawn receiver task
            let recv_handle = tokio::spawn(async move { receiver.recv().await.unwrap() });

            // Send message
            let msg = SmallMessage::new(1);
            sender.send(msg).await.unwrap();

            // Wait for receiver
            let received = recv_handle.await.unwrap();
            black_box(received);
        });
    });

    group.finish();
}

/// Benchmark sending medium messages through a channel
fn bench_medium_messages(c: &mut Criterion) {
    let mut group = c.benchmark_group("channel_throughput_medium");
    group.throughput(Throughput::Elements(1));
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("send_recv_1kb", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();

        b.to_async(&rt).iter(|| async {
            // Create channel
            let (sender, mut receiver) = Channel::<MediumMessage>::with_features_in_memory(
                ChannelId::new(),
                100,
                HashSet::new(),
            );

            // Spawn receiver task
            let recv_handle = tokio::spawn(async move { receiver.recv().await.unwrap() });

            // Send message
            let msg = MediumMessage::new(1);
            sender.send(msg).await.unwrap();

            // Wait for receiver
            let received = recv_handle.await.unwrap();
            black_box(received);
        });
    });

    group.finish();
}

/// Benchmark sending large messages through a channel
fn bench_large_messages(c: &mut Criterion) {
    let mut group = c.benchmark_group("channel_throughput_large");
    group.throughput(Throughput::Elements(1));
    group.measurement_time(Duration::from_secs(10));

    group.bench_function("send_recv_10kb", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();

        b.to_async(&rt).iter(|| async {
            // Create channel
            let (sender, mut receiver) = Channel::<LargeMessage>::with_features_in_memory(
                ChannelId::new(),
                100,
                HashSet::new(),
            );

            // Spawn receiver task
            let recv_handle = tokio::spawn(async move { receiver.recv().await.unwrap() });

            // Send message
            let msg = LargeMessage::new(1);
            sender.send(msg).await.unwrap();

            // Wait for receiver
            let received = recv_handle.await.unwrap();
            black_box(received);
        });
    });

    group.finish();
}

/// Benchmark batch sending through a channel
fn bench_batch_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("channel_throughput_batch");

    for batch_size in [10, 100, 1000].iter() {
        group.throughput(Throughput::Elements(*batch_size));
        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            batch_size,
            |b, &batch_size| {
                let rt = tokio::runtime::Runtime::new().unwrap();

                b.to_async(&rt).iter(|| async move {
                    // Create channel with larger buffer for batch
                    let buffer_size = (batch_size as usize) + 10;
                    let (sender, mut receiver) = Channel::<SmallMessage>::with_features_in_memory(
                        ChannelId::new(),
                        buffer_size,
                        HashSet::new(),
                    );

                    // Spawn receiver task
                    let recv_handle = tokio::spawn(async move {
                        for _ in 0..batch_size {
                            let _ = receiver.recv().await.unwrap();
                        }
                    });

                    // Send batch
                    for i in 0..batch_size {
                        let msg = SmallMessage::new(i);
                        sender.send(msg).await.unwrap();
                    }

                    // Wait for receiver
                    recv_handle.await.unwrap();
                });
            },
        );
    }

    group.finish();
}

/// Benchmark channel creation overhead
fn bench_channel_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("channel_creation");
    group.throughput(Throughput::Elements(1));

    group.bench_function("create_channel", |b| {
        b.iter(|| {
            let (_sender, _receiver) = Channel::<SmallMessage>::with_features_in_memory(
                ChannelId::new(),
                100,
                HashSet::new(),
            );
            black_box((_sender, _receiver));
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_small_messages,
    bench_medium_messages,
    bench_large_messages,
    bench_batch_throughput,
    bench_channel_creation
);
criterion_main!(benches);

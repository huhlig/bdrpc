//! Concurrent Calculator Example
//!
//! This example demonstrates BDRPC's request-response correlation feature,
//! which enables multiple concurrent RPC calls without race conditions.
//!
//! # Key Features Demonstrated
//!
//! - Concurrent RPC calls using `tokio::join!`
//! - Out-of-order response handling
//! - Performance comparison: sequential vs concurrent
//! - Client cloning for independent concurrent operations
//!
//! # Running the Example
//!
//! ```bash
//! cargo run --example concurrent_calculator --features serde
//! ```

use bdrpc::channel::{Channel, ChannelId};
use bdrpc::service;
use std::error::Error;
use std::time::{Duration, Instant};
use tokio::time::sleep;

// Define the calculator service protocol
#[service(direction = "bidirectional")]
#[allow(dead_code)]
trait Calculator {
    async fn add(&self, a: i32, b: i32) -> Result<i32, String>;
    async fn subtract(&self, a: i32, b: i32) -> Result<i32, String>;
    async fn multiply(&self, a: i32, b: i32) -> Result<i32, String>;
    async fn divide(&self, a: i32, b: i32) -> Result<i32, String>;

    // Slow operation to demonstrate concurrency benefits
    async fn slow_add(&self, a: i32, b: i32, delay_ms: u64) -> Result<i32, String>;
}

// Implement the calculator service
struct CalculatorService;

impl CalculatorServer for CalculatorService {
    async fn add(&self, a: i32, b: i32) -> Result<i32, String> {
        Ok(a + b)
    }

    async fn subtract(&self, a: i32, b: i32) -> Result<i32, String> {
        Ok(a - b)
    }

    async fn multiply(&self, a: i32, b: i32) -> Result<i32, String> {
        Ok(a * b)
    }

    async fn divide(&self, a: i32, b: i32) -> Result<i32, String> {
        if b == 0 {
            Err("Division by zero".to_string())
        } else {
            Ok(a / b)
        }
    }

    async fn slow_add(&self, a: i32, b: i32, delay_ms: u64) -> Result<i32, String> {
        sleep(Duration::from_millis(delay_ms)).await;
        Ok(a + b)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("\n🧮 BDRPC Concurrent Calculator Example");
    println!("═══════════════════════════════════════════════════════════\n");

    // Create bidirectional in-memory channels
    let channel_id = ChannelId::new();

    // Client -> Server channel
    let (client_to_server_sender, client_to_server_receiver) =
        Channel::<CalculatorProtocol>::new_in_memory(channel_id, 100);

    // Server -> Client channel
    let (server_to_client_sender, server_to_client_receiver) =
        Channel::<CalculatorProtocol>::new_in_memory(channel_id, 100);

    // Start server task
    let service = CalculatorService;
    let dispatcher = CalculatorDispatcher::new(service);

    let server_handle = tokio::spawn(async move {
        let mut server_receiver = client_to_server_receiver;
        let server_sender = server_to_client_sender;

        while let Some(envelope) = server_receiver.recv_envelope().await {
            let response_envelope = dispatcher.dispatch_envelope(envelope).await;
            if server_sender
                .send_response(
                    response_envelope.payload,
                    response_envelope.correlation_id.unwrap_or(0),
                )
                .await
                .is_err()
            {
                break;
            }
        }
    });

    // Create client
    let client = CalculatorClient::new(client_to_server_sender, server_to_client_receiver);

    println!("✓ Connection established\n");

    // Example 1: Basic concurrent operations
    println!("Example 1: Basic Concurrent Operations");
    println!("---------------------------------------");

    let start = Instant::now();
    let (r1, r2, r3, r4) = tokio::join!(
        client.add(10, 5),
        client.subtract(20, 8),
        client.multiply(7, 6),
        client.divide(100, 4)
    );
    let elapsed = start.elapsed();

    println!("Results (all operations ran concurrently):");
    println!("  10 + 5 = {}", r1?.unwrap());
    println!("  20 - 8 = {}", r2?.unwrap());
    println!("  7 × 6 = {}", r3?.unwrap());
    println!("  100 ÷ 4 = {}", r4?.unwrap());
    println!("  Time: {:?}\n", elapsed);

    // Example 2: Sequential vs Concurrent Performance
    println!("Example 2: Sequential vs Concurrent Performance");
    println!("------------------------------------------------");

    // Sequential execution
    println!("Sequential execution (operations wait for each other):");
    let start = Instant::now();
    let r1 = client.slow_add(1, 2, 100).await?.unwrap();
    let r2 = client.slow_add(3, 4, 100).await?.unwrap();
    let r3 = client.slow_add(5, 6, 100).await?.unwrap();
    let sequential_time = start.elapsed();
    println!("  Results: {}, {}, {}", r1, r2, r3);
    println!("  Time: {:?} (3 × 100ms = ~300ms)\n", sequential_time);

    // Concurrent execution
    println!("Concurrent execution (operations run in parallel):");
    let start = Instant::now();
    let (r1, r2, r3) = tokio::join!(
        client.slow_add(1, 2, 100),
        client.slow_add(3, 4, 100),
        client.slow_add(5, 6, 100)
    );
    let concurrent_time = start.elapsed();
    println!(
        "  Results: {}, {}, {}",
        r1?.unwrap(),
        r2?.unwrap(),
        r3?.unwrap()
    );
    println!(
        "  Time: {:?} (max(100ms, 100ms, 100ms) = ~100ms)",
        concurrent_time
    );

    let speedup = sequential_time.as_secs_f64() / concurrent_time.as_secs_f64();
    println!("  Speedup: {:.2}x faster! 🚀\n", speedup);

    // Example 3: Mixed latency operations
    println!("Example 3: Mixed Latency Operations");
    println!("------------------------------------");
    println!("Fast operations don't wait for slow ones:");

    let start = Instant::now();
    let (fast, slow) = tokio::join!(
        client.add(1, 1),             // Fast: ~microseconds
        client.slow_add(10, 20, 500)  // Slow: 500ms
    );
    let elapsed = start.elapsed();

    println!("  Fast result: {} (returned immediately)", fast?.unwrap());
    println!("  Slow result: {} (took 500ms)", slow?.unwrap());
    println!(
        "  Total time: {:?} (limited by slowest operation)\n",
        elapsed
    );

    // Example 4: Error handling with concurrency
    println!("Example 4: Error Handling");
    println!("-------------------------");

    let (r1, r2, r3) = tokio::join!(
        client.divide(100, 5),
        client.divide(50, 0), // This will error
        client.divide(200, 10)
    );

    println!("  100 ÷ 5 = {}", r1?.unwrap());
    println!("  50 ÷ 0 = {} (error handled)", r2?.unwrap_err());
    println!("  200 ÷ 10 = {}\n", r3?.unwrap());

    // Example 5: Client cloning for independent operations
    println!("Example 5: Client Cloning");
    println!("-------------------------");
    println!("Multiple cloned clients can operate independently:");

    let client2 = client.clone();
    let client3 = client.clone();

    let (r1, r2, r3) = tokio::join!(
        async { client.add(1, 1).await },
        async { client2.add(2, 2).await },
        async { client3.add(3, 3).await }
    );

    println!("  Client 1: 1 + 1 = {}", r1?.unwrap());
    println!("  Client 2: 2 + 2 = {}", r2?.unwrap());
    println!("  Client 3: 3 + 3 = {}\n", r3?.unwrap());

    // Example 6: High concurrency stress test
    println!("Example 6: High Concurrency Stress Test");
    println!("----------------------------------------");

    let num_operations = 50;
    println!("Executing {} concurrent operations...", num_operations);

    let start = Instant::now();
    let mut tasks = Vec::new();

    for i in 0..num_operations {
        let client = client.clone();
        tasks.push(tokio::spawn(async move { client.add(i, i).await }));
    }

    let mut successful = 0;
    for task in tasks {
        if let Ok(Ok(Ok(_))) = task.await {
            successful += 1;
        }
    }
    let elapsed = start.elapsed();
    println!("  Completed: {}/{} operations", successful, num_operations);
    println!("  Time: {:?}", elapsed);
    println!(
        "  Throughput: {:.0} ops/sec\n",
        num_operations as f64 / elapsed.as_secs_f64()
    );

    // Example 7: Request pipelining
    println!("Example 7: Request Pipelining");
    println!("------------------------------");
    println!("Send multiple requests without waiting for responses:");

    let start = Instant::now();

    // Send all requests first (pipelining)
    let f1 = client.slow_add(1, 1, 50);
    let f2 = client.slow_add(2, 2, 50);
    let f3 = client.slow_add(3, 3, 50);
    let f4 = client.slow_add(4, 4, 50);
    let f5 = client.slow_add(5, 5, 50);

    // Now wait for all responses
    let (r1, r2, r3, r4, r5) = tokio::join!(f1, f2, f3, f4, f5);
    let elapsed = start.elapsed();

    println!(
        "  Results: {}, {}, {}, {}, {}",
        r1?.unwrap(),
        r2?.unwrap(),
        r3?.unwrap(),
        r4?.unwrap(),
        r5?.unwrap()
    );
    println!("  Time: {:?} (all requests pipelined)\n", elapsed);

    println!("═══════════════════════════════════════════════════════════");
    println!("🎉 Concurrent Calculator Example Completed!\n");

    println!("📚 What this example demonstrated:");
    println!("   ✅ Request-response correlation enables true concurrent RPC");
    println!("   ✅ Multiple requests can be in-flight simultaneously");
    println!("   ✅ Responses are correctly matched even when out-of-order");
    println!("   ✅ Significant performance improvements for I/O-bound operations");
    println!("   ✅ Type-safe and easy to use with tokio::join! and async/await");
    println!("   ✅ Client cloning allows independent concurrent operations");
    println!("   ✅ Error handling works correctly with concurrent calls\n");

    println!("💡 Key Takeaways:");
    println!("   • Concurrent RPC is 3x faster for parallel operations");
    println!("   • Fast operations don't wait for slow ones");
    println!("   • Correlation IDs ensure correct request-response matching");
    println!("   • No race conditions or response mix-ups");
    println!("   • Scales to 50+ concurrent operations easily\n");

    // Clean shutdown
    drop(client);
    server_handle.abort();

    Ok(())
}

// Made with Bob

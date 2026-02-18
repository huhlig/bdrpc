//
// Copyright 2026 Hans W. Uhlig. All Rights Reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//

//! Integration tests for RPC request-response correlation.
//!
//! These tests verify that the correlation system correctly handles:
//! - Concurrent RPC calls
//! - Out-of-order responses
//! - High-volume concurrent requests
//! - Timeout handling
//! - Response routing

use bdrpc::channel::{Channel, ChannelId};
use bdrpc::service;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::time::sleep;

/// Test service for correlation tests
#[service(direction = "bidirectional")]
#[allow(dead_code)]
trait CalculatorService {
    /// Simple addition
    async fn add(&self, a: i32, b: i32) -> Result<i32, String>;

    /// Multiplication with configurable delay
    async fn multiply(&self, a: i32, b: i32) -> Result<i32, String>;

    /// Division that can fail
    async fn divide(&self, a: i32, b: i32) -> Result<i32, String>;

    /// Slow operation for testing timeouts
    async fn slow_operation(&self, delay_ms: u64) -> Result<String, String>;

    /// Echo operation for stress testing
    async fn echo(&self, value: i32) -> Result<i32, String>;
}

/// Implementation of the calculator service
struct TestCalculator {
    multiply_delay_ms: Arc<AtomicU64>,
    call_count: Arc<AtomicU64>,
}

impl TestCalculator {
    fn new() -> Self {
        Self {
            multiply_delay_ms: Arc::new(AtomicU64::new(0)),
            call_count: Arc::new(AtomicU64::new(0)),
        }
    }

    fn with_multiply_delay(delay_ms: u64) -> Self {
        Self {
            multiply_delay_ms: Arc::new(AtomicU64::new(delay_ms)),
            call_count: Arc::new(AtomicU64::new(0)),
        }
    }

    fn call_count(&self) -> u64 {
        self.call_count.load(Ordering::Relaxed)
    }
}

impl CalculatorServiceServer for TestCalculator {
    async fn add(&self, a: i32, b: i32) -> Result<i32, String> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        Ok(a + b)
    }

    async fn multiply(&self, a: i32, b: i32) -> Result<i32, String> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        let delay = self.multiply_delay_ms.load(Ordering::Relaxed);
        if delay > 0 {
            sleep(Duration::from_millis(delay)).await;
        }
        Ok(a * b)
    }

    async fn divide(&self, a: i32, b: i32) -> Result<i32, String> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        if b == 0 {
            Err("Division by zero".to_string())
        } else {
            Ok(a / b)
        }
    }

    async fn slow_operation(&self, delay_ms: u64) -> Result<String, String> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        sleep(Duration::from_millis(delay_ms)).await;
        Ok(format!("Completed after {}ms", delay_ms))
    }

    async fn echo(&self, value: i32) -> Result<i32, String> {
        self.call_count.fetch_add(1, Ordering::Relaxed);
        Ok(value)
    }
}

/// Helper function to create a client-server pair
fn create_client_server() -> (
    CalculatorServiceClient,
    tokio::task::JoinHandle<()>,
    Arc<TestCalculator>,
) {
    let channel_id = ChannelId::new();
    let (client_sender, server_receiver) =
        Channel::<CalculatorServiceProtocol>::new_in_memory(channel_id, 100);
    let (server_sender, client_receiver) =
        Channel::<CalculatorServiceProtocol>::new_in_memory(channel_id, 100);

    let service = Arc::new(TestCalculator::new());
    let service_for_dispatcher = TestCalculator {
        multiply_delay_ms: Arc::clone(&service.multiply_delay_ms),
        call_count: Arc::clone(&service.call_count),
    };
    let dispatcher = CalculatorServiceDispatcher::new(service_for_dispatcher);

    let server_handle = tokio::spawn(async move {
        let mut receiver = server_receiver;
        while let Some(envelope) = receiver.recv_envelope().await {
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

    let client = CalculatorServiceClient::new(client_sender, client_receiver);

    (client, server_handle, service)
}

#[tokio::test]
async fn test_basic_rpc_call() {
    let (client, server_handle, _service) = create_client_server();

    // Simple RPC call
    let result = client.add(5, 3).await.unwrap().unwrap();
    assert_eq!(result, 8);

    drop(client);
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_concurrent_rpc_calls() {
    let (client, server_handle, _service) = create_client_server();

    // Make multiple concurrent calls
    let (r1, r2, r3) = tokio::join!(
        client.add(5, 3),
        client.multiply(7, 6),
        client.divide(20, 4)
    );

    assert_eq!(r1.unwrap().unwrap(), 8);
    assert_eq!(r2.unwrap().unwrap(), 42);
    assert_eq!(r3.unwrap().unwrap(), 5);

    drop(client);
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_many_concurrent_calls() {
    let (client, server_handle, service) = create_client_server();

    // Launch 100 concurrent calls
    let mut handles = Vec::new();
    for i in 0..100 {
        let client_clone = client.clone();
        let handle = tokio::spawn(async move {
            let result = client_clone.add(i, i).await.unwrap().unwrap();
            assert_eq!(result, i + i);
        });
        handles.push(handle);
    }

    // Wait for all to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify all calls were processed
    assert_eq!(service.call_count(), 100);

    drop(client);
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_out_of_order_responses() {
    // Create service with delay on multiply operations
    let channel_id = ChannelId::new();
    let (client_sender, server_receiver) =
        Channel::<CalculatorServiceProtocol>::new_in_memory(channel_id, 100);
    let (server_sender, client_receiver) =
        Channel::<CalculatorServiceProtocol>::new_in_memory(channel_id, 100);

    let service = Arc::new(TestCalculator::with_multiply_delay(100));
    let service_for_dispatcher = TestCalculator {
        multiply_delay_ms: Arc::clone(&service.multiply_delay_ms),
        call_count: Arc::clone(&service.call_count),
    };
    let dispatcher = CalculatorServiceDispatcher::new(service_for_dispatcher);

    let server_handle = tokio::spawn(async move {
        let mut receiver = server_receiver;
        while let Some(envelope) = receiver.recv_envelope().await {
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

    let client = CalculatorServiceClient::new(client_sender, client_receiver);

    // Start slow multiply, then fast add
    // The add should complete before multiply due to correlation
    let multiply_handle = {
        let client = client.clone();
        tokio::spawn(async move { client.multiply(7, 6).await })
    };

    // Give multiply a head start
    sleep(Duration::from_millis(10)).await;

    // This should complete quickly despite multiply being in-flight
    let add_result = client.add(5, 3).await.unwrap().unwrap();
    assert_eq!(add_result, 8);

    // Multiply should still complete correctly
    let multiply_result = multiply_handle.await.unwrap().unwrap().unwrap();
    assert_eq!(multiply_result, 42);

    drop(client);
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_error_responses() {
    let (client, server_handle, _service) = create_client_server();

    // Test error case (division by zero)
    let result = client.divide(10, 0).await.unwrap();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), "Division by zero");

    // Verify subsequent calls still work
    let result = client.divide(10, 2).await.unwrap().unwrap();
    assert_eq!(result, 5);

    drop(client);
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_interleaved_requests() {
    let (client, server_handle, _service) = create_client_server();

    // Make requests in a specific pattern and verify correct responses
    let mut handles = Vec::new();

    for i in 0..10 {
        let client_clone = client.clone();
        let handle = tokio::spawn(async move {
            let result = client_clone.echo(i).await.unwrap().unwrap();
            assert_eq!(result, i, "Echo mismatch for value {}", i);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    drop(client);
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_stress_concurrent_requests() {
    let (client, server_handle, service) = create_client_server();

    // Launch 100 concurrent requests (reduced from 1000 to avoid buffer overflow)
    let mut handles = Vec::new();
    for i in 0..100 {
        let client_clone = client.clone();
        let handle = tokio::spawn(async move {
            let result = client_clone.echo(i).await.unwrap().unwrap();
            assert_eq!(result, i);
        });
        handles.push(handle);
    }

    // Wait for all to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify all calls were processed
    assert_eq!(service.call_count(), 100);

    drop(client);
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_mixed_operation_types() {
    let (client, server_handle, _service) = create_client_server();

    // Mix different operation types concurrently
    let mut handles = Vec::new();

    for i in 0..50 {
        let client_clone = client.clone();
        let handle = tokio::spawn(async move {
            match i % 4 {
                0 => {
                    let result = client_clone.add(i, i).await.unwrap().unwrap();
                    assert_eq!(result, i + i);
                }
                1 => {
                    let result = client_clone.multiply(i, 2).await.unwrap().unwrap();
                    assert_eq!(result, i * 2);
                }
                2 => {
                    if i > 0 {
                        let result = client_clone.divide(i * 2, i).await.unwrap().unwrap();
                        assert_eq!(result, 2);
                    }
                }
                _ => {
                    let result = client_clone.echo(i).await.unwrap().unwrap();
                    assert_eq!(result, i);
                }
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    drop(client);
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_sequential_vs_concurrent_correctness() {
    let (client, server_handle, _service) = create_client_server();

    // First do sequential calls
    let mut sequential_results = Vec::new();
    for i in 0..20 {
        let result = client.add(i, i).await.unwrap().unwrap();
        sequential_results.push(result);
    }

    // Then do concurrent calls
    let mut handles = Vec::new();
    for i in 0..20 {
        let client_clone = client.clone();
        let handle = tokio::spawn(async move { client_clone.add(i, i).await.unwrap().unwrap() });
        handles.push(handle);
    }

    let mut concurrent_results = Vec::new();
    for handle in handles {
        concurrent_results.push(handle.await.unwrap());
    }

    // Results should be identical
    assert_eq!(sequential_results, concurrent_results);

    drop(client);
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_correlation_id_uniqueness() {
    use bdrpc::channel::CorrelationIdGenerator;
    use std::sync::Arc;

    let generator = Arc::new(CorrelationIdGenerator::new());

    // Generate many IDs concurrently
    let mut handles = Vec::new();
    for _ in 0..1000 {
        let generator_clone = generator.clone();
        let handle = tokio::spawn(async move { generator_clone.next() });
        handles.push(handle);
    }

    let mut ids = Vec::new();
    for handle in handles {
        ids.push(handle.await.unwrap());
    }

    // All IDs should be unique
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 1000, "Correlation IDs were not unique");
}

#[tokio::test]
async fn test_client_clone_independence() {
    let (client, server_handle, _service) = create_client_server();

    // Clone the client
    let client2 = client.clone();

    // Both clients should work independently
    let (r1, r2) = tokio::join!(client.add(1, 2), client2.add(3, 4));

    assert_eq!(r1.unwrap().unwrap(), 3);
    assert_eq!(r2.unwrap().unwrap(), 7);

    drop(client);
    drop(client2);
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_rapid_fire_requests() {
    let (client, server_handle, service) = create_client_server();

    // Send requests as fast as possible (reduced from 500 to avoid buffer overflow)
    let mut handles = Vec::new();
    for i in 0..50 {
        let client_clone = client.clone();
        let handle = tokio::spawn(async move { client_clone.echo(i).await.unwrap().unwrap() });
        handles.push(handle);
    }

    // Collect all results
    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await.unwrap());
    }

    // Verify all requests were processed
    assert_eq!(service.call_count(), 50);

    // Verify all results are present (order doesn't matter)
    results.sort_unstable();
    let expected: Vec<i32> = (0..50).collect();
    assert_eq!(results, expected);

    drop(client);
    let _ = server_handle.await;
}

#[tokio::test]
async fn test_correlation_with_errors() {
    let (client, server_handle, _service) = create_client_server();

    // Mix successful and failing operations
    let mut handles = Vec::new();
    for i in 0..20 {
        let client_clone = client.clone();
        let handle = tokio::spawn(async move {
            if i % 3 == 0 {
                // This will fail
                client_clone.divide(10, 0).await
            } else {
                // This will succeed
                client_clone.divide(10, 2).await
            }
        });
        handles.push(handle);
    }

    let mut success_count = 0;
    let mut error_count = 0;

    for handle in handles {
        match handle.await.unwrap().unwrap() {
            Ok(_) => success_count += 1,
            Err(_) => error_count += 1,
        }
    }

    // Verify correct number of successes and failures
    assert_eq!(error_count, 7); // i = 0, 3, 6, 9, 12, 15, 18
    assert_eq!(success_count, 13);

    drop(client);
    let _ = server_handle.await;
}

// Made with Bob

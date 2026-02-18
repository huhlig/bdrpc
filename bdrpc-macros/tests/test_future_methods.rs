//! Test that the service macro accepts both async methods and manual Future implementations.

use bdrpc::service;
use std::future::Future;

// Test 1: Mix of async and impl Future methods
#[service(direction = "bidirectional")]
#[allow(dead_code)]
trait MixedService {
    /// Regular async method
    async fn async_method(&self, value: i32) -> Result<i32, String>;

    /// Method returning impl Future
    fn future_method(&self, value: String) -> impl Future<Output = Result<String, String>> + Send;
}

// Test 2: All async methods (existing behavior)
#[service(direction = "call")]
#[allow(dead_code)]
trait AllAsyncService {
    async fn method1(&self, a: i32) -> Result<i32, String>;
    async fn method2(&self, b: String) -> Result<String, String>;
}

// Test 3: All impl Future methods
#[service(direction = "respond")]
#[allow(dead_code)]
trait AllFutureService {
    fn method1(&self, a: i32) -> impl Future<Output = Result<i32, String>> + Send;
    fn method2(&self, b: String) -> impl Future<Output = Result<String, String>> + Send;
}

// Test 4: Complex return types
#[service(direction = "bidirectional")]
#[allow(dead_code)]
trait ComplexReturnService {
    async fn vec_result(&self) -> Result<Vec<String>, String>;

    fn future_vec_result(&self) -> impl Future<Output = Result<Vec<i32>, String>> + Send;

    async fn option_result(&self) -> Result<Option<i32>, String>;

    fn future_option_result(&self) -> impl Future<Output = Result<Option<String>, String>> + Send;
}

#[test]
fn test_mixed_service_compiles() {
    // This test just verifies that the macro expansion compiles successfully
    // The actual protocol enum and client/server should be generated

    // Verify protocol enum exists
    let _protocol_name = std::any::type_name::<MixedServiceProtocol>();

    // Verify client exists
    let _client_name = std::any::type_name::<MixedServiceClient>();
}

#[test]
fn test_all_async_service_compiles() {
    let _protocol_name = std::any::type_name::<AllAsyncServiceProtocol>();
    let _client_name = std::any::type_name::<AllAsyncServiceClient>();
}

#[test]
fn test_all_future_service_compiles() {
    let _protocol_name = std::any::type_name::<AllFutureServiceProtocol>();
    let _client_name = std::any::type_name::<AllFutureServiceClient>();
}

#[test]
fn test_complex_return_service_compiles() {
    let _protocol_name = std::any::type_name::<ComplexReturnServiceProtocol>();
    let _client_name = std::any::type_name::<ComplexReturnServiceClient>();
}

// Implementation test for mixed service
struct TestMixedService;

impl MixedServiceServer for TestMixedService {
    async fn async_method(&self, value: i32) -> Result<i32, String> {
        Ok(value * 2)
    }

    #[allow(clippy::manual_async_fn)]
    fn future_method(&self, value: String) -> impl Future<Output = Result<String, String>> + Send {
        async move { Ok(format!("processed: {}", value)) }
    }
}

#[tokio::test]
async fn test_mixed_service_implementation() {
    use bdrpc::channel::{Channel, ChannelId};

    let channel_id = ChannelId::new();
    let (client_sender, server_receiver) =
        Channel::<MixedServiceProtocol>::new_in_memory(channel_id, 10);
    let (server_sender, client_receiver) =
        Channel::<MixedServiceProtocol>::new_in_memory(channel_id, 10);

    // Create service and dispatcher
    let service = TestMixedService;
    let dispatcher = MixedServiceDispatcher::new(service);

    // Spawn server task
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

    // Create client
    let client = MixedServiceClient::new(client_sender, client_receiver);

    // Test async method
    let result = client.async_method(21).await.unwrap().unwrap();
    assert_eq!(result, 42);

    // Test impl Future method
    let result = client
        .future_method("test".to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result, "processed: test");

    drop(client);
    let _ = server_handle.await;
}

// Test that impl Future methods work in server trait
struct TestAllFutureService;

#[allow(clippy::manual_async_fn)]
impl AllFutureServiceServer for TestAllFutureService {
    fn method1(&self, a: i32) -> impl Future<Output = Result<i32, String>> + Send {
        async move { Ok(a + 10) }
    }

    fn method2(&self, b: String) -> impl Future<Output = Result<String, String>> + Send {
        async move { Ok(b.to_uppercase()) }
    }
}

#[tokio::test]
async fn test_all_future_service_implementation() {
    use bdrpc::channel::{Channel, ChannelId};

    let channel_id = ChannelId::new();
    let (client_sender, server_receiver) =
        Channel::<AllFutureServiceProtocol>::new_in_memory(channel_id, 10);
    let (server_sender, client_receiver) =
        Channel::<AllFutureServiceProtocol>::new_in_memory(channel_id, 10);

    let service = TestAllFutureService;
    let dispatcher = AllFutureServiceDispatcher::new(service);

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

    let client = AllFutureServiceClient::new(client_sender, client_receiver);

    let result = client.method1(5).await.unwrap().unwrap();
    assert_eq!(result, 15);

    let result = client.method2("hello".to_string()).await.unwrap().unwrap();
    assert_eq!(result, "HELLO");

    drop(client);
    let _ = server_handle.await;
}

// Made with Bob

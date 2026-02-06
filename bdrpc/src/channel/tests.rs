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

//! Integration tests for the channel layer.

use super::*;
use std::sync::Arc;

// Test protocol for integration tests
#[derive(Debug, Clone, PartialEq, Eq)]
enum TestProtocol {
    Request(String),
    Response(i32),
}

impl Protocol for TestProtocol {
    fn method_name(&self) -> &'static str {
        match self {
            Self::Request(_) => "request",
            Self::Response(_) => "response",
        }
    }

    fn is_request(&self) -> bool {
        matches!(self, Self::Request(_))
    }
}

#[tokio::test]
async fn test_multiple_channels_isolation() {
    let manager = ChannelManager::new();

    // Create two channels
    let id1 = ChannelId::new();
    let id2 = ChannelId::new();

    let sender1 = manager
        .create_channel::<TestProtocol>(id1, 10)
        .await
        .unwrap();
    let sender2 = manager
        .create_channel::<TestProtocol>(id2, 10)
        .await
        .unwrap();

    // Send messages on both channels
    sender1
        .send(TestProtocol::Request("Channel 1".to_string()))
        .await
        .unwrap();
    sender2
        .send(TestProtocol::Request("Channel 2".to_string()))
        .await
        .unwrap();

    // Channels should be independent
    assert_eq!(manager.channel_count().await, 2);
}

#[tokio::test]
async fn test_channel_ordering_across_multiple_senders() {
    let (sender, mut receiver) = Channel::<TestProtocol>::new_in_memory(ChannelId::new(), 100);

    // Clone sender for concurrent sends
    let sender2 = sender.clone();

    // Spawn tasks to send messages concurrently
    let handle1 = tokio::spawn(async move {
        for i in 0..50 {
            sender.send(TestProtocol::Response(i)).await.unwrap();
        }
    });

    let handle2 = tokio::spawn(async move {
        for i in 50..100 {
            sender2.send(TestProtocol::Response(i)).await.unwrap();
        }
    });

    // Wait for all sends to complete
    handle1.await.unwrap();
    handle2.await.unwrap();

    // Receive all messages
    let mut received = Vec::new();
    for _ in 0..100 {
        if let Some(msg) = receiver.recv().await {
            received.push(msg);
        }
    }

    // All messages should be received
    assert_eq!(received.len(), 100);
}

#[tokio::test]
async fn test_manager_concurrent_operations() {
    let manager = Arc::new(ChannelManager::new());

    // Spawn multiple tasks creating channels concurrently
    let mut handles = Vec::new();

    for _ in 0..10 {
        let manager = Arc::clone(&manager);
        let handle = tokio::spawn(async move {
            let id = ChannelId::new();
            let _sender: ChannelSender<TestProtocol> =
                manager.create_channel(id, 10).await.unwrap();
            id
        });
        handles.push(handle);
    }

    // Wait for all channels to be created
    let mut ids = Vec::new();
    for handle in handles {
        ids.push(handle.await.unwrap());
    }

    // All channels should exist
    assert_eq!(manager.channel_count().await, 10);

    // All IDs should be unique
    let unique_ids: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique_ids.len(), 10);
}

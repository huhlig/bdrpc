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

//! Stress tests for concurrent operations and edge cases.

use bdrpc::channel::{ChannelId, ChannelManager, Protocol};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

/// Test protocol for stress tests.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
enum StressTestProtocol {
    Request(u64),
    Response(u64),
}

impl Protocol for StressTestProtocol {
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
async fn test_concurrent_channel_creation() {
    let manager = Arc::new(ChannelManager::new());
    let mut handles = vec![];

    // Spawn 100 tasks, each creating 10 channels
    for task_id in 0..100 {
        let manager_clone = manager.clone();
        let handle = tokio::spawn(async move {
            for i in 0..10 {
                let channel_id = ChannelId::from((task_id * 10 + i) as u64);
                let result = manager_clone
                    .create_channel::<StressTestProtocol>(channel_id, 100)
                    .await;
                assert!(
                    result.is_ok(),
                    "Channel creation failed for task {} channel {}",
                    task_id,
                    i
                );
            }
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await.expect("Task panicked");
    }

    // Verify all 1000 channels were created
    assert_eq!(manager.channel_count().await, 1000);
}

#[tokio::test]
async fn test_concurrent_send_receive() {
    let manager = Arc::new(ChannelManager::new());
    let channel_id = ChannelId::from(1);

    // Create a single channel
    manager
        .create_channel::<StressTestProtocol>(channel_id, 1000)
        .await
        .expect("Channel creation failed");

    let sender = manager
        .get_sender::<StressTestProtocol>(channel_id)
        .await
        .expect("Failed to get sender");

    let mut receiver = manager
        .get_receiver::<StressTestProtocol>(channel_id)
        .await
        .expect("Failed to get receiver");

    // Spawn 10 sender tasks
    let mut send_handles = vec![];
    for task_id in 0..10 {
        let sender_clone = sender.clone();
        let handle = tokio::spawn(async move {
            for i in 0..100 {
                let msg = StressTestProtocol::Request((task_id * 100 + i) as u64);
                sender_clone.send(msg).await.expect("Send failed");
            }
        });
        send_handles.push(handle);
    }

    // Spawn receiver task
    let recv_handle = tokio::spawn(async move {
        let mut count = 0;
        while count < 1000 {
            if let Some(_msg) = receiver.try_recv() {
                count += 1;
            } else {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }
        count
    });

    // Wait for all senders
    for handle in send_handles {
        handle.await.expect("Sender task panicked");
    }

    // Wait for receiver and verify count
    let received_count = recv_handle.await.expect("Receiver task panicked");
    assert_eq!(received_count, 1000, "Not all messages were received");
}

#[tokio::test]
async fn test_rapid_channel_creation_and_removal() {
    let manager = Arc::new(ChannelManager::new());

    for iteration in 0..100 {
        let channel_id = ChannelId::from(iteration);

        // Create channel
        manager
            .create_channel::<StressTestProtocol>(channel_id, 100)
            .await
            .expect("Channel creation failed");

        // Verify it exists
        assert!(
            manager
                .get_sender::<StressTestProtocol>(channel_id)
                .await
                .is_ok(),
            "Channel should exist"
        );

        // Remove channel
        manager.remove_channel(channel_id).await;

        // Verify it's gone
        let _ = manager.remove_channel(channel_id).await;
        assert!(
            manager
                .get_sender::<StressTestProtocol>(channel_id)
                .await
                .is_err(),
            "Channel should be removed"
        );
    }
}

// Note: MemoryTransport doesn't have create_pair, so we'll skip this test
// or implement it differently when needed

// Note: Endpoint registration requires &mut self, so concurrent registration
// isn't possible. This test would need a different approach.

#[tokio::test]
async fn test_channel_backpressure_under_load() {
    let manager = Arc::new(ChannelManager::new());
    let channel_id = ChannelId::from(1);

    // Create channel with small buffer to trigger backpressure
    manager
        .create_channel::<StressTestProtocol>(channel_id, 10)
        .await
        .expect("Channel creation failed");

    let sender = manager
        .get_sender::<StressTestProtocol>(channel_id)
        .await
        .expect("Failed to get sender");

    let mut receiver = manager
        .get_receiver::<StressTestProtocol>(channel_id)
        .await
        .expect("Failed to get receiver");

    // Spawn aggressive sender
    let send_handle = tokio::spawn(async move {
        for i in 0..100 {
            let msg = StressTestProtocol::Request(i);
            // This should block when buffer is full
            sender.send(msg).await.expect("Send failed");
        }
    });

    // Slow receiver
    let recv_handle = tokio::spawn(async move {
        let mut count = 0;
        while count < 100 {
            if let Some(_msg) = receiver.try_recv() {
                count += 1;
                // Simulate slow processing
                tokio::time::sleep(Duration::from_millis(10)).await;
            } else {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }
        count
    });

    // Both should complete without deadlock
    let send_result = timeout(Duration::from_secs(5), send_handle).await;
    assert!(send_result.is_ok(), "Sender should complete");

    let recv_result = timeout(Duration::from_secs(5), recv_handle).await;
    assert!(recv_result.is_ok(), "Receiver should complete");

    let received = recv_result.unwrap().unwrap();
    assert_eq!(received, 100, "All messages should be received");
}

#[tokio::test]
async fn test_multiple_channels_concurrent_operations() {
    let manager = Arc::new(ChannelManager::new());
    let num_channels = 20;

    // Create multiple channels
    for i in 0..num_channels {
        let channel_id = ChannelId::from(i);
        manager
            .create_channel::<StressTestProtocol>(channel_id, 100)
            .await
            .expect("Channel creation failed");
    }

    let mut handles = vec![];

    // Spawn tasks for each channel
    for i in 0..num_channels {
        let manager_clone = manager.clone();
        let channel_id = ChannelId::from(i);

        let handle = tokio::spawn(async move {
            let sender = manager_clone
                .get_sender::<StressTestProtocol>(channel_id)
                .await
                .expect("Failed to get sender");

            let mut receiver = manager_clone
                .get_receiver::<StressTestProtocol>(channel_id)
                .await
                .expect("Failed to get receiver");

            // Send and receive on this channel
            for j in 0..50 {
                let msg = StressTestProtocol::Request(j);
                sender.send(msg).await.expect("Send failed");

                let msg_opt = receiver.recv().await;
                assert!(msg_opt.is_some(), "Should receive message");
            }
        });

        handles.push(handle);
    }

    // Wait for all tasks
    for handle in handles {
        handle.await.expect("Task panicked");
    }

    // Verify all channels still exist
    assert_eq!(manager.channel_count().await, num_channels as usize);
}

#[tokio::test]
async fn test_channel_cleanup_under_concurrent_access() {
    let manager = Arc::new(ChannelManager::new());
    let num_channels = 50;

    // Create channels
    for i in 0..num_channels {
        let channel_id = ChannelId::from(i);
        manager
            .create_channel::<StressTestProtocol>(channel_id, 100)
            .await
            .expect("Channel creation failed");
    }

    let mut handles = vec![];

    // Spawn tasks that access and remove channels concurrently
    for i in 0..num_channels {
        let manager_clone = manager.clone();
        let channel_id = ChannelId::from(i);

        let handle = tokio::spawn(async move {
            // Try to get sender
            let _ = manager_clone
                .get_sender::<StressTestProtocol>(channel_id)
                .await;

            // Remove the channel
            let _ = manager_clone.remove_channel(channel_id).await;

            // Try to get sender again (should fail)
            let result = manager_clone
                .get_sender::<StressTestProtocol>(channel_id)
                .await;
            assert!(result.is_err(), "Channel should be removed");
        });

        handles.push(handle);
    }

    // Wait for all tasks
    for handle in handles {
        handle.await.expect("Task panicked");
    }

    // Verify all channels are removed
    assert_eq!(manager.channel_count().await, 0);
}

// Note: Protocol registration requires &mut self and doesn't support multiple versions
// of the same protocol. This test would need a different approach or API changes.

// Made with Bob

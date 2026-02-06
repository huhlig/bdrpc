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

//! Integration tests for dynamic channel negotiation.

use bdrpc::channel::{ChannelId, ChannelManager, SYSTEM_CHANNEL_ID, SystemProtocol};
use bdrpc::endpoint::{
    ChannelNegotiator, DefaultChannelNegotiator, Endpoint, EndpointConfig, ProtocolDirection,
};
use bdrpc::serialization::JsonSerializer;
use std::collections::HashMap;

/// Test protocol for channel negotiation tests.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
enum TestProtocol {
    Request(String),
    Response(String),
}

impl bdrpc::channel::Protocol for TestProtocol {
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
async fn test_system_channel_creation() {
    // Create a channel manager
    let manager = ChannelManager::new();

    // Create system channel
    let result = manager
        .create_channel::<SystemProtocol>(SYSTEM_CHANNEL_ID, 100)
        .await;

    assert!(result.is_ok(), "System channel creation should succeed");

    // Verify we can get the sender
    let sender_result = manager
        .get_sender::<SystemProtocol>(SYSTEM_CHANNEL_ID)
        .await;
    assert!(
        sender_result.is_ok(),
        "Should be able to get system channel sender"
    );

    // Verify we can get the receiver
    let receiver_result = manager
        .get_receiver::<SystemProtocol>(SYSTEM_CHANNEL_ID)
        .await;
    assert!(
        receiver_result.is_ok(),
        "Should be able to get system channel receiver"
    );
}

#[tokio::test]
async fn test_channel_create_request_serialization() {
    // Create a channel creation request
    let request = SystemProtocol::ChannelCreateRequest {
        channel_id: ChannelId::from(42),
        protocol_name: "TestProtocol".to_string(),
        protocol_version: 1,
        direction: ProtocolDirection::Bidirectional,
        buffer_size: Some(100),
        metadata: HashMap::from([("key".to_string(), "value".to_string())]),
    };

    // Serialize to JSON
    let json = serde_json::to_string(&request).expect("Should serialize");

    // Deserialize back
    let deserialized: SystemProtocol = serde_json::from_str(&json).expect("Should deserialize");

    // Verify it's the same
    match deserialized {
        SystemProtocol::ChannelCreateRequest {
            channel_id,
            protocol_name,
            protocol_version,
            direction,
            buffer_size,
            metadata,
        } => {
            assert_eq!(channel_id.as_u64(), 42);
            assert_eq!(protocol_name, "TestProtocol");
            assert_eq!(protocol_version, 1);
            assert_eq!(direction, ProtocolDirection::Bidirectional);
            assert_eq!(buffer_size, Some(100));
            assert_eq!(metadata.get("key").map(|s| s.as_str()), Some("value"));
        }
        _ => panic!("Wrong variant"),
    }
}

#[tokio::test]
async fn test_channel_create_response_serialization() {
    // Create a successful response
    let response = SystemProtocol::ChannelCreateResponse {
        channel_id: ChannelId::from(42),
        success: true,
        error: None,
    };

    let json = serde_json::to_string(&response).expect("Should serialize");
    let deserialized: SystemProtocol = serde_json::from_str(&json).expect("Should deserialize");

    match deserialized {
        SystemProtocol::ChannelCreateResponse {
            channel_id,
            success,
            error,
        } => {
            assert_eq!(channel_id.as_u64(), 42);
            assert!(success);
            assert_eq!(error, None);
        }
        _ => panic!("Wrong variant"),
    }

    // Create a failed response
    let response = SystemProtocol::ChannelCreateResponse {
        channel_id: ChannelId::from(42),
        success: false,
        error: Some("Protocol not supported".to_string()),
    };

    let json = serde_json::to_string(&response).expect("Should serialize");
    let deserialized: SystemProtocol = serde_json::from_str(&json).expect("Should deserialize");

    match deserialized {
        SystemProtocol::ChannelCreateResponse {
            channel_id,
            success,
            error,
        } => {
            assert_eq!(channel_id.as_u64(), 42);
            assert!(!success);
            assert_eq!(error, Some("Protocol not supported".to_string()));
        }
        _ => panic!("Wrong variant"),
    }
}

#[tokio::test]
async fn test_default_channel_negotiator_rejects_by_default() {
    let negotiator = DefaultChannelNegotiator::new();

    // Should reject protocols not in allowlist
    let result = negotiator
        .on_channel_create_request(
            ChannelId::from(1),
            "TestProtocol",
            1,
            ProtocolDirection::Bidirectional,
            &HashMap::new(),
        )
        .await;

    assert!(
        result.is_err(),
        "Default negotiator should reject protocols not in allowlist"
    );
}

#[tokio::test]
async fn test_default_channel_negotiator_accepts_allowed() {
    let negotiator = DefaultChannelNegotiator::new();

    // Allow the protocol
    negotiator.allow_protocol("TestProtocol").await;

    // Should now accept the protocol
    let result = negotiator
        .on_channel_create_request(
            ChannelId::from(1),
            "TestProtocol",
            1,
            ProtocolDirection::Bidirectional,
            &HashMap::new(),
        )
        .await;

    assert!(
        result.is_ok(),
        "Default negotiator should accept allowed protocols"
    );
}

#[tokio::test]
async fn test_endpoint_request_channel_timeout() {
    let config = EndpointConfig::default();
    let endpoint = Endpoint::new(JsonSerializer::default(), config);

    // Request a channel on a non-existent connection
    let result = endpoint
        .request_channel(
            "non-existent-connection",
            ChannelId::from(1),
            "TestProtocol",
            1,
            ProtocolDirection::Bidirectional,
            HashMap::new(),
        )
        .await;

    // Should fail because the connection doesn't exist (no system channel)
    assert!(
        result.is_err(),
        "Request should fail when connection doesn't exist"
    );

    // Verify it's a channel error (system channel not found)
    match result {
        Err(e) => {
            let error_msg = e.to_string();
            assert!(
                error_msg.contains("channel") || error_msg.contains("not found"),
                "Error should indicate channel/connection not found: {}",
                error_msg
            );
        }
        Ok(_) => panic!("Expected error but got Ok"),
    }
}

#[tokio::test]
async fn test_ping_pong_serialization() {
    let timestamp = 12345u64;

    // Test Ping
    let ping = SystemProtocol::Ping { timestamp };
    let json = serde_json::to_string(&ping).expect("Should serialize");
    let deserialized: SystemProtocol = serde_json::from_str(&json).expect("Should deserialize");

    match deserialized {
        SystemProtocol::Ping { timestamp: ts } => {
            assert_eq!(ts, timestamp);
        }
        _ => panic!("Wrong variant"),
    }

    // Test Pong
    let pong = SystemProtocol::Pong { timestamp };
    let json = serde_json::to_string(&pong).expect("Should serialize");
    let deserialized: SystemProtocol = serde_json::from_str(&json).expect("Should deserialize");

    match deserialized {
        SystemProtocol::Pong { timestamp: ts } => {
            assert_eq!(ts, timestamp);
        }
        _ => panic!("Wrong variant"),
    }
}

#[tokio::test]
async fn test_channel_close_notification_serialization() {
    let notification = SystemProtocol::ChannelCloseNotification {
        channel_id: ChannelId::from(42),
        reason: "Test closure".to_string(),
    };

    let json = serde_json::to_string(&notification).expect("Should serialize");
    let deserialized: SystemProtocol = serde_json::from_str(&json).expect("Should deserialize");

    match deserialized {
        SystemProtocol::ChannelCloseNotification { channel_id, reason } => {
            assert_eq!(channel_id.as_u64(), 42);
            assert_eq!(reason, "Test closure");
        }
        _ => panic!("Wrong variant"),
    }
}

#[tokio::test]
async fn test_channel_close_ack_serialization() {
    let ack = SystemProtocol::ChannelCloseAck {
        channel_id: ChannelId::from(42),
    };

    let json = serde_json::to_string(&ack).expect("Should serialize");
    let deserialized: SystemProtocol = serde_json::from_str(&json).expect("Should deserialize");

    match deserialized {
        SystemProtocol::ChannelCloseAck { channel_id } => {
            assert_eq!(channel_id.as_u64(), 42);
        }
        _ => panic!("Wrong variant"),
    }
}

#[tokio::test]
async fn test_multiple_channel_creation() {
    let manager = ChannelManager::new();

    // Create multiple channels
    for i in 1..=10 {
        let channel_id = ChannelId::from(i);
        let result = manager
            .create_channel::<TestProtocol>(channel_id, 100)
            .await;
        assert!(result.is_ok(), "Channel {} creation should succeed", i);
    }

    // Verify all channels exist
    for i in 1..=10 {
        let channel_id = ChannelId::from(i);
        let sender_result = manager.get_sender::<TestProtocol>(channel_id).await;
        assert!(
            sender_result.is_ok(),
            "Should be able to get sender for channel {}",
            i
        );
    }
}

#[tokio::test]
async fn test_duplicate_channel_creation_fails() {
    let manager = ChannelManager::new();

    let channel_id = ChannelId::from(1);

    // Create first channel
    let result1 = manager
        .create_channel::<TestProtocol>(channel_id, 100)
        .await;
    assert!(result1.is_ok(), "First channel creation should succeed");

    // Try to create duplicate
    let result2 = manager
        .create_channel::<TestProtocol>(channel_id, 100)
        .await;
    assert!(result2.is_err(), "Duplicate channel creation should fail");
}

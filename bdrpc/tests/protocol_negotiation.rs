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

//! Integration tests for protocol version negotiation edge cases.

use bdrpc::endpoint::{ProtocolCapability, ProtocolDirection, negotiate_protocols};

#[test]
fn test_negotiate_single_common_version() {
    let our_caps = vec![ProtocolCapability::new(
        "TestService",
        vec![1, 2, 3],
        ProtocolDirection::Bidirectional,
    )];

    let peer_caps = vec![ProtocolCapability::new(
        "TestService",
        vec![3, 4, 5],
        ProtocolDirection::Bidirectional,
    )];

    let negotiated = negotiate_protocols(&our_caps, &peer_caps);
    assert_eq!(negotiated.len(), 1);
    assert_eq!(negotiated[0].version, 3); // Highest common version
    assert!(negotiated[0].is_bidirectional());
}

#[test]
fn test_negotiate_multiple_common_versions() {
    let our_caps = vec![ProtocolCapability::new(
        "TestService",
        vec![1, 2, 3, 4],
        ProtocolDirection::Bidirectional,
    )];

    let peer_caps = vec![ProtocolCapability::new(
        "TestService",
        vec![2, 3, 4, 5],
        ProtocolDirection::Bidirectional,
    )];

    let negotiated = negotiate_protocols(&our_caps, &peer_caps);
    assert_eq!(negotiated.len(), 1);
    assert_eq!(negotiated[0].version, 4); // Highest common version
}

#[test]
fn test_negotiate_no_common_version() {
    let our_caps = vec![ProtocolCapability::new(
        "TestService",
        vec![1, 2],
        ProtocolDirection::Bidirectional,
    )];

    let peer_caps = vec![ProtocolCapability::new(
        "TestService",
        vec![3, 4],
        ProtocolDirection::Bidirectional,
    )];

    let negotiated = negotiate_protocols(&our_caps, &peer_caps);
    assert_eq!(negotiated.len(), 0); // No common version
}

#[test]
fn test_negotiate_different_protocols() {
    let our_caps = vec![
        ProtocolCapability::new("ServiceA", vec![1], ProtocolDirection::Bidirectional),
        ProtocolCapability::new("ServiceB", vec![1], ProtocolDirection::Bidirectional),
    ];

    let peer_caps = vec![
        ProtocolCapability::new("ServiceB", vec![1], ProtocolDirection::Bidirectional),
        ProtocolCapability::new("ServiceC", vec![1], ProtocolDirection::Bidirectional),
    ];

    let negotiated = negotiate_protocols(&our_caps, &peer_caps);
    assert_eq!(negotiated.len(), 1);
    assert_eq!(negotiated[0].name, "ServiceB");
}

#[test]
fn test_negotiate_call_only_with_respond_only() {
    let our_caps = vec![ProtocolCapability::new(
        "TestService",
        vec![1],
        ProtocolDirection::CallOnly,
    )];

    let peer_caps = vec![ProtocolCapability::new(
        "TestService",
        vec![1],
        ProtocolDirection::RespondOnly,
    )];

    let negotiated = negotiate_protocols(&our_caps, &peer_caps);
    assert_eq!(negotiated.len(), 1);
    assert!(negotiated[0].we_can_call);
    assert!(!negotiated[0].we_can_respond);
    assert!(!negotiated[0].is_bidirectional());
}

#[test]
fn test_negotiate_respond_only_with_call_only() {
    let our_caps = vec![ProtocolCapability::new(
        "TestService",
        vec![1],
        ProtocolDirection::RespondOnly,
    )];

    let peer_caps = vec![ProtocolCapability::new(
        "TestService",
        vec![1],
        ProtocolDirection::CallOnly,
    )];

    let negotiated = negotiate_protocols(&our_caps, &peer_caps);
    assert_eq!(negotiated.len(), 1);
    assert!(!negotiated[0].we_can_call);
    assert!(negotiated[0].we_can_respond);
}

#[test]
fn test_negotiate_incompatible_directions() {
    // Both CallOnly - incompatible
    let our_caps = vec![ProtocolCapability::new(
        "TestService",
        vec![1],
        ProtocolDirection::CallOnly,
    )];

    let peer_caps = vec![ProtocolCapability::new(
        "TestService",
        vec![1],
        ProtocolDirection::CallOnly,
    )];

    let negotiated = negotiate_protocols(&our_caps, &peer_caps);
    assert_eq!(negotiated.len(), 0);

    // Both RespondOnly - incompatible
    let our_caps = vec![ProtocolCapability::new(
        "TestService",
        vec![1],
        ProtocolDirection::RespondOnly,
    )];

    let peer_caps = vec![ProtocolCapability::new(
        "TestService",
        vec![1],
        ProtocolDirection::RespondOnly,
    )];

    let negotiated = negotiate_protocols(&our_caps, &peer_caps);
    assert_eq!(negotiated.len(), 0);
}

#[test]
fn test_negotiate_bidirectional_with_call_only() {
    let our_caps = vec![ProtocolCapability::new(
        "TestService",
        vec![1],
        ProtocolDirection::Bidirectional,
    )];

    let peer_caps = vec![ProtocolCapability::new(
        "TestService",
        vec![1],
        ProtocolDirection::CallOnly,
    )];

    let negotiated = negotiate_protocols(&our_caps, &peer_caps);
    assert_eq!(negotiated.len(), 1);
    assert!(!negotiated[0].we_can_call); // Peer can only call
    assert!(negotiated[0].we_can_respond); // We can respond
}

#[test]
fn test_negotiate_bidirectional_with_respond_only() {
    let our_caps = vec![ProtocolCapability::new(
        "TestService",
        vec![1],
        ProtocolDirection::Bidirectional,
    )];

    let peer_caps = vec![ProtocolCapability::new(
        "TestService",
        vec![1],
        ProtocolDirection::RespondOnly,
    )];

    let negotiated = negotiate_protocols(&our_caps, &peer_caps);
    assert_eq!(negotiated.len(), 1);
    assert!(negotiated[0].we_can_call); // We can call
    assert!(!negotiated[0].we_can_respond); // Peer can only respond
}

#[test]
fn test_negotiate_features_intersection() {
    let our_caps = vec![
        ProtocolCapability::new("TestService", vec![1], ProtocolDirection::Bidirectional)
            .with_feature("streaming")
            .with_feature("compression")
            .with_feature("encryption"),
    ];

    let peer_caps = vec![
        ProtocolCapability::new("TestService", vec![1], ProtocolDirection::Bidirectional)
            .with_feature("streaming")
            .with_feature("encryption")
            .with_feature("batching"),
    ];

    let negotiated = negotiate_protocols(&our_caps, &peer_caps);
    assert_eq!(negotiated.len(), 1);
    assert_eq!(negotiated[0].features.len(), 2);
    assert!(negotiated[0].features.contains("streaming"));
    assert!(negotiated[0].features.contains("encryption"));
    assert!(!negotiated[0].features.contains("compression"));
    assert!(!negotiated[0].features.contains("batching"));
}

#[test]
fn test_negotiate_no_common_features() {
    let our_caps = vec![
        ProtocolCapability::new("TestService", vec![1], ProtocolDirection::Bidirectional)
            .with_feature("feature_a")
            .with_feature("feature_b"),
    ];

    let peer_caps = vec![
        ProtocolCapability::new("TestService", vec![1], ProtocolDirection::Bidirectional)
            .with_feature("feature_c")
            .with_feature("feature_d"),
    ];

    let negotiated = negotiate_protocols(&our_caps, &peer_caps);
    assert_eq!(negotiated.len(), 1);
    assert!(negotiated[0].features.is_empty());
}

#[test]
fn test_negotiate_empty_capabilities() {
    let our_caps: Vec<ProtocolCapability> = vec![];
    let peer_caps: Vec<ProtocolCapability> = vec![];

    let negotiated = negotiate_protocols(&our_caps, &peer_caps);
    assert_eq!(negotiated.len(), 0);
}

#[test]
fn test_negotiate_one_side_empty() {
    let our_caps = vec![ProtocolCapability::new(
        "TestService",
        vec![1],
        ProtocolDirection::Bidirectional,
    )];
    let peer_caps: Vec<ProtocolCapability> = vec![];

    let negotiated = negotiate_protocols(&our_caps, &peer_caps);
    assert_eq!(negotiated.len(), 0);
}

#[test]
fn test_negotiate_multiple_protocols_mixed_results() {
    let our_caps = vec![
        ProtocolCapability::new("Service1", vec![1, 2], ProtocolDirection::Bidirectional),
        ProtocolCapability::new("Service2", vec![1], ProtocolDirection::CallOnly),
        ProtocolCapability::new("Service3", vec![3], ProtocolDirection::Bidirectional),
    ];

    let peer_caps = vec![
        ProtocolCapability::new("Service1", vec![2, 3], ProtocolDirection::Bidirectional),
        ProtocolCapability::new("Service2", vec![2], ProtocolDirection::RespondOnly),
        ProtocolCapability::new("Service3", vec![3], ProtocolDirection::CallOnly),
    ];

    let negotiated = negotiate_protocols(&our_caps, &peer_caps);
    assert_eq!(negotiated.len(), 2); // Service1 and Service3

    // Service1: version 2, bidirectional
    let service1 = negotiated.iter().find(|n| n.name == "Service1").unwrap();
    assert_eq!(service1.version, 2);
    assert!(service1.is_bidirectional());

    // Service2: no common version
    assert!(negotiated.iter().all(|n| n.name != "Service2"));

    // Service3: version 3, we can only respond
    let service3 = negotiated.iter().find(|n| n.name == "Service3").unwrap();
    assert_eq!(service3.version, 3);
    assert!(!service3.we_can_call);
    assert!(service3.we_can_respond);
}

#[test]
fn test_protocol_capability_version_support() {
    let cap = ProtocolCapability::new(
        "TestService",
        vec![1, 2, 3],
        ProtocolDirection::Bidirectional,
    );

    assert!(cap.supports_version(1));
    assert!(cap.supports_version(2));
    assert!(cap.supports_version(3));
    assert!(!cap.supports_version(4));
    assert!(!cap.supports_version(0));
}

#[test]
fn test_protocol_capability_feature_support() {
    let cap = ProtocolCapability::new("TestService", vec![1], ProtocolDirection::Bidirectional)
        .with_feature("streaming")
        .with_feature("compression");

    assert!(cap.has_feature("streaming"));
    assert!(cap.has_feature("compression"));
    assert!(!cap.has_feature("encryption"));
}

#[test]
fn test_negotiated_protocol_usability() {
    // Bidirectional - usable
    let mut proto = bdrpc::endpoint::NegotiatedProtocol::new("Test", 1);
    proto.we_can_call = true;
    proto.we_can_respond = true;
    assert!(proto.is_usable());
    assert!(proto.is_bidirectional());

    // Call only - usable
    let mut proto = bdrpc::endpoint::NegotiatedProtocol::new("Test", 1);
    proto.we_can_call = true;
    proto.we_can_respond = false;
    assert!(proto.is_usable());
    assert!(!proto.is_bidirectional());

    // Respond only - usable
    let mut proto = bdrpc::endpoint::NegotiatedProtocol::new("Test", 1);
    proto.we_can_call = false;
    proto.we_can_respond = true;
    assert!(proto.is_usable());
    assert!(!proto.is_bidirectional());

    // Neither - not usable
    let proto = bdrpc::endpoint::NegotiatedProtocol::new("Test", 1);
    assert!(!proto.is_usable());
    assert!(!proto.is_bidirectional());
}

#[test]
fn test_negotiate_version_priority() {
    // Should always pick highest common version
    let our_caps = vec![ProtocolCapability::new(
        "TestService",
        vec![1, 3, 5, 7, 9],
        ProtocolDirection::Bidirectional,
    )];

    let peer_caps = vec![ProtocolCapability::new(
        "TestService",
        vec![2, 3, 4, 5, 6],
        ProtocolDirection::Bidirectional,
    )];

    let negotiated = negotiate_protocols(&our_caps, &peer_caps);
    assert_eq!(negotiated.len(), 1);
    assert_eq!(negotiated[0].version, 5); // Highest common: 3 and 5, pick 5
}

#[test]
fn test_negotiate_with_duplicate_versions() {
    // Duplicate versions should be handled gracefully
    let our_caps = vec![ProtocolCapability::new(
        "TestService",
        vec![1, 2, 2, 3, 3, 3],
        ProtocolDirection::Bidirectional,
    )];

    let peer_caps = vec![ProtocolCapability::new(
        "TestService",
        vec![2, 2, 3, 3, 4, 4],
        ProtocolDirection::Bidirectional,
    )];

    let negotiated = negotiate_protocols(&our_caps, &peer_caps);
    assert_eq!(negotiated.len(), 1);
    assert_eq!(negotiated[0].version, 3);
}

#[test]
fn test_direction_compatibility_matrix() {
    let directions = vec![
        ProtocolDirection::Bidirectional,
        ProtocolDirection::CallOnly,
        ProtocolDirection::RespondOnly,
    ];

    // Test all combinations
    for our_dir in &directions {
        for peer_dir in &directions {
            let our_caps = vec![ProtocolCapability::new("Test", vec![1], *our_dir)];
            let peer_caps = vec![ProtocolCapability::new("Test", vec![1], *peer_dir)];

            let negotiated = negotiate_protocols(&our_caps, &peer_caps);

            // Check if compatible
            let is_compatible = our_dir.is_compatible_with(peer_dir);
            if is_compatible {
                assert_eq!(
                    negotiated.len(),
                    1,
                    "Should negotiate for {:?} + {:?}",
                    our_dir,
                    peer_dir
                );
            } else {
                assert_eq!(
                    negotiated.len(),
                    0,
                    "Should not negotiate for {:?} + {:?}",
                    our_dir,
                    peer_dir
                );
            }
        }
    }
}

// Made with Bob

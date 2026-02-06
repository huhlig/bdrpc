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

//! Integration tests for error recovery behavior.
//!
//! These tests verify that the error recovery strategies defined in ADR-004
//! and documented in docs/error-recovery.md are correctly implemented.

use bdrpc::BdrpcError;
use bdrpc::channel::{ChannelError, ChannelId};
use bdrpc::transport::TransportError;
use std::io;
use std::time::Duration;

/// Test that transport errors are identified correctly.
#[test]
fn test_transport_error_identification() {
    let error = BdrpcError::Transport(TransportError::Closed);
    assert!(error.is_transport_error());
    assert!(!error.is_channel_error());
    assert!(!error.is_application_error());
}

/// Test that channel errors are identified correctly.
#[test]
fn test_channel_error_identification() {
    let error = BdrpcError::Channel(ChannelError::Closed {
        channel_id: ChannelId::from(1),
    });
    assert!(!error.is_transport_error());
    assert!(error.is_channel_error());
    assert!(!error.is_application_error());
}

/// Test that application errors are identified correctly.
#[test]
fn test_application_error_identification() {
    let app_error = io::Error::other("test");
    let error = BdrpcError::Application(Box::new(app_error));
    assert!(!error.is_transport_error());
    assert!(!error.is_channel_error());
    assert!(error.is_application_error());
}

/// Test that recoverable transport errors are identified correctly.
#[test]
fn test_transport_error_recoverability() {
    // Connection failed - recoverable
    let error = TransportError::ConnectionFailed {
        address: "127.0.0.1:8080".to_string(),
        source: io::Error::new(io::ErrorKind::ConnectionRefused, "refused"),
    };
    assert!(error.is_recoverable());

    // Connection lost - recoverable
    let error = TransportError::ConnectionLost {
        reason: "peer closed".to_string(),
        source: None,
    };
    assert!(error.is_recoverable());

    // Timeout - recoverable
    let error = TransportError::Timeout {
        duration: Duration::from_secs(30),
    };
    assert!(error.is_recoverable());

    // Closed - not recoverable
    let error = TransportError::Closed;
    assert!(!error.is_recoverable());

    // Invalid configuration - not recoverable
    let error = TransportError::InvalidConfiguration {
        reason: "invalid port".to_string(),
    };
    assert!(!error.is_recoverable());
}

/// Test that transport errors trigger transport closure.
#[test]
fn test_transport_error_closes_transport() {
    // Most transport errors should close the transport
    let error = TransportError::ConnectionLost {
        reason: "network failure".to_string(),
        source: None,
    };
    assert!(error.should_close_transport());

    let error = TransportError::Closed;
    assert!(error.should_close_transport());

    // Connection failures happen before transport is established
    let error = TransportError::ConnectionFailed {
        address: "127.0.0.1:8080".to_string(),
        source: io::Error::new(io::ErrorKind::ConnectionRefused, "refused"),
    };
    assert!(!error.should_close_transport());
}

/// Test that channel errors close only the affected channel, not the transport.
#[test]
fn test_channel_error_closes_only_channel() {
    // Test that a closed channel error doesn't affect the transport
    let channel_id = ChannelId::from(1);
    let error = ChannelError::Closed { channel_id };
    let bdrpc_err = BdrpcError::Channel(error);

    // Channel errors should not close the transport
    assert!(!bdrpc_err.should_close_transport());

    // But should close the affected channel
    assert!(bdrpc_err.should_close_channel());

    // Verify error properties
    assert!(bdrpc_err.is_channel_error());
    assert!(!bdrpc_err.is_recoverable());
}

/// Test that recoverable channel errors (Full) don't close the channel.
#[test]
fn test_channel_full_error_is_recoverable() {
    let error = ChannelError::Full {
        channel_id: ChannelId::from(1),
        buffer_size: 100,
    };

    // Full errors are recoverable
    assert!(error.is_recoverable());

    // Should not close the channel
    let bdrpc_err = BdrpcError::Channel(error);
    assert!(!bdrpc_err.should_close_channel());
}

/// Test that application errors don't close channels or transports.
#[test]
fn test_application_error_propagation() {
    // Create an application error
    let app_error = io::Error::other("custom application error");
    let bdrpc_error = BdrpcError::Application(Box::new(app_error));

    // Verify it's an application error
    assert!(bdrpc_error.is_application_error());

    // Application errors should not close transport or channel
    assert!(!bdrpc_error.should_close_transport());
    assert!(!bdrpc_error.should_close_channel());

    // Application errors are not recoverable at framework level
    assert!(!bdrpc_error.is_recoverable());
}

/// Test BdrpcError conversion from TransportError.
#[test]
fn test_bdrpc_error_from_transport_error() {
    let transport_err = TransportError::Closed;
    let bdrpc_err: BdrpcError = transport_err.into();

    assert!(bdrpc_err.is_transport_error());
    assert!(!bdrpc_err.is_channel_error());
    assert!(!bdrpc_err.is_application_error());
    assert!(bdrpc_err.should_close_transport());
}

/// Test BdrpcError conversion from ChannelError.
#[test]
fn test_bdrpc_error_from_channel_error() {
    let channel_err = ChannelError::Closed {
        channel_id: ChannelId::from(1),
    };
    let bdrpc_err: BdrpcError = channel_err.into();

    assert!(!bdrpc_err.is_transport_error());
    assert!(bdrpc_err.is_channel_error());
    assert!(!bdrpc_err.is_application_error());
    assert!(!bdrpc_err.should_close_transport());
    assert!(bdrpc_err.should_close_channel());
}

/// Test that transport errors close all channels.
#[test]
fn test_transport_error_closes_all_channels() {
    let transport_err = TransportError::ConnectionLost {
        reason: "network failure".to_string(),
        source: None,
    };
    let bdrpc_err: BdrpcError = transport_err.into();

    // Transport errors should close all channels
    assert!(bdrpc_err.should_close_channel());
    assert!(bdrpc_err.should_close_transport());
}

/// Test error display formatting.
#[test]
fn test_error_display() {
    // Transport error
    let error = BdrpcError::Transport(TransportError::Closed);
    let display = format!("{}", error);
    assert!(display.contains("transport error"));

    // Channel error
    let error = BdrpcError::Channel(ChannelError::Closed {
        channel_id: ChannelId::from(1),
    });
    let display = format!("{}", error);
    assert!(display.contains("channel error"));

    // Application error
    let app_error = io::Error::other("test");
    let error = BdrpcError::Application(Box::new(app_error));
    let display = format!("{}", error);
    assert!(display.contains("application error"));
}

/// Test error source chain.
#[test]
fn test_error_source_chain() {
    use std::error::Error;

    // Transport error with source
    let io_error = io::Error::new(io::ErrorKind::ConnectionRefused, "refused");
    let transport_err = TransportError::ConnectionFailed {
        address: "127.0.0.1:8080".to_string(),
        source: io_error,
    };
    let bdrpc_err = BdrpcError::Transport(transport_err);

    // Should have a source
    assert!(bdrpc_err.source().is_some());

    // Source should be the transport error
    let source = bdrpc_err.source().unwrap();
    assert!(source.to_string().contains("failed to connect"));
}

/// Test channel error with channel ID.
#[test]
fn test_channel_error_channel_id() {
    let channel_id = ChannelId::from(42);
    let error = ChannelError::Closed { channel_id };

    assert_eq!(error.channel_id(), Some(channel_id));
    assert!(error.is_closed());
}

/// Test out-of-order channel error.
#[test]
fn test_channel_out_of_order_error() {
    let error = ChannelError::OutOfOrder {
        channel_id: ChannelId::from(1),
        expected: 5,
        received: 7,
    };

    assert!(!error.is_recoverable());
    assert_eq!(error.channel_id(), Some(ChannelId::from(1)));

    let display = format!("{}", error);
    assert!(display.contains("out of order"));
    assert!(display.contains("expected sequence 5"));
    assert!(display.contains("got 7"));
}

/// Test channel already exists error.
#[test]
fn test_channel_already_exists_error() {
    let error = ChannelError::AlreadyExists {
        channel_id: ChannelId::from(1),
    };

    assert!(!error.is_recoverable());
    assert_eq!(error.channel_id(), Some(ChannelId::from(1)));
}

/// Test internal channel error.
#[test]
fn test_channel_internal_error() {
    let error = ChannelError::Internal {
        message: "unexpected state".to_string(),
    };

    assert!(!error.is_recoverable());
    assert_eq!(error.channel_id(), None);
}

/// Test transient I/O errors are recoverable.
#[test]
fn test_transient_io_errors_recoverable() {
    // Interrupted - recoverable
    let error = TransportError::ReadFailed {
        source: io::Error::new(io::ErrorKind::Interrupted, "interrupted"),
    };
    assert!(error.is_recoverable());
    assert!(!error.should_close_transport());

    // WouldBlock - recoverable
    let error = TransportError::WriteFailed {
        source: io::Error::new(io::ErrorKind::WouldBlock, "would block"),
    };
    assert!(error.is_recoverable());
    assert!(!error.should_close_transport());

    // TimedOut - recoverable
    let error = TransportError::Io {
        source: io::Error::new(io::ErrorKind::TimedOut, "timed out"),
    };
    assert!(error.is_recoverable());
}

/// Test permanent I/O errors are not recoverable.
#[test]
fn test_permanent_io_errors_not_recoverable() {
    // BrokenPipe - not recoverable
    let error = TransportError::ReadFailed {
        source: io::Error::new(io::ErrorKind::BrokenPipe, "broken pipe"),
    };
    assert!(!error.is_recoverable());
    assert!(error.should_close_transport());

    // ConnectionReset - not recoverable
    let error = TransportError::WriteFailed {
        source: io::Error::new(io::ErrorKind::ConnectionReset, "reset"),
    };
    assert!(!error.is_recoverable());
    assert!(error.should_close_transport());
}

/// Test bind failed error.
#[test]
fn test_bind_failed_error() {
    let error = TransportError::BindFailed {
        address: "0.0.0.0:8080".to_string(),
        source: io::Error::new(io::ErrorKind::AddrInUse, "address in use"),
    };

    assert!(!error.is_recoverable());
    assert!(!error.should_close_transport()); // Not connected yet

    let display = format!("{}", error);
    assert!(display.contains("failed to bind"));
    assert!(display.contains("0.0.0.0:8080"));
}

/// Test not connected error.
#[test]
fn test_not_connected_error() {
    let error = TransportError::NotConnected;

    assert!(error.is_recoverable());
    assert!(!error.should_close_transport()); // Not connected yet
}

/// Test error recovery decision tree for transport errors.
#[test]
fn test_transport_error_decision_tree() {
    // Scenario 1: Connection failed → Recoverable, don't close (not connected yet)
    let error = TransportError::ConnectionFailed {
        address: "127.0.0.1:8080".to_string(),
        source: io::Error::new(io::ErrorKind::ConnectionRefused, "refused"),
    };
    assert!(error.is_recoverable());
    assert!(!error.should_close_transport());

    // Scenario 2: Connection lost → Recoverable, close transport
    let error = TransportError::ConnectionLost {
        reason: "peer closed".to_string(),
        source: None,
    };
    assert!(error.is_recoverable());
    assert!(error.should_close_transport());

    // Scenario 3: Closed → Not recoverable, close transport
    let error = TransportError::Closed;
    assert!(!error.is_recoverable());
    assert!(error.should_close_transport());

    // Scenario 4: Timeout → Recoverable, close transport
    let error = TransportError::Timeout {
        duration: Duration::from_secs(30),
    };
    assert!(error.is_recoverable());
    assert!(error.should_close_transport());
}

/// Test error recovery decision tree for channel errors.
#[test]
fn test_channel_error_decision_tree() {
    // Scenario 1: Full → Recoverable, don't close channel
    let error = ChannelError::Full {
        channel_id: ChannelId::from(1),
        buffer_size: 100,
    };
    let bdrpc_err = BdrpcError::Channel(error);
    assert!(bdrpc_err.is_recoverable());
    assert!(!bdrpc_err.should_close_channel());

    // Scenario 2: Closed → Not recoverable, close channel
    let error = ChannelError::Closed {
        channel_id: ChannelId::from(1),
    };
    let bdrpc_err = BdrpcError::Channel(error);
    assert!(!bdrpc_err.is_recoverable());
    assert!(bdrpc_err.should_close_channel());

    // Scenario 3: NotFound → Not recoverable, close channel
    let error = ChannelError::NotFound {
        channel_id: ChannelId::from(1),
    };
    let bdrpc_err = BdrpcError::Channel(error);
    assert!(!bdrpc_err.is_recoverable());
    assert!(bdrpc_err.should_close_channel());
}

/// Test error recovery decision tree for application errors.
#[test]
fn test_application_error_decision_tree() {
    // Application errors → Not recoverable, don't close anything
    let app_error = io::Error::other("custom error");
    let bdrpc_err = BdrpcError::Application(Box::new(app_error));

    assert!(!bdrpc_err.is_recoverable());
    assert!(!bdrpc_err.should_close_transport());
    assert!(!bdrpc_err.should_close_channel());
}

/// Test that channel NotFound error is not recoverable.
#[test]
fn test_channel_not_found_error() {
    let error = ChannelError::NotFound {
        channel_id: ChannelId::from(1),
    };

    assert!(!error.is_recoverable());
    assert_eq!(error.channel_id(), Some(ChannelId::from(1)));

    let display = format!("{}", error);
    assert!(display.contains("not found"));
}

/// Test comprehensive error recovery matrix.
#[test]
fn test_error_recovery_matrix() {
    // Transport errors
    let transport_errors = vec![
        (TransportError::Closed, false, true),
        (
            TransportError::ConnectionFailed {
                address: "test".to_string(),
                source: io::Error::new(io::ErrorKind::ConnectionRefused, "test"),
            },
            true,
            false,
        ),
        (
            TransportError::ConnectionLost {
                reason: "test".to_string(),
                source: None,
            },
            true,
            true,
        ),
        (
            TransportError::Timeout {
                duration: Duration::from_secs(1),
            },
            true,
            true,
        ),
    ];

    for (error, expected_recoverable, expected_close) in transport_errors {
        assert_eq!(
            error.is_recoverable(),
            expected_recoverable,
            "Error {:?} recoverability mismatch",
            error
        );
        assert_eq!(
            error.should_close_transport(),
            expected_close,
            "Error {:?} close behavior mismatch",
            error
        );
    }

    // Channel errors
    let channel_errors = vec![
        (
            ChannelError::Closed {
                channel_id: ChannelId::from(1),
            },
            false,
        ),
        (
            ChannelError::Full {
                channel_id: ChannelId::from(1),
                buffer_size: 100,
            },
            true,
        ),
        (
            ChannelError::NotFound {
                channel_id: ChannelId::from(1),
            },
            false,
        ),
    ];

    for (error, expected_recoverable) in channel_errors {
        assert_eq!(
            error.is_recoverable(),
            expected_recoverable,
            "Error {:?} recoverability mismatch",
            error
        );
    }
}

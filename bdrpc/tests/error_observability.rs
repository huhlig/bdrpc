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

//! Integration tests for error observability features.
//!
//! These tests verify that error metrics, callbacks, and logging work correctly
//! across all layers of the BDRPC framework.

use bdrpc::observability::{ErrorMetrics, ErrorObserver};
use bdrpc::{BdrpcError, ChannelError, ChannelId, TransportError};
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[test]
fn test_metrics_track_all_error_types() {
    let metrics = ErrorMetrics::new();

    // Record various error types
    metrics.record_error(&BdrpcError::Transport(TransportError::Closed));
    metrics.record_error(&BdrpcError::Channel(ChannelError::Closed {
        channel_id: ChannelId::from(1),
    }));
    metrics.record_error(&BdrpcError::Application(Box::new(io::Error::new(
        io::ErrorKind::Other,
        "test",
    ))));

    // Verify counts
    assert_eq!(metrics.transport_errors(), 1);
    assert_eq!(metrics.channel_errors(), 1);
    assert_eq!(metrics.application_errors(), 1);
    assert_eq!(metrics.total_errors(), 3);
}

#[test]
fn test_metrics_track_recoverability() {
    let metrics = ErrorMetrics::new();

    // Recoverable errors
    metrics.record_error(&BdrpcError::Channel(ChannelError::Full {
        channel_id: ChannelId::from(1),
        buffer_size: 100,
    }));
    metrics.record_error(&BdrpcError::Transport(TransportError::Timeout {
        duration: std::time::Duration::from_secs(30),
    }));

    // Non-recoverable errors
    metrics.record_error(&BdrpcError::Transport(TransportError::Closed));
    metrics.record_error(&BdrpcError::Channel(ChannelError::Closed {
        channel_id: ChannelId::from(1),
    }));

    assert_eq!(metrics.recoverable_errors(), 2);
    assert_eq!(metrics.total_errors(), 4);
}

#[test]
fn test_metrics_track_closures() {
    let metrics = ErrorMetrics::new();

    // Errors that close transport
    metrics.record_error(&BdrpcError::Transport(TransportError::Closed));
    metrics.record_error(&BdrpcError::Transport(TransportError::ConnectionLost {
        reason: "peer closed".to_string(),
        source: None,
    }));

    // Errors that close channel
    metrics.record_error(&BdrpcError::Channel(ChannelError::Closed {
        channel_id: ChannelId::from(1),
    }));
    metrics.record_error(&BdrpcError::Channel(ChannelError::NotFound {
        channel_id: ChannelId::from(2),
    }));

    // Errors that don't close anything
    metrics.record_error(&BdrpcError::Channel(ChannelError::Full {
        channel_id: ChannelId::from(3),
        buffer_size: 100,
    }));

    assert_eq!(metrics.transport_closures(), 2);
    assert_eq!(metrics.channel_closures(), 4); // Transport errors also close channels
}

#[test]
fn test_observer_notifies_all_callbacks() {
    let observer = ErrorObserver::new();
    let counter1 = Arc::new(AtomicU64::new(0));
    let counter2 = Arc::new(AtomicU64::new(0));
    let counter3 = Arc::new(AtomicU64::new(0));

    // Register multiple callbacks
    let c1 = counter1.clone();
    observer.on_error(move |_| {
        c1.fetch_add(1, Ordering::Relaxed);
    });

    let c2 = counter2.clone();
    observer.on_error(move |_| {
        c2.fetch_add(1, Ordering::Relaxed);
    });

    let c3 = counter3.clone();
    observer.on_error(move |_| {
        c3.fetch_add(1, Ordering::Relaxed);
    });

    // Notify with different error types
    observer.notify(&BdrpcError::Transport(TransportError::Closed));
    observer.notify(&BdrpcError::Channel(ChannelError::Closed {
        channel_id: ChannelId::from(1),
    }));

    // All callbacks should have been called twice
    assert_eq!(counter1.load(Ordering::Relaxed), 2);
    assert_eq!(counter2.load(Ordering::Relaxed), 2);
    assert_eq!(counter3.load(Ordering::Relaxed), 2);
}

#[test]
fn test_observer_callback_receives_correct_error() {
    let observer = ErrorObserver::new();
    let transport_count = Arc::new(AtomicU64::new(0));
    let channel_count = Arc::new(AtomicU64::new(0));
    let app_count = Arc::new(AtomicU64::new(0));

    let tc = transport_count.clone();
    let cc = channel_count.clone();
    let ac = app_count.clone();

    observer.on_error(move |error| {
        if error.is_transport_error() {
            tc.fetch_add(1, Ordering::Relaxed);
        } else if error.is_channel_error() {
            cc.fetch_add(1, Ordering::Relaxed);
        } else if error.is_application_error() {
            ac.fetch_add(1, Ordering::Relaxed);
        }
    });

    // Send different error types
    observer.notify(&BdrpcError::Transport(TransportError::Closed));
    observer.notify(&BdrpcError::Transport(TransportError::NotConnected));
    observer.notify(&BdrpcError::Channel(ChannelError::Closed {
        channel_id: ChannelId::from(1),
    }));
    observer.notify(&BdrpcError::Application(Box::new(io::Error::new(
        io::ErrorKind::Other,
        "test",
    ))));

    assert_eq!(transport_count.load(Ordering::Relaxed), 2);
    assert_eq!(channel_count.load(Ordering::Relaxed), 1);
    assert_eq!(app_count.load(Ordering::Relaxed), 1);
}

#[test]
fn test_combined_metrics_and_observer() {
    let metrics = ErrorMetrics::new();
    let observer = ErrorObserver::new();
    let callback_count = Arc::new(AtomicU64::new(0));

    let cc = callback_count.clone();
    let metrics_clone = Arc::new(metrics);
    let mc = metrics_clone.clone();

    observer.on_error(move |error| {
        mc.record_error(error);
        cc.fetch_add(1, Ordering::Relaxed);
    });

    // Generate errors
    let errors = vec![
        BdrpcError::Transport(TransportError::Closed),
        BdrpcError::Channel(ChannelError::Closed {
            channel_id: ChannelId::from(1),
        }),
        BdrpcError::Application(Box::new(io::Error::new(io::ErrorKind::Other, "test"))),
    ];

    for error in errors {
        observer.notify(&error);
    }

    // Verify both metrics and callbacks were triggered
    assert_eq!(callback_count.load(Ordering::Relaxed), 3);
    assert_eq!(metrics_clone.total_errors(), 3);
    assert_eq!(metrics_clone.transport_errors(), 1);
    assert_eq!(metrics_clone.channel_errors(), 1);
    assert_eq!(metrics_clone.application_errors(), 1);
}

#[test]
fn test_error_propagation_through_layers() {
    // Simulate error propagation from transport -> channel -> application
    let metrics = ErrorMetrics::new();

    // Transport error occurs
    let transport_error = TransportError::ConnectionLost {
        reason: "network failure".to_string(),
        source: None,
    };
    let bdrpc_error = BdrpcError::Transport(transport_error);
    metrics.record_error(&bdrpc_error);

    // Verify transport error properties
    assert!(bdrpc_error.is_transport_error());
    assert!(bdrpc_error.should_close_transport());
    assert!(bdrpc_error.should_close_channel()); // Transport errors close all channels
    assert!(bdrpc_error.is_recoverable());

    // Channel error occurs (independent of transport)
    let channel_error = ChannelError::Full {
        channel_id: ChannelId::from(1),
        buffer_size: 100,
    };
    let bdrpc_error = BdrpcError::Channel(channel_error);
    metrics.record_error(&bdrpc_error);

    // Verify channel error properties
    assert!(bdrpc_error.is_channel_error());
    assert!(!bdrpc_error.should_close_transport());
    assert!(!bdrpc_error.should_close_channel()); // Recoverable channel error
    assert!(bdrpc_error.is_recoverable());

    // Application error occurs
    let app_error = io::Error::new(io::ErrorKind::PermissionDenied, "access denied");
    let bdrpc_error = BdrpcError::Application(Box::new(app_error));
    metrics.record_error(&bdrpc_error);

    // Verify application error properties
    assert!(bdrpc_error.is_application_error());
    assert!(!bdrpc_error.should_close_transport());
    assert!(!bdrpc_error.should_close_channel());
    assert!(!bdrpc_error.is_recoverable()); // Application errors not recoverable at framework level

    // Verify metrics captured all layers
    assert_eq!(metrics.transport_errors(), 1);
    assert_eq!(metrics.channel_errors(), 1);
    assert_eq!(metrics.application_errors(), 1);
    assert_eq!(metrics.total_errors(), 3);
}

#[test]
fn test_metrics_thread_safety() {
    use std::thread;

    let metrics = Arc::new(ErrorMetrics::new());
    let mut handles = vec![];

    // Spawn multiple threads recording errors
    for i in 0..10 {
        let metrics_clone = metrics.clone();
        let handle = thread::spawn(move || {
            for _ in 0..100 {
                let error = if i % 3 == 0 {
                    BdrpcError::Transport(TransportError::Closed)
                } else if i % 3 == 1 {
                    BdrpcError::Channel(ChannelError::Closed {
                        channel_id: ChannelId::from(i),
                    })
                } else {
                    BdrpcError::Application(Box::new(io::Error::new(io::ErrorKind::Other, "test")))
                };
                metrics_clone.record_error(&error);
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify total count (10 threads * 100 errors each)
    assert_eq!(metrics.total_errors(), 1000);
}

#[test]
fn test_observer_thread_safety() {
    use std::thread;

    let observer = Arc::new(ErrorObserver::new());
    let counter = Arc::new(AtomicU64::new(0));

    let c = counter.clone();
    observer.on_error(move |_| {
        c.fetch_add(1, Ordering::Relaxed);
    });

    let mut handles = vec![];

    // Spawn multiple threads notifying errors
    for _ in 0..10 {
        let observer_clone = observer.clone();
        let handle = thread::spawn(move || {
            for _ in 0..100 {
                observer_clone.notify(&BdrpcError::Transport(TransportError::Closed));
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify callback was called for all notifications
    assert_eq!(counter.load(Ordering::Relaxed), 1000);
}

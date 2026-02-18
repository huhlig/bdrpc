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

//! Tracking of pending RPC requests awaiting responses.
//!
//! This module provides utilities for managing in-flight RPC requests
//! and routing responses back to the correct caller.

use std::collections::HashMap;
use tokio::sync::{Mutex, oneshot};

/// Tracks pending RPC requests awaiting responses.
///
/// This structure maintains a mapping from correlation IDs to response channels,
/// allowing responses to be routed back to the correct caller even when multiple
/// requests are in-flight concurrently.
///
/// # Thread Safety
///
/// This structure is thread-safe and can be shared across multiple tasks.
/// It uses a Tokio mutex for interior mutability.
///
/// # Example
///
/// ```rust
/// use bdrpc::channel::PendingRequests;
///
/// # async fn example() {
/// let pending = PendingRequests::<String>::new();
///
/// // Register a pending request
/// let rx = pending.register(42).await;
///
/// // Complete the request from another task
/// pending.complete(42, "response".to_string()).await;
///
/// // Receive the response
/// let response = rx.await.unwrap();
/// assert_eq!(response, "response");
/// # }
/// ```
#[derive(Debug)]
pub struct PendingRequests<T> {
    requests: Mutex<HashMap<u64, oneshot::Sender<T>>>,
}

impl<T> PendingRequests<T> {
    /// Creates a new pending requests tracker.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::channel::PendingRequests;
    ///
    /// let pending = PendingRequests::<String>::new();
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            requests: Mutex::new(HashMap::new()),
        }
    }

    /// Register a pending request with the given correlation ID.
    ///
    /// Returns a receiver that will be notified when the response arrives.
    /// The caller should await on this receiver to get the response.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::channel::PendingRequests;
    ///
    /// # async fn example() {
    /// let pending = PendingRequests::<String>::new();
    /// let rx = pending.register(42).await;
    /// // ... send request with correlation_id 42 ...
    /// // ... await rx for response ...
    /// # }
    /// ```
    pub async fn register(&self, correlation_id: u64) -> oneshot::Receiver<T> {
        let (tx, rx) = oneshot::channel();
        self.requests.lock().await.insert(correlation_id, tx);
        rx
    }

    /// Complete a pending request with the given response.
    ///
    /// Returns `true` if the request was found and completed successfully,
    /// `false` if the request was not found (already completed or cancelled).
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::channel::PendingRequests;
    ///
    /// # async fn example() {
    /// let pending = PendingRequests::<String>::new();
    /// let rx = pending.register(42).await;
    ///
    /// // Complete the request
    /// let completed = pending.complete(42, "response".to_string()).await;
    /// assert!(completed);
    ///
    /// // Receive the response
    /// let response = rx.await.unwrap();
    /// assert_eq!(response, "response");
    /// # }
    /// ```
    pub async fn complete(&self, correlation_id: u64, response: T) -> bool {
        if let Some(tx) = self.requests.lock().await.remove(&correlation_id) {
            tx.send(response).is_ok()
        } else {
            false
        }
    }

    /// Cancel a pending request (e.g., on timeout).
    ///
    /// Returns `true` if the request was found and cancelled,
    /// `false` if the request was not found.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::channel::PendingRequests;
    ///
    /// # async fn example() {
    /// let pending = PendingRequests::<String>::new();
    /// let rx = pending.register(42).await;
    ///
    /// // Cancel the request
    /// let cancelled = pending.cancel(42).await;
    /// assert!(cancelled);
    ///
    /// // Receiver will get an error
    /// assert!(rx.await.is_err());
    /// # }
    /// ```
    pub async fn cancel(&self, correlation_id: u64) -> bool {
        self.requests.lock().await.remove(&correlation_id).is_some()
    }

    /// Get the number of pending requests.
    ///
    /// This is useful for monitoring and debugging.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::channel::PendingRequests;
    ///
    /// # async fn example() {
    /// let pending = PendingRequests::<String>::new();
    /// assert_eq!(pending.len().await, 0);
    ///
    /// let _rx = pending.register(42).await;
    /// assert_eq!(pending.len().await, 1);
    /// # }
    /// ```
    pub async fn len(&self) -> usize {
        self.requests.lock().await.len()
    }

    /// Returns `true` if there are no pending requests.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::channel::PendingRequests;
    ///
    /// # async fn example() {
    /// let pending = PendingRequests::<String>::new();
    /// assert!(pending.is_empty().await);
    ///
    /// let _rx = pending.register(42).await;
    /// assert!(!pending.is_empty().await);
    /// # }
    /// ```
    pub async fn is_empty(&self) -> bool {
        self.requests.lock().await.is_empty()
    }
}

impl<T> Default for PendingRequests<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_and_complete() {
        let pending = PendingRequests::<String>::new();
        let rx = pending.register(42).await;

        assert_eq!(pending.len().await, 1);

        let completed = pending.complete(42, "response".to_string()).await;
        assert!(completed);

        let response = rx.await.unwrap();
        assert_eq!(response, "response");
        assert_eq!(pending.len().await, 0);
    }

    #[tokio::test]
    async fn test_complete_nonexistent() {
        let pending = PendingRequests::<String>::new();
        let completed = pending.complete(99, "response".to_string()).await;
        assert!(!completed);
    }

    #[tokio::test]
    async fn test_cancel() {
        let pending = PendingRequests::<String>::new();
        let rx = pending.register(42).await;

        let cancelled = pending.cancel(42).await;
        assert!(cancelled);

        // Receiver should get an error
        assert!(rx.await.is_err());
        assert_eq!(pending.len().await, 0);
    }

    #[tokio::test]
    async fn test_cancel_nonexistent() {
        let pending = PendingRequests::<String>::new();
        let cancelled = pending.cancel(99).await;
        assert!(!cancelled);
    }

    #[tokio::test]
    async fn test_multiple_pending() {
        let pending = PendingRequests::<String>::new();
        let rx1 = pending.register(1).await;
        let rx2 = pending.register(2).await;
        let rx3 = pending.register(3).await;

        assert_eq!(pending.len().await, 3);

        pending.complete(2, "two".to_string()).await;
        pending.complete(1, "one".to_string()).await;
        pending.complete(3, "three".to_string()).await;

        assert_eq!(rx1.await.unwrap(), "one");
        assert_eq!(rx2.await.unwrap(), "two");
        assert_eq!(rx3.await.unwrap(), "three");
        assert_eq!(pending.len().await, 0);
    }

    #[tokio::test]
    async fn test_is_empty() {
        let pending = PendingRequests::<String>::new();
        assert!(pending.is_empty().await);

        let _rx = pending.register(42).await;
        assert!(!pending.is_empty().await);

        pending.cancel(42).await;
        assert!(pending.is_empty().await);
    }

    #[tokio::test]
    async fn test_concurrent_operations() {
        use std::sync::Arc;

        let pending = Arc::new(PendingRequests::<u64>::new());
        let mut handles = vec![];

        // Spawn multiple tasks registering and completing requests
        for i in 0..100 {
            let pending_clone = pending.clone();
            handles.push(tokio::spawn(async move {
                let rx = pending_clone.register(i).await;
                pending_clone.complete(i, i * 2).await;
                rx.await.unwrap()
            }));
        }

        // All should complete successfully
        for (i, handle) in handles.into_iter().enumerate() {
            let result = handle.await.unwrap();
            assert_eq!(result, (i as u64) * 2);
        }

        assert!(pending.is_empty().await);
    }
}

// Made with Bob

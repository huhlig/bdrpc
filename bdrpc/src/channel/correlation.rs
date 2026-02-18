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

//! Correlation ID generation for request-response matching.
//!
//! This module provides utilities for generating unique correlation IDs
//! that enable concurrent RPC calls without race conditions.

use std::sync::atomic::{AtomicU64, Ordering};

/// Generates unique correlation IDs for request-response matching.
///
/// Correlation IDs are used to match responses to their corresponding requests
/// in RPC channels, enabling multiple concurrent requests without race conditions.
///
/// # Thread Safety
///
/// This generator is thread-safe and can be shared across multiple tasks.
/// It uses atomic operations for lock-free ID generation.
///
/// # ID Space
///
/// IDs start at 1 and increment monotonically. ID 0 is reserved for
/// non-correlated messages. The u64 space provides 2^64 unique IDs,
/// which is effectively unlimited for practical purposes.
///
/// # Example
///
/// ```rust
/// use bdrpc::channel::CorrelationIdGenerator;
///
/// let generator = CorrelationIdGenerator::new();
/// let id1 = generator.next();
/// let id2 = generator.next();
/// assert_ne!(id1, id2);
/// assert!(id1 > 0);
/// assert!(id2 > 0);
/// ```
#[derive(Debug)]
pub struct CorrelationIdGenerator {
    next_id: AtomicU64,
}

impl CorrelationIdGenerator {
    /// Creates a new correlation ID generator.
    ///
    /// The generator starts at ID 1 (ID 0 is reserved).
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::channel::CorrelationIdGenerator;
    ///
    /// let generator = CorrelationIdGenerator::new();
    /// assert_eq!(generator.next(), 1);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
        }
    }

    /// Generate the next correlation ID.
    ///
    /// IDs start at 1 and increment monotonically. This method is thread-safe
    /// and lock-free.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::channel::CorrelationIdGenerator;
    ///
    /// let generator = CorrelationIdGenerator::new();
    /// let id = generator.next();
    /// assert!(id > 0);
    /// ```
    #[must_use]
    pub fn next(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Returns the current ID value without incrementing.
    ///
    /// This is primarily useful for testing and debugging.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::channel::CorrelationIdGenerator;
    ///
    /// let generator = CorrelationIdGenerator::new();
    /// assert_eq!(generator.current(), 1);
    /// generator.next();
    /// assert_eq!(generator.current(), 2);
    /// ```
    #[must_use]
    pub fn current(&self) -> u64 {
        self.next_id.load(Ordering::Relaxed)
    }
}

impl Default for CorrelationIdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generator_starts_at_one() {
        let generator = CorrelationIdGenerator::new();
        assert_eq!(generator.next(), 1);
    }

    #[test]
    fn test_generator_increments() {
        let generator = CorrelationIdGenerator::new();
        assert_eq!(generator.next(), 1);
        assert_eq!(generator.next(), 2);
        assert_eq!(generator.next(), 3);
    }

    #[test]
    fn test_generator_current() {
        let generator = CorrelationIdGenerator::new();
        assert_eq!(generator.current(), 1);
        let _ = generator.next();
        assert_eq!(generator.current(), 2);
    }

    #[test]
    fn test_generator_uniqueness() {
        let generator = CorrelationIdGenerator::new();
        let mut ids = std::collections::HashSet::new();
        for _ in 0..1000 {
            let id = generator.next();
            assert!(ids.insert(id), "Duplicate ID generated: {}", id);
        }
    }

    #[tokio::test]
    async fn test_generator_concurrent() {
        use std::sync::Arc;

        let generator = Arc::new(CorrelationIdGenerator::new());
        let mut handles = vec![];

        for _ in 0..10 {
            let gen_clone = generator.clone();
            handles.push(tokio::spawn(async move {
                let mut ids = vec![];
                for _ in 0..100 {
                    ids.push(gen_clone.next());
                }
                ids
            }));
        }

        let mut all_ids = std::collections::HashSet::new();
        for handle in handles {
            let ids = handle.await.unwrap();
            for id in ids {
                assert!(
                    all_ids.insert(id),
                    "Duplicate ID in concurrent test: {}",
                    id
                );
            }
        }

        assert_eq!(all_ids.len(), 1000);
    }
}

// Made with Bob

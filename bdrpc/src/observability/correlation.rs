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

//! Correlation ID support for distributed tracing.
//!
//! This module provides correlation ID generation and propagation for tracking
//! requests across multiple services and components. Correlation IDs are used
//! to link related operations together in distributed systems.
//!
//! # Overview
//!
//! A correlation ID is a unique identifier that is:
//! - Generated at the entry point of a request
//! - Propagated through all related operations
//! - Included in logs and traces
//! - Used to correlate events across services
//!
//! # Examples
//!
//! ```rust
//! use bdrpc::observability::CorrelationId;
//!
//! // Generate a new correlation ID
//! let correlation_id = CorrelationId::new();
//! println!("Request ID: {}", correlation_id);
//!
//! // Parse from string
//! let parsed = CorrelationId::from_string("550e8400-e29b-41d4-a716-446655440000").unwrap();
//! assert_eq!(parsed.to_string(), "550e8400-e29b-41d4-a716-446655440000");
//!
//! // Use in tracing spans
//! #[cfg(feature = "observability")]
//! {
//!     tracing::info!(correlation_id = %correlation_id, "Processing request");
//! }
//! ```

use std::fmt;
use std::str::FromStr;

/// A unique identifier for correlating related operations across services.
///
/// Correlation IDs are used in distributed tracing to link related operations
/// together. They are typically generated at the entry point of a request and
/// propagated through all subsequent operations.
///
/// # Format
///
/// Correlation IDs are represented as UUIDs (RFC 4122) in the standard
/// hyphenated format: `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`
///
/// # Examples
///
/// ```rust
/// use bdrpc::observability::CorrelationId;
///
/// // Generate a new correlation ID
/// let id = CorrelationId::new();
/// println!("Correlation ID: {}", id);
///
/// // Parse from string
/// let parsed = CorrelationId::from_string("550e8400-e29b-41d4-a716-446655440000").unwrap();
/// assert_eq!(parsed.to_string(), "550e8400-e29b-41d4-a716-446655440000");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CorrelationId(uuid::Uuid);

impl CorrelationId {
    /// Creates a new random correlation ID.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::CorrelationId;
    ///
    /// let id = CorrelationId::new();
    /// println!("Generated ID: {}", id);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    /// Creates a correlation ID from a UUID.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::CorrelationId;
    /// use uuid::Uuid;
    ///
    /// let uuid = Uuid::new_v4();
    /// let id = CorrelationId::from_uuid(uuid);
    /// assert_eq!(id.as_uuid(), &uuid);
    /// ```
    #[must_use]
    pub const fn from_uuid(uuid: uuid::Uuid) -> Self {
        Self(uuid)
    }

    /// Creates a correlation ID from a string.
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not a valid UUID.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::CorrelationId;
    ///
    /// let id = CorrelationId::from_string("550e8400-e29b-41d4-a716-446655440000").unwrap();
    /// assert_eq!(id.to_string(), "550e8400-e29b-41d4-a716-446655440000");
    ///
    /// assert!(CorrelationId::from_string("invalid").is_err());
    /// ```
    pub fn from_string(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(uuid::Uuid::parse_str(s)?))
    }

    /// Returns the underlying UUID.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::CorrelationId;
    ///
    /// let id = CorrelationId::new();
    /// let uuid = id.as_uuid();
    /// println!("UUID: {}", uuid);
    /// ```
    #[must_use]
    pub const fn as_uuid(&self) -> &uuid::Uuid {
        &self.0
    }

    /// Converts the correlation ID to a string.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::CorrelationId;
    ///
    /// let id = CorrelationId::new();
    /// let s = id.to_string();
    /// assert_eq!(s.len(), 36); // UUID format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
    /// ```
    #[must_use]
    // Note: Display trait is implemented, use that instead of this method
    #[allow(clippy::inherent_to_string_shadow_display)]
    pub fn to_string(&self) -> String {
        self.0.to_string()
    }

    /// Returns the correlation ID as bytes.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::CorrelationId;
    ///
    /// let id = CorrelationId::new();
    /// let bytes = id.as_bytes();
    /// assert_eq!(bytes.len(), 16);
    /// ```
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
}

impl Default for CorrelationId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for CorrelationId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_string(s)
    }
}

impl From<uuid::Uuid> for CorrelationId {
    fn from(uuid: uuid::Uuid) -> Self {
        Self::from_uuid(uuid)
    }
}

impl From<CorrelationId> for uuid::Uuid {
    fn from(id: CorrelationId) -> Self {
        id.0
    }
}

/// Context for tracking correlation across async operations.
///
/// This provides a way to store and retrieve correlation IDs in async contexts,
/// allowing them to be propagated through async operations without explicit
/// parameter passing.
///
/// # Examples
///
/// ```rust
/// use bdrpc::observability::{CorrelationId, CorrelationContext};
///
/// # async fn example() {
/// // Execute code with a correlation ID
/// let id = CorrelationId::new();
/// let result = CorrelationContext::with(id, async {
///     // Correlation ID is available here
///     let current = CorrelationContext::get().await;
///     assert_eq!(current, Some(id));
///     42
/// }).await;
/// assert_eq!(result, 42);
/// # }
/// ```
pub struct CorrelationContext;

impl CorrelationContext {
    /// Gets the correlation ID for the current async task.
    ///
    /// Returns `None` if no correlation ID has been set.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::{CorrelationId, CorrelationContext};
    ///
    /// # async fn example() {
    /// if let Some(id) = CorrelationContext::get().await {
    ///     println!("Current correlation ID: {}", id);
    /// }
    /// # }
    /// ```
    pub async fn get() -> Option<CorrelationId> {
        CORRELATION_ID.try_with(|id| *id).ok().flatten()
    }

    /// Executes a future with a specific correlation ID.
    ///
    /// The correlation ID is set for the duration of the future and
    /// automatically cleared when the future completes.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::observability::{CorrelationId, CorrelationContext};
    ///
    /// # async fn example() {
    /// let id = CorrelationId::new();
    /// let result = CorrelationContext::with(id, async {
    ///     // Correlation ID is available here
    ///     let current = CorrelationContext::get().await;
    ///     assert_eq!(current, Some(id));
    ///     42
    /// }).await;
    /// assert_eq!(result, 42);
    /// # }
    /// ```
    pub async fn with<F, T>(id: CorrelationId, future: F) -> T
    where
        F: std::future::Future<Output = T>,
    {
        CORRELATION_ID.scope(Some(id), future).await
    }
}

tokio::task_local! {
    static CORRELATION_ID: Option<CorrelationId>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_correlation_id_new() {
        let id1 = CorrelationId::new();
        let id2 = CorrelationId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_correlation_id_from_uuid() {
        let uuid = uuid::Uuid::new_v4();
        let id = CorrelationId::from_uuid(uuid);
        assert_eq!(id.as_uuid(), &uuid);
    }

    #[test]
    fn test_correlation_id_from_string() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let id = CorrelationId::from_string(uuid_str).unwrap();
        assert_eq!(id.to_string(), uuid_str);
    }

    #[test]
    fn test_correlation_id_from_string_invalid() {
        assert!(CorrelationId::from_string("invalid").is_err());
    }

    #[test]
    fn test_correlation_id_display() {
        let id = CorrelationId::new();
        let s = format!("{}", id);
        assert_eq!(s.len(), 36); // UUID format
    }

    #[test]
    fn test_correlation_id_from_str() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let id: CorrelationId = uuid_str.parse().unwrap();
        assert_eq!(id.to_string(), uuid_str);
    }

    #[test]
    fn test_correlation_id_as_bytes() {
        let id = CorrelationId::new();
        let bytes = id.as_bytes();
        assert_eq!(bytes.len(), 16);
    }

    #[test]
    fn test_correlation_id_default() {
        let id1 = CorrelationId::default();
        let id2 = CorrelationId::default();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_correlation_id_clone() {
        let id1 = CorrelationId::new();
        let id2 = id1;
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_correlation_id_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        let id = CorrelationId::new();
        set.insert(id);
        assert!(set.contains(&id));
    }

    #[tokio::test]
    async fn test_correlation_context_get_none() {
        // Without setting a correlation ID, get should return None
        let retrieved = CorrelationContext::get().await;
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_correlation_context_with() {
        let id = CorrelationId::new();
        let result = CorrelationContext::with(id, async {
            let current = CorrelationContext::get().await;
            assert_eq!(current, Some(id));
            42
        })
        .await;
        assert_eq!(result, 42);

        // After the scope, correlation ID should not be available
        let after = CorrelationContext::get().await;
        assert!(after.is_none());
    }

    #[tokio::test]
    async fn test_correlation_context_nested() {
        let id1 = CorrelationId::new();
        let id2 = CorrelationId::new();

        CorrelationContext::with(id1, async {
            let current1 = CorrelationContext::get().await;
            assert_eq!(current1, Some(id1));

            // Nested scope with different ID
            CorrelationContext::with(id2, async {
                let current2 = CorrelationContext::get().await;
                assert_eq!(current2, Some(id2));
            })
            .await;

            // Back to outer scope
            let current1_again = CorrelationContext::get().await;
            assert_eq!(current1_again, Some(id1));
        })
        .await;
    }

    #[tokio::test]
    async fn test_correlation_context_isolation() {
        let id1 = CorrelationId::new();
        let id2 = CorrelationId::new();

        let handle1 = tokio::spawn(async move {
            CorrelationContext::with(id1, async {
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                CorrelationContext::get().await
            })
            .await
        });

        let handle2 = tokio::spawn(async move {
            CorrelationContext::with(id2, async {
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                CorrelationContext::get().await
            })
            .await
        });

        let result1 = handle1.await.unwrap();
        let result2 = handle2.await.unwrap();

        assert_eq!(result1, Some(id1));
        assert_eq!(result2, Some(id2));
    }
}

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

//! Postcard serializer implementation.
//!
//! This module provides a serializer based on the postcard format, which is
//! a compact, no_std-friendly binary serialization format for Rust types using serde.

use crate::serialization::{DeserializationError, SerializationError, Serializer};

/// Postcard serializer.
///
/// `PostcardSerializer` provides compact binary serialization using the postcard
/// format. This is the recommended default serializer for production use due to
/// its excellent balance of performance, size, and no_std compatibility.
///
/// # Features
///
/// - Very compact binary format
/// - Fast serialization and deserialization
/// - no_std compatible
/// - Deterministic output
/// - Good cross-platform compatibility
///
/// # Examples
///
/// ## Basic usage
///
/// ```rust
/// use bdrpc::serialization::{PostcardSerializer, Serializer};
/// use serde::{Serialize, Deserialize};
///
/// #[derive(Serialize, Deserialize, Debug, PartialEq)]
/// struct Message {
///     id: u32,
///     text: String,
/// }
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let serializer = PostcardSerializer::default();
/// let message = Message { id: 42, text: "Hello".to_string() };
///
/// let bytes = serializer.serialize(&message)?;
/// let decoded: Message = serializer.deserialize(&bytes)?;
/// assert_eq!(message, decoded);
/// # Ok(())
/// # }
/// ```
///
/// ## With maximum size limit
///
/// ```rust
/// use bdrpc::serialization::{PostcardSerializer, Serializer};
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Data {
///     value: u64,
/// }
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create with 1MB size limit
/// let serializer = PostcardSerializer::new().with_max_size(1024 * 1024);
/// let data = Data { value: 12345 };
/// let bytes = serializer.serialize(&data)?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct PostcardSerializer {
    max_size: Option<usize>,
}

impl PostcardSerializer {
    /// Creates a new postcard serializer with default configuration.
    ///
    /// The default configuration has no size limit.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::serialization::PostcardSerializer;
    ///
    /// let serializer = PostcardSerializer::new();
    /// ```
    pub fn new() -> Self {
        Self { max_size: None }
    }

    /// Sets a maximum size limit for serialization.
    ///
    /// This can help prevent denial-of-service attacks by limiting the maximum
    /// size of serialized data.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::serialization::PostcardSerializer;
    ///
    /// // Limit to 1MB
    /// let serializer = PostcardSerializer::new().with_max_size(1024 * 1024);
    /// ```
    pub fn with_max_size(mut self, max_size: usize) -> Self {
        self.max_size = Some(max_size);
        self
    }

    /// Removes any size limit.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::serialization::PostcardSerializer;
    ///
    /// let serializer = PostcardSerializer::new().with_no_limit();
    /// ```
    pub fn with_no_limit(mut self) -> Self {
        self.max_size = None;
        self
    }
}

impl Default for PostcardSerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl Serializer for PostcardSerializer {
    fn serialize<T>(&self, value: &T) -> Result<Vec<u8>, SerializationError>
    where
        T: serde::Serialize + ?Sized,
    {
        postcard::to_allocvec(value)
            .map_err(|e| SerializationError::with_source("Postcard serialization failed", e))
    }

    fn deserialize<T>(&self, bytes: &[u8]) -> Result<T, DeserializationError>
    where
        T: serde::de::DeserializeOwned,
    {
        // Check size limit if configured
        if let Some(max_size) = self.max_size {
            if bytes.len() > max_size {
                return Err(DeserializationError::new(format!(
                    "Data size {} exceeds maximum allowed size {}",
                    bytes.len(),
                    max_size
                )));
            }
        }

        postcard::from_bytes(bytes)
            .map_err(|e| DeserializationError::with_source("Postcard deserialization failed", e))
    }

    fn name(&self) -> &'static str {
        "postcard"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestMessage {
        id: u32,
        text: String,
        values: Vec<i32>,
    }

    #[test]
    fn test_postcard_basic() {
        let serializer = PostcardSerializer::default();
        let message = TestMessage {
            id: 42,
            text: "Hello, world!".to_string(),
            values: vec![1, 2, 3, 4, 5],
        };

        let bytes = serializer.serialize(&message).unwrap();
        let decoded: TestMessage = serializer.deserialize(&bytes).unwrap();

        assert_eq!(message, decoded);
    }

    #[test]
    fn test_postcard_empty_string() {
        let serializer = PostcardSerializer::default();
        let message = TestMessage {
            id: 0,
            text: String::new(),
            values: vec![],
        };

        let bytes = serializer.serialize(&message).unwrap();
        let decoded: TestMessage = serializer.deserialize(&bytes).unwrap();

        assert_eq!(message, decoded);
    }

    #[test]
    fn test_postcard_large_data() {
        let serializer = PostcardSerializer::default();
        let message = TestMessage {
            id: u32::MAX,
            text: "x".repeat(10000),
            values: (0..1000).collect(),
        };

        let bytes = serializer.serialize(&message).unwrap();
        let decoded: TestMessage = serializer.deserialize(&bytes).unwrap();

        assert_eq!(message, decoded);
    }

    #[test]
    fn test_postcard_invalid_data() {
        let serializer = PostcardSerializer::default();
        let invalid_bytes = vec![0xFF, 0xFF, 0xFF, 0xFF];

        let result: Result<TestMessage, _> = serializer.deserialize(&invalid_bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_postcard_name() {
        let serializer = PostcardSerializer::default();
        assert_eq!(serializer.name(), "postcard");
    }

    #[test]
    fn test_postcard_compact() {
        let serializer = PostcardSerializer::default();
        let message = TestMessage {
            id: 42,
            text: "test".to_string(),
            values: vec![1, 2, 3],
        };

        let bytes = serializer.serialize(&message).unwrap();

        // Postcard should produce compact output
        // This is just a sanity check that it's reasonably small
        assert!(bytes.len() < 100);
    }

    #[test]
    fn test_postcard_with_max_size() {
        // Create a serializer with a very small max size
        let serializer = PostcardSerializer::new().with_max_size(5);
        let message = TestMessage {
            id: 42,
            text: "test".to_string(),
            values: vec![1, 2, 3],
        };

        // Serialize with no limit
        let bytes = PostcardSerializer::new().serialize(&message).unwrap();

        // Deserialization should fail because data exceeds max size
        let result: Result<TestMessage, _> = serializer.deserialize(&bytes);
        assert!(result.is_err());

        // Verify the error message mentions size limit
        if let Err(e) = result {
            assert!(e.to_string().contains("exceeds maximum"));
        }
    }

    #[test]
    fn test_postcard_no_limit() {
        let serializer = PostcardSerializer::new().with_no_limit();
        let message = TestMessage {
            id: 42,
            text: "x".repeat(10000),
            values: (0..1000).collect(),
        };

        let bytes = serializer.serialize(&message).unwrap();
        let decoded: TestMessage = serializer.deserialize(&bytes).unwrap();

        assert_eq!(message, decoded);
    }

    #[test]
    fn test_postcard_clone() {
        let serializer1 = PostcardSerializer::default();
        let serializer2 = serializer1.clone();

        let value = 42u32;
        let bytes1 = serializer1.serialize(&value).unwrap();
        let bytes2 = serializer2.serialize(&value).unwrap();

        assert_eq!(bytes1, bytes2);
    }

    #[test]
    fn test_postcard_deterministic() {
        let serializer = PostcardSerializer::default();
        let message = TestMessage {
            id: 42,
            text: "test".to_string(),
            values: vec![1, 2, 3],
        };

        // Serialize the same message multiple times
        let bytes1 = serializer.serialize(&message).unwrap();
        let bytes2 = serializer.serialize(&message).unwrap();
        let bytes3 = serializer.serialize(&message).unwrap();

        // All should produce identical output
        assert_eq!(bytes1, bytes2);
        assert_eq!(bytes2, bytes3);
    }
}

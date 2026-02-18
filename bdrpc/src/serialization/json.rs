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

//! JSON serializer implementation.
//!
//! This module provides a serializer based on JSON, which is human-readable
//! and useful for debugging and development.

use crate::serialization::{DeserializationError, SerializationError, Serializer};

/// JSON serializer.
///
/// `JsonSerializer` provides human-readable JSON serialization. This is primarily
/// useful for debugging, development, and situations where human readability is
/// more important than performance or size.
///
/// # Features
///
/// - Human-readable format
/// - Easy to debug and inspect
/// - Cross-language compatibility
/// - Optional pretty-printing
///
/// # Trade-offs
///
/// - Larger output size than binary formats
/// - Slower serialization/deserialization
/// - Limited type support (no binary types without encoding)
///
/// # Examples
///
/// ## Basic usage
///
/// ```rust
/// use bdrpc::serialization::{JsonSerializer, Serializer};
/// use serde::{Serialize, Deserialize};
///
/// #[derive(Serialize, Deserialize, Debug, PartialEq)]
/// struct Message {
///     id: u32,
///     text: String,
/// }
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let serializer = JsonSerializer::default();
/// let message = Message { id: 42, text: "Hello".to_string() };
///
/// let bytes = serializer.serialize(&message)?;
/// println!("JSON: {}", String::from_utf8_lossy(&bytes));
///
/// let decoded: Message = serializer.deserialize(&bytes)?;
/// assert_eq!(message, decoded);
/// # Ok(())
/// # }
/// ```
///
/// ## Pretty-printed JSON
///
/// ```rust
/// use bdrpc::serialization::{JsonSerializer, Serializer};
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Config {
///     host: String,
///     port: u16,
/// }
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let serializer = JsonSerializer::new().with_pretty_print();
/// let config = Config {
///     host: "localhost".to_string(),
///     port: 8080,
/// };
///
/// let bytes = serializer.serialize(&config)?;
/// println!("{}", String::from_utf8_lossy(&bytes));
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, Default)]
pub struct JsonSerializer {
    pretty: bool,
}

impl JsonSerializer {
    /// Creates a new JSON serializer with default configuration.
    ///
    /// The default configuration produces compact JSON without whitespace.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::serialization::JsonSerializer;
    ///
    /// let serializer = JsonSerializer::new();
    /// ```
    pub fn new() -> Self {
        Self { pretty: false }
    }

    /// Configures the serializer to produce pretty-printed JSON.
    ///
    /// Pretty-printed JSON includes indentation and newlines for better
    /// human readability.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::serialization::JsonSerializer;
    ///
    /// let serializer = JsonSerializer::new().with_pretty_print();
    /// ```
    pub fn with_pretty_print(mut self) -> Self {
        self.pretty = true;
        self
    }

    /// Configures the serializer to produce compact JSON.
    ///
    /// This is the default behavior.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::serialization::JsonSerializer;
    ///
    /// let serializer = JsonSerializer::new().with_compact();
    /// ```
    pub fn with_compact(mut self) -> Self {
        self.pretty = false;
        self
    }
}

impl Serializer for JsonSerializer {
    fn serialize<T>(&self, value: &T) -> Result<Vec<u8>, SerializationError>
    where
        T: serde::Serialize + ?Sized,
    {
        if self.pretty {
            serde_json::to_vec_pretty(value).map_err(Into::into)
        } else {
            serde_json::to_vec(value).map_err(Into::into)
        }
    }

    fn deserialize<T>(&self, bytes: &[u8]) -> Result<T, DeserializationError>
    where
        T: serde::de::DeserializeOwned,
    {
        serde_json::from_slice(bytes).map_err(Into::into)
    }

    fn name(&self) -> &'static str {
        "json"
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
    fn test_json_basic() {
        let serializer = JsonSerializer::default();
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
    fn test_json_empty_string() {
        let serializer = JsonSerializer::default();
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
    fn test_json_large_data() {
        let serializer = JsonSerializer::default();
        let message = TestMessage {
            id: u32::MAX,
            text: "x".repeat(1000),
            values: (0..100).collect(),
        };

        let bytes = serializer.serialize(&message).unwrap();
        let decoded: TestMessage = serializer.deserialize(&bytes).unwrap();

        assert_eq!(message, decoded);
    }

    #[test]
    fn test_json_invalid_data() {
        let serializer = JsonSerializer::default();
        let invalid_bytes = b"not valid json {";

        let result: Result<TestMessage, _> = serializer.deserialize(invalid_bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_json_name() {
        let serializer = JsonSerializer::default();
        assert_eq!(serializer.name(), "json");
    }

    #[test]
    fn test_json_pretty_print() {
        let serializer = JsonSerializer::new().with_pretty_print();
        let message = TestMessage {
            id: 42,
            text: "test".to_string(),
            values: vec![1, 2, 3],
        };

        let bytes = serializer.serialize(&message).unwrap();
        let json_str = String::from_utf8(bytes).unwrap();

        // Pretty-printed JSON should contain newlines
        assert!(json_str.contains('\n'));
        assert!(json_str.contains("  ")); // Indentation
    }

    #[test]
    fn test_json_compact() {
        let serializer = JsonSerializer::new().with_compact();
        let message = TestMessage {
            id: 42,
            text: "test".to_string(),
            values: vec![1, 2, 3],
        };

        let bytes = serializer.serialize(&message).unwrap();
        let json_str = String::from_utf8(bytes).unwrap();

        // Compact JSON should not contain unnecessary whitespace
        assert!(!json_str.contains('\n'));
    }

    #[test]
    fn test_json_human_readable() {
        let serializer = JsonSerializer::default();
        let message = TestMessage {
            id: 123,
            text: "readable".to_string(),
            values: vec![1, 2, 3],
        };

        let bytes = serializer.serialize(&message).unwrap();
        let json_str = String::from_utf8(bytes).unwrap();

        // Should be able to read the values in the JSON
        assert!(json_str.contains("123"));
        assert!(json_str.contains("readable"));
    }

    #[test]
    fn test_json_clone() {
        let serializer1 = JsonSerializer::default();
        let serializer2 = serializer1.clone();

        let value = 42u32;
        let bytes1 = serializer1.serialize(&value).unwrap();
        let bytes2 = serializer2.serialize(&value).unwrap();

        assert_eq!(bytes1, bytes2);
    }

    #[test]
    fn test_json_special_characters() {
        let serializer = JsonSerializer::default();
        let message = TestMessage {
            id: 1,
            text: "Hello \"world\" with\nnewlines\tand\ttabs".to_string(),
            values: vec![],
        };

        let bytes = serializer.serialize(&message).unwrap();
        let decoded: TestMessage = serializer.deserialize(&bytes).unwrap();

        assert_eq!(message, decoded);
    }

    #[test]
    fn test_json_unicode() {
        let serializer = JsonSerializer::default();
        let message = TestMessage {
            id: 1,
            text: "Hello 世界 🌍".to_string(),
            values: vec![],
        };

        let bytes = serializer.serialize(&message).unwrap();
        let decoded: TestMessage = serializer.deserialize(&bytes).unwrap();

        assert_eq!(message, decoded);
    }
}

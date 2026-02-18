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

//! Serialization error types.
//!
//! This module defines error types for serialization and deserialization operations.

use std::fmt;

/// Error that occurs during serialization.
///
/// This error indicates that a value could not be serialized to bytes.
/// Common causes include:
/// - Invalid types structure
/// - Unsupported types
/// - Buffer allocation failures
///
/// # Examples
///
/// ```rust
/// use bdrpc::serialization::{SerializationError, PostcardSerializer, Serializer};
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Message {
///     types: String,
/// }
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let serializer = PostcardSerializer::default();
/// let message = Message { types: "test".to_string() };
/// let bytes = serializer.serialize(&message)?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct SerializationError {
    /// The underlying error message
    message: String,
    /// Optional source error
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl SerializationError {
    /// Creates a new serialization error with a message.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::serialization::SerializationError;
    ///
    /// let error = SerializationError::new("Failed to serialize value");
    /// ```
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }

    /// Creates a new serialization error with a message and source.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::serialization::SerializationError;
    /// use std::io;
    ///
    /// let io_error = io::Error::new(io::ErrorKind::Other, "test");
    /// let error = SerializationError::with_source("Failed to write", io_error);
    /// ```
    pub fn with_source(
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }
}

impl fmt::Display for SerializationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Serialization error: {}", self.message)?;
        if let Some(source) = &self.source {
            write!(f, " (caused by: {})", source)?;
        }
        Ok(())
    }
}

impl std::error::Error for SerializationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}

/// Error that occurs during deserialization.
///
/// This error indicates that bytes could not be deserialized into a value.
/// Common causes include:
/// - Corrupted types
/// - Version mismatch
/// - Invalid format
/// - Incomplete types
///
/// # Examples
///
/// ```rust
/// use bdrpc::serialization::{DeserializationError, PostcardSerializer, Serializer};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Message {
///     types: String,
/// }
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let serializer = PostcardSerializer::default();
/// let invalid_bytes = vec![0xFF, 0xFF, 0xFF];
/// let result: Result<Message, _> = serializer.deserialize(&invalid_bytes);
/// assert!(result.is_err());
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct DeserializationError {
    /// The underlying error message
    message: String,
    /// Optional source error
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl DeserializationError {
    /// Creates a new deserialization error with a message.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::serialization::DeserializationError;
    ///
    /// let error = DeserializationError::new("Invalid types format");
    /// ```
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            source: None,
        }
    }

    /// Creates a new deserialization error with a message and source.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::serialization::DeserializationError;
    /// use std::io;
    ///
    /// let io_error = io::Error::new(io::ErrorKind::UnexpectedEof, "test");
    /// let error = DeserializationError::with_source("Failed to read", io_error);
    /// ```
    pub fn with_source(
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }
}

impl fmt::Display for DeserializationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Deserialization error: {}", self.message)?;
        if let Some(source) = &self.source {
            write!(f, " (caused by: {})", source)?;
        }
        Ok(())
    }
}

impl std::error::Error for DeserializationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}

#[cfg(feature = "postcard")]
impl From<postcard::Error> for SerializationError {
    fn from(err: postcard::Error) -> Self {
        Self::with_source("Postcard serialization failed", err)
    }
}

#[cfg(feature = "postcard")]
impl From<postcard::Error> for DeserializationError {
    fn from(err: postcard::Error) -> Self {
        Self::with_source("Postcard deserialization failed", err)
    }
}

#[cfg(feature = "json")]
impl From<serde_json::Error> for SerializationError {
    fn from(err: serde_json::Error) -> Self {
        Self::with_source("JSON serialization failed", err)
    }
}

#[cfg(feature = "json")]
impl From<serde_json::Error> for DeserializationError {
    fn from(err: serde_json::Error) -> Self {
        Self::with_source("JSON deserialization failed", err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn test_serialization_error_new() {
        let error = SerializationError::new("test error");
        assert_eq!(error.to_string(), "Serialization error: test error");
        assert!(error.source().is_none());
    }

    #[test]
    fn test_serialization_error_with_source() {
        let source = std::io::Error::other("io error");
        let error = SerializationError::with_source("test error", source);
        assert!(error.to_string().contains("test error"));
        assert!(error.source().is_some());
    }

    #[test]
    fn test_deserialization_error_new() {
        let error = DeserializationError::new("test error");
        assert_eq!(error.to_string(), "Deserialization error: test error");
        assert!(error.source().is_none());
    }

    #[test]
    fn test_deserialization_error_with_source() {
        let source = std::io::Error::other("io error");
        let error = DeserializationError::with_source("test error", source);
        assert!(error.to_string().contains("test error"));
        assert!(error.source().is_some());
    }
}

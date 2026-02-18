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

//! Serialization trait definitions.
//!
//! This module defines the core [`Serializer`] trait that all serialization
//! implementations must implement.

use crate::serialization::{DeserializationError, SerializationError};

/// Trait for serializing and deserializing values.
///
/// The `Serializer` trait provides a pluggable abstraction for different
/// serialization formats. Implementations must be thread-safe and support
/// both serialization and deserialization.
///
/// # Thread Safety
///
/// All serializers must be `Send + Sync + 'static` to support concurrent
/// usage across multiple tasks and threads.
///
/// # Examples
///
/// ## Using a serializer
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
/// // Serialize
/// let bytes = serializer.serialize(&message)?;
/// println!("Serialized to {} bytes using {}", bytes.len(), serializer.name());
///
/// // Deserialize
/// let decoded: Message = serializer.deserialize(&bytes)?;
/// assert_eq!(message, decoded);
/// # Ok(())
/// # }
/// ```
///
/// ## Implementing a custom serializer
///
/// ```rust
/// use bdrpc::serialization::{Serializer, SerializationError, DeserializationError};
///
/// struct MySerializer;
///
/// impl Serializer for MySerializer {
///     fn serialize<T>(&self, value: &T) -> Result<Vec<u8>, SerializationError>
///     where
///         T: serde::Serialize + ?Sized,
///     {
///         // Custom serialization logic
///         serde_json::to_vec(value).map_err(Into::into)
///     }
///
///     fn deserialize<T>(&self, bytes: &[u8]) -> Result<T, DeserializationError>
///     where
///         T: serde::de::DeserializeOwned,
///     {
///         // Custom deserialization logic
///         serde_json::from_slice(bytes).map_err(Into::into)
///     }
///
///     fn name(&self) -> &'static str {
///         "my-serializer"
///     }
/// }
/// ```
pub trait Serializer: Send + Sync + 'static {
    /// Serializes a value to bytes.
    ///
    /// This method converts a value into a byte representation that can be
    /// transmitted over the wire or stored.
    ///
    /// # Errors
    ///
    /// Returns a [`SerializationError`] if the value cannot be serialized.
    /// Common causes include:
    /// - Unsupported types
    /// - Invalid types structures
    /// - Memory allocation failures
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::serialization::{PostcardSerializer, Serializer};
    /// use serde::Serialize;
    ///
    /// #[derive(Serialize)]
    /// struct Point { x: i32, y: i32 }
    ///
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let serializer = PostcardSerializer::default();
    /// let point = Point { x: 10, y: 20 };
    /// let bytes = serializer.serialize(&point)?;
    /// # Ok(())
    /// # }
    /// ```
    fn serialize<T>(&self, value: &T) -> Result<Vec<u8>, SerializationError>
    where
        T: serde::Serialize + ?Sized;

    /// Deserializes bytes to a value.
    ///
    /// This method converts a byte representation back into a value.
    ///
    /// # Errors
    ///
    /// Returns a [`DeserializationError`] if the bytes cannot be deserialized.
    /// Common causes include:
    /// - Corrupted types
    /// - Version mismatch
    /// - Invalid format
    /// - Incomplete types
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::serialization::{PostcardSerializer, Serializer};
    /// use serde::{Serialize, Deserialize};
    ///
    /// #[derive(Serialize, Deserialize, Debug, PartialEq)]
    /// struct Point { x: i32, y: i32 }
    ///
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let serializer = PostcardSerializer::default();
    /// let point = Point { x: 10, y: 20 };
    /// let bytes = serializer.serialize(&point)?;
    /// let decoded: Point = serializer.deserialize(&bytes)?;
    /// assert_eq!(point, decoded);
    /// # Ok(())
    /// # }
    /// ```
    fn deserialize<T>(&self, bytes: &[u8]) -> Result<T, DeserializationError>
    where
        T: serde::de::DeserializeOwned;

    /// Returns the name of this serializer.
    ///
    /// The name is used for protocol negotiation during connection handshake.
    /// It should be a unique, stable identifier for the serialization format.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::serialization::{PostcardSerializer, Serializer};
    ///
    /// let serializer = PostcardSerializer::default();
    /// assert_eq!(serializer.name(), "postcard");
    /// ```
    fn name(&self) -> &'static str;
}

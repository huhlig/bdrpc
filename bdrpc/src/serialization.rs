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

//! Serialization layer for BDRPC.
//!
//! This module provides a pluggable serialization system with multiple backends,
//! efficient framing, and buffer pooling for optimal performance.
//!
//! # Overview
//!
//! The serialization layer consists of several key components:
//!
//! - **[`Serializer`] trait**: Pluggable abstraction for different formats
//! - **Serialization backends**: Postcard (default), JSON, Rkyv (planned)
//! - **[`framing`] module**: Length-prefixed message framing
//! - **[`buffer_pool`] module**: Efficient buffer reuse
//! - **Error types**: [`SerializationError`] and [`DeserializationError`]
//!
//! # Serialization Backends
//!
//! BDRPC supports multiple serialization formats, each with different trade-offs:
//!
//! ## Postcard (Recommended)
//!
//! [`PostcardSerializer`] is the recommended default for production use:
//! - Very compact binary format
//! - Fast serialization and deserialization
//! - no_std compatible
//! - Deterministic output
//! - Good cross-platform compatibility
//!
//! ## JSON
//!
//! [`JsonSerializer`] is useful for debugging and development:
//! - Human-readable format
//! - Easy to inspect and debug
//! - Cross-language compatibility
//! - Larger output size
//! - Slower than binary formats
//!
//! ## Rkyv (Planned)
//!
//! [`RkyvSerializer`] will provide zero-copy deserialization:
//! - Highest performance option
//! - Zero-copy deserialization
//! - Requires Archive trait implementation
//! - Currently a placeholder
//!
//! # Message Framing
//!
//! The [`framing`] module provides length-prefixed message framing for reliable
//! message boundaries over stream-based transports:
//!
//! ```text
//! +------------------+----------------------+
//! | Length (4 bytes) | Payload (N bytes)    |
//! +------------------+----------------------+
//! ```
//!
//! - **Length**: u32 in big-endian format
//! - **Payload**: Serialized message types
//! - **Max size**: 16 MB (configurable)
//!
//! # Buffer Pooling
//!
//! The [`buffer_pool`] module provides efficient buffer reuse to reduce allocation
//! overhead in high-throughput scenarios:
//!
//! - Thread-safe buffer pooling
//! - Multiple size classes (256B to 1MB)
//! - Automatic buffer return on drop
//! - Bounded pool size to prevent memory bloat
//!
//! # Examples
//!
//! ## Basic serialization
//!
//! ```rust
//! use bdrpc::serialization::{PostcardSerializer, Serializer};
//! use serde::{Serialize, Deserialize};
//!
//! #[derive(Serialize, Deserialize, Debug, PartialEq)]
//! struct Message {
//!     id: u32,
//!     text: String,
//! }
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let serializer = PostcardSerializer::default();
//! let message = Message { id: 42, text: "Hello".to_string() };
//!
//! // Serialize
//! let bytes = serializer.serialize(&message)?;
//!
//! // Deserialize
//! let decoded: Message = serializer.deserialize(&bytes)?;
//! assert_eq!(message, decoded);
//! # Ok(())
//! # }
//! ```
//!
//! ## Framed messages over a transport
//!
//! ```rust
//! use bdrpc::serialization::{PostcardSerializer, Serializer};
//! use bdrpc::serialization::framing::{write_message, read_message};
//! use serde::{Serialize, Deserialize};
//!
//! #[derive(Serialize, Deserialize, Debug, PartialEq)]
//! struct Request {
//!     method: String,
//!     params: Vec<i32>,
//! }
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let serializer = PostcardSerializer::default();
//! let request = Request {
//!     method: "calculate".to_string(),
//!     params: vec![1, 2, 3],
//! };
//!
//! // Write framed message
//! let mut buffer = Vec::new();
//! write_message(&mut buffer, &serializer, &request).await?;
//!
//! // Read framed message
//! let mut reader = &buffer[..];
//! let decoded: Request = read_message(&mut reader, &serializer).await?;
//! assert_eq!(request, decoded);
//! # Ok(())
//! # }
//! ```
//!
//! ## Using buffer pool for efficiency
//!
//! ```rust
//! use bdrpc::serialization::buffer_pool::BufferPool;
//!
//! # fn example() {
//! // Get a buffer from the pool
//! let mut buffer = BufferPool::get(1024);
//!
//! // Use the buffer
//! buffer.extend_from_slice(b"Hello, world!");
//!
//! // Buffer is automatically returned to pool when dropped
//! drop(buffer);
//!
//! // Next allocation may reuse the buffer
//! let buffer2 = BufferPool::get(1024);
//! # }
//! ```
//!
//! ## Comparing serializers
//!
//! ```rust
//! use bdrpc::serialization::{PostcardSerializer, JsonSerializer, Serializer};
//! use serde::{Serialize, Deserialize};
//!
//! #[derive(Serialize, Deserialize, Debug, PartialEq)]
//! struct Data {
//!     value: u64,
//!     name: String,
//! }
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let types = Data {
//!     value: 12345,
//!     name: "test".to_string(),
//! };
//!
//! // Postcard: compact binary
//! let postcard = PostcardSerializer::default();
//! let postcard_bytes = postcard.serialize(&types)?;
//! println!("Postcard: {} bytes", postcard_bytes.len());
//!
//! // JSON: human-readable
//! let json = JsonSerializer::default();
//! let json_bytes = json.serialize(&types)?;
//! println!("JSON: {} bytes", json_bytes.len());
//! println!("JSON: {}", String::from_utf8_lossy(&json_bytes));
//! # Ok(())
//! # }
//! ```
//!
//! ## Custom serializer implementation
//!
//! ```rust
//! use bdrpc::serialization::{Serializer, SerializationError, DeserializationError};
//!
//! struct MySerializer;
//!
//! impl Serializer for MySerializer {
//!     fn serialize<T>(&self, value: &T) -> Result<Vec<u8>, SerializationError>
//!     where
//!         T: serde::Serialize + ?Sized,
//!     {
//!         // Custom serialization logic
//!         serde_json::to_vec(value).map_err(Into::into)
//!     }
//!
//!     fn deserialize<T>(&self, bytes: &[u8]) -> Result<T, DeserializationError>
//!     where
//!         T: serde::de::DeserializeOwned,
//!     {
//!         // Custom deserialization logic
//!         serde_json::from_slice(bytes).map_err(Into::into)
//!     }
//!
//!     fn name(&self) -> &'static str {
//!         "my-serializer"
//!     }
//! }
//! ```
//!
//! # Performance Considerations
//!
//! ## Serializer Selection
//!
//! - **Postcard**: Best overall performance and size
//! - **JSON**: Use only for debugging or when human readability is required
//! - **Rkyv**: Future option for maximum performance with zero-copy
//!
//! ## Buffer Pooling
//!
//! The buffer pool significantly reduces allocation overhead:
//! - Reuses buffers across multiple operations
//! - Organizes buffers by size class for efficiency
//! - Thread-safe with minimal contention
//! - Automatically integrated into framing layer
//!
//! ## Framing Overhead
//!
//! - 4-byte length prefix per message
//! - Negligible for messages >100 bytes
//! - Consider batching for very small messages
//!
//! # Error Handling
//!
//! The serialization layer provides two error types:
//!
//! - [`SerializationError`]: Errors during serialization
//! - [`DeserializationError`]: Errors during deserialization
//!
//! Both errors support error chaining and provide detailed context:
//!
//! ```rust
//! use bdrpc::serialization::{PostcardSerializer, Serializer};
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct Message {
//!     types: String,
//! }
//!
//! # fn example() {
//! let serializer = PostcardSerializer::default();
//! let invalid_bytes = vec![0xFF, 0xFF, 0xFF];
//!
//! match serializer.deserialize::<Message>(&invalid_bytes) {
//!     Ok(_) => println!("Success"),
//!     Err(e) => {
//!         println!("Deserialization failed: {}", e);
//!         if let Some(source) = std::error::Error::source(&e) {
//!             println!("Caused by: {}", source);
//!         }
//!     }
//! }
//! # }
//! ```
//!
//! # Feature Flags
//!
//! Serialization backends are controlled by feature flags:
//!
//! - `postcard` (default): Enable Postcard serializer
//! - `json`: Enable JSON serializer
//! - `rkyv`: Enable Rkyv serializer (planned)
//!
//! # Thread Safety
//!
//! All serializers implement `Send + Sync + 'static` and can be safely shared
//! across threads. The buffer pool is also fully thread-safe.

pub mod buffer_pool;
mod error;
pub mod framing;
mod traits;

#[cfg(feature = "json")]
mod json;
#[cfg(feature = "postcard")]
mod postcard;

#[cfg(feature = "rkyv")]
mod rkyv_impl;

pub use buffer_pool::{BufferPool, PooledBuffer};
pub use error::{DeserializationError, SerializationError};
pub use traits::Serializer;

#[cfg(feature = "json")]
pub use self::json::JsonSerializer;
#[cfg(feature = "postcard")]
pub use self::postcard::PostcardSerializer;

#[cfg(feature = "rkyv")]
pub use self::rkyv_impl::RkyvSerializer;

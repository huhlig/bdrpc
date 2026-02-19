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

//! Message framing for BDRPC.
//!
//! This module provides length-prefixed message framing for sending and receiving
//! serialized messages over transports. Each message is prefixed with a 4-byte
//! (u32) length header in big-endian format, followed by the message payload.
//!
//! # Protocol
//!
//! ```text
//! +----------------+------------------+
//! | Length (4 bytes) | Payload (N bytes) |
//! +----------------+------------------+
//! ```
//!
//! - **Length**: u32 in big-endian format, indicates payload size in bytes
//! - **Payload**: The serialized message types
//!
//! # Examples
//!
//! ## Writing framed messages
//!
//! ```rust
//! use bdrpc::serialization::framing::write_frame;
//! use tokio::io::AsyncWriteExt;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let mut buffer = Vec::new();
//! let message = b"Hello, world!";
//!
//! write_frame(&mut buffer, message).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Reading framed messages
//!
//! ```rust
//! use bdrpc::serialization::framing::read_frame;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let types = {
//! #     let mut buf = Vec::new();
//! #     buf.extend_from_slice(&5u32.to_be_bytes());
//! #     buf.extend_from_slice(b"Hello");
//! #     buf
//! # };
//! let mut reader = &types[..];
//! let message = read_frame(&mut reader).await?;
//! assert_eq!(message, b"Hello");
//! # Ok(())
//! # }
//! ```

use crate::channel::ChannelId;
use crate::serialization::{BufferPool, DeserializationError, SerializationError};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Maximum frame size (16 MB).
///
/// This limit prevents denial-of-service attacks by limiting the maximum
/// size of a single message. Adjust this value based on your application's needs.
pub const MAX_FRAME_SIZE: u32 = 16 * 1024 * 1024;

/// Size of the frame length header in bytes (4 bytes for length).
pub const FRAME_HEADER_SIZE: usize = 4;

/// Size of the channel ID in bytes (8 bytes for u64).
pub const CHANNEL_ID_SIZE: usize = 8;

/// Total frame header size (channel ID + length).
pub const TOTAL_FRAME_HEADER_SIZE: usize = CHANNEL_ID_SIZE + FRAME_HEADER_SIZE;

/// Writes a length-prefixed frame to an async writer.
///
/// The frame consists of a 4-byte big-endian length prefix followed by the payload.
///
/// # Errors
///
/// Returns a [`SerializationError`] if:
/// - The payload exceeds [`MAX_FRAME_SIZE`]
/// - Writing to the writer fails
///
/// # Examples
///
/// ```rust
/// use bdrpc::serialization::framing::write_frame;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mut buffer = Vec::new();
/// let message = b"Hello, world!";
///
/// write_frame(&mut buffer, message).await?;
///
/// // Verify the frame format
/// assert_eq!(&buffer[0..4], &13u32.to_be_bytes()); // Length prefix
/// assert_eq!(&buffer[4..], message); // Payload
/// # Ok(())
/// # }
/// ```
pub async fn write_frame<W>(writer: &mut W, payload: &[u8]) -> Result<(), SerializationError>
where
    W: AsyncWrite + Unpin,
{
    let len = payload.len();
    if len > MAX_FRAME_SIZE as usize {
        return Err(SerializationError::new(format!(
            "Frame size {} exceeds maximum allowed size {}",
            len, MAX_FRAME_SIZE
        )));
    }

    // Write length prefix (big-endian u32)
    let len_bytes = (len as u32).to_be_bytes();
    writer
        .write_all(&len_bytes)
        .await
        .map_err(|e| SerializationError::with_source("Failed to write frame length", e))?;

    // Write payload
    writer
        .write_all(payload)
        .await
        .map_err(|e| SerializationError::with_source("Failed to write frame payload", e))?;

    // Flush to ensure types is sent
    writer
        .flush()
        .await
        .map_err(|e| SerializationError::with_source("Failed to flush frame", e))?;

    Ok(())
}

/// Reads a length-prefixed frame from an async reader.
///
/// This function reads the 4-byte length prefix, validates it, and then reads
/// the payload of that length.
///
/// # Errors
///
/// Returns a [`DeserializationError`] if:
/// - The length prefix indicates a size exceeding [`MAX_FRAME_SIZE`]
/// - Reading from the reader fails
/// - The connection is closed before the full frame is received
///
/// # Examples
///
/// ```rust
/// use bdrpc::serialization::framing::read_frame;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create a framed message
/// let mut types = Vec::new();
/// types.extend_from_slice(&5u32.to_be_bytes()); // Length: 5
/// types.extend_from_slice(b"Hello"); // Payload
///
/// let mut reader = &types[..];
/// let message = read_frame(&mut reader).await?;
/// assert_eq!(message, b"Hello");
/// # Ok(())
/// # }
/// ```
pub async fn read_frame<R>(reader: &mut R) -> Result<Vec<u8>, DeserializationError>
where
    R: AsyncRead + Unpin,
{
    // Read length prefix (big-endian u32)
    let mut len_bytes = [0u8; FRAME_HEADER_SIZE];
    reader
        .read_exact(&mut len_bytes)
        .await
        .map_err(|e| DeserializationError::with_source("Failed to read frame length", e))?;

    let len = u32::from_be_bytes(len_bytes);

    // Validate length
    if len > MAX_FRAME_SIZE {
        return Err(DeserializationError::new(format!(
            "Frame size {} exceeds maximum allowed size {}",
            len, MAX_FRAME_SIZE
        )));
    }

    // Get a buffer from the pool
    let mut payload = BufferPool::get(len as usize);
    payload.resize(len as usize);

    // Read payload
    reader
        .read_exact(&mut payload)
        .await
        .map_err(|e| DeserializationError::with_source("Failed to read frame payload", e))?;

    // Convert PooledBuffer to Vec<u8> without copying (takes ownership)
    Ok(payload.into())
}

/// Writes a serialized and framed message to an async writer.
///
/// This is a convenience function that combines serialization and framing.
///
/// # Errors
///
/// Returns a [`SerializationError`] if serialization or framing fails.
///
/// # Examples
///
/// ```rust
/// use bdrpc::serialization::{PostcardSerializer, Serializer};
/// use bdrpc::serialization::framing::write_message;
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct Message {
///     id: u32,
///     text: String,
/// }
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mut buffer = Vec::new();
/// let serializer = PostcardSerializer::default();
/// let message = Message { id: 42, text: "Hello".to_string() };
///
/// write_message(&mut buffer, &serializer, &message).await?;
/// # Ok(())
/// # }
/// ```
pub async fn write_message<W, S, T>(
    writer: &mut W,
    serializer: &S,
    message: &T,
) -> Result<(), SerializationError>
where
    W: AsyncWrite + Unpin,
    S: crate::serialization::Serializer,
    T: serde::Serialize,
{
    let payload = serializer.serialize(message)?;
    write_frame(writer, &payload).await
}

/// Reads and deserializes a framed message from an async reader.
///
/// This is a convenience function that combines frame reading and deserialization.
///
/// # Errors
///
/// Returns a [`DeserializationError`] if reading or deserialization fails.
///
/// # Examples
///
/// ```rust
/// use bdrpc::serialization::{PostcardSerializer, Serializer};
/// use bdrpc::serialization::framing::{write_message, read_message};
/// use serde::{Serialize, Deserialize};
///
/// #[derive(Serialize, Deserialize, Debug, PartialEq)]
/// struct Message {
///     id: u32,
///     text: String,
/// }
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let serializer = PostcardSerializer::default();
/// let original = Message { id: 42, text: "Hello".to_string() };
///
/// // Write message
/// let mut buffer = Vec::new();
/// write_message(&mut buffer, &serializer, &original).await?;
///
/// // Read message
/// let mut reader = &buffer[..];
/// let decoded: Message = read_message(&mut reader, &serializer).await?;
/// assert_eq!(original, decoded);
/// # Ok(())
/// # }
/// ```
pub async fn read_message<R, S, T>(
    reader: &mut R,
    serializer: &S,
) -> Result<T, DeserializationError>
where
    R: AsyncRead + Unpin,
    S: crate::serialization::Serializer,
    T: serde::de::DeserializeOwned,
{
    let payload = read_frame(reader).await?;
    serializer.deserialize(&payload)
}

/// Writes a frame with channel ID prefix.
///
/// This function writes a complete frame with channel multiplexing support:
/// - Channel ID (8 bytes, big-endian u64)
/// - Length prefix (4 bytes, big-endian u32)
/// - Payload (N bytes)
///
/// # Errors
///
/// Returns a [`SerializationError`] if:
/// - The payload exceeds [`MAX_FRAME_SIZE`]
/// - Writing to the writer fails
///
/// # Examples
///
/// ```rust
/// use bdrpc::channel::ChannelId;
/// use bdrpc::serialization::framing::write_frame_with_channel;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mut buffer = Vec::new();
/// let channel_id = ChannelId::from(42);
/// let message = b"Hello, world!";
///
/// write_frame_with_channel(&mut buffer, channel_id, message).await?;
/// # Ok(())
/// # }
/// ```
pub async fn write_frame_with_channel<W>(
    writer: &mut W,
    channel_id: ChannelId,
    payload: &[u8],
) -> Result<(), SerializationError>
where
    W: AsyncWrite + Unpin,
{
    // Write channel ID (8 bytes, big-endian)
    let channel_id_bytes = channel_id.as_u64().to_be_bytes();
    writer
        .write_all(&channel_id_bytes)
        .await
        .map_err(|e| SerializationError::with_source("Failed to write channel ID", e))?;

    // Write frame (length + payload)
    write_frame(writer, payload).await
}

/// Reads a frame with channel ID prefix.
///
/// This function reads a complete frame with channel multiplexing support:
/// - Channel ID (8 bytes, big-endian u64)
/// - Length prefix (4 bytes, big-endian u32)
/// - Payload (N bytes)
///
/// # Errors
///
/// Returns a [`DeserializationError`] if:
/// - The length prefix indicates a size exceeding [`MAX_FRAME_SIZE`]
/// - Reading from the reader fails
/// - The connection is closed before the full frame is received
///
/// # Examples
///
/// ```rust
/// use bdrpc::channel::ChannelId;
/// use bdrpc::serialization::framing::read_frame_with_channel;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Create a framed message with channel ID
/// let mut data = Vec::new();
/// data.extend_from_slice(&42u64.to_be_bytes()); // Channel ID: 42
/// data.extend_from_slice(&5u32.to_be_bytes());  // Length: 5
/// data.extend_from_slice(b"Hello");             // Payload
///
/// let mut reader = &data[..];
/// let (channel_id, message) = read_frame_with_channel(&mut reader).await?;
/// assert_eq!(channel_id.as_u64(), 42);
/// assert_eq!(message, b"Hello");
/// # Ok(())
/// # }
/// ```
pub async fn read_frame_with_channel<R>(
    reader: &mut R,
) -> Result<(ChannelId, Vec<u8>), DeserializationError>
where
    R: AsyncRead + Unpin,
{
    // Read channel ID (8 bytes, big-endian)
    let mut channel_id_bytes = [0u8; CHANNEL_ID_SIZE];
    reader
        .read_exact(&mut channel_id_bytes)
        .await
        .map_err(|e| DeserializationError::with_source("Failed to read channel ID", e))?;

    let channel_id = ChannelId::from(u64::from_be_bytes(channel_id_bytes));

    // Read frame payload
    let payload = read_frame(reader).await?;

    Ok((channel_id, payload))
}

/// Frame structure for multiplexing channels over a transport.
///
/// Each frame contains a channel ID and a payload, allowing multiple
/// logical channels to share a single physical transport connection.
///
/// # Frame Format
///
/// ```text
/// +------------------+------------------+------------------+
/// | Channel ID (8)   | Length (4)       | Payload (N)      |
/// +------------------+------------------+------------------+
/// ```
///
/// - **Channel ID**: u64 in big-endian format (8 bytes)
/// - **Length**: u32 in big-endian format (4 bytes)
/// - **Payload**: Serialized message (N bytes)
///
/// # Examples
///
/// ```rust
/// use bdrpc::channel::ChannelId;
/// use bdrpc::serialization::framing::Frame;
///
/// let channel_id = ChannelId::from(42);
/// let payload = vec![1, 2, 3, 4, 5];
/// let frame = Frame::new(channel_id, payload);
///
/// assert_eq!(frame.channel_id(), channel_id);
/// assert_eq!(frame.payload(), &[1, 2, 3, 4, 5]);
/// ```
#[derive(Debug, Clone)]
pub struct Frame {
    /// Channel ID this frame belongs to
    channel_id: ChannelId,
    /// Serialized message payload
    payload: Vec<u8>,
}

impl Frame {
    /// Creates a new frame.
    ///
    /// # Arguments
    ///
    /// * `channel_id` - The channel this frame belongs to
    /// * `payload` - The serialized message payload
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::channel::ChannelId;
    /// use bdrpc::serialization::framing::Frame;
    ///
    /// let frame = Frame::new(ChannelId::from(1), vec![1, 2, 3]);
    /// ```
    pub fn new(channel_id: ChannelId, payload: Vec<u8>) -> Self {
        Self {
            channel_id,
            payload,
        }
    }

    /// Gets the channel ID.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::channel::ChannelId;
    /// use bdrpc::serialization::framing::Frame;
    ///
    /// let channel_id = ChannelId::from(42);
    /// let frame = Frame::new(channel_id, vec![]);
    /// assert_eq!(frame.channel_id(), channel_id);
    /// ```
    pub fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    /// Gets the payload.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::channel::ChannelId;
    /// use bdrpc::serialization::framing::Frame;
    ///
    /// let payload = vec![1, 2, 3];
    /// let frame = Frame::new(ChannelId::from(1), payload.clone());
    /// assert_eq!(frame.payload(), &payload[..]);
    /// ```
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Encodes the frame to bytes for transport.
    ///
    /// The format is: [channel_id: 8 bytes][payload_len: 4 bytes][payload: N bytes]
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::channel::ChannelId;
    /// use bdrpc::serialization::framing::Frame;
    ///
    /// let frame = Frame::new(ChannelId::from(42), vec![1, 2, 3]);
    /// let encoded = frame.encode();
    /// assert_eq!(encoded.len(), 8 + 4 + 3); // channel_id + length + payload
    /// ```
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(TOTAL_FRAME_HEADER_SIZE + self.payload.len());

        // Write channel ID (8 bytes, big-endian)
        bytes.extend_from_slice(&self.channel_id.as_u64().to_be_bytes());

        // Write payload length (4 bytes, big-endian)
        bytes.extend_from_slice(&(self.payload.len() as u32).to_be_bytes());

        // Write payload
        bytes.extend_from_slice(&self.payload);

        bytes
    }

    /// Decodes a frame from bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The bytes are too short to contain a valid frame
    /// - The payload length is invalid
    ///
    /// # Examples
    ///
    /// ```rust
    /// use bdrpc::channel::ChannelId;
    /// use bdrpc::serialization::framing::Frame;
    ///
    /// let original = Frame::new(ChannelId::from(42), vec![1, 2, 3]);
    /// let encoded = original.encode();
    /// let decoded = Frame::decode(&encoded).unwrap();
    /// assert_eq!(decoded.channel_id(), original.channel_id());
    /// assert_eq!(decoded.payload(), original.payload());
    /// ```
    pub fn decode(bytes: &[u8]) -> Result<Self, DeserializationError> {
        if bytes.len() < TOTAL_FRAME_HEADER_SIZE {
            return Err(DeserializationError::new(format!(
                "Frame too short: expected at least {} bytes, got {}",
                TOTAL_FRAME_HEADER_SIZE,
                bytes.len()
            )));
        }

        // Read channel ID (8 bytes)
        let channel_id_bytes: [u8; 8] = bytes[0..8]
            .try_into()
            .map_err(|e| DeserializationError::new(format!("Failed to read channel ID: {}", e)))?;
        let channel_id = ChannelId::from(u64::from_be_bytes(channel_id_bytes));

        // Read payload length (4 bytes)
        let len_bytes: [u8; 4] = bytes[8..12]
            .try_into()
            .map_err(|e| DeserializationError::new(format!("Failed to read length: {}", e)))?;
        let payload_len = u32::from_be_bytes(len_bytes) as usize;

        // Validate payload length
        if bytes.len() < TOTAL_FRAME_HEADER_SIZE + payload_len {
            return Err(DeserializationError::new(format!(
                "Incomplete frame: expected {} bytes, got {}",
                TOTAL_FRAME_HEADER_SIZE + payload_len,
                bytes.len()
            )));
        }

        // Read payload
        let payload =
            bytes[TOTAL_FRAME_HEADER_SIZE..TOTAL_FRAME_HEADER_SIZE + payload_len].to_vec();

        Ok(Self {
            channel_id,
            payload,
        })
    }

    /// Writes a frame to an async writer.
    ///
    /// # Errors
    ///
    /// Returns an error if writing fails or the payload exceeds MAX_FRAME_SIZE.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::channel::ChannelId;
    /// use bdrpc::serialization::framing::Frame;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let frame = Frame::new(ChannelId::from(1), vec![1, 2, 3]);
    /// let mut buffer = Vec::new();
    /// frame.write_to(&mut buffer).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn write_to<W>(&self, writer: &mut W) -> Result<(), SerializationError>
    where
        W: AsyncWrite + Unpin,
    {
        if self.payload.len() > MAX_FRAME_SIZE as usize {
            return Err(SerializationError::new(format!(
                "Frame payload size {} exceeds maximum allowed size {}",
                self.payload.len(),
                MAX_FRAME_SIZE
            )));
        }

        let encoded = self.encode();
        writer
            .write_all(&encoded)
            .await
            .map_err(|e| SerializationError::with_source("Failed to write frame", e))?;

        writer
            .flush()
            .await
            .map_err(|e| SerializationError::with_source("Failed to flush frame", e))?;

        Ok(())
    }

    /// Reads a frame from an async reader.
    ///
    /// # Errors
    ///
    /// Returns an error if reading fails or the frame is invalid.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use bdrpc::channel::ChannelId;
    /// use bdrpc::serialization::framing::Frame;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut reader = &b"\x00\x00\x00\x00\x00\x00\x00\x01\x00\x00\x00\x03\x01\x02\x03"[..];
    /// let frame = Frame::read_from(&mut reader).await?;
    /// println!("Received frame for channel {}", frame.channel_id());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn read_from<R>(reader: &mut R) -> Result<Self, DeserializationError>
    where
        R: AsyncRead + Unpin,
    {
        // Read channel ID (8 bytes)
        let mut channel_id_bytes = [0u8; CHANNEL_ID_SIZE];
        reader
            .read_exact(&mut channel_id_bytes)
            .await
            .map_err(|e| DeserializationError::with_source("Failed to read channel ID", e))?;
        let channel_id = ChannelId::from(u64::from_be_bytes(channel_id_bytes));

        // Read payload length (4 bytes)
        let mut len_bytes = [0u8; FRAME_HEADER_SIZE];
        reader
            .read_exact(&mut len_bytes)
            .await
            .map_err(|e| DeserializationError::with_source("Failed to read frame length", e))?;
        let payload_len = u32::from_be_bytes(len_bytes);

        // Validate length
        if payload_len > MAX_FRAME_SIZE {
            return Err(DeserializationError::new(format!(
                "Frame payload size {} exceeds maximum allowed size {}",
                payload_len, MAX_FRAME_SIZE
            )));
        }

        // Read payload
        let mut payload = BufferPool::get(payload_len as usize);
        payload.resize(payload_len as usize);
        reader
            .read_exact(&mut payload)
            .await
            .map_err(|e| DeserializationError::with_source("Failed to read frame payload", e))?;

        Ok(Self {
            channel_id,
            payload: payload.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serialization::PostcardSerializer;
    use serde::{Deserialize, Serialize};

    #[tokio::test]
    async fn test_write_read_frame() {
        let mut buffer = Vec::new();
        let message = b"Hello, world!";

        write_frame(&mut buffer, message).await.unwrap();

        let mut reader = &buffer[..];
        let decoded = read_frame(&mut reader).await.unwrap();

        assert_eq!(decoded, message);
    }

    #[tokio::test]
    async fn test_empty_frame() {
        let mut buffer = Vec::new();
        let message = b"";

        write_frame(&mut buffer, message).await.unwrap();

        let mut reader = &buffer[..];
        let decoded = read_frame(&mut reader).await.unwrap();

        assert_eq!(decoded, message);
    }

    #[tokio::test]
    async fn test_large_frame() {
        let mut buffer = Vec::new();
        let message = vec![0u8; 1024 * 1024]; // 1 MB

        write_frame(&mut buffer, &message).await.unwrap();

        let mut reader = &buffer[..];
        let decoded = read_frame(&mut reader).await.unwrap();

        assert_eq!(decoded, message);
    }

    #[tokio::test]
    async fn test_frame_too_large() {
        let mut buffer = Vec::new();
        let message = vec![0u8; (MAX_FRAME_SIZE + 1) as usize];

        let result = write_frame(&mut buffer, &message).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exceeds maximum"));
    }

    #[tokio::test]
    async fn test_invalid_frame_size() {
        // Create a frame with invalid size
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&(MAX_FRAME_SIZE + 1).to_be_bytes());

        let mut reader = &buffer[..];
        let result = read_frame(&mut reader).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exceeds maximum"));
    }

    #[tokio::test]
    async fn test_incomplete_frame() {
        // Create a frame header but no payload
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&10u32.to_be_bytes());
        buffer.extend_from_slice(b"short"); // Only 5 bytes, expected 10

        let mut reader = &buffer[..];
        let result = read_frame(&mut reader).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_multiple_frames() {
        let mut buffer = Vec::new();
        let messages = vec![
            b"first".as_slice(),
            b"second".as_slice(),
            b"third".as_slice(),
        ];

        // Write multiple frames
        for msg in &messages {
            write_frame(&mut buffer, msg).await.unwrap();
        }

        // Read multiple frames
        let mut reader = &buffer[..];
        for expected in &messages {
            let decoded = read_frame(&mut reader).await.unwrap();
            assert_eq!(&decoded[..], *expected);
        }
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestMessage {
        id: u32,
        text: String,
        values: Vec<i32>,
    }

    #[tokio::test]
    async fn test_write_read_message() {
        let mut buffer = Vec::new();
        let serializer = PostcardSerializer::default();
        let message = TestMessage {
            id: 42,
            text: "Hello, world!".to_string(),
            values: vec![1, 2, 3, 4, 5],
        };

        write_message(&mut buffer, &serializer, &message)
            .await
            .unwrap();

        let mut reader = &buffer[..];
        let decoded: TestMessage = read_message(&mut reader, &serializer).await.unwrap();

        assert_eq!(message, decoded);
    }

    #[tokio::test]
    async fn test_multiple_messages() {
        let mut buffer = Vec::new();
        let serializer = PostcardSerializer::default();
        let messages = vec![
            TestMessage {
                id: 1,
                text: "first".to_string(),
                values: vec![1],
            },
            TestMessage {
                id: 2,
                text: "second".to_string(),
                values: vec![2, 3],
            },
            TestMessage {
                id: 3,
                text: "third".to_string(),
                values: vec![4, 5, 6],
            },
        ];

        // Write multiple messages
        for msg in &messages {
            write_message(&mut buffer, &serializer, msg).await.unwrap();
        }

        // Read multiple messages
        let mut reader = &buffer[..];
        for expected in &messages {
            let decoded: TestMessage = read_message(&mut reader, &serializer).await.unwrap();
            assert_eq!(expected, &decoded);
        }
    }

    #[tokio::test]
    async fn test_frame_header_size() {
        assert_eq!(FRAME_HEADER_SIZE, 4);
    }

    #[tokio::test]
    async fn test_max_frame_size() {
        assert_eq!(MAX_FRAME_SIZE, 16 * 1024 * 1024);
    }
}

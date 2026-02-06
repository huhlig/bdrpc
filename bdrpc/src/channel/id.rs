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

//! Channel identifier types.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

/// A unique identifier for a channel.
///
/// Channel IDs are used to route messages to the correct channel when
/// multiplexing multiple channels over a single transport.
///
/// # Generation
///
/// Channel IDs are automatically generated using an atomic counter to ensure
/// uniqueness within a process. For distributed systems, you may want to
/// include additional context (e.g., endpoint ID) in the channel ID.
///
/// # Example
///
/// ```rust
/// use bdrpc::channel::ChannelId;
///
/// let id1 = ChannelId::new();
/// let id2 = ChannelId::new();
/// assert_ne!(id1, id2);
///
/// // Can also create from a specific value
/// let id3 = ChannelId::from(42);
/// assert_eq!(id3.as_u64(), 42);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ChannelId(u64);

/// Global counter for generating unique channel IDs.
static NEXT_CHANNEL_ID: AtomicU64 = AtomicU64::new(1);

impl ChannelId {
    /// Creates a new unique channel ID.
    ///
    /// This uses an atomic counter to ensure uniqueness within the process.
    /// The counter starts at 1 and increments for each new channel.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::channel::ChannelId;
    ///
    /// let id = ChannelId::new();
    /// println!("Created channel: {}", id);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self(NEXT_CHANNEL_ID.fetch_add(1, Ordering::Relaxed))
    }

    /// Returns the channel ID as a u64.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::channel::ChannelId;
    ///
    /// let id = ChannelId::from(42);
    /// assert_eq!(id.as_u64(), 42);
    /// ```
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// Creates a channel ID from a u64 value.
    ///
    /// This is useful when you need to create a channel ID from a specific
    /// value, such as when deserializing from the network.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::channel::ChannelId;
    ///
    /// let id = ChannelId::from(100);
    /// assert_eq!(id.as_u64(), 100);
    /// ```
    #[must_use]
    pub const fn from_u64(value: u64) -> Self {
        Self(value)
    }
}

impl Default for ChannelId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<u64> for ChannelId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl From<ChannelId> for u64 {
    fn from(id: ChannelId) -> Self {
        id.0
    }
}

impl fmt::Display for ChannelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Channel({})", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_id_uniqueness() {
        let id1 = ChannelId::new();
        let id2 = ChannelId::new();
        let id3 = ChannelId::new();

        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_channel_id_ordering() {
        let id1 = ChannelId::new();
        let id2 = ChannelId::new();

        assert!(id1 < id2);
    }

    #[test]
    fn test_channel_id_from_u64() {
        let id = ChannelId::from(42);
        assert_eq!(id.as_u64(), 42);
    }

    #[test]
    fn test_channel_id_conversion() {
        let value: u64 = 100;
        let id = ChannelId::from(value);
        let back: u64 = id.into();
        assert_eq!(value, back);
    }

    #[test]
    fn test_channel_id_display() {
        let id = ChannelId::from(42);
        assert_eq!(format!("{}", id), "Channel(42)");
    }

    #[test]
    fn test_channel_id_default() {
        let id = ChannelId::default();
        assert!(id.as_u64() > 0);
    }
}

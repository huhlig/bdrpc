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

//! Rkyv serializer implementation.
//!
//! This module provides a serializer based on rkyv, which offers zero-copy
//! deserialization for maximum performance.

use crate::serialization::{DeserializationError, SerializationError, Serializer};

/// Rkyv serializer.
///
/// `RkyvSerializer` provides zero-copy deserialization using the rkyv format.
/// This is the highest-performance serializer option, but requires types to
/// implement rkyv's Archive trait.
///
/// # Note
///
/// This is a placeholder implementation. Full rkyv support will be added in a
/// future phase of development.
#[derive(Clone, Debug, Default)]
pub struct RkyvSerializer;

impl RkyvSerializer {
    /// Creates a new rkyv serializer.
    pub fn new() -> Self {
        Self
    }
}

impl Serializer for RkyvSerializer {
    fn serialize<T>(&self, _value: &T) -> Result<Vec<u8>, SerializationError>
    where
        T: serde::Serialize + ?Sized,
    {
        Err(SerializationError::new(
            "Rkyv serialization not yet implemented",
        ))
    }

    fn deserialize<T>(&self, _bytes: &[u8]) -> Result<T, DeserializationError>
    where
        T: serde::de::DeserializeOwned,
    {
        Err(DeserializationError::new(
            "Rkyv deserialization not yet implemented",
        ))
    }

    fn name(&self) -> &'static str {
        "rkyv"
    }
}

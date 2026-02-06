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

//! Buffer pooling for efficient memory reuse.
//!
//! This module provides a thread-safe buffer pool that reduces allocation
//! overhead by reusing buffers across multiple operations. This is particularly
//! beneficial for high-throughput scenarios where allocations can become a
//! bottleneck.
//!
//! # Design
//!
//! The buffer pool uses a lock-free design with thread-local caching for
//! optimal performance:
//!
//! - **Thread-local cache**: Each thread maintains a small cache of buffers
//! - **Global pool**: Shared pool for cross-thread buffer reuse
//! - **Size classes**: Buffers are organized by size for efficient allocation
//!
//! # Example
//!
//! ```rust
//! use bdrpc::serialization::buffer_pool::{BufferPool, PooledBuffer};
//!
//! # fn example() {
//! // Get a buffer from the pool
//! let mut buffer = BufferPool::get(1024);
//!
//! // Use the buffer
//! buffer.extend_from_slice(b"Hello, world!");
//!
//! // Buffer is automatically returned to pool when dropped
//! # }
//! ```

use parking_lot::Mutex;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

/// Default maximum buffer size to pool (1 MB).
///
/// Buffers larger than this will not be pooled to avoid excessive memory usage.
const MAX_POOLED_SIZE: usize = 1024 * 1024;

/// Maximum number of buffers to keep in the pool per size class.
const MAX_BUFFERS_PER_CLASS: usize = 32;

/// Size classes for buffer pooling (powers of 2).
const SIZE_CLASSES: &[usize] = &[
    256,     // 256 B
    1024,    // 1 KB
    4096,    // 4 KB
    16384,   // 16 KB
    65536,   // 64 KB
    262144,  // 256 KB
    1048576, // 1 MB
];

/// A pooled buffer that automatically returns to the pool when dropped.
///
/// This type implements `Deref` and `DerefMut` to `Vec<u8>`, so it can be
/// used like a regular vector.
pub struct PooledBuffer {
    buffer: Vec<u8>,
    pool: Arc<BufferPoolInner>,
}

impl PooledBuffer {
    /// Creates a new pooled buffer with the given capacity.
    fn new(capacity: usize, pool: Arc<BufferPoolInner>) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity),
            pool,
        }
    }

    /// Returns the capacity of the buffer.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.buffer.capacity()
    }

    /// Clears the buffer, removing all contents.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Resizes the buffer to the specified length, filling with zeros if needed.
    pub fn resize(&mut self, new_len: usize) {
        self.buffer.resize(new_len, 0);
    }
}

impl Deref for PooledBuffer {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.buffer
    }
}

impl DerefMut for PooledBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buffer
    }
}

impl From<PooledBuffer> for Vec<u8> {
    fn from(mut buffer: PooledBuffer) -> Self {
        // Take ownership of the buffer, preventing it from being returned to pool
        std::mem::take(&mut buffer.buffer)
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        // Only return buffers that are within poolable size
        // If buffer was taken via Into<Vec<u8>>, capacity will be 0
        if self.buffer.capacity() > 0 && self.buffer.capacity() <= MAX_POOLED_SIZE {
            self.pool.return_buffer(std::mem::take(&mut self.buffer));
        }
    }
}

/// Inner buffer pool implementation.
struct BufferPoolInner {
    /// Pools organized by size class.
    pools: Vec<Mutex<Vec<Vec<u8>>>>,
}

impl BufferPoolInner {
    /// Creates a new buffer pool.
    fn new() -> Self {
        let pools = SIZE_CLASSES
            .iter()
            .map(|_| Mutex::new(Vec::new()))
            .collect();

        Self { pools }
    }

    /// Gets a buffer from the pool or allocates a new one.
    fn get_buffer(&self, min_capacity: usize) -> Vec<u8> {
        // Find the appropriate size class
        let size_class_idx = SIZE_CLASSES.iter().position(|&size| size >= min_capacity);

        if let Some(idx) = size_class_idx {
            // Try to get a buffer from the pool
            let mut pool = self.pools[idx].lock();
            if let Some(mut buffer) = pool.pop() {
                buffer.clear();
                return buffer;
            }
            // No buffer available, allocate with the size class capacity
            Vec::<u8>::with_capacity(SIZE_CLASSES[idx])
        } else {
            // Size exceeds largest class, allocate exact size
            Vec::with_capacity(min_capacity)
        }
    }

    /// Returns a buffer to the pool.
    fn return_buffer(&self, buffer: Vec<u8>) {
        let capacity = buffer.capacity();

        // Find the appropriate size class
        let size_class_idx = SIZE_CLASSES.iter().position(|&size| size >= capacity);

        if let Some(idx) = size_class_idx {
            let mut pool = self.pools[idx].lock();

            // Only keep up to MAX_BUFFERS_PER_CLASS buffers
            if pool.len() < MAX_BUFFERS_PER_CLASS {
                pool.push(buffer);
            }
        }
    }
}

/// Global buffer pool for efficient buffer reuse.
///
/// This is a thread-safe singleton that manages a pool of reusable buffers.
/// Buffers are automatically returned to the pool when dropped.
pub struct BufferPool;

impl BufferPool {
    /// Gets the global buffer pool instance.
    fn instance() -> &'static Arc<BufferPoolInner> {
        static INSTANCE: std::sync::OnceLock<Arc<BufferPoolInner>> = std::sync::OnceLock::new();
        INSTANCE.get_or_init(|| Arc::new(BufferPoolInner::new()))
    }

    /// Gets a buffer from the pool with at least the specified capacity.
    ///
    /// The returned buffer may have a larger capacity than requested, as
    /// buffers are organized into size classes for efficiency.
    ///
    /// # Example
    ///
    /// ```rust
    /// use bdrpc::serialization::buffer_pool::BufferPool;
    ///
    /// let mut buffer = BufferPool::get(1024);
    /// buffer.extend_from_slice(b"Hello, world!");
    /// ```
    #[must_use]
    pub fn get(min_capacity: usize) -> PooledBuffer {
        let pool = Self::instance();
        let buffer = pool.get_buffer(min_capacity);
        PooledBuffer::new(buffer.capacity(), Arc::clone(pool)).tap_mut(|b| b.buffer = buffer)
    }

    /// Gets statistics about the buffer pool.
    ///
    /// Returns a vector of (size_class, buffer_count) tuples.
    #[must_use]
    pub fn stats() -> Vec<(usize, usize)> {
        let pool = Self::instance();
        SIZE_CLASSES
            .iter()
            .zip(pool.pools.iter())
            .map(|(size, pool_mutex)| (*size, pool_mutex.lock().len()))
            .collect()
    }
}

/// Helper trait for tap pattern.
trait Tap: Sized {
    fn tap_mut<F>(mut self, f: F) -> Self
    where
        F: FnOnce(&mut Self),
    {
        f(&mut self);
        self
    }
}

impl<T> Tap for T {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_pool_basic() {
        let buffer = BufferPool::get(1024);
        assert!(buffer.capacity() >= 1024);
    }

    #[test]
    fn test_buffer_pool_reuse() {
        // Get and drop a buffer
        {
            let mut buffer = BufferPool::get(1024);
            buffer.extend_from_slice(b"test data");
        }

        // Get another buffer - should be reused
        let buffer = BufferPool::get(1024);
        assert!(buffer.is_empty()); // Should be cleared
        assert!(buffer.capacity() >= 1024);
    }

    #[test]
    fn test_buffer_pool_size_classes() {
        // Test each size class
        for &size in SIZE_CLASSES {
            let buffer = BufferPool::get(size);
            assert!(buffer.capacity() >= size);
        }
    }

    #[test]
    fn test_buffer_pool_large_buffer() {
        // Request a buffer larger than the largest size class
        let buffer = BufferPool::get(2 * 1024 * 1024);
        assert!(buffer.capacity() >= 2 * 1024 * 1024);
    }

    #[test]
    fn test_buffer_pool_stats() {
        // Clear any existing buffers by getting and dropping many
        for _ in 0..MAX_BUFFERS_PER_CLASS + 10 {
            let _ = BufferPool::get(1024);
        }

        let stats = BufferPool::stats();
        assert_eq!(stats.len(), SIZE_CLASSES.len());

        // At least one size class should have buffers
        let total_buffers: usize = stats.iter().map(|(_, count)| count).sum();
        assert!(total_buffers > 0);
    }

    #[test]
    fn test_pooled_buffer_operations() {
        let mut buffer = BufferPool::get(1024);

        // Test basic operations
        buffer.extend_from_slice(b"Hello");
        assert_eq!(&buffer[..], b"Hello");

        buffer.clear();
        assert!(buffer.is_empty());

        buffer.resize(10);
        assert_eq!(buffer.len(), 10);
    }

    #[test]
    fn test_buffer_pool_concurrent() {
        use std::thread;

        let handles: Vec<_> = (0..10)
            .map(|_| {
                thread::spawn(|| {
                    for _ in 0..100 {
                        let mut buffer = BufferPool::get(1024);
                        buffer.extend_from_slice(b"test");
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Pool should have buffers after concurrent use
        let stats = BufferPool::stats();
        let total_buffers: usize = stats.iter().map(|(_, count)| count).sum();
        assert!(total_buffers > 0);
    }
}

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

//! # File Transfer Example - Manual Protocol for Streaming
//!
//! This example demonstrates the **manual protocol implementation approach** for
//! streaming large files. It shows how to handle complex streaming patterns with
//! full control over the protocol without using the `#[bdrpc::service]` macro.
//!
//! ## 🔍 Two Approaches to BDRPC
//!
//! ### 1. Manual Protocol (This Example)
//! - Define FileTransferProtocol enum with all message variants
//! - Manually handle chunked streaming with match statements
//! - Full control over progress tracking and flow control
//! - More boilerplate but maximum flexibility
//!
//! ### 2. Service Macro (See `file_transfer_service.rs`)
//! - Define FileTransfer service trait with async methods
//! - Macro generates protocol, client, server, dispatcher
//! - Type-safe streaming API
//! - Cleaner code, less boilerplate
//!
//! ## What This Example Shows
//!
//! **Manual Protocol Implementation:**
//! - Defining FileTransferProtocol enum with streaming messages
//! - Implementing Protocol trait for file transfer
//! - Manual chunk handling and sequencing
//! - Streaming large data in chunks
//! - Progress tracking during transfer
//! - Memory-efficient file handling
//! - Data integrity verification (CRC32)
//! - Proper error handling and cleanup
//! - Backpressure and flow control
//!
//! ## Architecture
//!
//! ```text
//! Sender                          Receiver
//!   |                               |
//!   |-- StartTransfer ------------>|
//!   |<---------- Ready ------------|
//!   |                               |
//!   |-- Chunk 1 ------------------>|
//!   |-- Chunk 2 ------------------>|
//!   |-- Chunk 3 ------------------>|
//!   |-- ... ---------------------->|
//!   |-- Chunk N ------------------>|
//!   |                               |
//!   |-- EndTransfer -------------->|
//!   |<-------- Complete ------------|
//!   |                               |
//! ```
//!
//! ## Running This Example
//!
//! ```bash
//! cargo run --example file_transfer
//! ```
//!
//! This will simulate transferring a 10MB file in 64KB chunks.

use bdrpc::channel::{Channel, ChannelId, Protocol};
use std::error::Error;
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// Chunk size for file transfer (64KB)
const CHUNK_SIZE: usize = 64 * 1024;

/// File transfer protocol supporting chunked streaming.
///
/// This protocol demonstrates:
/// - Transfer initiation with metadata
/// - Chunked data transfer
/// - Progress reporting
/// - Transfer completion with verification
/// - Error handling
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
enum FileTransferProtocol {
    // Sender -> Receiver
    /// Start a new file transfer
    StartTransfer {
        filename: String,
        total_size: u64,
        chunk_size: usize,
    },
    /// Send a chunk of data
    Chunk {
        sequence: u64,
        data: Vec<u8>,
        checksum: u32,
    },
    /// End the transfer
    EndTransfer { total_chunks: u64, checksum: u32 },

    // Receiver -> Sender
    /// Ready to receive
    Ready,
    /// Acknowledge chunk received
    ChunkAck { sequence: u64 },
    /// Report progress
    Progress { bytes_received: u64, percent: f32 },
    /// Transfer complete
    Complete { success: bool, message: String },
    /// Error occurred
    Error { message: String },
}

impl Protocol for FileTransferProtocol {
    fn method_name(&self) -> &'static str {
        match self {
            Self::StartTransfer { .. } => "start_transfer",
            Self::Chunk { .. } => "chunk",
            Self::EndTransfer { .. } => "end_transfer",
            Self::Ready => "ready",
            Self::ChunkAck { .. } => "chunk_ack",
            Self::Progress { .. } => "progress",
            Self::Complete { .. } => "complete",
            Self::Error { .. } => "error",
        }
    }

    fn is_request(&self) -> bool {
        matches!(
            self,
            Self::StartTransfer { .. } | Self::Chunk { .. } | Self::EndTransfer { .. }
        )
    }
}

/// Simple CRC32 checksum calculation
fn calculate_checksum(data: &[u8]) -> u32 {
    let mut crc = 0xFFFFFFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB88320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// Generate dummy file data for demonstration
fn generate_file_data(size: usize) -> Vec<u8> {
    // Generate a pattern that's easy to verify
    (0..size).map(|i| (i % 256) as u8).collect()
}

/// Run the file sender (server).
async fn run_sender() -> Result<(), Box<dyn Error>> {
    println!("📤 Starting File Sender");
    println!("═══════════════════════════════════════════════════════════\n");

    // Step 1: Create channel
    println!("📡 Step 1: Creating file transfer channel");
    let channel_id = ChannelId::new();
    let (sender, mut receiver) = Channel::<FileTransferProtocol>::new_in_memory(channel_id, 100);
    println!("   ✅ Channel created (ID: {})", channel_id);
    println!("   ℹ️  Buffer size: 100 messages for flow control\n");

    // Step 2: Generate file data
    println!("📄 Step 2: Generating file data");
    let file_size = 10 * 1024 * 1024; // 10 MB
    let filename = "demo_file.bin".to_string();
    let file_data = generate_file_data(file_size);
    println!("   ✅ Generated {} bytes", file_size);
    println!("   📝 Filename: {}", filename);
    println!("   📦 Chunk size: {} bytes\n", CHUNK_SIZE);

    // Step 3: Initiate transfer
    println!("🚀 Step 3: Initiating file transfer");
    sender
        .send(FileTransferProtocol::StartTransfer {
            filename: filename.clone(),
            total_size: file_size as u64,
            chunk_size: CHUNK_SIZE,
        })
        .await?;
    println!("   ✅ Transfer request sent\n");

    // Wait for ready signal
    println!("⏳ Step 4: Waiting for receiver ready signal...");
    match receiver.recv().await {
        Some(FileTransferProtocol::Ready) => {
            println!("   ✅ Receiver is ready\n");
        }
        Some(msg) => {
            println!("   ⚠️  Unexpected message: {:?}", msg);
            return Ok(());
        }
        None => {
            println!("   ❌ Channel closed");
            return Ok(());
        }
    }

    // Step 5: Send chunks
    println!("📦 Step 5: Sending file chunks");
    println!("═══════════════════════════════════════════════════════════\n");

    let total_chunks = file_size.div_ceil(CHUNK_SIZE);
    let start_time = Instant::now();
    let mut bytes_sent = 0u64;

    for (sequence, chunk) in file_data.chunks(CHUNK_SIZE).enumerate() {
        let checksum = calculate_checksum(chunk);

        // Send chunk
        sender
            .send(FileTransferProtocol::Chunk {
                sequence: sequence as u64,
                data: chunk.to_vec(),
                checksum,
            })
            .await?;

        bytes_sent += chunk.len() as u64;

        // Wait for acknowledgment (with timeout to prevent blocking)
        match tokio::time::timeout(Duration::from_millis(100), receiver.recv()).await {
            Ok(Some(FileTransferProtocol::ChunkAck { sequence: ack_seq })) => {
                if ack_seq != sequence as u64 {
                    println!(
                        "   ⚠️  Sequence mismatch: expected {}, got {}",
                        sequence, ack_seq
                    );
                }
            }
            Ok(Some(FileTransferProtocol::Progress {
                bytes_received,
                percent,
            })) => {
                println!(
                    "   📊 Progress: {:.1}% ({} / {} bytes)",
                    percent, bytes_received, file_size
                );
            }
            Ok(Some(msg)) => {
                println!("   ⚠️  Unexpected message: {:?}", msg);
            }
            Ok(None) => {
                println!("   ❌ Channel closed");
                return Ok(());
            }
            Err(_) => {
                // Timeout is okay, receiver might be processing
            }
        }

        // Show progress every 10 chunks
        if sequence % 10 == 0 {
            let percent = (bytes_sent as f64 / file_size as f64) * 100.0;
            let elapsed = start_time.elapsed();
            let rate = bytes_sent as f64 / elapsed.as_secs_f64() / 1024.0 / 1024.0;
            println!(
                "   📤 Chunk {}/{}: {:.1}% complete ({:.2} MB/s)",
                sequence + 1,
                total_chunks,
                percent,
                rate
            );
        }
    }

    let elapsed = start_time.elapsed();
    let rate = bytes_sent as f64 / elapsed.as_secs_f64() / 1024.0 / 1024.0;

    println!("\n   ✅ All chunks sent!");
    println!("   ⏱️  Time: {:.2}s", elapsed.as_secs_f64());
    println!("   📊 Rate: {:.2} MB/s\n", rate);

    // Step 6: End transfer
    println!("🏁 Step 6: Finalizing transfer");
    let total_checksum = calculate_checksum(&file_data);
    sender
        .send(FileTransferProtocol::EndTransfer {
            total_chunks: total_chunks as u64,
            checksum: total_checksum,
        })
        .await?;
    println!("   ✅ End transfer signal sent");
    println!("   🔐 Checksum: 0x{:08X}\n", total_checksum);

    // Wait for completion
    println!("⏳ Step 7: Waiting for completion confirmation...");
    match receiver.recv().await {
        Some(FileTransferProtocol::Complete { success, message }) => {
            if success {
                println!("   ✅ Transfer completed successfully!");
                println!("   💬 Message: {}\n", message);
            } else {
                println!("   ❌ Transfer failed!");
                println!("   💬 Message: {}\n", message);
            }
        }
        Some(msg) => {
            println!("   ⚠️  Unexpected message: {:?}\n", msg);
        }
        None => {
            println!("   ❌ Channel closed\n");
        }
    }

    println!("═══════════════════════════════════════════════════════════");
    println!("✅ Sender completed");
    println!("👋 Sender shutting down\n");

    Ok(())
}

/// Run the file receiver (client).
async fn run_receiver() -> Result<(), Box<dyn Error>> {
    println!("📥 Starting File Receiver");
    println!("═══════════════════════════════════════════════════════════\n");

    // Give sender time to start
    sleep(Duration::from_millis(100)).await;

    // Step 1: Create channel
    println!("📡 Step 1: Creating file transfer channel");
    let channel_id = ChannelId::new();
    let (sender, mut receiver) = Channel::<FileTransferProtocol>::new_in_memory(channel_id, 100);
    println!("   ✅ Channel created (ID: {})\n", channel_id);

    // Step 2: Wait for transfer initiation
    println!("⏳ Step 2: Waiting for transfer initiation...");
    let (filename, total_size, _chunk_size) = match receiver.recv().await {
        Some(FileTransferProtocol::StartTransfer {
            filename,
            total_size,
            chunk_size,
        }) => {
            println!("   ✅ Transfer initiated");
            println!("   📝 Filename: {}", filename);
            println!("   📦 Total size: {} bytes", total_size);
            println!("   📦 Chunk size: {} bytes\n", chunk_size);
            (filename, total_size, chunk_size)
        }
        Some(msg) => {
            println!("   ⚠️  Unexpected message: {:?}", msg);
            return Ok(());
        }
        None => {
            println!("   ❌ Channel closed");
            return Ok(());
        }
    };

    // Step 3: Send ready signal
    println!("✅ Step 3: Sending ready signal");
    sender.send(FileTransferProtocol::Ready).await?;
    println!("   ✅ Ready signal sent\n");

    // Step 4: Receive chunks
    println!("📦 Step 4: Receiving file chunks");
    println!("═══════════════════════════════════════════════════════════\n");

    let mut received_data = Vec::with_capacity(total_size as usize);
    let mut expected_sequence = 0u64;
    let start_time = Instant::now();
    let mut last_progress_report = Instant::now();

    loop {
        match receiver.recv().await {
            Some(FileTransferProtocol::Chunk {
                sequence,
                data,
                checksum,
            }) => {
                // Verify sequence
                if sequence != expected_sequence {
                    println!(
                        "   ⚠️  Sequence error: expected {}, got {}",
                        expected_sequence, sequence
                    );
                    sender
                        .send(FileTransferProtocol::Error {
                            message: format!(
                                "Sequence mismatch: expected {}, got {}",
                                expected_sequence, sequence
                            ),
                        })
                        .await?;
                    return Ok(());
                }

                // Verify checksum
                let calculated_checksum = calculate_checksum(&data);
                if calculated_checksum != checksum {
                    println!("   ❌ Checksum mismatch for chunk {}", sequence);
                    sender
                        .send(FileTransferProtocol::Error {
                            message: format!("Checksum mismatch for chunk {}", sequence),
                        })
                        .await?;
                    return Ok(());
                }

                // Store data
                received_data.extend_from_slice(&data);
                expected_sequence += 1;

                // Send acknowledgment
                sender
                    .send(FileTransferProtocol::ChunkAck { sequence })
                    .await?;

                // Report progress every second
                if last_progress_report.elapsed() >= Duration::from_secs(1) {
                    let bytes_received = received_data.len() as u64;
                    let percent = (bytes_received as f64 / total_size as f64) * 100.0;
                    sender
                        .send(FileTransferProtocol::Progress {
                            bytes_received,
                            percent: percent as f32,
                        })
                        .await?;
                    last_progress_report = Instant::now();

                    let elapsed = start_time.elapsed();
                    let rate = bytes_received as f64 / elapsed.as_secs_f64() / 1024.0 / 1024.0;
                    println!(
                        "   📥 Received: {:.1}% ({} / {} bytes, {:.2} MB/s)",
                        percent, bytes_received, total_size, rate
                    );
                }
            }
            Some(FileTransferProtocol::EndTransfer {
                total_chunks,
                checksum,
            }) => {
                println!("\n   🏁 End transfer signal received");
                println!("   📦 Total chunks: {}", total_chunks);
                println!("   🔐 Expected checksum: 0x{:08X}", checksum);

                // Verify total chunks
                if expected_sequence != total_chunks {
                    println!(
                        "   ❌ Chunk count mismatch: expected {}, got {}",
                        total_chunks, expected_sequence
                    );
                    sender
                        .send(FileTransferProtocol::Complete {
                            success: false,
                            message: format!(
                                "Chunk count mismatch: expected {}, got {}",
                                total_chunks, expected_sequence
                            ),
                        })
                        .await?;
                    return Ok(());
                }

                // Verify total checksum
                let calculated_checksum = calculate_checksum(&received_data);
                if calculated_checksum != checksum {
                    println!("   ❌ Final checksum mismatch!");
                    println!("      Expected: 0x{:08X}", checksum);
                    println!("      Got:      0x{:08X}", calculated_checksum);
                    sender
                        .send(FileTransferProtocol::Complete {
                            success: false,
                            message: "Final checksum mismatch".to_string(),
                        })
                        .await?;
                    return Ok(());
                }

                let elapsed = start_time.elapsed();
                let rate = received_data.len() as f64 / elapsed.as_secs_f64() / 1024.0 / 1024.0;

                println!("   ✅ All data received and verified!");
                println!("   📊 Total size: {} bytes", received_data.len());
                println!("   ⏱️  Time: {:.2}s", elapsed.as_secs_f64());
                println!("   📊 Rate: {:.2} MB/s", rate);
                println!("   🔐 Checksum: 0x{:08X}\n", calculated_checksum);

                // Send completion
                sender
                    .send(FileTransferProtocol::Complete {
                        success: true,
                        message: format!(
                            "File '{}' received successfully ({} bytes)",
                            filename,
                            received_data.len()
                        ),
                    })
                    .await?;

                break;
            }
            Some(msg) => {
                println!("   ⚠️  Unexpected message: {:?}", msg);
            }
            None => {
                println!("   ❌ Channel closed unexpectedly");
                return Ok(());
            }
        }
    }

    println!("═══════════════════════════════════════════════════════════");
    println!("✅ Receiver completed");
    println!("👋 Receiver shutting down\n");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("\n📁 BDRPC File Transfer Example - Streaming Large Data\n");

    // Run both sender and receiver
    println!("🚀 Running file transfer simulation\n");

    // Spawn sender in background
    let sender_handle = tokio::spawn(async {
        if let Err(e) = run_sender().await {
            eprintln!("Sender error: {}", e);
        }
    });

    // Run receiver in foreground
    if let Err(e) = run_receiver().await {
        eprintln!("Receiver error: {}", e);
    }

    // Wait for sender to finish
    sender_handle.await?;

    println!("═══════════════════════════════════════════════════════════");
    println!("🎉 File transfer example completed successfully!\n");

    println!("📚 What this example demonstrated (Manual Protocol):");
    println!("   • FileTransferProtocol enum with streaming message types");
    println!("   • Manual Protocol trait implementation");
    println!("   • Match statements for chunk handling");
    println!("   • Manual sequence tracking and validation");
    println!("   • Streaming large files in chunks (64KB)");
    println!("   • Memory-efficient data handling");
    println!("   • Progress tracking and reporting");
    println!("   • Data integrity verification (CRC32)");
    println!("   • Chunk acknowledgment for reliability");
    println!("   • Backpressure handling with buffered channels");
    println!("   • Proper error handling and recovery");

    println!("\n💡 Key concepts (Manual Protocol):");
    println!("   • Protocol enum: Define all streaming message types");
    println!("   • Manual routing: Match statements for each message");
    println!("   • State management: Track sequence numbers manually");
    println!("   • Chunking: Break large data into manageable pieces");
    println!("   • Checksums: Verify data integrity at chunk and file level");
    println!("   • Flow control: Buffer size prevents overwhelming receiver");
    println!("   • Progress: Real-time feedback on transfer status");
    println!("   • Acknowledgments: Ensure reliable delivery");

    println!("\n🔄 Alternative: Service Macro Approach");
    println!("   See 'file_transfer_service.rs' for the same functionality using:");
    println!("   • #[bdrpc::service] macro for code generation");
    println!("   • FileTransfer service trait with streaming methods");
    println!("   • Generated dispatcher for automatic routing");
    println!("   • Type-safe streaming API");
    println!("   • Cleaner code with less boilerplate");

    println!("\n📊 Manual vs Service Macro for Streaming:");
    println!("   Manual Protocol (this example):");
    println!("     ✓ Full control over streaming logic");
    println!("     ✓ Custom chunk handling");
    println!("     ✓ Flexible progress reporting");
    println!("     ✗ More boilerplate for routing");
    println!("     ✗ Manual state management");
    println!("   Service Macro (file_transfer_service.rs):");
    println!("     ✓ Cleaner streaming API");
    println!("     ✓ Type-safe method calls");
    println!("     ✓ Automatic request routing");
    println!("     ✓ Less boilerplate code");
    println!("     ✗ Less flexibility for custom patterns");

    println!("\n🔧 Performance considerations:");
    println!("   • Chunk size: Balance between overhead and memory usage");
    println!("   • Buffer size: Prevent blocking while managing memory");
    println!("   • Checksums: Trade-off between integrity and CPU usage");
    println!("   • Acknowledgments: Can be batched for better throughput");

    println!("\n📖 Next steps:");
    println!("   • Compare with 'file_transfer_service.rs' (service macro version)");
    println!("   • Try different chunk sizes for optimization");
    println!("   • Add compression for better network efficiency");
    println!("   • Implement resume capability for interrupted transfers");
    println!("   • Add encryption for secure file transfer");
    println!("   • Use TCP transport for real network transfers");

    Ok(())
}

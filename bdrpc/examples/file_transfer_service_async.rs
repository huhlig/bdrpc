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

//! # File Transfer Service Example - Using AsyncRead for True Streaming
//!
//! This example demonstrates how to implement true streaming file transfers
//! using AsyncRead, where data is read and sent chunk-by-chunk without
//! buffering the entire file in memory.
//!
//! ## The Challenge
//!
//! The `#[service]` macro doesn't support `impl AsyncRead` parameters directly.
//! However, we can work around this by:
//! 1. Using a chunked protocol at the RPC level
//! 2. Providing client-side helper methods that stream from AsyncRead
//! 3. Server reassembles chunks into complete data
//!
//! This follows the pattern described in ADR-011 section 1.1.
//!
//! ## What This Example Shows
//!
//! - Chunked file transfer protocol using the service macro
//! - Client-side streaming from AsyncRead sources (true streaming!)
//! - Server-side chunk reassembly
//! - Memory-efficient transfers (no full buffering required)
//! - Progress tracking during streaming
//!
//! ## Running This Example
//!
//! ```bash
//! cargo run --example file_transfer_service_async --features serde
//! ```

use bdrpc::channel::{Channel, ChannelId};
use bdrpc::service;
use std::collections::HashMap;
use std::error::Error;
use std::io::Cursor;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::Mutex;
use tokio::time::sleep;

/// Chunk size for file transfer (64KB)
const CHUNK_SIZE: usize = 64 * 1024;

// ============================================================================
// STEP 1: Define a chunked transfer protocol
// ============================================================================

/// File transfer service using a chunked protocol.
///
/// This protocol allows streaming by breaking transfers into chunks.
/// The client can read from AsyncRead and send chunks as they're available.
#[service(direction = "bidirectional")]
#[allow(dead_code)]
trait FileTransfer {
    /// Start a new file transfer session.
    /// Returns a transfer_id to use for subsequent chunks.
    async fn start_transfer(
        &self,
        filename: String,
        total_size: Option<u64>, // None for unknown size (true streaming)
    ) -> Result<String, String>; // Returns transfer_id

    /// Send a chunk of data for an active transfer.
    async fn send_chunk(
        &self,
        transfer_id: String,
        chunk_index: u64,
        data: Vec<u8>,
        is_final: bool,
    ) -> Result<(), String>;

    /// Complete a transfer and get the result.
    async fn complete_transfer(&self, transfer_id: String) -> Result<String, String>;
}

// ============================================================================
// Helper Functions
// ============================================================================

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
    (0..size).map(|i| (i % 256) as u8).collect()
}

// ============================================================================
// STEP 2: Server-side transfer state management
// ============================================================================

/// State for an active transfer on the server
struct TransferState {
    filename: String,
    total_size: Option<u64>,
    chunks: HashMap<u64, Vec<u8>>,
    bytes_received: u64,
    started_at: Instant,
}

impl TransferState {
    fn new(filename: String, total_size: Option<u64>) -> Self {
        Self {
            filename,
            total_size,
            chunks: HashMap::new(),
            bytes_received: 0,
            started_at: Instant::now(),
        }
    }

    fn add_chunk(&mut self, chunk_index: u64, data: Vec<u8>) {
        self.bytes_received += data.len() as u64;
        self.chunks.insert(chunk_index, data);
    }

    fn finalize(self) -> Vec<u8> {
        let mut result = Vec::with_capacity(self.bytes_received as usize);
        let mut indices: Vec<_> = self.chunks.keys().copied().collect();
        indices.sort_unstable();

        for index in indices {
            if let Some(chunk) = self.chunks.get(&index) {
                result.extend_from_slice(chunk);
            }
        }

        result
    }
}

/// Our file transfer service implementation.
struct MyFileTransferService {
    active_transfers: Arc<Mutex<HashMap<String, TransferState>>>,
}

impl MyFileTransferService {
    fn new() -> Self {
        Self {
            active_transfers: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

/// Implement the generated FileTransferServer trait.
#[async_trait::async_trait]
impl FileTransferServer for MyFileTransferService {
    async fn start_transfer(
        &self,
        filename: String,
        total_size: Option<u64>,
    ) -> Result<String, String> {
        let transfer_id = uuid::Uuid::new_v4().to_string();

        println!("   ✅ Transfer started: {}", transfer_id);
        println!("   📝 Filename: {}", filename);
        if let Some(size) = total_size {
            println!("   📦 Total size: {} bytes", size);
        } else {
            println!("   📦 Total size: Unknown (streaming)");
        }

        let state = TransferState::new(filename, total_size);
        self.active_transfers
            .lock()
            .await
            .insert(transfer_id.clone(), state);

        Ok(transfer_id)
    }

    async fn send_chunk(
        &self,
        transfer_id: String,
        chunk_index: u64,
        data: Vec<u8>,
        is_final: bool,
    ) -> Result<(), String> {
        let mut transfers = self.active_transfers.lock().await;
        let state = transfers
            .get_mut(&transfer_id)
            .ok_or_else(|| format!("Transfer not found: {}", transfer_id))?;

        state.add_chunk(chunk_index, data);

        // Report progress occasionally
        if chunk_index % 10 == 0 || is_final {
            let elapsed = state.started_at.elapsed();
            let rate = state.bytes_received as f64 / elapsed.as_secs_f64() / 1024.0 / 1024.0;

            if let Some(total) = state.total_size {
                let percent = (state.bytes_received as f64 / total as f64) * 100.0;
                println!(
                    "   📥 Chunk {}: {:.1}% ({} / {} bytes, {:.2} MB/s)",
                    chunk_index, percent, state.bytes_received, total, rate
                );
            } else {
                println!(
                    "   📥 Chunk {}: {} bytes received ({:.2} MB/s)",
                    chunk_index, state.bytes_received, rate
                );
            }
        }

        Ok(())
    }

    async fn complete_transfer(&self, transfer_id: String) -> Result<String, String> {
        let mut transfers = self.active_transfers.lock().await;
        let state = transfers
            .remove(&transfer_id)
            .ok_or_else(|| format!("Transfer not found: {}", transfer_id))?;

        let elapsed = state.started_at.elapsed();
        let filename = state.filename.clone();
        let data = state.finalize();
        let checksum = calculate_checksum(&data);
        let rate = data.len() as f64 / elapsed.as_secs_f64() / 1024.0 / 1024.0;

        println!("\n   ✅ Transfer complete!");
        println!("   📝 Filename: {}", filename);
        println!("   📊 Total size: {} bytes", data.len());
        println!("   ⏱️  Time: {:.2}s", elapsed.as_secs_f64());
        println!("   📊 Rate: {:.2} MB/s", rate);
        println!("   🔐 Checksum: 0x{:08X}", checksum);

        Ok(format!(
            "File '{}' uploaded successfully ({} bytes)",
            filename,
            data.len()
        ))
    }
}

// ============================================================================
// STEP 3: Client-side streaming helpers
// ============================================================================

/// Extension trait for FileTransferClient to add streaming capabilities.
///
/// This is where the magic happens - we provide methods that accept
/// AsyncRead sources and handle the chunking automatically.
trait FileTransferClientExt {
    /// Upload from an AsyncRead source with true streaming.
    ///
    /// This method reads from the AsyncRead source chunk-by-chunk
    /// and sends each chunk as it's read. No full buffering required!
    async fn upload_from_reader<R>(
        &self,
        filename: String,
        reader: R,
        total_size: Option<u64>,
    ) -> Result<String, Box<dyn Error>>
    where
        R: AsyncRead + Unpin + Send;
}

impl FileTransferClientExt for FileTransferClient {
    async fn upload_from_reader<R>(
        &self,
        filename: String,
        mut reader: R,
        total_size: Option<u64>,
    ) -> Result<String, Box<dyn Error>>
    where
        R: AsyncRead + Unpin + Send,
    {
        println!("📤 Starting streaming upload");
        if let Some(size) = total_size {
            println!("   📦 Total size: {} bytes", size);
        } else {
            println!("   📦 Total size: Unknown (true streaming)");
        }

        // Start the transfer
        let transfer_id = match self.start_transfer(filename.clone(), total_size).await {
            Ok(Ok(id)) => id,
            Ok(Err(e)) => return Err(e.into()),
            Err(e) => return Err(e.into()),
        };

        println!("   🆔 Transfer ID: {}\n", transfer_id);

        let start_time = Instant::now();
        let mut chunk_index = 0u64;
        let mut bytes_sent = 0u64;
        let mut buffer = vec![0u8; CHUNK_SIZE];

        // Read and send chunks from the AsyncRead source
        // This is TRUE STREAMING - we never buffer the entire file!
        loop {
            let bytes_read = reader.read(&mut buffer).await?;

            if bytes_read == 0 {
                break; // EOF
            }

            let chunk = &buffer[..bytes_read];
            bytes_sent += bytes_read as u64;

            // Send the chunk
            let is_final = false; // We'll send a final empty chunk after EOF
            match self
                .send_chunk(transfer_id.clone(), chunk_index, chunk.to_vec(), is_final)
                .await
            {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return Err(e.into()),
                Err(e) => return Err(e.into()),
            }

            // Show progress every 10 chunks
            if chunk_index % 10 == 0 {
                let elapsed = start_time.elapsed();
                let rate = bytes_sent as f64 / elapsed.as_secs_f64() / 1024.0 / 1024.0;

                if let Some(total) = total_size {
                    let percent = (bytes_sent as f64 / total as f64) * 100.0;
                    println!(
                        "   📤 Chunk {}: {:.1}% ({:.2} MB/s)",
                        chunk_index, percent, rate
                    );
                } else {
                    println!(
                        "   📤 Chunk {}: {} bytes sent ({:.2} MB/s)",
                        chunk_index, bytes_sent, rate
                    );
                }
            }

            chunk_index += 1;
        }

        // Send final chunk marker
        match self
            .send_chunk(transfer_id.clone(), chunk_index, Vec::new(), true)
            .await
        {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e.into()),
            Err(e) => return Err(e.into()),
        }

        let elapsed = start_time.elapsed();
        let rate = bytes_sent as f64 / elapsed.as_secs_f64() / 1024.0 / 1024.0;

        println!("\n   ✅ All chunks sent!");
        println!("   📊 Total chunks: {}", chunk_index);
        println!("   📊 Total bytes: {}", bytes_sent);
        println!("   ⏱️  Time: {:.2}s", elapsed.as_secs_f64());
        println!("   📊 Rate: {:.2} MB/s\n", rate);

        // Complete the transfer
        match self.complete_transfer(transfer_id).await {
            Ok(Ok(message)) => Ok(message),
            Ok(Err(e)) => Err(e.into()),
            Err(e) => Err(e.into()),
        }
    }
}

// ============================================================================
// STEP 4: Demonstrate true streaming
// ============================================================================

/// Run the file sender using true AsyncRead streaming.
async fn run_sender(
    client_sender: bdrpc::channel::ChannelSender<FileTransferProtocol>,
    client_receiver: bdrpc::channel::ChannelReceiver<FileTransferProtocol>,
) -> Result<(), Box<dyn Error>> {
    println!("📤 Starting File Sender (True Streaming Mode)");
    println!("═══════════════════════════════════════════════════════════\n");

    // Create client
    let client = FileTransferClient::new(client_sender, client_receiver);

    // ========================================================================
    // Example 1: Stream from in-memory cursor (known size)
    // ========================================================================

    println!("📄 Example 1: Stream from Cursor (known size)");
    println!("───────────────────────────────────────────────────────────");

    let file_size = 5 * 1024 * 1024; // 5 MB
    let filename = "streamed_known_size.bin".to_string();
    let file_data = generate_file_data(file_size);

    // Create AsyncRead source
    let reader = Cursor::new(file_data);

    let result = client
        .upload_from_reader(filename, reader, Some(file_size as u64))
        .await?;
    println!("   📨 Server response: {}\n", result);

    sleep(Duration::from_millis(500)).await;

    // ========================================================================
    // Example 2: Stream with unknown size (true streaming scenario)
    // ========================================================================

    println!("📄 Example 2: Stream with unknown size");
    println!("───────────────────────────────────────────────────────────");

    let stream_size = 3 * 1024 * 1024; // 3 MB
    let stream_name = "streamed_unknown_size.bin".to_string();
    let stream_data = generate_file_data(stream_size);

    // Create AsyncRead source - in real usage, this could be a network stream
    // where we don't know the total size upfront
    let reader = Cursor::new(stream_data);

    let result = client.upload_from_reader(stream_name, reader, None).await?;
    println!("   📨 Server response: {}\n", result);

    println!("═══════════════════════════════════════════════════════════");
    println!("✅ Sender completed\n");

    Ok(())
}

/// Run the file receiver using the generated dispatcher.
async fn run_receiver(
    server_sender: bdrpc::channel::ChannelSender<FileTransferProtocol>,
    mut server_receiver: bdrpc::channel::ChannelReceiver<FileTransferProtocol>,
) -> Result<(), Box<dyn Error>> {
    println!("📥 Starting File Receiver");
    println!("═══════════════════════════════════════════════════════════\n");

    sleep(Duration::from_millis(100)).await;

    // Create service and dispatcher
    let service = MyFileTransferService::new();
    let dispatcher = FileTransferDispatcher::new(service);

    println!("⏳ Waiting for transfers...\n");

    let mut transfer_count = 0;

    // Process incoming requests using the dispatcher
    while let Some(request) = server_receiver.recv().await {
        let response = dispatcher.dispatch(request).await;

        // Check if this was a complete_transfer request
        let is_complete = matches!(
            response,
            FileTransferProtocol::CompleteTransferResponse { .. }
        );

        server_sender.send(response).await?;

        if is_complete {
            transfer_count += 1;
            println!();

            // Exit after both transfers complete
            if transfer_count >= 2 {
                break;
            }
        }
    }

    println!("═══════════════════════════════════════════════════════════");
    println!("✅ Receiver completed\n");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("\n📁 BDRPC File Transfer Service Example (True Streaming)");
    println!("═══════════════════════════════════════════════════════════");
    println!("Using AsyncRead for memory-efficient streaming (ADR-011)\n");

    // Create bidirectional in-memory channels
    let channel_id = ChannelId::new();
    println!("📡 Creating bidirectional channels (ID: {})\n", channel_id);

    let (sender_to_receiver_sender, sender_to_receiver_receiver) =
        Channel::<FileTransferProtocol>::new_in_memory(channel_id, 100);
    let (receiver_to_sender_sender, receiver_to_sender_receiver) =
        Channel::<FileTransferProtocol>::new_in_memory(channel_id, 100);

    // Spawn receiver task
    let receiver_handle = tokio::spawn(async move {
        if let Err(e) = run_receiver(receiver_to_sender_sender, sender_to_receiver_receiver).await {
            eprintln!("Receiver error: {}", e);
        }
    });

    // Run sender in foreground
    if let Err(e) = run_sender(sender_to_receiver_sender, receiver_to_sender_receiver).await {
        eprintln!("Sender error: {}", e);
    }

    // Wait for receiver to finish
    receiver_handle.await?;

    println!("═══════════════════════════════════════════════════════════");
    println!("🎉 File transfer service example completed successfully!\n");

    println!("📚 What this example demonstrated:");
    println!("   • TRUE STREAMING from AsyncRead sources");
    println!("   • No full buffering - chunks sent as they're read");
    println!("   • Chunked protocol using service macro");
    println!("   • Server-side chunk reassembly");
    println!("   • Support for both known and unknown sizes");
    println!("   • Memory-efficient transfers");

    println!("\n💡 Key implementation details:");
    println!("   • Service defines chunked protocol (start/send_chunk/complete)");
    println!("   • Client extension trait provides upload_from_reader()");
    println!("   • upload_from_reader() accepts any AsyncRead source");
    println!("   • Chunks are read and sent immediately (no buffering)");
    println!("   • Server reassembles chunks into complete data");

    println!("\n🔍 Why this approach works:");
    println!("   • Service macro doesn't support impl AsyncRead directly");
    println!("   • But we can use a chunked protocol at RPC level");
    println!("   • Client-side helpers handle AsyncRead streaming");
    println!("   • This gives us true streaming without macro limitations");

    println!("\n📖 Real-world usage:");
    println!("   // Stream from file");
    println!("   let file = File::open(\"large.bin\").await?;");
    println!("   let size = file.metadata().await?.len();");
    println!("   client.upload_from_reader(\"large.bin\", file, Some(size)).await?;");
    println!();
    println!("   // Stream from network (unknown size)");
    println!("   let response = reqwest::get(\"https://...\").await?;");
    println!("   let stream = StreamReader::new(response.bytes_stream());");
    println!("   client.upload_from_reader(\"download.bin\", stream, None).await?;");

    println!("\n📚 Related:");
    println!("   • See ADR-011 for full large transfer design");
    println!("   • See file_transfer_service.rs for manual chunking");
    println!("   • See file_transfer_manual for low-level protocol");

    Ok(())
}

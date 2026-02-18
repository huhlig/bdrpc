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

//! # File Transfer Service Example - Using #[bdrpc::service]
//!
//! This example demonstrates streaming large files using the `#[bdrpc::service]` macro.
//! Compare this with `file_transfer_manual` to see the difference between manual protocol
//! implementation and using the service macro.
//!
//! ## What This Example Shows
//!
//! - Defining a file transfer service with the `#[bdrpc::service]` macro
//! - Implementing the generated server trait
//! - Streaming large types in chunks
//! - Progress tracking during transfer
//! - Data integrity verification (CRC32)
//! - Using the dispatcher for request routing
//!
//! ## Benefits Over Manual Implementation
//!
//! - **Type Safety**: Compile-time checking of message types
//! - **Less Boilerplate**: No manual protocol enum matching
//! - **Automatic Routing**: Dispatcher handles request routing
//! - **Clean API**: Service methods are self-documenting
//! - **Structured Streaming**: Clear request/response flow
//!
//! ## Running This Example
//!
//! ```bash
//! cargo run --example file_transfer_service --features serde
//! ```

use bdrpc::channel::{Channel, ChannelId};
use bdrpc::service;
use std::error::Error;
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// Chunk size for file transfer (64KB)
const CHUNK_SIZE: usize = 64 * 1024;

// ============================================================================
// STEP 1: Define the service trait with the macro
// ============================================================================

/// File transfer service for streaming large files.
///
/// This macro generates:
/// - FileTransferProtocol enum (with all request/response variants)
/// - FileTransferClient struct (for making RPC calls)
/// - FileTransferServer trait (to implement)
/// - FileTransferDispatcher struct (for routing requests)
#[service(direction = "bidirectional")]
#[allow(dead_code)]
trait FileTransfer {
    /// Start a new file transfer
    async fn start_transfer(
        &self,
        filename: String,
        total_size: u64,
        chunk_size: usize,
    ) -> Result<String, String>;

    /// Send a chunk of types
    async fn send_chunk(&self, sequence: u64, data: Vec<u8>, checksum: u32) -> Result<u64, String>;

    /// End the transfer
    async fn end_transfer(&self, total_chunks: u64, checksum: u32) -> Result<String, String>;
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

/// Generate dummy file types for demonstration
fn generate_file_data(size: usize) -> Vec<u8> {
    (0..size).map(|i| (i % 256) as u8).collect()
}

// ============================================================================
// STEP 2: Implement the server trait
// ============================================================================

/// Our file transfer service implementation.
struct MyFileTransferService {
    received_data: std::sync::Arc<tokio::sync::Mutex<Vec<u8>>>,
    expected_sequence: std::sync::Arc<tokio::sync::Mutex<u64>>,
    total_size: std::sync::Arc<tokio::sync::Mutex<u64>>,
    start_time: std::sync::Arc<tokio::sync::Mutex<Option<Instant>>>,
}

impl MyFileTransferService {
    fn new() -> Self {
        Self {
            received_data: std::sync::Arc::new(tokio::sync::Mutex::new(Vec::new())),
            expected_sequence: std::sync::Arc::new(tokio::sync::Mutex::new(0)),
            total_size: std::sync::Arc::new(tokio::sync::Mutex::new(0)),
            start_time: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
        }
    }
}

/// Implement the generated FileTransferServer trait.
impl FileTransferServer for MyFileTransferService {
    async fn start_transfer(
        &self,
        filename: String,
        total_size: u64,
        chunk_size: usize,
    ) -> Result<String, String> {
        println!("   ✅ Transfer initiated");
        println!("   📝 Filename: {}", filename);
        println!("   📦 Total size: {} bytes", total_size);
        println!("   📦 Chunk size: {} bytes", chunk_size);

        // Initialize state
        *self.total_size.lock().await = total_size;
        *self.expected_sequence.lock().await = 0;
        self.received_data.lock().await.clear();
        self.received_data.lock().await.reserve(total_size as usize);
        *self.start_time.lock().await = Some(Instant::now());

        Ok("Ready to receive".to_string())
    }

    async fn send_chunk(&self, sequence: u64, data: Vec<u8>, checksum: u32) -> Result<u64, String> {
        // Verify sequence
        let expected = *self.expected_sequence.lock().await;
        if sequence != expected {
            return Err(format!(
                "Sequence mismatch: expected {}, got {}",
                expected, sequence
            ));
        }

        // Verify checksum
        let calculated_checksum = calculate_checksum(&data);
        if calculated_checksum != checksum {
            return Err(format!("Checksum mismatch for chunk {}", sequence));
        }

        // Store types
        let mut received = self.received_data.lock().await;
        received.extend_from_slice(&data);

        // Update sequence
        *self.expected_sequence.lock().await += 1;

        // Report progress occasionally
        let total_size = *self.total_size.lock().await;
        let bytes_received = received.len() as u64;
        let percent = (bytes_received as f64 / total_size as f64) * 100.0;

        if sequence % 10 == 0 {
            if let Some(start) = *self.start_time.lock().await {
                let elapsed = start.elapsed();
                let rate = bytes_received as f64 / elapsed.as_secs_f64() / 1024.0 / 1024.0;
                println!(
                    "   📥 Received: {:.1}% ({} / {} bytes, {:.2} MB/s)",
                    percent, bytes_received, total_size, rate
                );
            }
        }

        Ok(sequence)
    }

    async fn end_transfer(&self, total_chunks: u64, checksum: u32) -> Result<String, String> {
        println!("\n   🏁 End transfer signal received");
        println!("   📦 Total chunks: {}", total_chunks);
        println!("   🔐 Expected checksum: 0x{:08X}", checksum);

        // Verify total chunks
        let expected_seq = *self.expected_sequence.lock().await;
        if expected_seq != total_chunks {
            return Err(format!(
                "Chunk count mismatch: expected {}, got {}",
                total_chunks, expected_seq
            ));
        }

        // Verify total checksum
        let received = self.received_data.lock().await;
        let calculated_checksum = calculate_checksum(&received);
        if calculated_checksum != checksum {
            return Err(format!(
                "Final checksum mismatch: expected 0x{:08X}, got 0x{:08X}",
                checksum, calculated_checksum
            ));
        }

        if let Some(start) = *self.start_time.lock().await {
            let elapsed = start.elapsed();
            let rate = received.len() as f64 / elapsed.as_secs_f64() / 1024.0 / 1024.0;

            println!("   ✅ All types received and verified!");
            println!("   📊 Total size: {} bytes", received.len());
            println!("   ⏱️  Time: {:.2}s", elapsed.as_secs_f64());
            println!("   📊 Rate: {:.2} MB/s", rate);
            println!("   🔐 Checksum: 0x{:08X}", calculated_checksum);
        }

        Ok(format!(
            "File received successfully ({} bytes)",
            received.len()
        ))
    }
}

// ============================================================================
// STEP 3: Use the generated client and server
// ============================================================================

/// Run the file sender using the generated client.
async fn run_sender(
    client_sender: bdrpc::channel::ChannelSender<FileTransferProtocol>,
    client_receiver: bdrpc::channel::ChannelReceiver<FileTransferProtocol>,
) -> Result<(), Box<dyn Error>> {
    println!("📤 Starting File Sender");
    println!("═══════════════════════════════════════════════════════════\n");

    // Generate file types
    println!("📄 Generating file types");
    let file_size = 10 * 1024 * 1024; // 10 MB
    let filename = "demo_file.bin".to_string();
    let file_data = generate_file_data(file_size);
    println!("   ✅ Generated {} bytes\n", file_size);

    // Create client
    let client = FileTransferClient::new(client_sender, client_receiver);

    // Initiate transfer
    println!("🚀 Initiating file transfer");
    match client
        .start_transfer(filename.clone(), file_size as u64, CHUNK_SIZE)
        .await
    {
        Ok(Ok(message)) => {
            println!("   ✅ {}\n", message);
        }
        Ok(Err(e)) => {
            return Err(format!("Start transfer failed: {}", e).into());
        }
        Err(e) => {
            return Err(format!("Channel error: {}", e).into());
        }
    }

    // Send chunks
    println!("📦 Sending file chunks");
    println!("═══════════════════════════════════════════════════════════\n");

    let total_chunks = file_size.div_ceil(CHUNK_SIZE);
    let start_time = Instant::now();
    let mut bytes_sent = 0u64;

    for (sequence, chunk) in file_data.chunks(CHUNK_SIZE).enumerate() {
        let checksum = calculate_checksum(chunk);

        match client
            .send_chunk(sequence as u64, chunk.to_vec(), checksum)
            .await
        {
            Ok(Ok(ack_seq)) => {
                if ack_seq != sequence as u64 {
                    println!(
                        "   ⚠️  Sequence mismatch: expected {}, got {}",
                        sequence, ack_seq
                    );
                }
            }
            Ok(Err(e)) => {
                return Err(format!("Send chunk failed: {}", e).into());
            }
            Err(e) => {
                return Err(format!("Channel error: {}", e).into());
            }
        }

        bytes_sent += chunk.len() as u64;

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

    // End transfer
    println!("🏁 Finalizing transfer");
    let total_checksum = calculate_checksum(&file_data);
    match client
        .end_transfer(total_chunks as u64, total_checksum)
        .await
    {
        Ok(Ok(message)) => {
            println!("   ✅ {}\n", message);
        }
        Ok(Err(e)) => {
            return Err(format!("End transfer failed: {}", e).into());
        }
        Err(e) => {
            return Err(format!("Channel error: {}", e).into());
        }
    }

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

    // Give sender time to start
    sleep(Duration::from_millis(100)).await;

    // Create service and dispatcher
    let service = MyFileTransferService::new();
    let dispatcher = FileTransferDispatcher::new(service);

    println!("⏳ Waiting for transfer initiation...\n");

    // Process incoming requests using the dispatcher
    while let Some(request) = server_receiver.recv().await {
        let response = dispatcher.dispatch(request).await;

        // Check if this was the end transfer request
        let is_end = matches!(response, FileTransferProtocol::EndTransferResponse { .. });

        server_sender.send(response).await?;

        if is_end {
            break;
        }
    }

    println!("\n═══════════════════════════════════════════════════════════");
    println!("✅ Receiver completed\n");

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("\n📁 BDRPC File Transfer Service Example");
    println!("═══════════════════════════════════════════════════════════");
    println!("Using the #[bdrpc::service] macro for streaming large files\n");

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
    println!("   • Defining a file transfer service with #[bdrpc::service]");
    println!("   • Implementing the generated FileTransferServer trait");
    println!("   • Using FileTransferDispatcher for request routing");
    println!("   • Using FileTransferClient for type-safe RPC calls");
    println!("   • Streaming large files in chunks (64KB)");
    println!("   • Progress tracking and reporting");
    println!("   • Data integrity verification (CRC32)");

    println!("\n💡 Benefits over manual implementation:");
    println!("   • Type-safe service methods");
    println!("   • Automatic request/response matching");
    println!("   • Clean separation of concerns");
    println!("   • Less boilerplate code");
    println!("   • Self-documenting service interface");

    println!("\n🔍 Compare with file_transfer_manual:");
    println!("   • file_transfer_manual: Manual FileTransferProtocol enum");
    println!("   • file_transfer_service.rs: Generated via macro");
    println!("   • Service macro provides cleaner streaming API");

    println!("\n📖 Next steps:");
    println!("   • See service_macro_demo.rs for detailed explanation");
    println!("   • Try calculator_service.rs for simpler example");
    println!("   • Check chat_server_service.rs for multi-client pattern");
    println!("   • Read the macro documentation for advanced features");

    Ok(())
}

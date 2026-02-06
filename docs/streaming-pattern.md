# Streaming Large Data with AsyncRead

This document describes the pattern for streaming large data transfers using `tokio::io::AsyncRead` in BDRPC, as demonstrated in the `file_transfer_service_async.rs` example.

## Overview

When transferring large files or streaming data, you want to avoid loading the entire payload into memory. The AsyncRead streaming pattern allows you to:

- Read data chunk-by-chunk from any AsyncRead source
- Send chunks as they're read (no full buffering)
- Support unknown sizes (true streaming scenarios)
- Maintain memory efficiency regardless of data size

## The Challenge

The `#[service]` macro doesn't support generic type parameters like `impl AsyncRead` directly in trait method signatures. This is a limitation of Rust's trait system when combined with proc macros.

```rust
// ❌ This doesn't work with the service macro
#[service(direction = "bidirectional")]
trait FileTransfer {
    async fn upload<R: AsyncRead>(&self, reader: R) -> Result<(), String>;
}
```

## The Solution: Chunked Protocol + Client Extensions

The solution is to use a two-layer approach:

1. **RPC Layer**: Define a chunked protocol using the service macro
2. **Client Layer**: Provide extension methods that accept AsyncRead sources

### Layer 1: Chunked RPC Protocol

Define a service that handles chunks of data:

```rust
#[service(direction = "bidirectional")]
trait FileTransfer {
    /// Start a new transfer session
    async fn start_transfer(
        &self,
        filename: String,
        total_size: Option<u64>, // None for unknown size
    ) -> Result<String, String>; // Returns transfer_id
    
    /// Send a chunk of data
    async fn send_chunk(
        &self,
        transfer_id: String,
        chunk_index: u64,
        data: Vec<u8>,
        is_final: bool,
    ) -> Result<(), String>;
    
    /// Complete the transfer
    async fn complete_transfer(
        &self,
        transfer_id: String,
    ) -> Result<String, String>;
}
```

### Layer 2: Client Extension with AsyncRead

Provide a client extension trait that accepts AsyncRead sources:

```rust
trait FileTransferClientExt {
    /// Upload from any AsyncRead source
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
        // 1. Start the transfer
        let transfer_id = self.start_transfer(filename, total_size).await??;
        
        // 2. Stream chunks from the reader
        let mut chunk_index = 0u64;
        let mut buffer = vec![0u8; CHUNK_SIZE];
        
        loop {
            let bytes_read = reader.read(&mut buffer).await?;
            if bytes_read == 0 {
                break; // EOF
            }
            
            // Send chunk immediately (no buffering!)
            self.send_chunk(
                transfer_id.clone(),
                chunk_index,
                buffer[..bytes_read].to_vec(),
                false,
            ).await??;
            
            chunk_index += 1;
        }
        
        // 3. Send final marker and complete
        self.send_chunk(transfer_id.clone(), chunk_index, Vec::new(), true).await??;
        self.complete_transfer(transfer_id).await
    }
}
```

## Server-Side Implementation

The server maintains transfer state and reassembles chunks:

```rust
struct TransferState {
    filename: String,
    total_size: Option<u64>,
    chunks: HashMap<u64, Vec<u8>>,
    bytes_received: u64,
}

impl FileTransferServer for MyService {
    async fn start_transfer(
        &self,
        filename: String,
        total_size: Option<u64>,
    ) -> Result<String, String> {
        let transfer_id = Uuid::new_v4().to_string();
        let state = TransferState::new(filename, total_size);
        self.active_transfers.insert(transfer_id.clone(), state);
        Ok(transfer_id)
    }
    
    async fn send_chunk(
        &self,
        transfer_id: String,
        chunk_index: u64,
        data: Vec<u8>,
        is_final: bool,
    ) -> Result<(), String> {
        let state = self.active_transfers.get_mut(&transfer_id)?;
        state.add_chunk(chunk_index, data);
        Ok(())
    }
    
    async fn complete_transfer(
        &self,
        transfer_id: String,
    ) -> Result<String, String> {
        let state = self.active_transfers.remove(&transfer_id)?;
        let complete_data = state.finalize(); // Reassemble chunks
        // Process complete_data...
        Ok("Transfer complete".to_string())
    }
}
```

## Usage Examples

### Example 1: Stream from File

```rust
use tokio::fs::File;

let file = File::open("large_file.bin").await?;
let metadata = file.metadata().await?;
let size = metadata.len();

client.upload_from_reader(
    "large_file.bin".to_string(),
    file,
    Some(size),
).await?;
```

### Example 2: Stream from Network (Unknown Size)

```rust
use tokio_util::io::StreamReader;

let response = reqwest::get("https://example.com/data").await?;
let stream = response.bytes_stream();
let reader = StreamReader::new(stream);

client.upload_from_reader(
    "downloaded_data.bin".to_string(),
    reader,
    None, // Unknown size
).await?;
```

### Example 3: Stream Compressed Data

```rust
use async_compression::tokio::bufread::GzipEncoder;
use tokio::io::BufReader;

let file = File::open("data.txt").await?;
let buffered = BufReader::new(file);
let compressed = GzipEncoder::new(buffered);

client.upload_from_reader(
    "data.txt.gz".to_string(),
    compressed,
    None, // Size unknown after compression
).await?;
```

### Example 4: Stream from In-Memory Cursor

```rust
use std::io::Cursor;

let data = vec![0u8; 10_000_000]; // 10 MB
let cursor = Cursor::new(data);

client.upload_from_reader(
    "memory_data.bin".to_string(),
    cursor,
    Some(10_000_000),
).await?;
```

## Benefits

### Memory Efficiency
- **No Full Buffering**: Data is never fully loaded into memory
- **Constant Memory Usage**: Memory usage is O(chunk_size), not O(file_size)
- **Scalable**: Can handle files of any size

### Flexibility
- **Any AsyncRead Source**: Works with files, network streams, pipes, etc.
- **Unknown Sizes**: Supports streaming without knowing total size upfront
- **Composable**: Can chain AsyncRead adapters (compression, encryption, etc.)

### Performance
- **Pipelined**: Chunks can be sent while reading continues
- **Natural Backpressure**: AsyncRead provides built-in flow control
- **Efficient**: No unnecessary copying or buffering

## Comparison with Other Approaches

### vs. In-Memory Vec<u8>

```rust
// ❌ Bad: Loads entire file into memory
let data = tokio::fs::read("large_file.bin").await?;
client.upload_file("large_file.bin", data).await?;

// ✅ Good: Streams from file
let file = File::open("large_file.bin").await?;
client.upload_from_reader("large_file.bin", file, None).await?;
```

### vs. Manual Chunking

```rust
// ❌ Bad: Manual chunking is error-prone
let mut file = File::open("large_file.bin").await?;
let mut buffer = vec![0u8; CHUNK_SIZE];
let mut chunk_index = 0;

loop {
    let n = file.read(&mut buffer).await?;
    if n == 0 { break; }
    client.send_chunk(chunk_index, buffer[..n].to_vec()).await?;
    chunk_index += 1;
}

// ✅ Good: Extension method handles everything
let file = File::open("large_file.bin").await?;
client.upload_from_reader("large_file.bin", file, None).await?;
```

## Implementation Checklist

When implementing this pattern:

- [ ] Define chunked protocol in service trait
- [ ] Create client extension trait with generic AsyncRead method
- [ ] Implement server-side transfer state management
- [ ] Handle chunk reassembly on server
- [ ] Support both known and unknown sizes
- [ ] Add progress tracking (optional)
- [ ] Implement proper error handling
- [ ] Add timeout handling for stalled transfers
- [ ] Consider adding checksums for data integrity
- [ ] Document usage examples

## Future Enhancements

The `#[large_transfer]` attribute mentioned in ADR-011 would automate this pattern:

```rust
// Future syntax (not yet implemented)
#[service(direction = "bidirectional")]
trait FileTransfer {
    #[large_transfer(chunk_size = 65536)]
    async fn upload<R: AsyncRead>(
        &self,
        filename: String,
        reader: R,
    ) -> Result<(), String>;
}
```

This would automatically generate:
- The chunked protocol methods
- The client extension trait
- Server-side state management
- Chunk reassembly logic

## Related Documentation

- [ADR-011: Large Transfer Streaming](adr/ADR-011-large-transfer-streaming.md)
- [file_transfer_service_async.rs](../bdrpc/examples/file_transfer_service_async.rs) - Complete working example
- [file_transfer_service.rs](../bdrpc/examples/file_transfer_service.rs) - Manual chunking approach
- [file_transfer.rs](../bdrpc/examples/file_transfer_manual.rs) - Low-level protocol

## Summary

The AsyncRead streaming pattern provides a clean, efficient way to transfer large data in BDRPC:

1. **Define a chunked protocol** using the service macro
2. **Provide client extensions** that accept AsyncRead sources
3. **Implement server-side reassembly** of chunks
4. **Use extension methods** for streaming from any source

This pattern gives you true streaming capabilities while working within the constraints of the service macro, and provides a foundation for future automatic code generation via the `#[large_transfer]` attribute.
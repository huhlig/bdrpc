# ADR-011: Large Transfer Streaming

## Status
Proposed (Future Implementation)

## Context

The current BDRPC design uses a fixed maximum frame size for all messages. While this works well for typical RPC calls with small payloads, certain use cases require transferring large amounts of data:

### Use Cases
- **File transfers**: Uploading/downloading files through RPC methods
- **Bulk data export**: Exporting large datasets or database dumps
- **Media streaming**: Transferring video, audio, or image data
- **Log retrieval**: Fetching large log files or historical data
- **Backup/restore**: Transferring backup archives

### Current Limitations
- All messages must fit within the maximum frame size (typically 1-16 MB)
- Large payloads require chunking at the application level
- No built-in flow control for large transfers
- Memory pressure from buffering entire payloads
- No progress tracking or cancellation support

### Problem Statement
When a method needs to transfer data larger than the frame size, developers must:
1. Manually chunk the data
2. Implement their own streaming protocol
3. Handle flow control and backpressure
4. Manage memory efficiently
5. Provide progress feedback

This is error-prone and leads to inconsistent implementations across different protocols.

## Decision

We will introduce a **`#[large_transfer]` attribute** for methods that need to handle data exceeding normal packet sizes. This attribute will enable automatic streaming with built-in flow control, progress tracking, and efficient memory usage.

### 1. Attribute Syntax

```rust
use bdrpc::prelude::*;

#[bdrpc_protocol]
pub trait FileTransfer {
    /// Upload a file with streaming support
    #[large_transfer(chunk_size = 1_048_576)] // 1 MB chunks
    async fn upload_file(&self, path: String, data: Vec<u8>) -> Result<(), String>;
    
    /// Download a file with streaming support
    #[large_transfer(chunk_size = 1_048_576)]
    async fn download_file(&self, path: String) -> Result<Vec<u8>, String>;
    
    /// Stream with custom configuration
    #[large_transfer(
        chunk_size = 2_097_152,  // 2 MB chunks
        max_concurrent_chunks = 4,
        enable_compression = true,
        progress_callback = true
    )]
    async fn backup_database(&self, db_name: String) -> Result<Vec<u8>, String>;
}

### 1.1. Streaming from AsyncRead Sources

The `#[large_transfer]` attribute supports both in-memory data (`Vec<u8>`) and streaming from any `AsyncRead` source:

```rust
use tokio::io::AsyncRead;
use tokio::fs::File;

#[bdrpc_protocol]
pub trait FileTransfer {
    /// Upload from in-memory buffer
    #[large_transfer(chunk_size = 1_048_576)]
    async fn upload_file(&self, path: String, data: Vec<u8>) -> Result<(), String>;
    
    /// Upload from any AsyncRead source (file, network stream, etc.)
    #[large_transfer(chunk_size = 1_048_576)]
    async fn upload_stream(
        &self, 
        path: String, 
        reader: impl AsyncRead + Send + Unpin
    ) -> Result<(), String>;
}

// Usage examples
async fn example_usage(client: &FileTransferClient) -> Result<(), Box<dyn std::error::Error>> {
    // Example 1: Upload from in-memory buffer
    let data = vec![0u8; 10_000_000]; // 10 MB
    client.upload_file("data.bin".to_string(), data).await?;
    
    // Example 2: Upload from file
    let file = File::open("large_file.bin").await?;
    client.upload_stream("remote.bin".to_string(), file).await?;
    
    // Example 3: Upload from network stream
    let response = reqwest::get("https://example.com/data").await?;
    let stream = response.bytes_stream();
    let reader = tokio_util::io::StreamReader::new(stream);
    client.upload_stream("downloaded.bin".to_string(), reader).await?;
    
    // Example 4: Upload from compressed data on-the-fly
    use async_compression::tokio::bufread::GzipEncoder;
    let file = File::open("data.txt").await?;
    let compressed = GzipEncoder::new(tokio::io::BufReader::new(file));
    client.upload_stream("data.txt.gz".to_string(), compressed).await?;
    
    Ok(())
}
```
```

### 2. Generated Code Structure

The macro will generate streaming infrastructure:

```rust
// Generated for upload_file
pub struct UploadFileRequest {
    pub path: String,
    pub transfer_id: TransferId,
    pub total_size: u64,
    pub chunk_count: u32,
}

pub struct UploadFileChunk {
    pub transfer_id: TransferId,
    pub chunk_index: u32,
    pub data: Vec<u8>,
    pub is_final: bool,
}

pub struct UploadFileResponse {
    pub transfer_id: TransferId,
    pub result: Result<(), String>,
}

// Progress tracking
pub struct TransferProgress {
    pub transfer_id: TransferId,
    pub bytes_transferred: u64,
    pub total_bytes: u64,
    pub chunks_completed: u32,
    pub total_chunks: u32,
}

// Client-side implementation for Vec<u8>
impl FileTransferClient {
    pub async fn upload_file(
        &self,
        path: String,
        data: Vec<u8>,
    ) -> Result<TransferHandle<()>, RpcError> {
        let transfer_id = TransferId::new();
        let chunk_size = 1_048_576;
        let total_size = data.len() as u64;
        let chunk_count = ((total_size + chunk_size - 1) / chunk_size) as u32;
        
        // Send initial request
        let request = UploadFileRequest {
            path,
            transfer_id,
            total_size,
            chunk_count,
        };
        self.channel.send(request).await?;
        
        // Create transfer handle
        let handle = TransferHandle::new(transfer_id, total_size);
        
        // Spawn streaming task
        tokio::spawn(stream_upload_from_buffer(
            self.channel.clone(),
            transfer_id,
            data,
            chunk_size,
            handle.progress_tx.clone(),
        ));
        
        Ok(handle)
    }
    
    // Client-side implementation for AsyncRead sources
    pub async fn upload_stream(
        &self,
        path: String,
        mut reader: impl AsyncRead + Send + Unpin + 'static,
    ) -> Result<TransferHandle<()>, RpcError> {
        use tokio::io::AsyncReadExt;
        
        let transfer_id = TransferId::new();
        let chunk_size = 1_048_576;
        
        // For streaming, we don't know total size upfront
        // Send initial request with unknown size (0 indicates streaming)
        let request = UploadFileRequest {
            path,
            transfer_id,
            total_size: 0, // Unknown size for streaming
            chunk_count: 0, // Unknown chunk count
        };
        self.channel.send(request).await?;
        
        // Create transfer handle with unknown size
        let handle = TransferHandle::new(transfer_id, 0);
        
        // Spawn streaming task that reads from AsyncRead
        let channel = self.channel.clone();
        let progress_tx = handle.progress_tx.clone();
        
        tokio::spawn(async move {
            let mut chunk_index = 0u32;
            let mut total_bytes = 0u64;
            let mut buffer = vec![0u8; chunk_size];
            
            loop {
                // Read chunk from reader
                let bytes_read = match reader.read(&mut buffer).await {
                    Ok(0) => break, // EOF
                    Ok(n) => n,
                    Err(e) => {
                        tracing::error!("Error reading from stream: {}", e);
                        break;
                    }
                };
                
                total_bytes += bytes_read as u64;
                
                // Send chunk
                let chunk = UploadFileChunk {
                    transfer_id,
                    chunk_index,
                    data: buffer[..bytes_read].to_vec(),
                    is_final: false,
                };
                
                if let Err(e) = channel.send(chunk).await {
                    tracing::error!("Error sending chunk: {}", e);
                    break;
                }
                
                // Update progress
                let _ = progress_tx.send(TransferProgress {
                    transfer_id,
                    bytes_transferred: total_bytes,
                    total_bytes: 0, // Unknown for streaming
                    chunks_completed: chunk_index + 1,
                    total_chunks: 0, // Unknown for streaming
                });
                
                chunk_index += 1;
            }
            
            // Send final chunk marker
            let final_chunk = UploadFileChunk {
                transfer_id,
                chunk_index,
                data: Vec::new(),
                is_final: true,
            };
            let _ = channel.send(final_chunk).await;
        });
        
        Ok(handle)
    }
}

// Server-side implementation
impl FileTransferServer {
    async fn handle_upload_file_request(
        &self,
        request: UploadFileRequest,
    ) -> Result<(), RpcError> {
        // Initialize transfer state
        let state = TransferState::new(
            request.transfer_id,
            request.total_size,
            request.chunk_count,
        );
        
        self.active_transfers.insert(request.transfer_id, state);
        
        // Actual upload will be handled by chunks
        Ok(())
    }
    
    async fn handle_upload_file_chunk(
        &self,
        chunk: UploadFileChunk,
    ) -> Result<(), RpcError> {
        let mut state = self.active_transfers
            .get_mut(&chunk.transfer_id)
            .ok_or(RpcError::TransferNotFound)?;
        
        // Append chunk data
        state.append_chunk(chunk.chunk_index, chunk.data)?;
        
        // If final chunk, complete the transfer
        if chunk.is_final {
            let complete_data = state.finalize()?;
            
            // Call actual implementation
            let result = self.implementation
                .upload_file(state.path.clone(), complete_data)
                .await;
            
            // Send response
            let response = UploadFileResponse {
                transfer_id: chunk.transfer_id,
                result,
            };
            self.channel.send(response).await?;
            
            // Cleanup
            self.active_transfers.remove(&chunk.transfer_id);
        }
        
        Ok(())
    }
}
```

### 3. Transfer Handle API

Provide a handle for monitoring and controlling transfers:

```rust
/// Handle for monitoring and controlling a large transfer
pub struct TransferHandle<T> {
    transfer_id: TransferId,
    total_size: u64,
    progress_rx: watch::Receiver<TransferProgress>,
    result_rx: oneshot::Receiver<Result<T, RpcError>>,
    cancel_tx: oneshot::Sender<()>,
}

impl<T> TransferHandle<T> {
    /// Get current transfer progress
    pub fn progress(&self) -> TransferProgress {
        *self.progress_rx.borrow()
    }
    
    /// Wait for transfer completion
    pub async fn wait(self) -> Result<T, RpcError> {
        self.result_rx.await?
    }
    
    /// Cancel the transfer
    pub fn cancel(self) -> Result<(), RpcError> {
        self.cancel_tx.send(()).map_err(|_| RpcError::TransferCancelled)
    }
    
    /// Subscribe to progress updates
    pub fn subscribe_progress(&self) -> watch::Receiver<TransferProgress> {
        self.progress_rx.clone()
    }
    
    /// Get transfer ID
    pub fn id(&self) -> TransferId {
        self.transfer_id
    }
}
```

### 4. Flow Control and Backpressure

Implement window-based flow control:

```rust
/// Flow control for large transfers
pub struct TransferFlowControl {
    /// Maximum number of chunks in flight
    window_size: usize,
    /// Currently in-flight chunks
    in_flight: Arc<Mutex<HashSet<u32>>>,
    /// Semaphore for flow control
    semaphore: Arc<Semaphore>,
}

impl TransferFlowControl {
    pub fn new(window_size: usize) -> Self {
        Self {
            window_size,
            in_flight: Arc::new(Mutex::new(HashSet::new())),
            semaphore: Arc::new(Semaphore::new(window_size)),
        }
    }
    
    /// Acquire permission to send a chunk
    pub async fn acquire(&self, chunk_index: u32) -> FlowControlPermit {
        let permit = self.semaphore.acquire().await.unwrap();
        self.in_flight.lock().await.insert(chunk_index);
        FlowControlPermit {
            chunk_index,
            in_flight: self.in_flight.clone(),
            _permit: permit,
        }
    }
}

pub struct FlowControlPermit {
    chunk_index: u32,
    in_flight: Arc<Mutex<HashSet<u32>>>,
    _permit: SemaphorePermit<'static>,
}

impl Drop for FlowControlPermit {
    fn drop(&mut self) {
        let in_flight = self.in_flight.clone();
        let chunk_index = self.chunk_index;
        tokio::spawn(async move {
            in_flight.lock().await.remove(&chunk_index);
        });
    }
}
```

### 5. Compression Support

Optional compression for large transfers:

```rust
/// Compression configuration for large transfers
#[derive(Debug, Clone)]
pub enum CompressionMode {
    None,
    Zstd { level: i32 },
    Lz4 { level: u32 },
    Gzip { level: u32 },
}

impl CompressionMode {
    pub fn compress(&self, data: &[u8]) -> Result<Vec<u8>, CompressionError> {
        match self {
            Self::None => Ok(data.to_vec()),
            Self::Zstd { level } => {
                zstd::encode_all(data, *level)
                    .map_err(CompressionError::Zstd)
            }
            Self::Lz4 { level } => {
                lz4::compress(data, *level)
                    .map_err(CompressionError::Lz4)
            }
            Self::Gzip { level } => {
                flate2::compress(data, *level)
                    .map_err(CompressionError::Gzip)
            }
        }
    }
    
    pub fn decompress(&self, data: &[u8]) -> Result<Vec<u8>, CompressionError> {
        match self {
            Self::None => Ok(data.to_vec()),
            Self::Zstd { .. } => {
                zstd::decode_all(data)
                    .map_err(CompressionError::Zstd)
            }
            Self::Lz4 { .. } => {
                lz4::decompress(data)
                    .map_err(CompressionError::Lz4)
            }
            Self::Gzip { .. } => {
                flate2::decompress(data)
                    .map_err(CompressionError::Gzip)
            }
        }
    }
}
```

### 6. Transfer State Management

Track active transfers on both sides:

```rust
/// State for an active large transfer
pub struct TransferState {
    pub transfer_id: TransferId,
    pub total_size: u64,
    pub chunk_count: u32,
    pub received_chunks: HashMap<u32, Vec<u8>>,
    pub bytes_received: u64,
    pub started_at: Instant,
    pub metadata: HashMap<String, String>,
}

impl TransferState {
    pub fn new(
        transfer_id: TransferId,
        total_size: u64,
        chunk_count: u32,
    ) -> Self {
        Self {
            transfer_id,
            total_size,
            chunk_count,
            received_chunks: HashMap::new(),
            bytes_received: 0,
            started_at: Instant::now(),
            metadata: HashMap::new(),
        }
    }
    
    pub fn append_chunk(
        &mut self,
        chunk_index: u32,
        data: Vec<u8>,
    ) -> Result<(), TransferError> {
        if chunk_index >= self.chunk_count {
            return Err(TransferError::InvalidChunkIndex {
                index: chunk_index,
                max: self.chunk_count,
            });
        }
        
        if self.received_chunks.contains_key(&chunk_index) {
            return Err(TransferError::DuplicateChunk { index: chunk_index });
        }
        
        self.bytes_received += data.len() as u64;
        self.received_chunks.insert(chunk_index, data);
        
        Ok(())
    }
    
    pub fn is_complete(&self) -> bool {
        self.received_chunks.len() == self.chunk_count as usize
    }
    
    pub fn finalize(self) -> Result<Vec<u8>, TransferError> {
        if !self.is_complete() {
            return Err(TransferError::IncompleteTransfer {
                received: self.received_chunks.len(),
                expected: self.chunk_count as usize,
            });
        }
        
        // Reassemble chunks in order
        let mut result = Vec::with_capacity(self.total_size as usize);
        for i in 0..self.chunk_count {
            let chunk = self.received_chunks.get(&i)
                .ok_or(TransferError::MissingChunk { index: i })?;
            result.extend_from_slice(chunk);
        }
        
        Ok(result)
    }
    
    pub fn progress(&self) -> TransferProgress {
        TransferProgress {
            transfer_id: self.transfer_id,
            bytes_transferred: self.bytes_received,
            total_bytes: self.total_size,
            chunks_completed: self.received_chunks.len() as u32,
            total_chunks: self.chunk_count,
        }
    }
}
```

### 7. Error Handling

Specific errors for large transfers:

```rust
#[derive(Debug, thiserror::Error)]
pub enum TransferError {
    #[error("Transfer not found: {0}")]
    TransferNotFound(TransferId),
    
    #[error("Invalid chunk index {index}, max is {max}")]
    InvalidChunkIndex { index: u32, max: u32 },
    
    #[error("Duplicate chunk received: {index}")]
    DuplicateChunk { index: u32 },
    
    #[error("Missing chunk: {index}")]
    MissingChunk { index: u32 },
    
    #[error("Incomplete transfer: received {received}/{expected} chunks")]
    IncompleteTransfer { received: usize, expected: usize },
    
    #[error("Transfer cancelled")]
    TransferCancelled,
    
    #[error("Transfer timeout after {elapsed:?}")]
    TransferTimeout { elapsed: Duration },
    
    #[error("Compression error: {0}")]
    CompressionError(#[from] CompressionError),
    
    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
}
```

## Consequences

### Positive
- **Automatic streaming**: No manual chunking required
- **AsyncRead support**: Stream from any readable source (files, network, pipes, etc.)
- **Zero-copy streaming**: Read directly from source without buffering entire payload
- **Flexible data sources**: Support both in-memory buffers and streaming sources
- **Flow control**: Built-in backpressure handling
- **Progress tracking**: Monitor transfer progress
- **Memory efficient**: Streams data without buffering entire payload
- **Cancellation support**: Can abort transfers
- **Compression**: Optional compression for bandwidth savings
- **Consistent API**: Same pattern across all large transfers
- **Error recovery**: Can retry failed chunks

### Negative
- **Complexity**: Adds significant complexity to macro generation
- **State management**: Must track active transfers on both sides
- **Memory overhead**: Transfer state and chunk buffers
- **Protocol overhead**: Additional messages for chunks and control
- **Testing complexity**: More scenarios to test
- **Debugging difficulty**: Harder to debug streaming issues

### Neutral
- **Chunk size tuning**: May need adjustment per use case
- **Timeout configuration**: Need sensible defaults
- **Concurrent transfers**: Must limit to prevent resource exhaustion
- **Ordering guarantees**: Chunks may arrive out of order
- **Unknown size handling**: Streaming from AsyncRead may not know total size upfront
- **Progress estimation**: For unknown-size streams, progress is bytes transferred only

## Alternatives Considered

### 1. Application-Level Streaming
Let applications implement their own streaming. Rejected because:
- Inconsistent implementations
- Error-prone
- No reusable infrastructure
- Poor developer experience

### 2. Separate Transfer Protocol
Create a dedicated file transfer protocol. Rejected because:
- Doesn't integrate with RPC methods
- Requires separate connection management
- Harder to correlate with RPC calls

### 3. HTTP-Style Multipart
Use multipart encoding like HTTP. Rejected because:
- Doesn't fit RPC model well
- Less efficient than binary chunks
- Harder to implement flow control

### 4. gRPC-Style Streaming
Use bidirectional streaming like gRPC. Considered for future:
- More complex to implement
- Requires different method signatures
- May be added as `#[stream]` attribute later

## Implementation Notes

### Phasing
This is a **future implementation** to be done after core functionality is stable:

**Phase 1**: Basic chunking
- Implement chunk protocol
- Basic flow control
- Simple progress tracking

**Phase 2**: Advanced features
- Compression support
- Retry logic
- Checksum verification

**Phase 3**: Optimization
- Parallel chunk transfer
- Adaptive chunk sizing
- Bandwidth throttling

### Configuration
Add to `EndpointConfig`:

```rust
pub struct LargeTransferConfig {
    /// Default chunk size (1 MB)
    pub default_chunk_size: usize,
    /// Maximum concurrent chunks in flight
    pub max_concurrent_chunks: usize,
    /// Transfer timeout
    pub transfer_timeout: Duration,
    /// Maximum active transfers
    pub max_active_transfers: usize,
    /// Enable compression by default
    pub default_compression: CompressionMode,
}

impl Default for LargeTransferConfig {
    fn default() -> Self {
        Self {
            default_chunk_size: 1_048_576, // 1 MB
            max_concurrent_chunks: 4,
            transfer_timeout: Duration::from_secs(300), // 5 minutes
            max_active_transfers: 10,
            default_compression: CompressionMode::None,
        }
    }
}
```

### Security Considerations
- **Resource limits**: Prevent DoS via many transfers
- **Size validation**: Validate total_size matches actual data
- **Checksum verification**: Detect corruption or tampering
- **Rate limiting**: Limit transfer initiation rate
- **Authentication**: Verify permissions for large transfers

### Testing Strategy
- Unit tests for chunking logic
- Integration tests for complete transfers
- Test cancellation scenarios
- Test timeout handling
- Test concurrent transfers
- Benchmark transfer performance
- Test with various chunk sizes
- Test compression effectiveness

## Related ADRs
- ADR-001: Core Architecture (message framing)
- ADR-003: Backpressure and Flow Control (flow control mechanisms)
- ADR-004: Error Handling Hierarchy (transfer errors)
- ADR-005: Serialization Strategy (chunk serialization)

## References
- gRPC Streaming
- HTTP/2 Flow Control
- QUIC Stream Management
- BitTorrent Protocol (chunking strategy)
- rsync Algorithm (efficient transfers)
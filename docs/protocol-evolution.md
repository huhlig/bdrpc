# Protocol Evolution Guide

This guide explains how to evolve your BDRPC protocols over time while maintaining backward compatibility with existing clients and servers.

## Table of Contents

- [Overview](#overview)
- [Versioning Strategy](#versioning-strategy)
- [Evolution Patterns](#evolution-patterns)
- [Best Practices](#best-practices)
- [Examples](#examples)
- [Testing Compatibility](#testing-compatibility)

## Overview

Protocol evolution is the process of changing your message types and RPC interfaces over time while ensuring that:

1. **Backward Compatibility**: New servers can communicate with old clients
2. **Forward Compatibility**: Old servers can communicate with new clients (when possible)
3. **Graceful Degradation**: Systems handle unknown messages gracefully

BDRPC provides several mechanisms to support protocol evolution:

- **Protocol Versioning**: Version negotiation during handshake
- **Feature Negotiation**: Dynamic capability discovery
- **Serialization Flexibility**: Multiple serialization formats
- **Error Handling**: Graceful handling of unknown messages

## Versioning Strategy

### Semantic Versioning

Use semantic versioning for your protocols:

```rust
use bdrpc::Protocol;

#[derive(Protocol, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum MyProtocolV1 {
    Request { id: u64, data: String },
    Response { id: u64, result: i32 },
}

#[derive(Protocol, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum MyProtocolV2 {
    Request { id: u64, data: String },
    Response { id: u64, result: i32 },
    // New message type in v2
    Notification { message: String },
}
```

**Version Numbering**:
- **Major version**: Breaking changes (incompatible)
- **Minor version**: New features (backward compatible)
- **Patch version**: Bug fixes (fully compatible)

### Version Negotiation

BDRPC automatically negotiates protocol versions during the handshake:

```rust
use bdrpc::endpoint::{Endpoint, EndpointConfig};
use std::collections::HashSet;

// Server supports multiple versions
let mut server_features = HashSet::new();
server_features.insert("protocol:v1".to_string());
server_features.insert("protocol:v2".to_string());

let config = EndpointConfig::default()
    .with_features(server_features);

let endpoint = Endpoint::new(config);
```

After handshake, check negotiated features:

```rust
let negotiated = endpoint.negotiated_features();
if negotiated.contains("protocol:v2") {
    // Use v2 protocol
} else if negotiated.contains("protocol:v1") {
    // Fall back to v1 protocol
}
```

## Evolution Patterns

### Pattern 1: Adding New Message Types

**Safe**: Adding new message types is backward compatible.

```rust
// Version 1
#[derive(Protocol, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ApiV1 {
    GetUser { id: u64 },
    UserResponse { id: u64, name: String },
}

// Version 2 - Add new message types
#[derive(Protocol, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ApiV2 {
    GetUser { id: u64 },
    UserResponse { id: u64, name: String },
    // New in v2
    GetUserList,
    UserListResponse { users: Vec<String> },
}
```

**Implementation**:

```rust
async fn handle_message_v2(msg: ApiV2) -> Result<ApiV2, Error> {
    match msg {
        ApiV2::GetUser { id } => {
            // Handle v1 message
            Ok(ApiV2::UserResponse { 
                id, 
                name: get_user_name(id).await? 
            })
        }
        ApiV2::GetUserList => {
            // Handle v2 message
            Ok(ApiV2::UserListResponse { 
                users: get_all_users().await? 
            })
        }
        _ => Err(Error::UnexpectedMessage),
    }
}
```

### Pattern 2: Adding Optional Fields

**Safe**: Use `Option<T>` for new fields.

```rust
// Version 1
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UserInfo {
    pub id: u64,
    pub name: String,
}

// Version 2 - Add optional field
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UserInfoV2 {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,  // New optional field
}
```

**Serialization Compatibility**:

With JSON serialization, old clients will simply omit the new field:

```json
// V1 client sends:
{"id": 1, "name": "Alice"}

// V2 server receives and interprets as:
{"id": 1, "name": "Alice", "email": null}
```

### Pattern 3: Deprecating Messages

**Safe**: Mark messages as deprecated but keep them functional.

```rust
#[derive(Protocol, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ApiV3 {
    #[deprecated(note = "Use GetUserV2 instead")]
    GetUser { id: u64 },
    
    // New improved version
    GetUserV2 { 
        id: u64, 
        include_details: bool 
    },
    
    UserResponse { 
        id: u64, 
        name: String,
        #[serde(default)]
        details: Option<UserDetails>,
    },
}
```

**Implementation**:

```rust
async fn handle_message_v3(msg: ApiV3) -> Result<ApiV3, Error> {
    match msg {
        #[allow(deprecated)]
        ApiV3::GetUser { id } => {
            // Still handle old message, but convert to new format
            handle_get_user_v2(id, false).await
        }
        ApiV3::GetUserV2 { id, include_details } => {
            handle_get_user_v2(id, include_details).await
        }
        _ => Err(Error::UnexpectedMessage),
    }
}
```

### Pattern 4: Using Enums for Extensibility

**Safe**: Use enums with catch-all variants.

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum Action {
    Create { name: String },
    Update { id: u64, name: String },
    Delete { id: u64 },
    
    // Catch-all for unknown actions
    #[serde(other)]
    Unknown,
}
```

This allows old clients to receive new action types without crashing:

```rust
match action {
    Action::Create { name } => handle_create(name).await,
    Action::Update { id, name } => handle_update(id, name).await,
    Action::Delete { id } => handle_delete(id).await,
    Action::Unknown => {
        // Log and ignore unknown action
        warn!("Received unknown action type");
        Ok(())
    }
}
```

### Pattern 5: Protocol Adapters

**Safe**: Use adapter pattern to bridge versions.

```rust
pub struct ProtocolAdapter {
    version: u32,
}

impl ProtocolAdapter {
    pub fn v1_to_v2(&self, msg: ApiV1) -> ApiV2 {
        match msg {
            ApiV1::GetUser { id } => ApiV2::GetUser { id },
            ApiV1::UserResponse { id, name } => {
                ApiV2::UserResponse { id, name }
            }
        }
    }
    
    pub fn v2_to_v1(&self, msg: ApiV2) -> Option<ApiV1> {
        match msg {
            ApiV2::GetUser { id } => Some(ApiV1::GetUser { id }),
            ApiV2::UserResponse { id, name } => {
                Some(ApiV1::UserResponse { id, name })
            }
            // V2-only messages can't be converted
            ApiV2::GetUserList | ApiV2::UserListResponse { .. } => None,
        }
    }
}
```

## Best Practices

### 1. Plan for Evolution

Design your protocols with evolution in mind from the start:

```rust
// Good: Extensible design
#[derive(Protocol, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Api {
    Request { 
        id: u64, 
        #[serde(flatten)]
        payload: RequestPayload 
    },
    Response { 
        id: u64, 
        #[serde(flatten)]
        payload: ResponsePayload 
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum RequestPayload {
    GetUser { user_id: u64 },
    CreateUser { name: String },
    // Easy to add new request types
}
```

### 2. Use Feature Flags

Advertise capabilities during handshake:

```rust
let mut features = HashSet::new();
features.insert("api:v2".to_string());
features.insert("feature:streaming".to_string());
features.insert("feature:compression".to_string());

let config = EndpointConfig::default()
    .with_features(features);
```

### 3. Version Your Serialization

Include version information in your messages:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VersionedMessage<T> {
    pub version: u32,
    pub payload: T,
}

impl<T> VersionedMessage<T> {
    pub fn new(payload: T) -> Self {
        Self {
            version: 2,  // Current version
            payload,
        }
    }
}
```

### 4. Test Compatibility

Always test backward and forward compatibility:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_v1_to_v2_compatibility() {
        let v1_msg = ApiV1::GetUser { id: 42 };
        let adapter = ProtocolAdapter { version: 2 };
        let v2_msg = adapter.v1_to_v2(v1_msg);
        
        match v2_msg {
            ApiV2::GetUser { id } => assert_eq!(id, 42),
            _ => panic!("Conversion failed"),
        }
    }
    
    #[test]
    fn test_v2_to_v1_compatibility() {
        let v2_msg = ApiV2::GetUser { id: 42 };
        let adapter = ProtocolAdapter { version: 1 };
        let v1_msg = adapter.v2_to_v1(v2_msg);
        
        assert!(v1_msg.is_some());
    }
}
```

### 5. Document Breaking Changes

Maintain a CHANGELOG.md documenting all protocol changes:

```markdown
## [2.0.0] - 2026-02-06

### Breaking Changes
- Removed deprecated `GetUser` message (use `GetUserV2`)
- Changed `UserResponse.id` from `u32` to `u64`

### Added
- New `GetUserList` message for batch queries
- Optional `email` field in `UserInfo`

### Deprecated
- `GetUser` message (use `GetUserV2` instead)
```

## Examples

### Example 1: Multi-Version Server

```rust
use bdrpc::{Protocol, Channel, endpoint::Endpoint};
use std::collections::HashSet;

#[derive(Protocol, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ApiV1 {
    Ping,
    Pong,
}

#[derive(Protocol, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ApiV2 {
    Ping,
    Pong,
    Echo { message: String },
}

async fn run_multi_version_server() -> Result<(), Box<dyn std::error::Error>> {
    // Server supports both v1 and v2
    let mut features = HashSet::new();
    features.insert("api:v1".to_string());
    features.insert("api:v2".to_string());
    
    let endpoint = Endpoint::builder()
        .with_features(features)
        .build();
    
    // After connection, check negotiated version
    let negotiated = endpoint.negotiated_features();
    
    if negotiated.contains("api:v2") {
        // Use v2 protocol
        let channel: Channel<ApiV2> = endpoint.create_channel().await?;
        handle_v2_client(channel).await?;
    } else {
        // Fall back to v1 protocol
        let channel: Channel<ApiV1> = endpoint.create_channel().await?;
        handle_v1_client(channel).await?;
    }
    
    Ok(())
}

async fn handle_v1_client(channel: Channel<ApiV1>) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        match channel.recv().await? {
            ApiV1::Ping => {
                channel.send(ApiV1::Pong).await?;
            }
            _ => {}
        }
    }
}

async fn handle_v2_client(channel: Channel<ApiV2>) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        match channel.recv().await? {
            ApiV2::Ping => {
                channel.send(ApiV2::Pong).await?;
            }
            ApiV2::Echo { message } => {
                channel.send(ApiV2::Echo { message }).await?;
            }
            _ => {}
        }
    }
}
```

### Example 2: Gradual Migration

```rust
use bdrpc::{Protocol, Channel};

// Old protocol (v1)
#[derive(Protocol, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum OldApi {
    GetData { key: String },
    DataResponse { value: String },
}

// New protocol (v2) with additional features
#[derive(Protocol, Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum NewApi {
    GetData { key: String },
    DataResponse { 
        value: String,
        #[serde(default)]
        metadata: Option<Metadata>,
    },
    // New message type
    BatchGetData { keys: Vec<String> },
    BatchDataResponse { values: Vec<(String, String)> },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Metadata {
    pub timestamp: u64,
    pub version: u32,
}

// Migration helper
pub struct ApiMigration;

impl ApiMigration {
    pub fn upgrade_request(old: OldApi) -> NewApi {
        match old {
            OldApi::GetData { key } => NewApi::GetData { key },
            OldApi::DataResponse { value } => {
                NewApi::DataResponse { 
                    value, 
                    metadata: None 
                }
            }
        }
    }
    
    pub fn downgrade_response(new: NewApi) -> Option<OldApi> {
        match new {
            NewApi::GetData { key } => Some(OldApi::GetData { key }),
            NewApi::DataResponse { value, .. } => {
                Some(OldApi::DataResponse { value })
            }
            // New messages can't be downgraded
            NewApi::BatchGetData { .. } | 
            NewApi::BatchDataResponse { .. } => None,
        }
    }
}
```

## Testing Compatibility

### Integration Tests

Create integration tests that verify compatibility between versions:

```rust
#[cfg(test)]
mod compatibility_tests {
    use super::*;
    use bdrpc::transport::MemoryTransport;
    
    #[tokio::test]
    async fn test_v1_client_v2_server() {
        let (client_transport, server_transport) = MemoryTransport::pair();
        
        // V1 client
        let client_endpoint = Endpoint::builder()
            .with_features(["api:v1".to_string()].into())
            .build();
        
        // V2 server (supports both v1 and v2)
        let server_endpoint = Endpoint::builder()
            .with_features(["api:v1".to_string(), "api:v2".to_string()].into())
            .build();
        
        // Connect and verify communication works
        // ... test implementation
    }
    
    #[tokio::test]
    async fn test_v2_client_v1_server() {
        // Test that v2 client can fall back to v1 when talking to v1 server
        // ... test implementation
    }
}
```

### Serialization Tests

Test that messages can be serialized and deserialized across versions:

```rust
#[cfg(test)]
mod serialization_tests {
    use super::*;
    
    #[test]
    fn test_v1_deserializes_as_v2() {
        let v1_json = r#"{"GetData":{"key":"test"}}"#;
        
        // V2 should be able to deserialize V1 messages
        let v2_msg: NewApi = serde_json::from_str(v1_json).unwrap();
        
        match v2_msg {
            NewApi::GetData { key } => assert_eq!(key, "test"),
            _ => panic!("Wrong message type"),
        }
    }
    
    #[test]
    fn test_v2_with_optional_fields() {
        // V1 format (no metadata)
        let v1_json = r#"{"DataResponse":{"value":"data"}}"#;
        let v2_msg: NewApi = serde_json::from_str(v1_json).unwrap();
        
        match v2_msg {
            NewApi::DataResponse { value, metadata } => {
                assert_eq!(value, "data");
                assert!(metadata.is_none());
            }
            _ => panic!("Wrong message type"),
        }
        
        // V2 format (with metadata)
        let v2_json = r#"{"DataResponse":{"value":"data","metadata":{"timestamp":123,"version":2}}}"#;
        let v2_msg: NewApi = serde_json::from_str(v2_json).unwrap();
        
        match v2_msg {
            NewApi::DataResponse { value, metadata } => {
                assert_eq!(value, "data");
                assert!(metadata.is_some());
            }
            _ => panic!("Wrong message type"),
        }
    }
}
```

## Related Documentation

- [Architecture Guide](architecture-guide.md) - Understanding BDRPC's design
- [ADR-006: Protocol Evolution & Versioning](adr/ADR-006-protocol-evolution-versioning.md)
- [ADR-010: Dynamic Channel Negotiation](adr/ADR-010-dynamic-channel-negotiation.md)
- [Quick Start Guide](quick-start.md) - Getting started with BDRPC
- [Migration Guide](migration-guide.md) - Upgrading between BDRPC versions

## Summary

Protocol evolution is essential for long-lived systems. BDRPC provides:

1. **Version Negotiation**: Automatic capability discovery
2. **Flexible Serialization**: Multiple formats with extensibility
3. **Safe Patterns**: Guidelines for backward-compatible changes
4. **Testing Tools**: Verify compatibility across versions

**Key Takeaways**:
- Always use semantic versioning
- Add new features, don't modify existing ones
- Use `Option<T>` for new fields
- Test compatibility thoroughly
- Document all changes

By following these patterns and best practices, you can evolve your protocols safely while maintaining compatibility with existing deployments.
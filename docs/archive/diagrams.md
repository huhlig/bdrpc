# BDRPC Architecture and Sequence Diagrams

This document contains visual diagrams showing the architecture and common patterns in BDRPC.

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Channel Creation Flow](#channel-creation-flow)
3. [Sequence Diagrams](#sequence-diagrams)
   - [get_channels() Pattern](#get_channels-pattern)
   - [accept_channels() Pattern](#accept_channels-pattern)
   - [Bidirectional Communication](#bidirectional-communication)
   - [Connection Establishment](#connection-establishment)
   - [Error Recovery](#error-recovery)

---

## Architecture Overview

This diagram shows the high-level architecture of BDRPC and how components interact.

```mermaid
graph TB
    subgraph "Application Layer"
        APP[Application Code]
        PROTO[Protocol Definitions]
    end
    
    subgraph "BDRPC Core"
        EP[Endpoint]
        CM[Channel Manager]
        NEG[Channel Negotiator]
        
        subgraph "Channel Layer"
            CH[Channel]
            SEND[Sender]
            RECV[Receiver]
        end
        
        subgraph "Serialization"
            SER[Serializer]
            JSON[JSON]
            POST[Postcard]
            RKYV[rkyv]
        end
        
        subgraph "Transport Layer"
            TM[Transport Manager]
            TCP[TCP Transport]
            MEM[Memory Transport]
            TLS[TLS Transport]
        end
        
        subgraph "Observability"
            METRICS[Metrics]
            HEALTH[Health Checks]
            CORR[Correlation IDs]
        end
    end
    
    APP --> EP
    APP --> PROTO
    EP --> CM
    EP --> NEG
    EP --> TM
    CM --> CH
    CH --> SEND
    CH --> RECV
    EP --> SER
    SER --> JSON
    SER --> POST
    SER --> RKYV
    TM --> TCP
    TM --> MEM
    TM --> TLS
    EP --> METRICS
    EP --> HEALTH
    EP --> CORR
    
    style EP fill:#4a90e2,stroke:#2e5c8a,color:#fff
    style CM fill:#50c878,stroke:#2d7a4a,color:#fff
    style CH fill:#f39c12,stroke:#c87f0a,color:#fff
    style TM fill:#9b59b6,stroke:#6c3483,color:#fff
```

---

## Channel Creation Flow

This diagram illustrates the complete flow of creating a bidirectional channel using `get_channels()`.

```mermaid
graph TB
    START([Application calls get_channels])
    
    subgraph "Client Endpoint"
        C1[1. Generate Channel ID]
        C2[2. Create Local Channel]
        C3[3. Register in ChannelManager]
        C4[4. Send Channel Request]
        C5[5. Wait for Response]
        C6{Response OK?}
        C7[7. Return Sender/Receiver]
        C8[8. Cleanup Local Channel]
    end
    
    subgraph "Network"
        NET[Channel Request Message]
        RESP[Channel Response Message]
    end
    
    subgraph "Server Endpoint"
        S1[1. Receive Request]
        S2[2. Validate with Negotiator]
        S3{Protocol Allowed?}
        S4[4. Send Acceptance]
        S5[5. Send Rejection]
    end
    
    START --> C1
    C1 --> C2
    C2 --> C3
    C3 --> C4
    C4 --> NET
    NET --> S1
    S1 --> S2
    S2 --> S3
    S3 -->|Yes| S4
    S3 -->|No| S5
    S4 --> RESP
    S5 --> RESP
    RESP --> C5
    C5 --> C6
    C6 -->|Success| C7
    C6 -->|Failure| C8
    C7 --> END([Channels Ready])
    C8 --> ERROR([Error Returned])
    
    style C2 fill:#50c878,stroke:#2d7a4a,color:#fff
    style C3 fill:#50c878,stroke:#2d7a4a,color:#fff
    style S2 fill:#f39c12,stroke:#c87f0a,color:#fff
    style S4 fill:#50c878,stroke:#2d7a4a,color:#fff
    style S5 fill:#e74c3c,stroke:#c0392b,color:#fff
    style C7 fill:#50c878,stroke:#2d7a4a,color:#fff
    style C8 fill:#e74c3c,stroke:#c0392b,color:#fff
```

---

## Sequence Diagrams

### get_channels() Pattern

This sequence diagram shows the client-initiated channel creation pattern.

```mermaid
sequenceDiagram
    participant App as Application
    participant Client as Client Endpoint
    participant CM as Channel Manager
    participant Net as Network
    participant Server as Server Endpoint
    participant Neg as Negotiator
    
    App->>Client: get_channels::<MyProtocol>()
    
    Note over Client: Generate Channel ID
    Client->>CM: create_channel(id, protocol)
    CM-->>Client: Channel created locally
    
    Client->>Net: Send ChannelRequest
    Note over Net: {channel_id, protocol_name}
    
    Net->>Server: Receive ChannelRequest
    Server->>Neg: validate_request(protocol)
    
    alt Protocol Allowed
        Neg-->>Server: Validation OK
        Server->>Net: Send ChannelAccepted
        Net->>Client: Receive ChannelAccepted
        Client->>CM: get_channel(id)
        CM-->>Client: (Sender, Receiver)
        Client-->>App: Ok((sender, receiver))
    else Protocol Not Allowed
        Neg-->>Server: Validation Failed
        Server->>Net: Send ChannelRejected
        Net->>Client: Receive ChannelRejected
        Client->>CM: remove_channel(id)
        CM-->>Client: Channel removed
        Client-->>App: Err(ChannelRequestRejected)
    end
    
    Note over App,Server: Channel ready for bidirectional communication
```

### accept_channels() Pattern

This sequence diagram shows the server-side manual acceptance pattern.

```mermaid
sequenceDiagram
    participant App as Application
    participant Server as Server Endpoint
    participant Queue as Connection Queue
    participant BG as Background Task
    participant Net as Network
    participant Client as Client Endpoint
    
    App->>Server: listen_manual(addr)
    Server->>BG: Spawn acceptance task
    Server-->>App: Listener
    
    Note over BG: Continuously accept connections
    
    Client->>Net: Connect to server
    Net->>BG: New connection
    BG->>Queue: Enqueue PendingConnection
    
    App->>Server: accept_channels::<Protocol>()
    Server->>Queue: Dequeue connection
    Queue-->>Server: PendingConnection
    
    Server->>Net: Perform handshake
    Net->>Client: Handshake exchange
    Client->>Net: Handshake response
    Net->>Server: Handshake complete
    
    Note over Server: Create bidirectional channel
    Server->>Server: create_channel(protocol)
    Server-->>App: Ok((sender, receiver))
    
    Note over App,Client: Channel ready for communication
```

### Bidirectional Communication

This sequence diagram shows typical bidirectional message exchange.

```mermaid
sequenceDiagram
    participant A as Endpoint A
    participant ACh as A's Channel
    participant Net as Network
    participant BCh as B's Channel
    participant B as Endpoint B
    
    Note over A,B: Both endpoints have bidirectional channels
    
    A->>ACh: sender.send(request)
    ACh->>Net: Serialize & transmit
    Net->>BCh: Receive & deserialize
    BCh->>B: receiver.recv() → request
    
    Note over B: Process request
    
    B->>BCh: sender.send(response)
    BCh->>Net: Serialize & transmit
    Net->>ACh: Receive & deserialize
    ACh->>A: receiver.recv() → response
    
    Note over A: Process response
    
    A->>ACh: sender.send(next_request)
    ACh->>Net: Serialize & transmit
    Net->>BCh: Receive & deserialize
    BCh->>B: receiver.recv() → next_request
    
    Note over A,B: Communication continues...
```

### Connection Establishment

This sequence diagram shows the complete connection establishment flow.

```mermaid
sequenceDiagram
    participant Client as Client Endpoint
    participant TCP as TCP Transport
    participant Net as Network
    participant Server as Server Endpoint
    participant TM as Transport Manager
    
    Client->>TCP: connect(addr)
    TCP->>Net: TCP SYN
    Net->>Server: TCP SYN
    Server->>Net: TCP SYN-ACK
    Net->>TCP: TCP SYN-ACK
    TCP->>Net: TCP ACK
    Net->>Server: TCP ACK
    
    Note over Client,Server: TCP connection established
    
    Client->>Net: Send Handshake
    Note over Net: {version, capabilities}
    Net->>Server: Receive Handshake
    
    Server->>Server: Validate handshake
    Server->>Net: Send Handshake Response
    Note over Net: {version, accepted_capabilities}
    Net->>Client: Receive Response
    
    Client->>Client: Validate response
    
    Note over Client,Server: Connection ready
    
    Client->>TM: Spawn read/write tasks
    Server->>TM: Spawn read/write tasks
    
    Note over Client,Server: Background tasks handle I/O
```

### Error Recovery

This sequence diagram shows error detection and recovery flow.

```mermaid
sequenceDiagram
    participant App as Application
    participant EP as Endpoint
    participant Recon as Reconnection Strategy
    participant Net as Network
    participant Remote as Remote Endpoint
    
    App->>EP: send_message()
    EP->>Net: Transmit
    
    Note over Net: Connection lost!
    
    Net-->>EP: Error: ConnectionLost
    EP->>EP: Detect failure
    
    EP->>Recon: should_reconnect()
    Recon-->>EP: Yes (with backoff)
    
    Note over EP: Wait for backoff delay
    
    EP->>Net: Attempt reconnect
    
    alt Reconnection Successful
        Net->>Remote: Connect
        Remote-->>Net: Accept
        Net-->>EP: Connected
        EP->>EP: Restore channels
        EP-->>App: Connection restored
        
        Note over App: Retry failed operation
        App->>EP: send_message()
        EP->>Net: Transmit
        Net->>Remote: Deliver
    else Reconnection Failed
        Net-->>EP: Error: ConnectionRefused
        EP->>Recon: should_reconnect()
        
        alt Retry Limit Not Reached
            Recon-->>EP: Yes (increased backoff)
            Note over EP: Wait longer, retry again
        else Retry Limit Reached
            Recon-->>EP: No (give up)
            EP-->>App: Error: MaxRetriesExceeded
        end
    end
```

---

## Component Interaction Details

### Channel Manager Responsibilities

```mermaid
graph LR
    subgraph "Channel Manager"
        CREATE[Create Channel]
        STORE[Store Channel]
        LOOKUP[Lookup Channel]
        REMOVE[Remove Channel]
        CLEANUP[Cleanup Closed]
    end
    
    EP[Endpoint] --> CREATE
    CREATE --> STORE
    EP --> LOOKUP
    LOOKUP --> STORE
    EP --> REMOVE
    REMOVE --> STORE
    BG[Background Task] --> CLEANUP
    CLEANUP --> STORE
    
    style CREATE fill:#50c878,stroke:#2d7a4a,color:#fff
    style LOOKUP fill:#4a90e2,stroke:#2e5c8a,color:#fff
    style REMOVE fill:#e74c3c,stroke:#c0392b,color:#fff
```

### Negotiator Decision Flow

```mermaid
graph TB
    START([Channel Request Received])
    CHECK1{Protocol Registered?}
    CHECK2{Protocol Allowed?}
    CHECK3{Direction Compatible?}
    CHECK4{Custom Validation?}
    ACCEPT[Accept Request]
    REJECT[Reject Request]
    
    START --> CHECK1
    CHECK1 -->|No| REJECT
    CHECK1 -->|Yes| CHECK2
    CHECK2 -->|No| REJECT
    CHECK2 -->|Yes| CHECK3
    CHECK3 -->|No| REJECT
    CHECK3 -->|Yes| CHECK4
    CHECK4 -->|Fail| REJECT
    CHECK4 -->|Pass| ACCEPT
    
    style ACCEPT fill:#50c878,stroke:#2d7a4a,color:#fff
    style REJECT fill:#e74c3c,stroke:#c0392b,color:#fff
```

### Transport Layer State Machine

```mermaid
stateDiagram-v2
    [*] --> Disconnected
    Disconnected --> Connecting: connect()
    Connecting --> Connected: Success
    Connecting --> Disconnected: Failure
    Connected --> Disconnected: close() / error
    Connected --> Reconnecting: Connection Lost
    Reconnecting --> Connected: Success
    Reconnecting --> Disconnected: Max Retries
    Disconnected --> [*]: shutdown()
    
    note right of Connected
        Read/Write tasks active
        Messages flowing
    end note
    
    note right of Reconnecting
        Exponential backoff
        Retry with delay
    end note
```

---

## Usage Examples

### Creating Channels (Client Side)

```rust
// Using get_channels() - recommended pattern
let (sender, receiver) = endpoint
    .get_channels::<MyProtocol>()
    .await?;

// Now you can send and receive
sender.send(request).await?;
let response = receiver.recv().await?;
```

### Accepting Channels (Server Side)

```rust
// Using accept_channels() - recommended pattern
let listener = endpoint.listen_manual("127.0.0.1:8080").await?;

loop {
    let (sender, receiver) = listener
        .accept_channels::<MyProtocol>()
        .await?;
    
    // Handle connection in separate task
    tokio::spawn(async move {
        handle_client(sender, receiver).await
    });
}
```

### Bidirectional Communication

```rust
// Both endpoints can send and receive simultaneously
tokio::select! {
    msg = receiver.recv() => {
        // Handle incoming message
        process_request(msg?).await?;
    }
    _ = sender.send(outgoing) => {
        // Message sent successfully
    }
}
```

---

## Best Practices

1. **Always use `get_channels()` for client-side channel creation** - it handles all the complexity
2. **Use `accept_channels()` for simple server patterns** - reduces boilerplate significantly
3. **Register protocols before creating channels** - automatic allowlisting works seamlessly
4. **Handle errors appropriately** - check for timeout, rejection, and connection failures
5. **Use bidirectional channels when both sides need to communicate** - more efficient than two unidirectional channels
6. **Implement proper cleanup** - channels are automatically cleaned up when dropped
7. **Monitor connection health** - use observability features for production deployments

---

## Related Documentation

- [Architecture Guide](architecture-guide.md) - Detailed architectural decisions
- [Best Practices](best-practices.md) - Comprehensive best practices guide
- [Quick Start](quick-start.md) - Getting started tutorial
- [Troubleshooting Guide](troubleshooting-guide.md) - Common issues and solutions
- [ADR-010: Dynamic Channel Negotiation](ADR/ADR-010-dynamic-channel-negotiation.md) - Channel negotiation design
//! Stress tests for the Enhanced Transport Manager
//!
//! These tests validate the transport manager's behavior under high load,
//! focusing on the TransportManager API itself.

use bdrpc::{
    transport::{TransportManager, TransportConfig, TransportType},
    reconnection::ExponentialBackoff,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

/// Test adding and removing many listeners
#[tokio::test]
async fn stress_test_listener_management() {
    let manager = TransportManager::new();
    
    // Add 100 listeners
    for i in 0..100 {
        manager
            .add_listener(
                format!("listener-{}", i),
                TransportConfig::new(TransportType::Tcp, format!("127.0.0.1:{}", 20000 + i)),
            )
            .await
            .expect("Failed to add listener");
    }
    
    // Verify count
    assert_eq!(manager.listener_count().await, 100);
    
    // List all listeners
    let listeners = manager.list_listeners().await;
    assert_eq!(listeners.len(), 100);
    
    // Remove half
    for i in 0..50 {
        manager
            .remove_listener(&format!("listener-{}", i))
            .await
            .expect("Failed to remove listener");
    }
    
    // Verify count after removal
    assert_eq!(manager.listener_count().await, 50);
}

/// Test adding and removing many callers
#[tokio::test]
async fn stress_test_caller_management() {
    let manager = TransportManager::new();
    
    // Add 100 callers with reconnection strategies
    for i in 0..100 {
        let config = TransportConfig::new(TransportType::Tcp, format!("127.0.0.1:{}", 21000 + i))
            .with_reconnection_strategy(Arc::new(ExponentialBackoff::default()));
            
        manager
            .add_caller(format!("caller-{}", i), config)
            .await
            .expect("Failed to add caller");
    }
    
    // Verify count
    assert_eq!(manager.caller_count().await, 100);
    
    // List all callers
    let callers = manager.list_callers().await;
    assert_eq!(callers.len(), 100);
    
    // Remove half
    for i in 0..50 {
        manager
            .remove_caller(&format!("caller-{}", i))
            .await
            .expect("Failed to remove caller");
    }
    
    // Verify count after removal
    assert_eq!(manager.caller_count().await, 50);
}

/// Test concurrent listener operations
#[tokio::test]
async fn stress_test_concurrent_listener_operations() {
    let manager = Arc::new(TransportManager::new());
    let mut tasks = Vec::new();
    
    // Spawn 10 tasks, each adding 10 listeners
    for task_id in 0..10 {
        let mgr = manager.clone();
        let task = tokio::spawn(async move {
            for i in 0..10 {
                mgr.add_listener(
                    format!("listener-{}-{}", task_id, i),
                    TransportConfig::new(TransportType::Tcp, format!("127.0.0.1:{}", 22000 + task_id * 10 + i)),
                )
                .await
                .expect("Failed to add listener");
            }
        });
        tasks.push(task);
    }
    
    // Wait for all tasks
    for task in tasks {
        task.await.expect("Task failed");
    }
    
    // Verify total count
    assert_eq!(manager.listener_count().await, 100);
}

/// Test concurrent caller operations
#[tokio::test]
async fn stress_test_concurrent_caller_operations() {
    let manager = Arc::new(TransportManager::new());
    let mut tasks = Vec::new();
    
    // Spawn 10 tasks, each adding 10 callers
    for task_id in 0..10 {
        let mgr = manager.clone();
        let task = tokio::spawn(async move {
            for i in 0..10 {
                let config = TransportConfig::new(
                    TransportType::Tcp,
                    format!("127.0.0.1:{}", 23000 + task_id * 10 + i)
                ).with_reconnection_strategy(Arc::new(ExponentialBackoff::default()));
                
                mgr.add_caller(format!("caller-{}-{}", task_id, i), config)
                    .await
                    .expect("Failed to add caller");
            }
        });
        tasks.push(task);
    }
    
    // Wait for all tasks
    for task in tasks {
        task.await.expect("Task failed");
    }
    
    // Verify total count
    assert_eq!(manager.caller_count().await, 100);
}

/// Test enable/disable operations under load
#[tokio::test]
async fn stress_test_enable_disable_operations() {
    let manager = TransportManager::new();
    
    // Add listeners
    for i in 0..20 {
        manager
            .add_listener(
                format!("listener-{}", i),
                TransportConfig::new(TransportType::Tcp, format!("127.0.0.1:{}", 24000 + i)),
            )
            .await
            .expect("Failed to add listener");
    }
    
    // Rapidly enable/disable transports
    for _ in 0..10 {
        for i in 0..20 {
            manager
                .disable_transport(&format!("listener-{}", i))
                .await
                .expect("Failed to disable");
            
            sleep(Duration::from_millis(1)).await;
            
            manager
                .enable_transport(&format!("listener-{}", i))
                .await
                .expect("Failed to enable");
        }
    }
    
    // Verify all still exist
    assert_eq!(manager.listener_count().await, 20);
}

/// Test mixed operations (add/remove/enable/disable)
#[tokio::test]
async fn stress_test_mixed_operations() {
    let manager = Arc::new(TransportManager::new());
    let mut tasks = Vec::new();
    
    // Task 1: Add listeners
    let mgr1 = manager.clone();
    tasks.push(tokio::spawn(async move {
        for i in 0..30 {
            mgr1.add_listener(
                format!("listener-{}", i),
                TransportConfig::new(TransportType::Tcp, format!("127.0.0.1:{}", 25000 + i)),
            )
            .await
            .expect("Failed to add listener");
            sleep(Duration::from_millis(1)).await;
        }
    }));
    
    // Task 2: Add callers
    let mgr2 = manager.clone();
    tasks.push(tokio::spawn(async move {
        for i in 0..30 {
            let config = TransportConfig::new(
                TransportType::Tcp,
                format!("127.0.0.1:{}", 26000 + i)
            ).with_reconnection_strategy(Arc::new(ExponentialBackoff::default()));
            
            mgr2.add_caller(format!("caller-{}", i), config)
                .await
                .expect("Failed to add caller");
            sleep(Duration::from_millis(1)).await;
        }
    }));
    
    // Task 3: Enable/disable (after some delay)
    let mgr3 = manager.clone();
    tasks.push(tokio::spawn(async move {
        sleep(Duration::from_millis(50)).await;
        for i in 0..10 {
            let _ = mgr3.disable_transport(&format!("listener-{}", i)).await;
            sleep(Duration::from_millis(2)).await;
            let _ = mgr3.enable_transport(&format!("listener-{}", i)).await;
        }
    }));
    
    // Wait for all tasks
    for task in tasks {
        task.await.expect("Task failed");
    }
    
    // Verify final state
    let listener_count = manager.listener_count().await;
    let caller_count = manager.caller_count().await;
    
    assert_eq!(listener_count, 30);
    assert_eq!(caller_count, 30);
}

/// Test metadata tracking under load
#[tokio::test]
async fn stress_test_metadata_tracking() {
    let manager = TransportManager::new();
    
    // Add many transports with metadata
    for i in 0..100 {
        let config = TransportConfig::new(TransportType::Tcp, format!("127.0.0.1:{}", 27000 + i))
            .with_metadata("region", "us-west")
            .with_metadata("priority", if i % 2 == 0 { "high" } else { "low" })
            .with_metadata("index", i.to_string());
        
        manager
            .add_listener(format!("listener-{}", i), config)
            .await
            .expect("Failed to add listener");
    }
    
    // Verify all were added
    assert_eq!(manager.listener_count().await, 100);
    
    // List and verify
    let listeners = manager.list_listeners().await;
    assert_eq!(listeners.len(), 100);
    
    // Remove and re-add some
    for i in 0..20 {
        manager
            .remove_listener(&format!("listener-{}", i))
            .await
            .expect("Failed to remove");
        
        let config = TransportConfig::new(TransportType::Tcp, format!("127.0.0.1:{}", 27000 + i))
            .with_metadata("region", "us-east")
            .with_metadata("priority", "medium");
        
        manager
            .add_listener(format!("listener-{}", i), config)
            .await
            .expect("Failed to re-add listener");
    }
    
    // Verify count unchanged
    assert_eq!(manager.listener_count().await, 100);
}

/// Test rapid add/remove cycles
#[tokio::test]
async fn stress_test_rapid_add_remove_cycles() {
    let manager = TransportManager::new();
    
    for cycle in 0..50 {
        // Add 10 listeners
        for i in 0..10 {
            manager
                .add_listener(
                    format!("listener-{}-{}", cycle, i),
                    TransportConfig::new(TransportType::Tcp, format!("127.0.0.1:{}", 28000 + i)),
                )
                .await
                .expect("Failed to add listener");
        }
        
        // Verify they were added
        assert_eq!(manager.listener_count().await, 10);
        
        // Remove all
        for i in 0..10 {
            manager
                .remove_listener(&format!("listener-{}-{}", cycle, i))
                .await
                .expect("Failed to remove listener");
        }
        
        // Verify they were removed
        assert_eq!(manager.listener_count().await, 0);
    }
}

/// Test memory stability with many operations
#[tokio::test]
async fn stress_test_memory_stability() {
    let manager = TransportManager::new();
    
    for iteration in 0..100 {
        // Add 20 transports
        for i in 0..20 {
            manager
                .add_listener(
                    format!("listener-{}-{}", iteration, i),
                    TransportConfig::new(TransportType::Tcp, format!("127.0.0.1:{}", 29000 + i)),
                )
                .await
                .expect("Failed to add listener");
        }
        
        // Do some operations
        let _ = manager.list_listeners().await;
        let _ = manager.listener_count().await;
        
        // Remove all
        for i in 0..20 {
            manager
                .remove_listener(&format!("listener-{}-{}", iteration, i))
                .await
                .expect("Failed to remove listener");
        }
        
        // Small delay
        sleep(Duration::from_millis(1)).await;
    }
    
    // Verify clean state
    assert_eq!(manager.listener_count().await, 0);
    assert_eq!(manager.caller_count().await, 0);
}

// Made with Bob

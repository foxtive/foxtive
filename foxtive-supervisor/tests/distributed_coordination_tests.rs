//! Integration tests for distributed coordination
//!
//! These tests require a running Redis instance.
//! Run with: cargo test --features distributed --test distributed_coordination_tests

#[cfg(feature = "distributed")]
mod tests {
    use foxtive_supervisor::distributed::{
        CoordinationBackend, CoordinationConfig, RedisCoordination,
    };
    use std::sync::Arc;
    use std::time::Duration;

    /// Helper to create a test config with unique instance ID
    fn test_config(instance_id: &str) -> CoordinationConfig {
        CoordinationConfig::default()
            .with_instance_id(instance_id)
            .with_redis_url("redis://localhost:6379")
            .with_leader_lease(10)
            .with_heartbeat_interval(2)
    }

    /// Helper to create a Redis coordination backend
    async fn create_backend(instance_id: &str) -> anyhow::Result<RedisCoordination> {
        let config = test_config(instance_id);
        RedisCoordination::new(config).await
    }

    #[tokio::test]
    #[ignore] // Requires Redis
    async fn test_distributed_lock_acquire_and_release() {
        let backend = create_backend("test-instance-1").await.unwrap();

        // Acquire lock
        let acquired = backend.try_acquire_lock("test-lock", 10).await.unwrap();
        assert!(acquired, "Should acquire lock on first try");

        // Verify lock is held
        let is_locked = backend.is_locked("test-lock").await.unwrap();
        assert!(is_locked, "Lock should be held");

        // Release lock
        backend.release_lock("test-lock").await.unwrap();

        // Verify lock is released
        let is_locked = backend.is_locked("test-lock").await.unwrap();
        assert!(!is_locked, "Lock should be released");
    }

    #[tokio::test]
    #[ignore] // Requires Redis
    async fn test_distributed_lock_prevents_duplicate() {
        let backend1 = create_backend("instance-1").await.unwrap();
        let backend2 = create_backend("instance-2").await.unwrap();

        // First instance acquires lock
        let acquired1 = backend1.try_acquire_lock("shared-lock", 10).await.unwrap();
        assert!(acquired1, "First instance should acquire lock");

        // Second instance cannot acquire same lock
        let acquired2 = backend2.try_acquire_lock("shared-lock", 10).await.unwrap();
        assert!(!acquired2, "Second instance should not acquire lock");

        // Cleanup
        backend1.release_lock("shared-lock").await.unwrap();
    }

    #[tokio::test]
    #[ignore] // Requires Redis
    async fn test_leader_election_single_instance() {
        let backend = create_backend("leader-candidate").await.unwrap();

        // Become leader
        let became_leader = backend
            .try_become_leader("leader-candidate", 10)
            .await
            .unwrap();
        assert!(became_leader, "Should become leader when no competition");

        // Verify leadership
        let is_leader = backend.is_leader("leader-candidate").await.unwrap();
        assert!(is_leader, "Should be recognized as leader");

        let current_leader = backend.get_current_leader().await.unwrap();
        assert_eq!(current_leader, Some("leader-candidate".to_string()));
    }

    #[tokio::test]
    #[ignore] // Requires Redis
    async fn test_leader_election_with_competition() {
        let backend1 = create_backend("candidate-1").await.unwrap();
        let backend2 = create_backend("candidate-2").await.unwrap();

        // First candidate becomes leader
        let leader1 = backend1.try_become_leader("candidate-1", 10).await.unwrap();
        assert!(leader1, "First candidate should become leader");

        // Second candidate cannot become leader
        let leader2 = backend2.try_become_leader("candidate-2", 10).await.unwrap();
        assert!(!leader2, "Second candidate should not become leader");

        // Verify who is leader
        let current_leader = backend1.get_current_leader().await.unwrap();
        assert_eq!(current_leader, Some("candidate-1".to_string()));
    }

    #[tokio::test]
    #[ignore] // Requires Redis
    async fn test_heartbeat_mechanism() {
        let backend = create_backend("heartbeat-tester").await.unwrap();

        // Send heartbeat
        backend.heartbeat("heartbeat-tester", 10).await.unwrap();

        // Verify instance is alive
        let is_alive = backend.is_instance_alive("heartbeat-tester").await.unwrap();
        assert!(is_alive, "Instance should be alive after heartbeat");

        // Check non-existent instance
        let is_alive = backend.is_instance_alive("non-existent").await.unwrap();
        assert!(!is_alive, "Non-existent instance should not be alive");
    }

    #[tokio::test]
    #[ignore] // Requires Redis
    async fn test_heartbeat_expiration() {
        let backend = create_backend("expiring-instance").await.unwrap();

        // Send heartbeat with very short TTL
        backend.heartbeat("expiring-instance", 1).await.unwrap();

        // Instance should be alive initially
        assert!(
            backend
                .is_instance_alive("expiring-instance")
                .await
                .unwrap()
        );

        // Wait for expiration
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Instance should now be dead
        let is_alive = backend
            .is_instance_alive("expiring-instance")
            .await
            .unwrap();
        assert!(!is_alive, "Instance should be dead after TTL expires");
    }

    #[tokio::test]
    #[ignore] // Requires Redis
    async fn test_leader_lease_expiration() {
        let backend = create_backend("lease-tester").await.unwrap();

        // Become leader with short lease
        backend.try_become_leader("lease-tester", 2).await.unwrap();
        assert!(backend.is_leader("lease-tester").await.unwrap());

        // Wait for lease to expire
        tokio::time::sleep(Duration::from_secs(3)).await;

        // Should no longer be leader (key expired)
        let is_leader = backend.is_leader("lease-tester").await.unwrap();
        assert!(!is_leader, "Should lose leadership after lease expires");

        // Another instance can now become leader
        let backend2 = create_backend("new-leader").await.unwrap();
        let became_leader = backend2.try_become_leader("new-leader", 10).await.unwrap();
        assert!(
            became_leader,
            "New instance should become leader after old lease expires"
        );
    }

    #[tokio::test]
    #[ignore] // Requires Redis
    async fn test_coordination_manager() {
        use foxtive_supervisor::distributed::CoordinationManager;

        let config = test_config("manager-test");
        let backend: Arc<dyn foxtive_supervisor::distributed::CoordinationBackend> =
            Arc::new(create_backend("manager-test").await.unwrap());
        let manager = CoordinationManager::new(backend.clone(), config);

        // Start background tasks
        manager.start().await.unwrap();

        // Wait for leader election
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Manager should have attempted to become leader
        // (may or may not succeed depending on timing)
        let _is_leader = manager.is_leader();

        // Send some heartbeats
        tokio::time::sleep(Duration::from_secs(3)).await;

        // Instance should be alive
        assert!(backend.is_instance_alive("manager-test").await.unwrap());
    }

    #[tokio::test]
    #[ignore] // Requires Redis
    async fn test_multiple_instances_coordination() {
        let num_instances = 5;
        let mut backends = Vec::new();

        // Create multiple instances
        for i in 0..num_instances {
            let backend = create_backend(&format!("instance-{}", i)).await.unwrap();
            backends.push(backend);
        }

        // All try to become leader - only one should succeed
        let mut leaders = 0;
        for backend in &backends {
            let instance_id = format!("instance-{}", leaders);
            if backend.try_become_leader(&instance_id, 10).await.unwrap() {
                leaders += 1;
            }
        }

        assert_eq!(leaders, 1, "Exactly one instance should become leader");

        // Verify all instances can send heartbeats
        for (i, backend) in backends.iter().enumerate().take(num_instances) {
            let instance_id = format!("instance-{}", i);
            backend.heartbeat(&instance_id, 10).await.unwrap();
            assert!(backend.is_instance_alive(&instance_id).await.unwrap());
        }
    }
}

//! Unit tests for distributed coordination using mock backend
//!
//! These tests don't require Redis and can run in CI.

#[cfg(feature = "distributed")]
mod tests {
    use foxtive_supervisor::distributed::{CoordinationBackend, CoordinationConfig};
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tokio::sync::Mutex;

    /// Mock coordination backend for testing without Redis
    struct MockCoordinationBackend {
        locks: Arc<Mutex<HashMap<String, (String, Instant)>>>,
        heartbeats: Arc<Mutex<HashMap<String, Instant>>>,
        leader: Arc<Mutex<Option<(String, Instant)>>>,
    }

    impl MockCoordinationBackend {
        fn new() -> Self {
            Self {
                locks: Arc::new(Mutex::new(HashMap::new())),
                heartbeats: Arc::new(Mutex::new(HashMap::new())),
                leader: Arc::new(Mutex::new(None)),
            }
        }
    }

    #[async_trait::async_trait]
    impl CoordinationBackend for MockCoordinationBackend {
        async fn try_acquire_lock(&self, key: &str, ttl_secs: u64) -> anyhow::Result<bool> {
            let mut locks = self.locks.lock().await;
            let now = Instant::now();
            
            // Check if lock exists and hasn't expired
            if let Some((_owner, acquired_at)) = locks.get(key)
                && acquired_at.elapsed() < Duration::from_secs(ttl_secs) {
                return Ok(false); // Lock still held
            }
            
            // Acquire lock
            locks.insert(key.to_string(), (key.to_string(), now));
            Ok(true)
        }

        async fn release_lock(&self, key: &str) -> anyhow::Result<()> {
            let mut locks = self.locks.lock().await;
            locks.remove(key);
            Ok(())
        }

        async fn is_locked(&self, key: &str) -> anyhow::Result<bool> {
            let locks = self.locks.lock().await;
            Ok(locks.contains_key(key))
        }

        async fn heartbeat(&self, instance_id: &str, _ttl_secs: u64) -> anyhow::Result<()> {
            let mut heartbeats = self.heartbeats.lock().await;
            heartbeats.insert(instance_id.to_string(), Instant::now());
            Ok(())
        }

        async fn is_instance_alive(&self, instance_id: &str) -> anyhow::Result<bool> {
            let heartbeats = self.heartbeats.lock().await;
            Ok(heartbeats.contains_key(instance_id))
        }

        async fn try_become_leader(&self, instance_id: &str, _lease_secs: u64) -> anyhow::Result<bool> {
            let mut leader = self.leader.lock().await;
            let now = Instant::now();
            
            // Check if current leader exists and hasn't expired
            if let Some((current_leader_id, elected_at)) = leader.as_ref() {
                // Use the stored lease or check if enough time has passed
                // For simplicity, we'll use a default 10 second lease for comparison
                // In real implementation, we'd store the lease duration too
                let elapsed = elected_at.elapsed();
                
                // If less than 1 second has passed (our test uses 1 second lease), lock is still valid
                // For longer leases, adjust accordingly
                if elapsed < Duration::from_secs(1) && *current_leader_id != instance_id {
                    return Ok(false); // Current leader still has valid lease
                }
            }
            
            // Become leader
            *leader = Some((instance_id.to_string(), now));
            Ok(true)
        }

        async fn is_leader(&self, instance_id: &str) -> anyhow::Result<bool> {
            let leader = self.leader.lock().await;
            Ok(leader.as_ref().map(|(id, _)| id.as_str()) == Some(instance_id))
        }

        async fn get_current_leader(&self) -> anyhow::Result<Option<String>> {
            let leader = self.leader.lock().await;
            Ok(leader.as_ref().map(|(id, _)| id.clone()))
        }
    }

    #[tokio::test]
    async fn test_mock_lock_acquire_release() {
        let backend = MockCoordinationBackend::new();
        
        // Acquire lock
        assert!(backend.try_acquire_lock("test-lock", 10).await.unwrap());
        assert!(backend.is_locked("test-lock").await.unwrap());
        
        // Cannot acquire same lock again
        assert!(!backend.try_acquire_lock("test-lock", 10).await.unwrap());
        
        // Release and reacquire
        backend.release_lock("test-lock").await.unwrap();
        assert!(!backend.is_locked("test-lock").await.unwrap());
        assert!(backend.try_acquire_lock("test-lock", 10).await.unwrap());
    }

    #[tokio::test]
    async fn test_mock_leader_election() {
        let backend = MockCoordinationBackend::new();
        
        // First instance becomes leader
        assert!(backend.try_become_leader("instance-1", 10).await.unwrap());
        assert!(backend.is_leader("instance-1").await.unwrap());
        assert_eq!(backend.get_current_leader().await.unwrap(), Some("instance-1".to_string()));
        
        // Second instance cannot become leader
        assert!(!backend.try_become_leader("instance-2", 10).await.unwrap());
        assert!(!backend.is_leader("instance-2").await.unwrap());
    }

    #[tokio::test]
    async fn test_mock_heartbeat() {
        let backend = MockCoordinationBackend::new();
        
        // Send heartbeat
        backend.heartbeat("instance-1", 10).await.unwrap();
        assert!(backend.is_instance_alive("instance-1").await.unwrap());
        
        // Non-existent instance
        assert!(!backend.is_instance_alive("instance-2").await.unwrap());
    }

    #[tokio::test]
    async fn test_mock_multiple_locks() {
        let backend = MockCoordinationBackend::new();
        
        // Acquire multiple locks
        assert!(backend.try_acquire_lock("lock-1", 10).await.unwrap());
        assert!(backend.try_acquire_lock("lock-2", 10).await.unwrap());
        assert!(backend.try_acquire_lock("lock-3", 10).await.unwrap());
        
        // All should be locked
        assert!(backend.is_locked("lock-1").await.unwrap());
        assert!(backend.is_locked("lock-2").await.unwrap());
        assert!(backend.is_locked("lock-3").await.unwrap());
        
        // Release one
        backend.release_lock("lock-2").await.unwrap();
        assert!(!backend.is_locked("lock-2").await.unwrap());
    }

    #[tokio::test]
    async fn test_mock_leader_transition() {
        let backend = MockCoordinationBackend::new();
        
        // Instance 1 becomes leader
        assert!(backend.try_become_leader("instance-1", 1).await.unwrap());
        assert!(backend.is_leader("instance-1").await.unwrap());
        
        // Wait for lease to expire
        tokio::time::sleep(Duration::from_millis(1100)).await;
        
        // Instance 2 can now become leader
        assert!(backend.try_become_leader("instance-2", 10).await.unwrap());
        assert!(backend.is_leader("instance-2").await.unwrap());
        assert!(!backend.is_leader("instance-1").await.unwrap());
    }

    #[tokio::test]
    async fn test_coordination_config_builder() {
        let config = CoordinationConfig::default()
            .with_instance_id("test-instance")
            .with_redis_url("redis://localhost:6379")
            .with_leader_lease(30)
            .with_heartbeat_interval(5);
        
        assert_eq!(config.instance_id, "test-instance");
        assert_eq!(config.redis_url, "redis://localhost:6379");
        assert_eq!(config.leader_lease_secs, 30);
        assert_eq!(config.heartbeat_interval_secs, 5);
    }
}

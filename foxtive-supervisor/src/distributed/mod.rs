//! Distributed coordination for multi-instance deployments
//!
//! This module provides primitives for coordinating multiple supervisor instances
//! across different processes or machines using Redis as the coordination backend.

#[cfg(feature = "distributed")]
mod redis_impl;

#[cfg(feature = "distributed")]
pub use redis_impl::{CoordinationManager, RedisCoordination};

/// Trait for distributed coordination backends
#[async_trait::async_trait]
pub trait CoordinationBackend: Send + Sync {
    /// Attempt to acquire a distributed lock
    /// Returns true if lock was acquired, false if already held by another instance
    async fn try_acquire_lock(&self, key: &str, ttl_secs: u64) -> anyhow::Result<bool>;

    /// Release a previously acquired lock
    async fn release_lock(&self, key: &str) -> anyhow::Result<()>;

    /// Check if a lock is currently held
    async fn is_locked(&self, key: &str) -> anyhow::Result<bool>;

    /// Update heartbeat timestamp for this instance
    async fn heartbeat(&self, instance_id: &str, ttl_secs: u64) -> anyhow::Result<()>;

    /// Check if an instance is alive (has recent heartbeat)
    async fn is_instance_alive(&self, instance_id: &str) -> anyhow::Result<bool>;

    /// Try to become the leader (acquire leader lock)
    async fn try_become_leader(&self, instance_id: &str, lease_secs: u64) -> anyhow::Result<bool>;

    /// Check if this instance is currently the leader
    async fn is_leader(&self, instance_id: &str) -> anyhow::Result<bool>;

    /// Get the current leader instance ID (if any)
    async fn get_current_leader(&self) -> anyhow::Result<Option<String>>;
}

/// Configuration for distributed coordination
#[derive(Debug, Clone)]
pub struct CoordinationConfig {
    /// Unique identifier for this instance
    pub instance_id: String,
    /// Redis connection URL (e.g., "redis://localhost:6379")
    pub redis_url: String,
    /// Leader lease duration in seconds
    pub leader_lease_secs: u64,
    /// Heartbeat interval in seconds
    pub heartbeat_interval_secs: u64,
    /// Lock TTL in seconds (for task execution locks)
    pub lock_ttl_secs: u64,
}

impl Default for CoordinationConfig {
    fn default() -> Self {
        Self {
            instance_id: format!("instance-{}", uuid::Uuid::new_v4()),
            redis_url: "redis://localhost:6379".to_string(),
            leader_lease_secs: 30,
            heartbeat_interval_secs: 5,
            lock_ttl_secs: 300, // 5 minutes default
        }
    }
}

impl CoordinationConfig {
    /// Create a new config with custom instance ID
    pub fn with_instance_id(mut self, id: impl Into<String>) -> Self {
        self.instance_id = id.into();
        self
    }

    /// Set Redis URL
    pub fn with_redis_url(mut self, url: impl Into<String>) -> Self {
        self.redis_url = url.into();
        self
    }

    /// Set leader lease duration
    pub fn with_leader_lease(mut self, secs: u64) -> Self {
        self.leader_lease_secs = secs;
        self
    }

    /// Set heartbeat interval
    pub fn with_heartbeat_interval(mut self, secs: u64) -> Self {
        self.heartbeat_interval_secs = secs;
        self
    }
}

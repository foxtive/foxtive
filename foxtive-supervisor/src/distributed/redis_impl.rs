//! Redis-based implementation of distributed coordination

use super::{CoordinationBackend, CoordinationConfig};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info};

/// Redis-based coordination backend
pub struct RedisCoordination {
    client: redis::Client,
    config: CoordinationConfig,
}

impl RedisCoordination {
    /// Create a new Redis coordination backend
    pub async fn new(config: CoordinationConfig) -> anyhow::Result<Self> {
        let client = redis::Client::open(config.redis_url.clone())?;
        
        // Test connection
        let mut conn = client.get_multiplexed_async_connection().await?;
        let _: String = redis::cmd("PING").query_async(&mut conn).await?;
        
        info!(
            instance_id = %config.instance_id,
            "Connected to Redis for distributed coordination"
        );
        
        Ok(Self { client, config })
    }

    /// Get a Redis connection
    async fn get_conn(&self) -> anyhow::Result<redis::aio::MultiplexedConnection> {
        Ok(self.client.get_multiplexed_async_connection().await?)
    }
}

#[async_trait::async_trait]
impl CoordinationBackend for RedisCoordination {
    async fn try_acquire_lock(&self, key: &str, ttl_secs: u64) -> anyhow::Result<bool> {
        let lock_key = format!("lock:{}", key);
        let instance_id = &self.config.instance_id;
        
        let mut conn = self.get_conn().await?;
        
        // Use SET NX EX for atomic lock acquisition with TTL
        let result: Option<String> = redis::cmd("SET")
            .arg(&lock_key)
            .arg(instance_id)
            .arg("NX")
            .arg("EX")
            .arg(ttl_secs)
            .query_async(&mut conn)
            .await?;
        
        let acquired = result.is_some();
        if acquired {
            debug!(key = %key, "Lock acquired");
        } else {
            debug!(key = %key, "Lock already held by another instance");
        }
        
        Ok(acquired)
    }

    async fn release_lock(&self, key: &str) -> anyhow::Result<()> {
        let lock_key = format!("lock:{}", key);
        let instance_id = &self.config.instance_id;
        
        let mut conn = self.get_conn().await?;
        
        // Only release if we own the lock (Lua script for atomicity)
        let script = redis::Script::new(r#"
            if redis.call("GET", KEYS[1]) == ARGV[1] then
                return redis.call("DEL", KEYS[1])
            else
                return 0
            end
        "#);
        
        let _: i32 = script.key(&lock_key).arg(instance_id).invoke_async(&mut conn).await?;
        debug!(key = %key, "Lock released");
        
        Ok(())
    }

    async fn is_locked(&self, key: &str) -> anyhow::Result<bool> {
        let lock_key = format!("lock:{}", key);
        let mut conn = self.get_conn().await?;
        
        let exists: bool = redis::cmd("EXISTS")
            .arg(&lock_key)
            .query_async(&mut conn)
            .await?;
        
        Ok(exists)
    }

    async fn heartbeat(&self, instance_id: &str, ttl_secs: u64) -> anyhow::Result<()> {
        let heartbeat_key = format!("heartbeat:{}", instance_id);
        let mut conn = self.get_conn().await?;
        
        let _: () = redis::cmd("SET")
            .arg(&heartbeat_key)
            .arg(chrono::Utc::now().timestamp())
            .arg("EX")
            .arg(ttl_secs)
            .query_async(&mut conn)
            .await?;
        
        debug!(instance_id = %instance_id, "Heartbeat sent");
        
        Ok(())
    }

    async fn is_instance_alive(&self, instance_id: &str) -> anyhow::Result<bool> {
        let heartbeat_key = format!("heartbeat:{}", instance_id);
        let mut conn = self.get_conn().await?;
        
        let exists: bool = redis::cmd("EXISTS")
            .arg(&heartbeat_key)
            .query_async(&mut conn)
            .await?;
        
        Ok(exists)
    }

    async fn try_become_leader(&self, instance_id: &str, lease_secs: u64) -> anyhow::Result<bool> {
        let leader_key = "leader:current";
        
        let mut conn = self.get_conn().await?;
        
        // Try to atomically set leader key with TTL
        let result: Option<String> = redis::cmd("SET")
            .arg(leader_key)
            .arg(instance_id)
            .arg("NX")
            .arg("EX")
            .arg(lease_secs)
            .query_async(&mut conn)
            .await?;
        
        let became_leader = result.is_some();
        if became_leader {
            info!(instance_id = %instance_id, "Became leader");
        }
        
        Ok(became_leader)
    }

    async fn is_leader(&self, instance_id: &str) -> anyhow::Result<bool> {
        let leader_key = "leader:current";
        let mut conn = self.get_conn().await?;
        
        let current_leader: Option<String> = redis::cmd("GET")
            .arg(leader_key)
            .query_async(&mut conn)
            .await?;
        
        Ok(current_leader.as_deref() == Some(instance_id))
    }

    async fn get_current_leader(&self) -> anyhow::Result<Option<String>> {
        let leader_key = "leader:current";
        let mut conn = self.get_conn().await?;
        
        let leader: Option<String> = redis::cmd("GET")
            .arg(leader_key)
            .query_async(&mut conn)
            .await?;
        
        Ok(leader)
    }
}

/// Background task that maintains leader status and heartbeat
pub struct CoordinationManager {
    backend: Arc<dyn CoordinationBackend>,
    config: CoordinationConfig,
    is_leader: Arc<std::sync::atomic::AtomicBool>,
}

impl CoordinationManager {
    /// Create a new coordination manager
    pub fn new(backend: Arc<dyn CoordinationBackend>, config: CoordinationConfig) -> Self {
        Self {
            backend,
            config,
            is_leader: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Get leader status
    pub fn is_leader(&self) -> bool {
        self.is_leader.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Start background tasks for leader election and heartbeat
    pub async fn start(&self) -> anyhow::Result<()> {
        let instance_id = self.config.instance_id.clone();
        let backend = self.backend.clone();
        let is_leader = self.is_leader.clone();
        let lease_secs = self.config.leader_lease_secs;
        let heartbeat_secs = self.config.heartbeat_interval_secs;

        // Spawn leader election task
        tokio::spawn(async move {
            loop {
                match backend.try_become_leader(&instance_id, lease_secs).await {
                    Ok(true) => {
                        is_leader.store(true, std::sync::atomic::Ordering::SeqCst);
                        info!("This instance is now the leader");
                    }
                    Ok(false) => {
                        is_leader.store(false, std::sync::atomic::Ordering::SeqCst);
                        if let Ok(leader) = backend.get_current_leader().await {
                            debug!(leader = ?leader, "Another instance is leader");
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "Failed to attempt leader election");
                    }
                }
                
                // Renew leadership or retry after half the lease time
                tokio::time::sleep(Duration::from_secs(lease_secs / 2)).await;
            }
        });

        // Spawn heartbeat task
        let instance_id_hb = self.config.instance_id.clone();
        let backend_hb = self.backend.clone();
        
        tokio::spawn(async move {
            loop {
                if let Err(e) = backend_hb.heartbeat(&instance_id_hb, heartbeat_secs * 2).await {
                    error!(error = %e, "Failed to send heartbeat");
                }
                
                tokio::time::sleep(Duration::from_secs(heartbeat_secs)).await;
            }
        });

        Ok(())
    }
}

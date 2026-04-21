//! Task pool and load balancing support
//!
//! This module provides the ability to create pools of worker tasks that can
//! distribute workload across multiple instances for better performance and reliability.

use crate::Supervisor;
use crate::contracts::SupervisedTask;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Load balancing strategy for task pools
#[derive(Debug, Clone, Copy, Default)]
pub enum LoadBalancingStrategy {
    /// Round-robin: Distribute tasks evenly in order
    #[default]
    RoundRobin,
    /// Random: Select a random worker from the pool
    Random,
    /// Least loaded: Select the worker with fewest active tasks (requires tracking)
    LeastLoaded,
}

/// A pool of worker tasks for load balancing
pub struct TaskPool {
    /// Pool identifier
    pub id: String,
    /// Number of workers in the pool
    pub pool_size: usize,
    /// Load balancing strategy
    pub strategy: LoadBalancingStrategy,
    /// Current round-robin index
    current_index: Arc<RwLock<usize>>,
}

impl TaskPool {
    /// Create a new task pool
    pub fn new(id: impl Into<String>, pool_size: usize, strategy: LoadBalancingStrategy) -> Self {
        Self {
            id: id.into(),
            pool_size,
            strategy,
            current_index: Arc::new(RwLock::new(0)),
        }
    }

    /// Get the next worker index based on the load balancing strategy
    pub async fn get_next_worker(&self) -> usize {
        match self.strategy {
            LoadBalancingStrategy::RoundRobin => {
                let mut index = self.current_index.write().await;
                let worker = *index;
                *index = (*index + 1) % self.pool_size;
                worker
            }
            LoadBalancingStrategy::Random => {
                // Note: rand is a non-optional dependency because it's used here (always compiled)
                // and in supervision.rs for jitter (feature-gated under 'cron')
                rand::random_range(0..self.pool_size)
            }
            LoadBalancingStrategy::LeastLoaded => {
                // For now, fall back to round-robin
                // In a real implementation, we'd track active tasks per worker
                let mut index = self.current_index.write().await;
                let worker = *index;
                *index = (*index + 1) % self.pool_size;
                worker
            }
        }
    }

    /// Build a supervisor with pooled workers
    pub fn build_pool<T, F>(&self, task_factory: F) -> Supervisor
    where
        T: SupervisedTask + 'static,
        F: Fn(usize) -> T,
    {
        let mut supervisor = Supervisor::new();

        for i in 0..self.pool_size {
            let task = task_factory(i);
            supervisor = supervisor.add(task);
        }

        info!(pool_id = %self.id, size = self.pool_size, "Created task pool");
        supervisor
    }

    /// Get pool information
    pub fn info(&self) -> PoolInfo {
        PoolInfo {
            id: self.id.clone(),
            pool_size: self.pool_size,
            strategy: self.strategy,
        }
    }
}

/// Information about a task pool
#[derive(Debug, Clone)]
pub struct PoolInfo {
    pub id: String,
    pub pool_size: usize,
    pub strategy: LoadBalancingStrategy,
}

/// Builder for creating task pools
pub struct TaskPoolBuilder {
    id: String,
    pool_size: usize,
    strategy: LoadBalancingStrategy,
}

impl TaskPoolBuilder {
    /// Create a new task pool builder
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            pool_size: 1,
            strategy: LoadBalancingStrategy::default(),
        }
    }

    /// Set the pool size
    pub fn with_size(mut self, size: usize) -> Self {
        self.pool_size = size.max(1);
        self
    }

    /// Set the load balancing strategy
    pub fn with_strategy(mut self, strategy: LoadBalancingStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Build the task pool
    pub fn build(self) -> TaskPool {
        TaskPool::new(self.id, self.pool_size, self.strategy)
    }

    /// Build and immediately create a supervisor with pooled workers
    pub fn build_and_create<T, F>(self, task_factory: F) -> Supervisor
    where
        T: SupervisedTask + 'static,
        F: Fn(usize) -> T,
    {
        let pool = self.build();
        pool.build_pool(task_factory)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_round_robin_distribution() {
        let pool = TaskPool::new("test-pool", 3, LoadBalancingStrategy::RoundRobin);

        // Should cycle through 0, 1, 2, 0, 1, 2...
        assert_eq!(pool.get_next_worker().await, 0);
        assert_eq!(pool.get_next_worker().await, 1);
        assert_eq!(pool.get_next_worker().await, 2);
        assert_eq!(pool.get_next_worker().await, 0);
        assert_eq!(pool.get_next_worker().await, 1);
    }

    #[tokio::test]
    async fn test_random_distribution() {
        let pool = TaskPool::new("test-pool", 5, LoadBalancingStrategy::Random);

        // All results should be within bounds
        for _ in 0..20 {
            let worker = pool.get_next_worker().await;
            assert!(worker < 5);
        }
    }

    #[tokio::test]
    async fn test_pool_builder() {
        let pool = TaskPoolBuilder::new("worker-pool")
            .with_size(4)
            .with_strategy(LoadBalancingStrategy::RoundRobin)
            .build();

        assert_eq!(pool.id, "worker-pool");
        assert_eq!(pool.pool_size, 4);
        assert!(matches!(pool.strategy, LoadBalancingStrategy::RoundRobin));
    }

    #[tokio::test]
    async fn test_pool_info() {
        let pool = TaskPool::new("info-test", 3, LoadBalancingStrategy::Random);
        let info = pool.info();

        assert_eq!(info.id, "info-test");
        assert_eq!(info.pool_size, 3);
        assert!(matches!(info.strategy, LoadBalancingStrategy::Random));
    }
}

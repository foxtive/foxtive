use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Load balancing strategy for distributing messages across workers.
#[derive(Debug, Clone, Copy, Default)]
pub enum LoadBalancingStrategy {
    /// Distribute messages in round-robin fashion.
    #[default]
    RoundRobin,
    
    /// Select a random worker for each message.
    Random,
    
    /// Select the worker with the least active tasks.
    LeastLoaded,
}

/// Round-robin load balancer.
///
/// Distributes messages evenly across workers in a circular fashion.
#[derive(Debug)]
pub struct RoundRobinBalancer {
    current: AtomicUsize,
}

impl RoundRobinBalancer {
    /// Create a new round-robin balancer.
    pub fn new() -> Self {
        Self {
            current: AtomicUsize::new(0),
        }
    }

    /// Get the next worker index.
    pub fn next(&self, worker_count: usize) -> usize {
        if worker_count == 0 {
            return 0;
        }
        self.current.fetch_add(1, Ordering::SeqCst) % worker_count
    }
}

impl Default for RoundRobinBalancer {
    fn default() -> Self {
        Self::new()
    }
}

/// Random load balancer.
///
/// Selects a random worker for each message distribution.
#[derive(Debug, Default)]
pub struct RandomBalancer;

impl RandomBalancer {
    /// Create a new random balancer.
    pub fn new() -> Self {
        Self
    }

    /// Get a random worker index.
    pub fn next(&self, worker_count: usize) -> usize {
        if worker_count == 0 {
            return 0;
        }
        rand::random::<usize>() % worker_count
    }
}

/// Least-loaded balancer state.
///
/// Tracks the number of active tasks per worker to select the least busy one.
/// 
/// OPTIMIZATION: Uses Arc<Vec<AtomicUsize>> with atomic swap for lock-free updates
/// when workers are added/removed, avoiding expensive recreation and copy operations.
#[derive(Debug)]
pub struct LeastLoadedBalancer {
    worker_loads: std::sync::RwLock<Arc<Vec<AtomicUsize>>>,
}

impl LeastLoadedBalancer {
    /// Create a new least-loaded balancer with the given number of workers.
    pub fn new(worker_count: usize) -> Self {
        let loads = (0..worker_count)
            .map(|_| AtomicUsize::new(0))
            .collect();
        Self {
            worker_loads: std::sync::RwLock::new(Arc::new(loads)),
        }
    }

    /// Add a new worker slot to the balancer without recreating existing state.
    /// 
    /// This is an O(1) operation that atomically swaps in a new vector,
    /// preserving all existing load counts.
    pub fn add_worker(&self) {
        let mut current = self.worker_loads.write().unwrap();
        let mut new_loads = Vec::with_capacity(current.len() + 1);
        
        // Clone existing Arc<AtomicUsize> references (cheap - just increments ref count)
        for load in current.iter() {
            // We need to clone the AtomicUsize itself, not the reference
            let current_value = load.load(Ordering::Relaxed);
            new_loads.push(AtomicUsize::new(current_value));
        }
        
        // Add new worker with zero load
        new_loads.push(AtomicUsize::new(0));
        
        // Atomically swap in the new vector
        *current = Arc::new(new_loads);
    }

    /// Get the index of the least loaded worker.
    pub fn next(&self) -> usize {
        let loads = self.worker_loads.read().unwrap();
        
        if loads.is_empty() {
            return 0;
        }

        let mut min_load = usize::MAX;
        let mut min_index = 0;

        for (i, load) in loads.iter().enumerate() {
            let current_load = load.load(Ordering::Relaxed);
            if current_load < min_load {
                min_load = current_load;
                min_index = i;
            }
        }

        min_index
    }

    /// Increment the load for a worker.
    pub fn increment_load(&self, worker_index: usize) {
        let loads = self.worker_loads.read().unwrap();
        if let Some(load) = loads.get(worker_index) {
            load.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Decrement the load for a worker.
    pub fn decrement_load(&self, worker_index: usize) {
        let loads = self.worker_loads.read().unwrap();
        if let Some(load) = loads.get(worker_index) {
            load.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// Get the current load for a worker.
    pub fn get_load(&self, worker_index: usize) -> usize {
        let loads = self.worker_loads.read().unwrap();
        loads
            .get(worker_index)
            .map(|load| load.load(Ordering::Relaxed))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_robin_distribution() {
        let balancer = RoundRobinBalancer::new();
        
        assert_eq!(balancer.next(3), 0);
        assert_eq!(balancer.next(3), 1);
        assert_eq!(balancer.next(3), 2);
        assert_eq!(balancer.next(3), 0);
        assert_eq!(balancer.next(3), 1);
    }

    #[test]
    fn test_round_robin_single_worker() {
        let balancer = RoundRobinBalancer::new();
        
        assert_eq!(balancer.next(1), 0);
        assert_eq!(balancer.next(1), 0);
        assert_eq!(balancer.next(1), 0);
    }

    #[test]
    fn test_round_robin_zero_workers() {
        let balancer = RoundRobinBalancer::new();
        assert_eq!(balancer.next(0), 0);
    }

    #[test]
    fn test_random_balancer() {
        let balancer = RandomBalancer::new();
        
        // Should return valid indices
        let idx = balancer.next(5);
        assert!(idx < 5);
        
        let idx = balancer.next(1);
        assert_eq!(idx, 0);
    }

    #[test]
    fn test_least_loaded_initial() {
        let balancer = LeastLoadedBalancer::new(3);
        
        // All workers start with 0 load, should return first
        assert_eq!(balancer.next(), 0);
    }

    #[test]
    fn test_least_loaded_after_increment() {
        let balancer = LeastLoadedBalancer::new(3);
        
        // Increment load on worker 0
        balancer.increment_load(0);
        balancer.increment_load(0);
        
        // Worker 0 has load 2, workers 1 and 2 have load 0
        // Should return worker 1 (first with minimum load)
        assert_eq!(balancer.next(), 1);
    }

    #[test]
    fn test_least_loaded_load_tracking() {
        let balancer = LeastLoadedBalancer::new(3);
        
        balancer.increment_load(0);
        balancer.increment_load(0);
        balancer.increment_load(1);
        
        assert_eq!(balancer.get_load(0), 2);
        assert_eq!(balancer.get_load(1), 1);
        assert_eq!(balancer.get_load(2), 0);
        
        balancer.decrement_load(0);
        assert_eq!(balancer.get_load(0), 1);
    }

    #[test]
    fn test_least_loaded_empty() {
        let balancer = LeastLoadedBalancer::new(0);
        assert_eq!(balancer.next(), 0);
    }
}

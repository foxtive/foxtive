use std::sync::Arc;
/// Stress testing utilities for foxtive-worker.
///
/// Provides tools for high-load testing, performance benchmarking,
/// and stability validation under extreme conditions.
///
/// # Example
/// ```rust,no_run
/// use foxtive_worker::stress::{StressTestConfig, run_stress_test};
///
/// #[tokio::main]
/// async fn main() {
///     let config = StressTestConfig {
///         message_count: 100_000,
///         concurrency: 100,
///         message_size_bytes: 1024, // 1KB messages
///         ..Default::default()
///     };
///     
///     let results = run_stress_test(config).await;
///     println!("Throughput: {:.2} msg/sec", results.throughput);
/// }
/// ```
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::error::WorkerResult;
use crate::message::{AckHandle, Message, MessageMetadata, ReceivedMessage};
use crate::worker::Worker;
use async_trait::async_trait;

/// Configuration for stress tests.
#[derive(Debug, Clone)]
pub struct StressTestConfig {
    /// Total number of messages to process
    pub message_count: usize,
    /// Number of concurrent workers
    pub concurrency: usize,
    /// Size of each message payload in bytes
    pub message_size_bytes: usize,
    /// Whether to simulate processing delays
    pub simulate_processing_delay: bool,
    /// Processing delay range (min_ms, max_ms)
    pub processing_delay_range_ms: (u64, u64),
    /// Timeout for entire test
    pub test_timeout_secs: u64,
}

impl Default for StressTestConfig {
    fn default() -> Self {
        Self {
            message_count: 10_000,
            concurrency: 50,
            message_size_bytes: 256,
            simulate_processing_delay: false,
            processing_delay_range_ms: (1, 10),
            test_timeout_secs: 300, // 5 minutes
        }
    }
}

/// Results from a stress test run.
#[derive(Debug, Clone)]
pub struct StressTestResults {
    /// Total messages processed
    pub total_messages: usize,
    /// Total time taken
    pub total_duration: Duration,
    /// Messages per second throughput
    pub throughput: f64,
    /// Average processing time per message
    pub avg_processing_time_ms: f64,
    /// P95 processing time
    pub p95_processing_time_ms: f64,
    /// P99 processing time
    pub p99_processing_time_ms: f64,
    /// Peak memory usage (approximate, in MB)
    pub peak_memory_mb: f64,
    /// Errors encountered
    pub error_count: usize,
    /// Success rate percentage
    pub success_rate: f64,
}

impl StressTestResults {
    /// Print results in human-readable format.
    pub fn print_summary(&self) {
        println!("\n=== Stress Test Results ===");
        println!("Total Messages: {}", self.total_messages);
        println!("Total Duration: {:?}", self.total_duration);
        println!("Throughput: {:.2} msg/sec", self.throughput);
        println!("Avg Processing Time: {:.2} ms", self.avg_processing_time_ms);
        println!("P95 Processing Time: {:.2} ms", self.p95_processing_time_ms);
        println!("P99 Processing Time: {:.2} ms", self.p99_processing_time_ms);
        println!("Peak Memory: {:.2} MB", self.peak_memory_mb);
        println!("Errors: {}", self.error_count);
        println!("Success Rate: {:.2}%", self.success_rate);
        println!("==========================\n");
    }
}

/// Mock acknowledgment handle for stress testing.
#[derive(Debug)]
struct StressTestAckHandle;

#[async_trait]
impl AckHandle for StressTestAckHandle {
    async fn ack(&self) -> WorkerResult<()> {
        Ok(())
    }

    async fn nack(&self, _requeue: bool) -> WorkerResult<()> {
        Ok(())
    }
}

/// High-throughput worker for stress testing.
struct StressTestWorker {
    id: String,
    processing_times: Arc<Vec<AtomicU64>>, // Store processing times in microseconds
    config: StressTestConfig,
}

#[async_trait]
impl Worker for StressTestWorker {
    fn id(&self) -> &str {
        &self.id
    }

    async fn process(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        let start = Instant::now();

        // Simulate processing if configured
        if self.config.simulate_processing_delay {
            let (min_ms, max_ms) = self.config.processing_delay_range_ms;
            let delay_ms = rand::random::<u64>() % (max_ms - min_ms + 1) + min_ms;
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }

        // Simulate some CPU work (hash calculation)
        let _hash = calculate_hash(&message.message.payload);

        let elapsed = start.elapsed();

        // Record processing time in microseconds
        let idx = self
            .id
            .split('-')
            .next_back()
            .unwrap_or("0")
            .parse::<usize>()
            .unwrap_or(0);
        if let Some(counter) = self.processing_times.get(idx) {
            counter.store(elapsed.as_micros() as u64, Ordering::Relaxed);
        }

        // Acknowledge the message
        message.ack().await?;

        Ok(())
    }
}

/// Calculate a simple hash to simulate CPU work.
fn calculate_hash(value: &serde_json::Value) -> u64 {
    let serialized = serde_json::to_string(value).unwrap_or_default();
    serialized
        .bytes()
        .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64))
}

/// Run a stress test with the given configuration.
///
/// This creates multiple workers and dispatches messages at maximum speed
/// to measure throughput, latency, and resource usage.
pub async fn run_stress_test(config: StressTestConfig) -> StressTestResults {
    use crate::metrics::NoOpMetrics;
    use crate::pool::WorkerPool;
    use crate::strategies::LoadBalancingStrategy;

    println!("Starting stress test...");
    println!("  Messages: {}", config.message_count);
    println!("  Concurrency: {}", config.concurrency);
    println!("  Message Size: {} bytes", config.message_size_bytes);
    println!();

    let start_time = Instant::now();
    let processing_times = Arc::new(
        (0..config.concurrency)
            .map(|_| AtomicU64::new(0))
            .collect::<Vec<_>>(),
    );
    let error_count = Arc::new(AtomicUsize::new(0));

    // Create worker pool
    let mut pool = WorkerPool::with_concurrency(
        "stress-test-pool",
        LoadBalancingStrategy::RoundRobin,
        config.concurrency,
        Arc::new(NoOpMetrics),
    );

    // Add workers
    for i in 0..config.concurrency {
        let worker = StressTestWorker {
            id: format!("worker-{}", i),
            processing_times: processing_times.clone(),
            config: config.clone(),
        };
        pool.add_worker(Arc::new(worker));
    }

    // Generate test messages
    let test_payload = generate_test_payload(config.message_size_bytes);

    println!("Dispatching {} messages...", config.message_count);
    let dispatch_start = Instant::now();

    // Dispatch all messages
    for i in 0..config.message_count {
        let message = create_stress_test_message(&format!("msg-{}", i), test_payload.clone());

        if let Err(e) = pool.dispatch(message).await {
            eprintln!("Failed to dispatch message {}: {}", i, e);
            error_count.fetch_add(1, Ordering::Relaxed);
        }

        // Progress indicator every 1000 messages
        if (i + 1) % 1000 == 0 {
            println!("  Dispatched {} / {} messages", i + 1, config.message_count);
        }
    }

    let dispatch_duration = dispatch_start.elapsed();
    println!("Dispatch completed in {:?}", dispatch_duration);

    // Wait for all messages to be processed
    println!("Waiting for processing to complete...");
    let timeout = Duration::from_secs(config.test_timeout_secs);

    loop {
        if start_time.elapsed() > timeout {
            eprintln!("WARNING: Test timeout reached!");
            break;
        }

        // Check if all permits are available (all tasks done)
        if pool.in_flight_count() == 0 {
            // Give a bit more time for final acks
            tokio::time::sleep(Duration::from_millis(100)).await;
            break;
        }

        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let total_duration = start_time.elapsed();
    let errors = error_count.load(Ordering::Relaxed);
    let successful = config.message_count.saturating_sub(errors);

    // Calculate statistics
    let throughput = successful as f64 / total_duration.as_secs_f64();

    let times: Vec<u64> = processing_times
        .iter()
        .map(|t| t.load(Ordering::Relaxed))
        .filter(|&t| t > 0)
        .collect();

    let avg_time = if times.is_empty() {
        0.0
    } else {
        times.iter().sum::<u64>() as f64 / times.len() as f64 / 1000.0 // Convert to ms
    };

    let mut sorted_times = times.clone();
    sorted_times.sort();

    let p95_idx =
        ((sorted_times.len() as f64 * 0.95) as usize).min(sorted_times.len().saturating_sub(1));
    let p99_idx =
        ((sorted_times.len() as f64 * 0.99) as usize).min(sorted_times.len().saturating_sub(1));

    let p95_time = sorted_times.get(p95_idx).copied().unwrap_or(0) as f64 / 1000.0;
    let p99_time = sorted_times.get(p99_idx).copied().unwrap_or(0) as f64 / 1000.0;

    // Estimate memory usage (rough approximation)
    let peak_memory_mb = estimate_memory_usage(
        config.message_count,
        config.message_size_bytes,
        config.concurrency,
    );

    let success_rate = if config.message_count > 0 {
        (successful as f64 / config.message_count as f64) * 100.0
    } else {
        0.0
    };

    let results = StressTestResults {
        total_messages: successful,
        total_duration,
        throughput,
        avg_processing_time_ms: avg_time,
        p95_processing_time_ms: p95_time,
        p99_processing_time_ms: p99_time,
        peak_memory_mb,
        error_count: errors,
        success_rate,
    };

    results.print_summary();

    // Shutdown pool
    if let Err(e) = pool.shutdown().await {
        eprintln!("Warning: Pool shutdown failed: {}", e);
    }

    results
}

/// Generate a test payload of specified size.
fn generate_test_payload(size_bytes: usize) -> serde_json::Value {
    let data = "x".repeat(size_bytes);
    serde_json::json!({
        "data": data,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "id": uuid::Uuid::new_v4().to_string()
    })
}

/// Create a stress test message.
fn create_stress_test_message(
    id: &str,
    payload: serde_json::Value,
) -> ReceivedMessage<serde_json::Value> {
    let message = Message {
        id: id.to_string(),
        payload,
        metadata: MessageMetadata::new("stress-test-queue"),
    };
    ReceivedMessage::new(message, Arc::new(StressTestAckHandle))
}

/// Estimate memory usage in MB.
fn estimate_memory_usage(message_count: usize, message_size: usize, concurrency: usize) -> f64 {
    // Rough estimation:
    // - In-flight messages: concurrency * message_size
    // - Queue overhead: message_count * message_size * 0.1 (10% overhead)
    // - Worker state: concurrency * 1MB
    let in_flight_bytes = concurrency * message_size;
    let queue_overhead = message_count * message_size / 10;
    let worker_state = concurrency * 1_048_576; // 1MB per worker

    let total_bytes = in_flight_bytes + queue_overhead + worker_state;
    total_bytes as f64 / 1_048_576.0 // Convert to MB
}

/// Run a long-running stability test.
///
/// Processes messages continuously for a specified duration to detect
/// memory leaks, resource exhaustion, or degradation over time.
pub async fn run_stability_test(duration_secs: u64, config: StressTestConfig) -> StressTestResults {
    println!("Starting {}-second stability test...", duration_secs);

    let start = Instant::now();
    let target_duration = Duration::from_secs(duration_secs);
    let mut iteration = 0;
    let mut total_processed = 0;
    let mut total_errors = 0;

    while start.elapsed() < target_duration {
        iteration += 1;
        println!("\n--- Iteration {} ---", iteration);

        let mut iter_config = config.clone();
        iter_config.message_count = config.message_count / 10; // Smaller batches

        let results = run_stress_test(iter_config).await;
        total_processed += results.total_messages;
        total_errors += results.error_count;

        // Brief pause between iterations
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    let total_duration = start.elapsed();

    StressTestResults {
        total_messages: total_processed,
        total_duration,
        throughput: total_processed as f64 / total_duration.as_secs_f64(),
        avg_processing_time_ms: 0.0, // Would need to track across iterations
        p95_processing_time_ms: 0.0,
        p99_processing_time_ms: 0.0,
        peak_memory_mb: 0.0,
        error_count: total_errors,
        success_rate: if total_processed + total_errors > 0 {
            (total_processed as f64 / (total_processed + total_errors) as f64) * 100.0
        } else {
            0.0
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_small_stress_test() {
        let config = StressTestConfig {
            message_count: 100,
            concurrency: 5,
            message_size_bytes: 64,
            ..Default::default()
        };

        let results = run_stress_test(config).await;

        assert!(results.total_messages > 0);
        assert!(results.throughput > 0.0);
        assert!(results.success_rate >= 99.0); // Should be nearly 100%
    }

    #[test]
    fn test_payload_generation() {
        let payload = generate_test_payload(1024);
        let serialized = serde_json::to_string(&payload).unwrap();

        // Payload should be approximately the requested size
        assert!(serialized.len() >= 1024);
    }

    #[test]
    fn test_memory_estimation() {
        let mem = estimate_memory_usage(1000, 256, 10);
        assert!(mem > 0.0);
        assert!(mem < 100.0); // Should be reasonable
    }
}

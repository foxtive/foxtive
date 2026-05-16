use std::time::Instant;

/// Trait for collecting worker-related metrics.
pub trait WorkerMetrics: Send + Sync {
    /// Records that a message was received by a worker.
    fn record_message_received(&self, worker_id: &str, queue_name: &str);

    /// Records that a message was successfully processed.
    fn record_message_processed(&self, worker_id: &str, queue_name: &str, start_time: Instant);

    /// Records that a message failed processing.
    fn record_message_failed(&self, worker_id: &str, queue_name: &str, error_type: &str, start_time: Instant);

    /// Records that a message was retried.
    fn record_message_retried(&self, worker_id: &str, queue_name: &str, attempt: u32);

    /// Records that a message exhausted its retries and was sent to a DLQ or dropped.
    fn record_message_retries_exhausted(&self, worker_id: &str, queue_name: &str);

    /// Records that a message was sent to the dead letter queue.
    fn record_message_sent_to_dlq(&self, queue_name: &str, is_poison_pill: bool);

    /// Records the current number of active workers.
    fn record_active_workers(&self, count: usize);

    /// Records the current number of in-flight messages.
    fn record_in_flight_messages(&self, count: usize);
}

/// A no-op implementation of `WorkerMetrics` when the "metrics" feature is not enabled.
pub struct NoOpMetrics;

impl WorkerMetrics for NoOpMetrics {
    fn record_message_received(&self, _worker_id: &str, _queue_name: &str) {}
    fn record_message_processed(&self, _worker_id: &str, _queue_name: &str, _start_time: Instant) {}
    fn record_message_failed(&self, _worker_id: &str, _queue_name: &str, _error_type: &str, _start_time: Instant) {}
    fn record_message_retried(&self, _worker_id: &str, _queue_name: &str, _attempt: u32) {}
    fn record_message_retries_exhausted(&self, _worker_id: &str, _queue_name: &str) {}
    fn record_message_sent_to_dlq(&self, _queue_name: &str, _is_poison_pill: bool) {}
    fn record_active_workers(&self, _count: usize) {}
    fn record_in_flight_messages(&self, _count: usize) {}
}

#[cfg(feature = "metrics")]
pub struct MetricsCollector;

#[cfg(feature = "metrics")]
impl WorkerMetrics for MetricsCollector {
    fn record_message_received(&self, worker_id: &str, queue_name: &str) {
        metrics::counter!("foxtive_worker_messages_received_total",
            "worker_id" => worker_id.to_string(),
            "queue_name" => queue_name.to_string(),
        ).increment(1);
    }

    fn record_message_processed(&self, worker_id: &str, queue_name: &str, start_time: Instant) {
        metrics::counter!("foxtive_worker_messages_processed_total",
            "worker_id" => worker_id.to_string(),
            "queue_name" => queue_name.to_string(),
            "status" => "success",
        ).increment(1);
        metrics::histogram!("foxtive_worker_message_processing_duration_seconds",
            "worker_id" => worker_id.to_string(),
            "queue_name" => queue_name.to_string(),
            "status" => "success",
        ).record(start_time.elapsed().as_secs_f64());
    }

    fn record_message_failed(&self, worker_id: &str, queue_name: &str, error_type: &str, start_time: Instant) {
        metrics::counter!("foxtive_worker_messages_processed_total",
            "worker_id" => worker_id.to_string(),
            "queue_name" => queue_name.to_string(),
            "status" => "failure",
            "error_type" => error_type.to_string(),
        ).increment(1);
        metrics::histogram!("foxtive_worker_message_processing_duration_seconds",
            "worker_id" => worker_id.to_string(),
            "queue_name" => queue_name.to_string(),
            "status" => "failure",
            "error_type" => error_type.to_string(),
        ).record(start_time.elapsed().as_secs_f64());
    }

    fn record_message_retried(&self, worker_id: &str, queue_name: &str, attempt: u32) {
        metrics::counter!("foxtive_worker_messages_retried_total",
            "worker_id" => worker_id.to_string(),
            "queue_name" => queue_name.to_string(),
            "attempt" => attempt.to_string(),
        ).increment(1);
    }

    fn record_message_retries_exhausted(&self, worker_id: &str, queue_name: &str) {
        metrics::counter!("foxtive_worker_messages_retries_exhausted_total",
            "worker_id" => worker_id.to_string(),
            "queue_name" => queue_name.to_string(),
        ).increment(1);
    }

    fn record_message_sent_to_dlq(&self, queue_name: &str, is_poison_pill: bool) {
        let pill_type = if is_poison_pill { "poison_pill" } else { "normal" };
        metrics::counter!("foxtive_worker_messages_sent_to_dlq_total",
            "queue_name" => queue_name.to_string(),
            "type" => pill_type.to_string(),
        ).increment(1);
    }

    fn record_active_workers(&self, count: usize) {
        metrics::gauge!("foxtive_worker_active_workers").set(count as f64);
    }

    fn record_in_flight_messages(&self, count: usize) {
        metrics::gauge!("foxtive_worker_in_flight_messages").set(count as f64);
    }
}

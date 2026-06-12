use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, AtomicBool, Ordering};
use std::time::Duration;

use foxtive_worker::{
    Worker, WorkerResult, ReceivedMessage, Message, MessageMetadata, AckHandle,
};

/// Mock acknowledgment handle for testing.
#[derive(Debug)]
pub struct MockAckHandle {
    pub acked: Arc<AtomicBool>,
    pub nacked: Arc<AtomicBool>,
    pub requeued: Arc<AtomicBool>,
}

impl MockAckHandle {
    pub fn new() -> (Self, Arc<AtomicBool>, Arc<AtomicBool>, Arc<AtomicBool>) {
        let acked = Arc::new(AtomicBool::new(false));
        let nacked = Arc::new(AtomicBool::new(false));
        let requeued = Arc::new(AtomicBool::new(false));
        (
            Self {
                acked: acked.clone(),
                nacked: nacked.clone(),
                requeued: requeued.clone(),
            },
            acked,
            nacked,
            requeued,
        )
    }
}

#[async_trait]
impl AckHandle for MockAckHandle {
    async fn ack(&self) -> WorkerResult<()> {
        self.acked.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn nack(&self, requeue: bool) -> WorkerResult<()> {
        self.nacked.store(true, Ordering::SeqCst);
        self.requeued.store(requeue, Ordering::SeqCst);
        Ok(())
    }
}

/// Test worker that tracks processing statistics.
pub struct TestWorker {
    pub id: String,
    pub process_count: Arc<AtomicUsize>,
    pub concurrent_count: Arc<AtomicUsize>,
    pub max_concurrent: Arc<AtomicUsize>,
    pub processing_delay: Duration,
    pub should_fail: Arc<AtomicBool>,
}

#[allow(dead_code)]
impl TestWorker {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            process_count: Arc::new(AtomicUsize::new(0)),
            concurrent_count: Arc::new(AtomicUsize::new(0)),
            max_concurrent: Arc::new(AtomicUsize::new(0)),
            processing_delay: Duration::from_millis(10),
            should_fail: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.processing_delay = delay;
        self
    }

    #[allow(unused)]
    pub fn set_should_fail(&self, fail: bool) {
        self.should_fail.store(fail, Ordering::SeqCst);
    }
}

#[async_trait]
impl Worker for TestWorker {
    fn id(&self) -> &str {
        &self.id
    }

    async fn process(&self, _message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        // Track concurrency
        let current = self.concurrent_count.fetch_add(1, Ordering::SeqCst) + 1;
        
        // Update max concurrent
        let mut max = self.max_concurrent.load(Ordering::SeqCst);
        while current > max {
            match self.max_concurrent.compare_exchange_weak(
                max,
                current,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(new_max) => max = new_max,
            }
        }

        // Simulate processing
        if !self.processing_delay.is_zero() {
            tokio::time::sleep(self.processing_delay).await;
        }

        // Decrement concurrency
        self.concurrent_count.fetch_sub(1, Ordering::SeqCst);
        
        // Increment process count
        self.process_count.fetch_add(1, Ordering::SeqCst);

        // Check if should fail
        if self.should_fail.load(Ordering::SeqCst) {
            return Err(foxtive_worker::WorkerError::ProcessingFailed("Intentional failure".to_string()));
        }

        Ok(())
    }
}

/// Create a test message with the given ID.
pub fn create_test_message(id: &str) -> ReceivedMessage<serde_json::Value> {
    let (ack_handle, _, _, _) = MockAckHandle::new();
    let message = Message {
        id: id.to_string(),
        payload: serde_json::json!({"test": "data", "id": id}),
        metadata: MessageMetadata::new("test-queue"),
    };
    ReceivedMessage::new(message, Arc::new(ack_handle))
}

/// Wait for condition to be true with timeout.
#[allow(unused)]
pub async fn wait_for_condition<F>(mut condition: F, timeout: Duration) -> bool
where
    F: FnMut() -> bool,
{
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if condition() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    false
}

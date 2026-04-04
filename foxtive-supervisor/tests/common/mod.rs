#[allow(dead_code)] // This module contains testing utilities that are used across multiple test files.
pub mod testing;

use foxtive_supervisor::contracts::SupervisedTask;
use foxtive_supervisor::enums::{BackoffStrategy, RestartPolicy};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

#[allow(dead_code)]
pub struct MockTask {
    pub name: String,
    pub fail_count: AtomicUsize,
    pub max_fails: usize,
    pub restart_policy: RestartPolicy,
    pub backoff: BackoffStrategy,
    pub priority: i32,
    pub concurrency_limit: Option<usize>,
}

impl MockTask {
    #[allow(dead_code)]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            fail_count: AtomicUsize::new(0),
            max_fails: 0,
            restart_policy: RestartPolicy::Always,
            backoff: BackoffStrategy::Fixed(Duration::from_millis(10)),
            priority: 0,
            concurrency_limit: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_failures(mut self, max_fails: usize) -> Self {
        self.max_fails = max_fails;
        self
    }

    #[allow(dead_code)]
    pub fn with_policy(mut self, policy: RestartPolicy) -> Self {
        self.restart_policy = policy;
        self
    }

    #[allow(dead_code)]
    pub fn with_backoff(mut self, backoff: BackoffStrategy) -> Self {
        self.backoff = backoff;
        self
    }

    #[allow(dead_code)]
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    #[allow(dead_code)]
    pub fn with_concurrency_limit(mut self, limit: usize) -> Self {
        self.concurrency_limit = Some(limit);
        self
    }
}

#[async_trait::async_trait]
impl SupervisedTask for MockTask {
    fn id(&self) -> &'static str {
        Box::leak(self.name.clone().into_boxed_str())
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    async fn run(&self) -> anyhow::Result<()> {
        let count = self.fail_count.fetch_add(1, Ordering::SeqCst);
        if count < self.max_fails {
            anyhow::bail!("Simulated failure {}", count);
        }
        Ok(())
    }

    fn restart_policy(&self) -> RestartPolicy {
        self.restart_policy.clone()
    }

    fn backoff_strategy(&self) -> BackoffStrategy {
        self.backoff.clone()
    }

    fn priority(&self) -> i32 {
        self.priority
    }

    fn concurrency_limit(&self) -> Option<usize> {
        self.concurrency_limit
    }
}

#[allow(dead_code)]
pub struct PanickingTask {
    pub name: String,
    pub panic_count: AtomicUsize,
    pub max_panics: usize,
}

#[async_trait::async_trait]
impl SupervisedTask for PanickingTask {
    fn id(&self) -> &'static str {
        Box::leak(self.name.clone().into_boxed_str())
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    async fn run(&self) -> anyhow::Result<()> {
        let count = self.panic_count.fetch_add(1, Ordering::SeqCst);
        if count < self.max_panics {
            panic!("Intentional panic {}", count);
        }
        Ok(())
    }
}

#[allow(dead_code)]
pub struct HookTrackingTask {
    pub name: String,
    pub setup_called: Arc<AtomicBool>,
    pub cleanup_called: Arc<AtomicBool>,
    pub restart_calls: Arc<AtomicUsize>,
    pub error_calls: Arc<AtomicUsize>,
    pub panic_calls: Arc<AtomicUsize>,
    pub shutdown_called: Arc<AtomicBool>,
    pub fail_once: AtomicBool,
}

impl HookTrackingTask {
    #[allow(dead_code)]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            setup_called: Arc::new(AtomicBool::new(false)),
            cleanup_called: Arc::new(AtomicBool::new(false)),
            restart_calls: Arc::new(AtomicUsize::new(0)),
            error_calls: Arc::new(AtomicUsize::new(0)),
            panic_calls: Arc::new(AtomicUsize::new(0)),
            shutdown_called: Arc::new(AtomicBool::new(false)),
            fail_once: AtomicBool::new(true),
        }
    }
}

#[async_trait::async_trait]
impl SupervisedTask for HookTrackingTask {
    fn id(&self) -> &'static str {
        Box::leak(self.name.clone().into_boxed_str())
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn restart_policy(&self) -> RestartPolicy {
        RestartPolicy::MaxAttempts(2)
    }

    fn backoff_strategy(&self) -> BackoffStrategy {
        BackoffStrategy::Fixed(Duration::from_millis(10))
    }

    async fn setup(&self) -> anyhow::Result<()> {
        self.setup_called.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn run(&self) -> anyhow::Result<()> {
        if self.fail_once.swap(false, Ordering::SeqCst) {
            anyhow::bail!("First attempt fails")
        }
        Ok(())
    }

    async fn cleanup(&self) {
        self.cleanup_called.store(true, Ordering::SeqCst);
    }

    async fn on_restart(&self, _attempt: usize) {
        self.restart_calls.fetch_add(1, Ordering::SeqCst);
    }

    async fn on_error(&self, _msg: &str, _attempt: usize) {
        self.error_calls.fetch_add(1, Ordering::SeqCst);
    }

    async fn on_panic(&self, _msg: &str, _attempt: usize) {
        self.panic_calls.fetch_add(1, Ordering::SeqCst);
    }

    async fn on_shutdown(&self) {
        self.shutdown_called.store(true, Ordering::SeqCst);
    }
}

use crate::contracts::SupervisedTask;
use crate::enums::{BackoffStrategy, RestartPolicy};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub(crate) struct MockTask {
    pub(crate) name: String,
    pub(crate) fail_count: AtomicUsize,
    pub(crate) max_fails: usize,
    pub(crate) restart_policy: RestartPolicy,
    pub(crate) backoff: BackoffStrategy,
}

impl MockTask {
    pub(crate) fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            fail_count: AtomicUsize::new(0),
            max_fails: 0,
            restart_policy: RestartPolicy::Always,
            backoff: BackoffStrategy::Fixed(Duration::from_millis(10)),
        }
    }

    pub(crate) fn with_failures(mut self, max_fails: usize) -> Self {
        self.max_fails = max_fails;
        self
    }

    pub(crate) fn with_policy(mut self, policy: RestartPolicy) -> Self {
        self.restart_policy = policy;
        self
    }

    pub(crate) fn with_backoff(mut self, backoff: BackoffStrategy) -> Self {
        self.backoff = backoff;
        self
    }
}

#[async_trait::async_trait]
impl SupervisedTask for MockTask {
    fn id(&self) -> &'static str {
        "mock-task"
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
}

pub(crate) struct PanickingTask {
    pub(crate) name: String,
    pub(crate) panic_count: AtomicUsize,
    pub(crate) max_panics: usize,
}

#[async_trait::async_trait]
impl SupervisedTask for PanickingTask {
    fn id(&self) -> &'static str {
        "panicking-task"
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

pub(crate) struct SetupFailTask {
    pub(crate) name: String,
}

#[async_trait::async_trait]
impl SupervisedTask for SetupFailTask {
    fn id(&self) -> &'static str {
        "setup-fail-task"
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    async fn run(&self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn setup(&self) -> anyhow::Result<()> {
        anyhow::bail!("Setup failed intentionally")
    }
}

pub(crate) struct HookTrackingTask {
    pub(crate) name: String,
    pub(crate) setup_called: Arc<AtomicBool>,
    pub(crate) cleanup_called: Arc<AtomicBool>,
    pub(crate) restart_calls: Arc<AtomicUsize>,
    pub(crate) error_calls: Arc<AtomicUsize>,
    pub(crate) panic_calls: Arc<AtomicUsize>,
    pub(crate) shutdown_called: Arc<AtomicBool>,
    pub(crate) fail_once: AtomicBool,
}

impl HookTrackingTask {
    pub(crate) fn new(name: &str) -> Self {
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
        "hook-tracking-task"
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    fn restart_policy(&self) -> RestartPolicy {
        RestartPolicy::MaxAttempts(2)
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

pub(crate) struct ConditionalRestartTask {
    pub(crate) name: String,
    pub(crate) fail_count: AtomicUsize,
    pub(crate) prevent_restart_after: usize,
}

#[async_trait::async_trait]
impl SupervisedTask for ConditionalRestartTask {
    fn id(&self) -> &'static str {
        "conditional-restart-task"
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    async fn run(&self) -> anyhow::Result<()> {
        self.fail_count.fetch_add(1, Ordering::SeqCst);
        anyhow::bail!("Always fails")
    }

    async fn should_restart(&self, attempt: usize, _error: &str) -> bool {
        attempt < self.prevent_restart_after
    }
}

pub(crate) struct LongRunningTask {
    pub(crate) name: String,
    pub(crate) started: Arc<AtomicBool>,
}

#[async_trait::async_trait]
impl SupervisedTask for LongRunningTask {
    fn id(&self) -> &'static str {
        "long-running-task"
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    async fn run(&self) -> anyhow::Result<()> {
        self.started.store(true, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_secs(3600)).await;
        Ok(())
    }
}

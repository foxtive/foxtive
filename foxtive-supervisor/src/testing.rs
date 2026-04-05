//! Testing utilities for `foxtive-supervisor`.
//!
//! This module provides tools to simplify testing supervised tasks,
//! including mock implementations and assertion helpers.

use crate::contracts::SupervisedTask;
use crate::enums::{BackoffStrategy, HealthStatus, RestartPolicy, TaskState};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::Mutex;

/// A mock task that can be programmed to succeed or fail.
#[derive(Clone)]
pub struct MockTask {
    pub id: &'static str,
    pub fail_count: Arc<AtomicUsize>,
    pub max_fails: Arc<AtomicUsize>,
    pub run_count: Arc<AtomicUsize>,
    pub setup_called: Arc<AtomicBool>,
    pub cleanup_called: Arc<AtomicBool>,
    pub shutdown_called: Arc<AtomicBool>,
    pub restart_policy: Arc<Mutex<RestartPolicy>>,
    pub backoff: Arc<Mutex<BackoffStrategy>>,
    pub priority: i32,
    pub concurrency_limit: Option<usize>,
}

impl MockTask {
    /// Creates a new `MockTask` with the given ID.
    pub fn new(id: &'static str) -> Self {
        Self {
            id,
            fail_count: Arc::new(AtomicUsize::new(0)),
            max_fails: Arc::new(AtomicUsize::new(0)),
            run_count: Arc::new(AtomicUsize::new(0)),
            setup_called: Arc::new(AtomicBool::new(false)),
            cleanup_called: Arc::new(AtomicBool::new(false)),
            shutdown_called: Arc::new(AtomicBool::new(false)),
            restart_policy: Arc::new(Mutex::new(RestartPolicy::Always)),
            backoff: Arc::new(Mutex::new(BackoffStrategy::Fixed(Duration::from_millis(10)))),
            priority: 0,
            concurrency_limit: None,
        }
    }

    /// Sets the number of times the task should fail before succeeding.
    pub fn fail_for(self, count: usize) -> Self {
        self.max_fails.store(count, Ordering::SeqCst);
        self
    }

    /// Sets the restart policy.
    pub async fn with_policy(self, policy: RestartPolicy) -> Self {
        let mut p = self.restart_policy.lock().await;
        *p = policy;
        self
    }

    /// Sets the backoff strategy.
    pub async fn with_backoff(self, backoff: BackoffStrategy) -> Self {
        let mut b = self.backoff.lock().await;
        *b = backoff;
        self
    }

    /// Returns the number of times `run()` was called.
    pub fn run_count(&self) -> usize {
        self.run_count.load(Ordering::SeqCst)
    }

    /// Returns `true` if `setup()` was called.
    pub fn setup_called(&self) -> bool {
        self.setup_called.load(Ordering::SeqCst)
    }

    /// Returns `true` if `cleanup()` was called.
    pub fn cleanup_called(&self) -> bool {
        self.cleanup_called.load(Ordering::SeqCst)
    }

    /// Returns `true` if `on_shutdown()` was called.
    pub fn shutdown_called(&self) -> bool {
        self.shutdown_called.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl SupervisedTask for MockTask {
    fn id(&self) -> &'static str { self.id }

    async fn setup(&self) -> anyhow::Result<()> {
        self.setup_called.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn run(&self) -> anyhow::Result<()> {
        self.run_count.fetch_add(1, Ordering::SeqCst);
        let current_fails = self.fail_count.fetch_add(1, Ordering::SeqCst);
        let max = self.max_fails.load(Ordering::SeqCst);
        if current_fails < max {
            anyhow::bail!("Simulated failure {}/{}", current_fails + 1, max);
        }
        Ok(())
    }

    async fn cleanup(&self) {
        self.cleanup_called.store(true, Ordering::SeqCst);
    }

    async fn on_shutdown(&self) {
        self.shutdown_called.store(true, Ordering::SeqCst);
    }

    fn restart_policy(&self) -> RestartPolicy {
        // MockTask uses internal policy tracking, trait method returns default
        RestartPolicy::Always
    }

    // We override these in specific tests or use internal logic
}

/// A mock prerequisite that can be manually resolved.
pub struct MockPrerequisite {
    resolved: Arc<AtomicBool>,
    name: &'static str,
}

impl MockPrerequisite {
    /// Creates a new `MockPrerequisite`.
    pub fn new(name: &'static str) -> (Self, Arc<AtomicBool>) {
        let resolved = Arc::new(AtomicBool::new(false));
        (
            Self {
                resolved: resolved.clone(),
                name,
            },
            resolved,
        )
    }

    /// Returns a future that resolves when the prerequisite is satisfied.
    pub async fn wait(&self) -> anyhow::Result<()> {
        while !self.resolved.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok(())
    }
}

/// A test harness that simplifies common supervision testing patterns.
pub struct TestHarness {
    pub supervisor: crate::Supervisor,
}

impl Default for TestHarness {
    fn default() -> Self {
        Self::new()
    }
}

impl TestHarness {
    /// Creates a new `TestHarness`.
    pub fn new() -> Self {
        Self {
            supervisor: crate::Supervisor::new(),
        }
    }

    /// Registers a `MockTask` and returns it for further assertions.
    pub fn add_mock(&mut self, id: &'static str) -> MockTask {
        let task = MockTask::new(id);
        self.supervisor = self.supervisor.clone().add(task.clone());
        task
    }

    /// Runs a test with fake time enabled.
    /// Note: This uses `tokio::time::pause()` which requires the `test-util` feature of tokio.
    pub async fn run_with_fake_time<F, Fut>(f: F) -> Fut::Output
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future,
    {
        tokio::time::pause();
        f().await
    }
}

/// Assertions for verifying task state in tests.
pub struct TaskAssertions<'a> {
    runtime: &'a crate::runtime::TaskRuntime,
}

impl<'a> TaskAssertions<'a> {
    /// Creates a new `TaskAssertions` instance for the given runtime.
    pub fn new(runtime: &'a crate::runtime::TaskRuntime) -> Self {
        Self { runtime }
    }

    /// Asserts that a task with the given ID exists and has the expected health status.
    pub async fn assert_health(&self, id: &str, expected: HealthStatus) {
        let info = self.runtime.get_task_info(id).await.expect("Task not found");
        assert_eq!(info.health, expected, "Task {} health mismatch", id);
    }
}

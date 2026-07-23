use crate::enums::AppMessage;
use crate::results::AppResult;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::task::{JoinHandle, spawn_blocking};
use tokio::{spawn, time};
use tracing::{error, warn};

/// Provides bounded blocking and async-bridging operations, plus
/// task-scheduling utilities (tick, timeout, backoff).
///
/// Registered in the DI container during `AppBuilder::build()`.
/// Inject as `Arc<Tokio>` into services that need to run blocking code
/// (e.g., Diesel queries) or bridge sync→async.
///
/// # Concurrency Control
///
/// `block()` and `run_async()` use **separate semaphores** because they
/// target different resource pools:
///
/// - `block()` dispatches to `spawn_blocking` on the **caller's runtime**
///   (or global fallback). Its semaphore bounds concurrent blocking thread usage.
/// - `run_async()` always uses the **global fallback runtime** via `block_on()`.
///   Its semaphore bounds concurrent sync→async bridges on that dedicated runtime.
///
/// Configure via `RuntimeConfig::max_concurrent_blocking_tasks()` and
/// `RuntimeConfig::max_concurrent_async_bridges()`.
#[derive(Clone)]
pub struct Tokio {
    block_semaphore: Arc<Semaphore>,
    run_async_semaphore: Arc<Semaphore>,
}

pub use tokio_util::sync::CancellationToken;

impl Tokio {
    /// Create a new `Tokio` with independent concurrency limits.
    ///
    /// # Arguments
    /// * `max_blocking` — Maximum concurrent `block()` calls. Bounds
    ///   `spawn_blocking` usage across all callers.
    /// * `max_async_bridges` — Maximum concurrent `run_async()` calls.
    ///   Bounds `block_on` usage on the global fallback runtime.
    pub(crate) fn new(max_blocking: usize, max_async_bridges: usize) -> Self {
        Self {
            block_semaphore: Arc::new(Semaphore::new(max_blocking)),
            run_async_semaphore: Arc::new(Semaphore::new(max_async_bridges)),
        }
    }

    /// Spawn a blocking function on the Tokio blocking thread pool,
    /// bounded by the configured concurrency limit.
    ///
    /// Acquires a semaphore permit **before** dispatching to `spawn_blocking`.
    /// The permit is held for the duration of the closure and released on
    /// completion (or panic). When the semaphore is full, the caller awaits
    /// (backpressure) — no blocking thread is wasted waiting.
    ///
    /// # Runtime Selection
    /// - **Inside a Tokio context**: dispatches to the caller's blocking pool.
    /// - **No Tokio context**: dispatches to the global fallback runtime.
    ///
    /// # Example
    /// ```ignore
    /// #[derive(Service)]
    /// struct UserService {
    ///     tokio: Arc<Tokio>,
    /// }
    ///
    /// impl UserService {
    ///     async fn find_user(&self, id: i64) -> AppResult<User> {
    ///         self.tokio.block(move || {
    ///             // Diesel query here
    ///             Ok(users::table.find(id).first(&mut conn?)?)
    ///         }).await
    ///     }
    /// }
    /// ```
    pub async fn block<F, R>(&self, f: F) -> AppResult<R>
    where
        F: FnOnce() -> AppResult<R> + Send + Sync + 'static,
        R: Send + 'static,
    {
        // Acquire permit in async context (no thread blocked while waiting)
        let _permit = self
            .block_semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| AppMessage::Infrastructure {
                message: "Blocking semaphore closed".into(),
                source: None,
            })?;

        let handle = if tokio::runtime::Handle::try_current().is_ok() {
            spawn_blocking(f)
        } else {
            crate::helpers::tokio::runtime().spawn_blocking(f)
        };

        handle.await.map_err(|e| AppMessage::Infrastructure {
            message: format!("Failed to spawn blocking task: {e}"),
            source: Some(Box::new(e)),
        })?
    }

    /// Bridge from blocking code into async: run a future to completion on the
    /// global fallback runtime, bounded by a dedicated concurrency limit.
    ///
    /// This is a **synchronous** function designed to be called from blocking contexts
    /// (e.g., inside a `block()` closure or a sync function). It uses the global fallback
    /// runtime's `block_on()` to execute the future, blocking the current thread until
    /// the future completes.
    ///
    /// The `run_async_semaphore` limits how many concurrent sync→async bridges can run
    /// on the dedicated runtime, preventing resource exhaustion.
    ///
    /// # When to Use
    /// When you're in a **blocking/sync context** and need to call async code.
    /// If you're already in an async context, prefer `.await` directly.
    ///
    /// # Example
    /// ```ignore
    /// #[derive(Service)]
    /// struct UserService {
    ///     tokio: Arc<Tokio>,
    /// }
    ///
    /// impl UserService {
    ///     // Called from blocking context (e.g., inside block() closure)
    ///     fn find_user_sync(&self, id: i64) -> AppResult<User> {
    ///         // Bridge from sync to async
    ///         self.tokio.run_async(async {
    ///             // async database call
    ///             db.find_user(id).await
    ///         })
    ///     }
    /// }
    /// ```
    pub fn run_async<F, R>(&self, fut: F) -> AppResult<R>
    where
        F: Future<Output = AppResult<R>>,
    {
        let sem = &self.run_async_semaphore;
        crate::helpers::tokio::runtime().block_on(async {
            let _permit = sem
                .acquire()
                .await
                .map_err(|_| AppMessage::Infrastructure {
                    message: "Run-async semaphore closed".into(),
                    source: None,
                })?;
            fut.await
        })
    }

    /// Run a function once after a delay.
    ///
    /// Returns a [`JoinHandle`] so the caller can track or abort the task.
    /// The task checks `cancel` before executing - if cancelled, it exits early.
    pub fn timeout<Fun, Fut>(
        interval: u64,
        func: Fun,
        name: &str,
        cancel: CancellationToken,
    ) -> JoinHandle<()>
    where
        Fun: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = AppResult<()>> + Send + 'static,
    {
        let name = name.to_owned();
        spawn(async move {
            let mut interval = time::interval(Duration::from_millis(interval));

            interval.tick().await;
            tokio::select! {
                _ = interval.tick() => {}
                _ = cancel.cancelled() => {
                    tracing::debug!(task = %name, "Task cancelled before execution");
                    return;
                }
            }

            match func().await {
                Ok(_) => {}
                Err(err) => {
                    error!("[execution-error][{name}] {err:?}");
                }
            }
        })
    }

    /// Run a function repeatedly on a fixed interval.
    ///
    /// Returns a [`JoinHandle`] so the caller can track or abort the task.
    /// The loop exits when `cancel` is triggered.
    pub fn tick<Fun, Fut>(
        interval: u64,
        func: Fun,
        name: &str,
        cancel: CancellationToken,
    ) -> JoinHandle<()>
    where
        Fun: Fn() -> Fut + Send + 'static,
        Fut: Future<Output = AppResult<()>> + Send + 'static,
    {
        Self::tick_with_backoff(
            interval,
            func,
            name,
            Duration::from_millis(500),
            Duration::from_secs(30),
            cancel,
        )
    }

    /// Run a function repeatedly on a fixed interval with exponential backoff on consecutive errors.
    ///
    /// Returns a [`JoinHandle`] so the caller can track or abort the task.
    /// The loop exits when `cancel` is triggered.
    ///
    /// When the function returns an error, the delay between executions doubles starting from
    /// `min_backoff` up to `max_backoff`. The delay resets to the normal `interval` after a
    /// successful execution.
    pub fn tick_with_backoff<Fun, Fut>(
        interval: u64,
        func: Fun,
        name: &str,
        min_backoff: Duration,
        max_backoff: Duration,
        cancel: CancellationToken,
    ) -> JoinHandle<()>
    where
        Fun: Fn() -> Fut + Send + 'static,
        Fut: Future<Output = AppResult<()>> + Send + 'static,
    {
        let name = name.to_owned();
        spawn(async move {
            let base_interval = Duration::from_millis(interval);
            let mut current_delay = base_interval;
            let mut consecutive_errors: u32 = 0;

            loop {
                tokio::select! {
                    _ = time::sleep(current_delay) => {}
                    _ = cancel.cancelled() => {
                        tracing::debug!(task = %name, "Task cancelled, exiting loop");
                        return;
                    }
                }

                match func().await {
                    Ok(_) => {
                        if consecutive_errors > 0 {
                            warn!(
                                task = %name,
                                consecutive_errors,
                                "Task recovered after {} consecutive errors",
                                consecutive_errors,
                            );
                            consecutive_errors = 0;
                            current_delay = base_interval;
                        }
                    }
                    Err(err) => {
                        consecutive_errors += 1;
                        error!("[execution-error][{name}] {err:?}");

                        let factor = 2u32.saturating_pow(consecutive_errors.saturating_sub(1));
                        let backoff = min_backoff * factor;
                        current_delay = backoff.min(max_backoff);

                        warn!(
                            task = %name,
                            consecutive_errors,
                            next_retry_ms = current_delay.as_millis() as u64,
                            "Backing off after consecutive errors",
                        );
                    }
                }
            }
        })
    }
}

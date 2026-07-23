use std::sync::OnceLock;
use tokio::runtime::Runtime;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

/// Configuration for the global fallback Tokio runtime.
///
/// Set via [`AppBuilder::runtime_config()`] before calling `build()`.
/// Once the runtime is created, configuration changes have no effect.
///
/// # Example
///
/// ```no_run
/// use foxtive::App;
/// use foxtive::helpers::RuntimeConfig;
///
/// # async fn run() -> foxtive::results::AppResult<()> {
/// let app = App::builder("my-app", "MYAPP")
///     .runtime_config(RuntimeConfig::new()
///         .worker_threads(4)
///         .max_blocking_threads(64)
///         .thread_name("my-app-worker"))
///     .build()
///     .await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Number of worker threads (default: number of CPU cores).
    pub worker_threads: Option<usize>,
    /// Maximum number of blocking threads (default: 512).
    pub max_blocking_threads: Option<usize>,
    /// Name prefix for worker threads (default: "foxtive-worker").
    pub thread_name: Option<String>,
    /// Enable the I/O driver (default: true).
    pub enable_io: bool,
    /// Enable the timer (default: true).
    pub enable_time: bool,
    /// Maximum concurrent `Tokio::block()` calls (default: 512).
    /// Bounds `spawn_blocking` usage across all callers.
    pub max_concurrent_blocking_tasks: Option<usize>,
    /// Maximum concurrent `Tokio::run_async()` calls (default: 128).
    /// Bounds `block_on` usage on the global fallback runtime.
    pub max_concurrent_async_bridges: Option<usize>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            worker_threads: None,
            max_blocking_threads: None,
            thread_name: None,
            enable_io: true,
            enable_time: true,
            max_concurrent_blocking_tasks: None,
            max_concurrent_async_bridges: None,
        }
    }
}

#[allow(dead_code)]
impl RuntimeConfig {
    /// Create a new configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the number of worker threads.
    pub fn worker_threads(mut self, count: usize) -> Self {
        self.worker_threads = Some(count);
        self
    }

    /// Set the maximum number of blocking threads.
    pub fn max_blocking_threads(mut self, count: usize) -> Self {
        self.max_blocking_threads = Some(count);
        self
    }

    /// Set the thread name prefix.
    pub fn thread_name(mut self, name: impl Into<String>) -> Self {
        self.thread_name = Some(name.into());
        self
    }

    /// Set the maximum number of concurrent `Tokio::block()` calls.
    ///
    /// Controls the semaphore that bounds `Tokio::block()`. When the limit
    /// is reached, new calls wait until a slot frees up (backpressure).
    ///
    /// Default: 512. Set based on expected concurrent blocking operations
    /// (e.g., database queries). Should not exceed `max_blocking_threads`.
    pub fn max_concurrent_blocking_tasks(mut self, count: usize) -> Self {
        self.max_concurrent_blocking_tasks = Some(count);
        self
    }

    /// Set the maximum number of concurrent `Tokio::run_async()` calls.
    ///
    /// Controls the semaphore that bounds `Tokio::run_async()`. When the
    /// limit is reached, new calls wait until a slot frees up.
    ///
    /// Default: 128. This is lower than `max_concurrent_blocking_tasks`
    /// because `run_async()` holds a dedicated runtime thread for the
    /// entire duration of the future.
    pub fn max_concurrent_async_bridges(mut self, count: usize) -> Self {
        self.max_concurrent_async_bridges = Some(count);
        self
    }
}

static RUNTIME_CONFIG: OnceLock<RuntimeConfig> = OnceLock::new();

/// Set the global fallback runtime configuration.
///
/// Called by `AppBuilder::build_inner()` before the runtime is first used.
/// Subsequent calls are ignored (OnceLock semantics).
pub(crate) fn set_runtime_config(config: RuntimeConfig) {
    let _ = RUNTIME_CONFIG.set(config);
}

fn build_runtime() -> Runtime {
    let config = RUNTIME_CONFIG.get().cloned().unwrap_or_default();
    let mut builder = tokio::runtime::Builder::new_multi_thread();

    if let Some(workers) = config.worker_threads {
        builder.worker_threads(workers);
    }
    if let Some(blocking) = config.max_blocking_threads {
        builder.max_blocking_threads(blocking);
    }
    builder.thread_name(config.thread_name.as_deref().unwrap_or("foxtive-worker"));
    if config.enable_io {
        builder.enable_io();
    }
    if config.enable_time {
        builder.enable_time();
    }

    builder.build().expect("Failed to create Tokio runtime")
}

pub(crate) fn runtime() -> &'static Runtime {
    RUNTIME.get_or_init(build_runtime)
}

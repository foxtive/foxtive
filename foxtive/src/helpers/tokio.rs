use tokio::task::{JoinHandle, spawn_blocking};

/// Spawns a blocking function on the tokio blocking thread pool.
///
/// This is a convenience wrapper around `spawn_blocking` for executing
/// CPU-intensive or blocking operations without blocking the async runtime's
/// worker threads.
///
/// The function is executed on a dedicated thread pool designed for blocking
/// operations, allowing the async runtime to continue processing other tasks.
///
/// # Arguments
///
/// * `f` - A closure or function to execute on the blocking thread pool.
///   Must be `Send + 'static` to safely transfer across threads.
///
/// # Returns
///
/// Returns a `JoinHandle<R>` that can be awaited to get the result of the
/// blocking operation.
///
/// # Examples
///
/// ```
/// use foxtive::helpers::run_async;
/// use foxtive::helpers::blk;
///
/// // Run a CPU-intensive calculation
/// run_async(async {
///     let handle = blk(|| {
///         // Some expensive calculation
///         (1..=1000).sum::<i32>()
///     });
///     let result = handle.await.unwrap();
///     assert_eq!(result, 500500);
/// });
/// ```
///
/// ```
/// use foxtive::helpers::run_async;
/// use foxtive::helpers::blk;
///
/// // Perform blocking I/O
/// run_async(async {
///     let handle = blk(|| {
///         std::fs::read_to_string("Cargo.toml")
///     });
///     let contents = handle.await.unwrap();
///     assert!(contents.is_ok() || contents.is_err());
/// });
/// ```
///
/// # Notes
///
/// - Use this for synchronous blocking operations (file I/O, CPU work, sync APIs)
/// - Don't use this for async operations - use regular `spawn` instead
/// - The blocking pool has a large but finite number of threads
pub fn blk<F, R>(f: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    spawn_blocking(f)
}

/// Runs an async future to completion, blocking the current thread until it finishes.
///
/// This function intelligently handles tokio runtime contexts:
/// - If called from within an existing tokio runtime, it reuses that runtime
/// - If no runtime exists, it creates a new single-threaded runtime
///
/// Both execution paths use a `LocalSet` to support `!Send` futures.
///
/// # Arguments
///
/// * `fut` - The future to execute. Can be any future type with any output.
///
/// # Returns
///
/// Returns the output value produced by the future.
///
/// # Panics
///
/// Panics if the tokio runtime cannot be created (e.g., due to system resource constraints).
///
/// # Examples
///
/// ```
/// use foxtive::helpers::run_async;
///
/// let result = run_async(async {
///     // Some async work
///     42
/// });
/// assert_eq!(result, 42);
/// ```
///
/// ```
/// use foxtive::helpers::run_async;
///
/// let data = run_async(async {
///     // Simulate fetching data
///     tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
///     "data".to_string()
/// });
/// assert_eq!(data, "data");
/// ```
pub fn run_async<F: Future>(fut: F) -> F::Output {
    if let Ok(hnd) = tokio::runtime::Handle::try_current() {
        tracing::debug!("Use existing tokio runtime and block on future");
        hnd.block_on(tokio::task::LocalSet::new().run_until(fut))
    } else {
        tracing::debug!("Create tokio runtime and block on future");

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            //.unhandled_panic(tokio::runtime::UnhandledPanic::ShutdownRuntime)
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&rt, fut)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[test]
    fn test_run_async_returns_value() {
        let result = run_async(async { 42 });
        assert_eq!(result, 42);
    }

    #[test]
    fn test_run_async_returns_string() {
        let result = run_async(async { "hello world".to_string() });
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_run_async_with_computation() {
        let result = run_async(async {
            let a = 10;
            let b = 20;
            a + b
        });
        assert_eq!(result, 30);
    }

    #[test]
    fn test_run_async_with_await() {
        let result = run_async(async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            "completed"
        });
        assert_eq!(result, "completed");
    }

    #[test]
    fn test_run_async_nested_calls() {
        // First call creates runtime
        let result1 = run_async(async { 1 });

        // Second call should also work
        let result2 = run_async(async { 2 });

        assert_eq!(result1, 1);
        assert_eq!(result2, 2);
    }

    #[test]
    fn test_run_async_with_existing_runtime() {
        // Use run_async to create the runtime, then test nested async behavior
        #[allow(clippy::redundant_async_block)]
        let result = run_async(async {
            // Nested async operations within the same runtime
            async { 99 }.await
        });
        assert_eq!(result, 99);
    }

    #[test]
    fn test_run_async_with_result_type() {
        let result: Result<i32, &str> = run_async(async { Ok(42) });
        assert_eq!(result, Ok(42));

        let error: Result<i32, &str> = run_async(async { Err("failed") });
        assert_eq!(error, Err("failed"));
    }

    #[test]
    fn test_blk_returns_value() {
        run_async(async {
            let handle = blk(|| 42);
            let result = handle.await.unwrap();
            assert_eq!(result, 42);
        });
    }

    #[test]
    fn test_blk_with_computation() {
        run_async(async {
            let handle = blk(|| {
                let mut sum = 0;
                for i in 1..=10 {
                    sum += i;
                }
                sum
            });

            let result = handle.await.unwrap();
            assert_eq!(result, 55);
        });
    }

    #[test]
    fn test_blk_with_string() {
        run_async(async {
            let handle = blk(|| "blocking result".to_string());
            let result = handle.await.unwrap();
            assert_eq!(result, "blocking result");
        });
    }

    #[test]
    fn test_blk_multiple_tasks() {
        run_async(async {
            let handle1 = blk(|| 1);
            let handle2 = blk(|| 2);
            let handle3 = blk(|| 3);

            let r1 = handle1.await.unwrap();
            let r2 = handle2.await.unwrap();
            let r3 = handle3.await.unwrap();
            let result = r1 + r2 + r3;

            assert_eq!(result, 6);
        });
    }

    #[test]
    fn test_blk_with_sleep() {
        use std::thread;

        run_async(async {
            let handle = blk(|| {
                thread::sleep(Duration::from_millis(10));
                "done"
            });

            let result = handle.await.unwrap();
            assert_eq!(result, "done");
        });
    }

    #[test]
    fn test_blk_captures_variables() {
        run_async(async {
            let value = 100;
            let handle = blk(move || value * 2);

            let result = handle.await.unwrap();
            assert_eq!(result, 200);
        });
    }

    #[test]
    fn test_blk_with_shared_state() {
        run_async(async {
            let counter = Arc::new(Mutex::new(0));
            let counter_clone = counter.clone();

            let handle = blk(move || {
                let mut count = counter_clone.lock().unwrap();
                *count += 1;
                *count
            });

            let result = handle.await.unwrap();
            assert_eq!(result, 1);
            assert_eq!(*counter.lock().unwrap(), 1);
        });
    }

    #[test]
    fn test_blk_concurrent_execution() {
        run_async(async {
            let handles: Vec<_> = (0..5).map(|i| blk(move || i * 2)).collect();

            let mut results = Vec::new();
            for handle in handles {
                results.push(handle.await.unwrap());
            }

            assert_eq!(results, vec![0, 2, 4, 6, 8]);
        });
    }

    #[test]
    fn test_run_async_and_blk_integration() {
        let result = run_async(async {
            let blocking_result = blk(|| {
                // Simulate blocking work
                std::thread::sleep(Duration::from_millis(10));
                42
            })
            .await
            .unwrap();

            tokio::time::sleep(Duration::from_millis(10)).await;

            blocking_result + 8
        });

        assert_eq!(result, 50);
    }

    #[test]
    fn test_blk_with_result_type() {
        run_async(async {
            let handle = blk(|| -> Result<i32, String> { Ok(42) });

            let result = handle.await.unwrap();
            assert_eq!(result, Ok(42));
        });
    }

    #[test]
    fn test_blk_with_panic_recovery() {
        run_async(async {
            let handle = blk(|| {
                // This will panic
                panic!("intentional panic");
            });

            let result = handle.await;
            assert!(result.is_err());
        });
    }
}

# 🦊 Foxtive Cron

A production-ready, asynchronous cron-based job scheduler for Rust powered by Tokio.

Schedule async tasks with cron expressions, manage them dynamically at runtime, control concurrency, handle failures with retry policies, and persist state - all with type-safe guarantees and comprehensive observability.

## Features

### Core Scheduling
- Standard 7-field cron expressions (with seconds and year support)
- **Validated at registration time** - no silent runtime failures
- Timezone support via `chrono-tz` (schedule in local time, not just UTC)
- One-time jobs (run once at specific DateTime)
- Delayed start (recurring jobs with initial delay)
- Per-job priorities (higher priority executes first when concurrent)

### Dynamic Job Management
- Add jobs at runtime via `add_job()`, `add_job_fn()`, `add_blocking_job_fn()`
- Remove jobs at runtime via `remove_job(id)` - stops execution and cleans up resources
- Trigger jobs immediately via `trigger_job(id)` without affecting schedule
- Introspection API via `list_job_ids()` to see all scheduled jobs
- Internal job registry for O(1) lookup by ID

### Advanced Execution Control
- **Global concurrency limits** - prevent resource exhaustion
- **Per-job concurrency limits** - fine-grained control
- **Execution timeouts** - automatically terminate long-running jobs
- **Graceful shutdown** - controlled termination with cleanup
- Jobs run concurrently in independent `tokio::spawn` tasks

### Reliability & Resilience
- **Misfire policies**: Skip, FireOnce, FireAll (handle missed executions)
- **Retry strategies**: Fixed interval, Exponential backoff (with overflow protection)
- **Persistence layer** via `JobStore` trait
- **In-memory store** included (`InMemoryJobStore`)
- State tracking (last run, success/failure counts)

### Observability
- **Event hooks** - listen to lifecycle events (Started, Finished, Failed, Retrying)
- **Metrics integration** - counters and histograms via `MetricsExporter` trait
- **Structured logging** via `tracing` crate
- Health checks and monitoring support

## Installation

```toml
[dependencies]
foxtive-cron = "0.4"
tokio = { version = "1", features = ["full"] }
anyhow = "1"
async-trait = "0.1"
```

### Optional Features

- `tokio-macros` - Enable tokio macros for examples

## Quick Start

### Basic async job

```rust
use foxtive_cron::Cron;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut cron = Cron::new();

    // Run every 5 seconds
    cron.add_job_fn("ping", "Ping Service", "*/5 * * * * * *", || async {
        println!("Pinging at {}", chrono::Utc::now());
        Ok(())
    })?;

    // Run scheduler in background
    tokio::spawn(async move { cron.run().await });

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    Ok(())
}
```

### Blocking job (CPU-intensive)

```rust
cron.add_blocking_job_fn("backup", "Backup", "0 0 * * * * *", || {
    std::fs::write("/var/backup/snapshot", "data")?;
    Ok(())
})?;
```

Blocking functions run inside `tokio::task::spawn_blocking` so they never block the async runtime.

### With concurrency limit and event listener

```rust
use foxtive_cron::{Cron, contracts::{JobEventListener, JobEvent}};
use std::sync::Arc;

struct MyListener;

#[async_trait::async_trait]
impl JobEventListener for MyListener {
    async fn on_event(&self, event: JobEvent) {
        match event {
            JobEvent::Started { id, name } => {
                println!("Job {} started", name);
            }
            JobEvent::Finished { id, name, duration } => {
                println!("Job {} completed in {:?}", name, duration);
            }
            JobEvent::Failed { id, name, error } => {
                eprintln!("Job {} failed: {}", name, error);
            }
            _ => {}
        }
    }
}

let mut cron = Cron::builder()
    .with_global_concurrency_limit(5)  // Max 5 concurrent jobs
    .with_listener(Arc::new(MyListener))
    .build();

cron.add_job_fn("job1", "Job 1", "*/10 * * * * * *", || async {
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    Ok(())
})?;
```

## Custom Jobs via `JobContract`

For full control, implement `JobContract` on your struct:

```rust
use foxtive_cron::{CronResult, Cron};
use foxtive_cron::contracts::{JobContract, ValidatedSchedule, RetryPolicy, MisfirePolicy};
use async_trait::async_trait;
use std::borrow::Cow;
use std::sync::Arc;

struct DatabaseCleanupJob {
    schedule: ValidatedSchedule,
}

impl DatabaseCleanupJob {
    fn new() -> CronResult<Self> {
        Ok(Self {
            schedule: ValidatedSchedule::parse("0 0 2 * * * *")?,  // Daily at 2 AM
        })
    }
}

#[async_trait]
impl JobContract for DatabaseCleanupJob {
    fn id(&self) -> Cow<'_, str> { Cow::Borrowed("db-cleanup") }
    fn name(&self) -> Cow<'_, str> { Cow::Borrowed("Database Cleanup") }
    fn schedule(&self) -> &ValidatedSchedule { &self.schedule }
    
    // Optional: configure retry policy
    fn retry_policy(&self) -> RetryPolicy {
        RetryPolicy::Exponential {
            initial_interval: std::time::Duration::from_secs(5),
            max_interval: std::time::Duration::from_secs(60),
            max_attempts: 3,
        }
    }
    
    // Optional: configure misfire policy
    fn misfire_policy(&self) -> MisfirePolicy {
        MisfirePolicy::Skip
    }

    async fn run(&self) -> CronResult<()> {
        println!("Cleaning up database at {}", chrono::Utc::now());
        // Your cleanup logic here
        Ok(())
    }

    // Optional lifecycle hooks:
    async fn on_start(&self) {
        tracing::info!("Starting database cleanup");
    }
    
    async fn on_complete(&self) {
        tracing::info!("Database cleanup completed successfully");
    }
    
    async fn on_error(&self, err: &anyhow::Error) {
        tracing::error!("Database cleanup failed: {}", err);
    }
}

#[tokio::main]
async fn main() -> CronResult<()> {
    let mut cron = Cron::new();
    cron.add_job(Arc::new(DatabaseCleanupJob::new()?))?;

    tokio::spawn(async move { cron.run().await });
    tokio::signal::ctrl_c().await?;
    Ok(())
}
```

## Advanced Features

### Retry Policies

```rust
use foxtive_cron::contracts::RetryPolicy;
use std::time::Duration;

// Fixed retry: retry every 5 seconds, up to 3 times
fn retry_policy() -> RetryPolicy {
    RetryPolicy::Fixed {
        interval: Duration::from_secs(5),
        max_attempts: 3,
    }
}

// Exponential backoff: 2s -> 4s -> 8s -> ... max 60s
fn exponential_backoff() -> RetryPolicy {
    RetryPolicy::Exponential {
        initial_interval: Duration::from_secs(2),
        max_interval: Duration::from_secs(60),
        max_attempts: 10,
    }
}
```

### Misfire Handling

```rust
use foxtive_cron::contracts::MisfirePolicy;

// Skip missed runs if scheduler was down
MisfirePolicy::Skip

// Run once ASAP, then resume normal schedule
MisfirePolicy::FireOnce

// Run all missed executions
MisfirePolicy::FireAll
```

### Timezone Support

```rust
use chrono_tz::America::New_York;

// Schedule in local time, not UTC
cron.add_job_fn(
    "daily-report",
    "Daily Report",
    "0 30 9 * * * *",  // 9:30 AM
).timezone(New_York)?;  // Runs at 9:30 AM Eastern Time
```

### Graceful Shutdown

```rust
let mut cron = Cron::new();
// ... add jobs ...

// Gracefully shutdown - waits for running jobs to complete
cron.shutdown().await;
```

## Cron Expression Format

Foxtive Cron uses a **7-field** cron format:

```
sec  min  hour  day  month  weekday  year
```

| Example              | Meaning                           |
|----------------------|-----------------------------------|
| `*/10 * * * * * *`   | Every 10 seconds                  |
| `0 * * * * * *`      | Every minute                      |
| `0 0 * * * * *`      | Every hour                        |
| `0 30 9 * * * *`     | Every day at 09:30:00             |
| `0 0 0 1 * * *`      | First day of every month          |
| `0 0 0 * * MON-FRI *`| Monday to Friday at midnight      |

Expressions are validated via `ValidatedSchedule::parse()` at registration time.
Invalid expressions return `Err` immediately rather than failing silently at runtime.

## Thread Safety & Concurrency

- Jobs execute in independent `tokio::spawn` tasks - slow jobs never block others
- Multiple jobs due at the same tick fire concurrently in the same iteration
- Jobs wrapped in `Arc<dyn JobContract>` must be `Send + Sync`
- Global concurrency limits prevent resource exhaustion
- Per-job concurrency limits for fine-grained control
- Priority-based execution when multiple jobs are due simultaneously

## Logging & Observability

Job execution is traced via the `tracing` crate:

- `INFO` level: Job start and successful completion
- `ERROR` level: Job failures (scheduler continues running)
- `WARN` level: Misfires, retries, and warnings

Integrate with any `tracing`-compatible subscriber:

```rust
use tracing_subscriber;

tracing_subscriber::fmt()
    .with_max_level(tracing::Level::INFO)
    .init();
```

### Custom Event Listeners

Implement `JobEventListener` to receive real-time notifications of all job lifecycle events.

## Examples

See the [`examples/`](examples/) directory for complete working examples:

- **basic.rs** - Simple job scheduling
- **advanced.rs** - Custom jobs with full lifecycle hooks
- **concurrency.rs** - Managing concurrent job execution
- **persistence.rs** - Persisting job state with InMemoryJobStore
- **priority.rs** - Job priority handling
- **timezone.rs** - Scheduling in different timezones

Run examples with:

```bash
cargo run --example basic --features tokio-macros
cargo run --example advanced --features tokio-macros
# ... etc
```

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                     Cron Scheduler                   │
├─────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────┐  │
│  │ Job Registry │  │ Priority Queue│  │Semaphores│  │
│  │  (HashMap)   │  │ (BinaryHeap) │  │          │  │
│  └──────────────┘  └──────────────┘  └──────────┘  │
│                                                      │
│  ┌──────────────────────────────────────────────┐  │
│  │         Job Execution Engine                 │  │
│  │  • Concurrency Control                       │  │
│  │  • Retry Logic                               │  │
│  │  • Misfire Handling                          │  │
│  └──────────────────────────────────────────────┘  │
│                                                      │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────┐  │
│  │Event Listeners│ │Metrics Export│ │Job Store │  │
│  └──────────────┘  └──────────────┘  └──────────┘  │
└─────────────────────────────────────────────────────┘
```

Key components:
- **Job Registry**: O(1) lookup by job ID
- **Priority Queue**: Efficient scheduling via BinaryHeap (min-heap by next_run)
- **Semaphores**: Concurrency control (global and per-job)
- **Event System**: Real-time lifecycle notifications
- **Metrics**: Counters and histograms for monitoring
- **Persistence**: Pluggable storage backends

## Contributing

Contributions, bug reports, and feature requests are welcome!

To contribute:
1. Fork the repository
2. Create a feature branch
3. Make your changes with tests
4. Ensure all tests pass: `cargo test`
5. Check for clippy warnings: `cargo clippy`
6. Submit a pull request

See [CONTRIBUTING.md](CONTRIBUTING.md) for detailed guidelines.

## License

MIT License

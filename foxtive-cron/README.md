# 🦊 Foxtive Cron

A production-ready, asynchronous cron-based job scheduler for Rust powered by Tokio.

Schedule async tasks with cron expressions, manage them dynamically at runtime, control concurrency, handle failures with retry policies, and persist state - all with type-safe guarantees and comprehensive observability.

## Features

### Cron Expression Builder (NEW in 0.5.0)
- **Fluent builder API** - Build cron expressions programmatically with type safety
- **Type-safe enums** - `Month` and `Weekday` enums prevent invalid values
- **Field composition** - Intervals, ranges, lists, and single values for all fields
- **Common presets** - `hourly()`, `daily()`, `weekly()`, `monthly()` shortcuts
- **Timezone support** - Schedule jobs in any timezone via `chrono-tz`
- **Blackout dates** - Exclude specific dates (holidays, maintenance windows)
- **Execution jitter** - Add random delays to prevent thundering herd problems
- **Compile-time validation** - Invalid expressions caught at build time

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

### Using the Cron Expression Builder (Recommended)

```rust
use foxtive_cron::builder::{CronExpression, Weekday};
use chrono_tz::US::Eastern;
use std::time::Duration;

// Build a cron expression using the fluent API
let schedule = CronExpression::builder()
    .weekdays_only()
    .hours_range(9, 17)
    .minutes_interval(30)
    .with_timezone(Eastern)
    .with_jitter(Duration::from_secs(30))
    .build();

println!("Generated expression: {}", schedule);
// Output: "0 */30 9-17 * * 1-5 *"
```

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

### Builder API with Advanced Features

```rust
use foxtive_cron::builder::{CronExpression, Month, Weekday};
use chrono::NaiveDate;
use chrono_tz::Europe::London;
use std::time::Duration;

// Complex schedule with multiple features
let schedule = CronExpression::builder()
    .weekdays_only()                          // Monday-Friday only
    .hours_range(9, 17)                      // Business hours
    .minutes_interval(30)                    // Every 30 minutes
    .with_timezone(London)                   // London timezone
    .with_jitter(Duration::from_secs(60))   // ±60s random delay
    .exclude_date(NaiveDate::from_ymd_opt(2024, 12, 25).unwrap()) // Christmas
    .exclude_date(NaiveDate::from_ymd_opt(2024, 12, 26).unwrap()) // Boxing Day
    .build();

println!("Schedule: {}", schedule);
```

### Common Builder Patterns

```rust
// Daily backup at 2:30 AM
let daily_backup = CronExpression::builder()
    .daily()
    .hour(2)
    .minute(30)
    .build();

// Health check every 30 seconds
let health_check = CronExpression::builder()
    .seconds_interval(30)
    .build();

// Monthly report on 1st at 9 AM
let monthly_report = CronExpression::builder()
    .monthly()
    .hour(9)
    .build();

// Multiple specific times per day
let digest_emails = CronExpression::builder()
    .daily()
    .hours_list(&[8, 12, 18])  // 8 AM, 12 PM, 6 PM
    .minute(0)
    .build();
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

### Builder API Examples (NEW)
- **builder_basic.rs** - Comprehensive cron expression builder demonstrations
- **builder_real_world_scenarios.rs** - Production-ready scheduling patterns
- **builder_timezone_advanced.rs** - Advanced timezone-aware scheduling
- **builder_blackout_and_jitter.rs** - Blackout dates and jitter strategies
- **builder_complex_composition.rs** - Complex field composition patterns
- **builder_devops_automation.rs** - DevOps and infrastructure automation
- **builder_edge_cases.rs** - Edge cases and validation scenarios

### Core Features
- **basic.rs** - Simple job scheduling
- **advanced.rs** - Custom jobs with full lifecycle hooks
- **concurrency.rs** - Managing concurrent job execution
- **persistence.rs** - Persisting job state with InMemoryJobStore
- **priority.rs** - Job priority handling
- **timezone.rs** - Scheduling in different timezones

Run examples with:

```bash
cargo run --example builder_basic
cargo run --example builder_real_world_scenarios --features tokio-macros
cargo run --example builder_timezone_advanced
cargo run --example builder_blackout_and_jitter
cargo run --example builder_complex_composition
cargo run --example builder_devops_automation
cargo run --example builder_edge_cases
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

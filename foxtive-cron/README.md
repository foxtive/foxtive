# 🦊 Foxtive Cron

A lightweight, asynchronous, cron-based job scheduler for Rust powered by Tokio.
Schedule and execute async tasks, including closures and blocking code, using standard
cron expressions with per-second precision.

## Features

- Schedule jobs using standard 7-field cron expressions (with seconds)
- Cron expressions are **validated at registration time**, no silent runtime failures
- Supports both async and blocking closures out of the box
- Fully extensible via the `JobContract` trait for custom job types
- Lifecycle hooks: `on_start`, `on_complete`, `on_error`
- Concurrent job execution, multiple jobs due at the same tick all fire together
- Efficient scheduling via `BinaryHeap` (min-heap by next run time)
- Built for the Tokio async runtime
- Simple, ergonomic API

## Installation

```toml
[dependencies]
foxtive-cron = "0.1"
tokio = { version = "1", features = ["full"] }
```

## Usage

### Async closure

```rust
use foxtive_cron::Cron;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut cron = Cron::new();

    // id, human-readable name, cron expression, async closure
    cron.add_job_fn("ping", "Ping", "*/5 * * * * * *", || async {
        println!("Ping at {}", chrono::Utc::now());
        Ok(())
    })?;

    tokio::spawn(async move { cron.run().await });

    tokio::signal::ctrl_c().await?;
    Ok(())
}
```

### Blocking closure

```rust
cron.add_blocking_job_fn("backup", "Backup", "0 0 * * * * *", || {
    std::fs::write("/var/backup/snapshot", "data")?;
    Ok(())
})?;
```

The blocking function runs inside `tokio::task::spawn_blocking` so it never stalls
the async runtime.

## Custom Jobs via `JobContract`

For full control, implement `JobContract` directly on your own struct:

```rust
use foxtive_cron::{CronResult, Cron};
use foxtive_cron::contracts::{JobContract, ValidatedSchedule};
use async_trait::async_trait;
use std::borrow::Cow;
use std::sync::Arc;

struct MyJob {
    schedule: ValidatedSchedule,
}

impl MyJob {
    fn new() -> CronResult<Self> {
        Ok(Self {
            schedule: ValidatedSchedule::parse("0 * * * * * *")?,
        })
    }
}

#[async_trait]
impl JobContract for MyJob {
    fn id(&self) -> Cow<'_, str> { Cow::Borrowed("my-job") }
    fn name(&self) -> Cow<'_, str> { Cow::Borrowed("My Job") }
    fn schedule(&self) -> &ValidatedSchedule { &self.schedule }

    async fn run(&self) -> CronResult<()> {
        println!("Running my custom job at {}", chrono::Utc::now());
        Ok(())
    }

    // Optional lifecycle hooks (all default to no-ops):
    async fn on_start(&self) { println!("Starting..."); }
    async fn on_complete(&self) { println!("Done."); }
    async fn on_error(&self, err: &anyhow::Error) { eprintln!("Error: {err}"); }
}

#[tokio::main]
async fn main() -> CronResult<()> {
    let mut cron = Cron::new();
    cron.add_job(Arc::new(MyJob::new()?))?;

    tokio::spawn(async move { cron.run().await });

    tokio::signal::ctrl_c().await?;
    Ok(())
}
```

## Cron Expression Format

Foxtive Cron uses a **7-field** cron format:

```
sec  min  hour  day  month  weekday  year
```

| Example              | Meaning                    |
|----------------------|----------------------------|
| `*/10 * * * * * *`   | Every 10 seconds           |
| `0 * * * * * *`      | Every minute               |
| `0 0 * * * * *`      | Every hour                 |
| `0 30 9 * * * *`     | Every day at 09:30:00      |
| `0 0 0 1 * * *`      | First day of every month   |

Expressions are validated via `ValidatedSchedule::parse` at registration time.
An invalid expression returns an `Err` immediately rather than failing silently at runtime.

## Lifecycle Hooks

Every job can optionally implement three hooks:

| Hook          | Called when                        |
|---------------|------------------------------------|
| `on_start`    | Just before `run` is invoked       |
| `on_complete` | After `run` returns `Ok`           |
| `on_error`    | After `run` returns `Err`          |

All three default to no-ops, so you only implement what you need.

## Thread Safety & Concurrency

- Jobs are executed in independent `tokio::spawn` tasks — a slow job never blocks others.
- Multiple jobs due at the same tick all fire concurrently in the same scheduler iteration.
- Jobs are wrapped in `Arc<dyn JobContract>` and are required to be `Send + Sync`.

## Logging

Job execution is traced via the `tracing` crate:

- `INFO` on job start and successful completion
- `ERROR` on job failure (the scheduler continues running)

Integrate with any `tracing`-compatible subscriber such as `tracing-subscriber`.

## Roadmap

- [ ] Graceful shutdown support
- [ ] Pause / resume individual jobs
- [ ] Remove a scheduled job by ID
- [ ] Configurable retry logic on failure
- [ ] Persistence / job state recovery

## 🙌 Contributing

Contributions, bug reports, and feature requests are welcome.
Feel free to open issues or pull requests.

## License

MIT License
# Foxtive Cron
Foxtive Cron is a lightweight, asynchronous, cron-based job scheduler for Rust powered by Tokio. 
It allows you to schedule and execute asynchronous tasks (including closures and blocking code) 
using standard cron expressions with precision down to the second.

## ✨ Features
- Schedule jobs using standard cron expressions (with seconds)
- Supports both async and blocking closures
- Fully extensible with custom job types via the JobContract trait
- Uses BinaryHeap to efficiently manage job execution order
- Built for tokio async runtime
- Simple and ergonomic API

## 📦 Installation
Add the following to your Cargo.toml:
```toml
[dependencies]
foxtive-cron = "0.1"
tokio = { version = "1", features = ["full"] }
```

## 🚀 Usage
```rust
use foxtive_cron::Cron;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut cron = Cron::new();

    // Run every 5 seconds
    cron.add_job_fn("Ping", "*/5 * * * * * *", || async {
        println!("Ping at {}", chrono::Utc::now());
        Ok(())
    })?;

    tokio::spawn(async move {
        cron.run().await;
    });

    // keep the main task alive
    tokio::signal::ctrl_c().await?;
    Ok(())
}

```

## 🛠 Advanced Usage
Custom Job with JobContract
You can define your own job logic by implementing the JobContract trait:
```rust
use foxtive_cron::CronResult;
use foxtive_cron::contracts::JobContract;
use async_trait::async_trait;
use std::sync::Arc;

struct MyJob;

#[async_trait]
impl JobContract for MyJob {
    fn name(&self) -> &str {
        "MyCustomJob"
    }

    fn schedule(&self) -> &str {
        "0 * * * * * *"
    }

    async fn run(&self) -> foxtive_cron::CronResult<()> {
        println!("Running my custom job!");
        Ok(())
    }
}

#[tokio::main]
async fn main() -> CronResult<()> {
    let mut cron = Cron::new();

    // Register:
    cron.add_job(Arc::new(MyJob))?;
    
    tokio::spawn(async move {
        cron.run().await;
    });

    // keep the main task alive
    tokio::signal::ctrl_c().await?;
    Ok(())
}

```

## ⏰ Cron Expression Format
This library uses a 7-field cron format:
sec min hour day month weekday year

Examples:

- `*/10 * * * * * *` → every 10 seconds
- `0 0 * * * * *` → top of every hour
- `5 0 12 * * * *` → at 12:00:05 every day

## Thread Safety
Jobs are executed in separate tasks and can be cloned using Arc. 
The internal scheduler uses BinaryHeap to prioritize the next job run time efficiently.

## Logging
This library logs job execution status using the log crate.

- Logs job start, completion, and errors
- Integrate with any compatible logging framework like env_logger, tracing, etc.

## Roadmap
Add persistence / job state recovery

- Graceful shutdown support
- Pause/resume jobs
- Remove scheduled jobs
- Better error recovery / retries

## 🙌 Contributing
Contributions, bug reports, and feature requests are welcome! Feel free to open issues or PRs.

## License
This project is licensed under the MIT License.




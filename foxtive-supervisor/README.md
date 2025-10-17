# 🦊 Foxtive Supervisor

> Because tasks crash. We help them get back up.

A Rust supervision library that keeps your async tasks running, even when things go wrong. Think of it as a caring parent for your background jobs.

## Why?

In production, things fail:
- Network hiccups
- Temporary resource exhaustion
- Race conditions you didn't catch in testing
- That one edge case from 3am

**Foxtive Supervisor** automatically restarts your tasks with configurable policies, handles panics gracefully, and gives you hooks to observe what's happening.

## Installation

```toml
[dependencies]
foxtive-supervisor = "0.1"
tokio = { version = "1", features = ["full"] }
anyhow = "1"
async-trait = "0.1"
```

## Quick Start

```rust
use foxtive_supervisor::{SupervisedTask, spawn_supervised};

struct MyWorker;

#[async_trait::async_trait]
impl SupervisedTask for MyWorker {
    fn name(&self) -> String {
        "my-worker".to_string()
    }

    async fn run(&self) -> anyhow::Result<()> {
        // Your task logic here
        process_messages().await?;
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let handle = spawn_supervised(MyWorker);
    
    // Task runs in background, auto-restarts on failure
    handle.await.unwrap();
}
```

That's it. Your task will automatically restart if it fails.

## Restart Policies

Control *when* tasks restart:

```rust
impl SupervisedTask for MyTask {
    fn restart_policy(&self) -> RestartPolicy {
        // Never restart (run once)
        RestartPolicy::Never
        
        // Try up to 5 times, then give up
        RestartPolicy::MaxAttempts(5)
        
        // Keep trying forever (default)
        RestartPolicy::Always
    }
}
```

## Backoff Strategies

Control *how long* to wait between restarts:

```rust
use std::time::Duration;

impl SupervisedTask for MyTask {
    fn backoff_strategy(&self) -> BackoffStrategy {
        // Wait same amount each time
        BackoffStrategy::Constant(Duration::from_secs(5))
        
        // Double the wait time after each failure (recommended)
        BackoffStrategy::Exponential {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(60),
        }
    }
}
```

## Lifecycle Hooks

Get notified about what's happening:

```rust
#[async_trait::async_trait]
impl SupervisedTask for MyTask {
    // Run before the first attempt
    async fn setup(&self) -> anyhow::Result<()> {
        println!("Setting up connections...");
        Ok(())
    }

    // Run after task completes (success or failure)
    async fn cleanup(&self) {
        println!("Closing connections...");
    }

    // Called before each restart attempt
    async fn on_restart(&self, attempt: usize) {
        println!("Restarting (attempt #{})", attempt);
    }

    // Called when task returns an error
    async fn on_error(&self, error: &str, attempt: usize) {
        eprintln!("Task failed: {} (attempt #{})", error, attempt);
    }

    // Called when task panics
    async fn on_panic(&self, msg: &str, attempt: usize) {
        eprintln!("Task panicked: {} (attempt #{})", msg, attempt);
    }

    // Decide if restart should happen
    async fn should_restart(&self, attempt: usize, error: &str) -> bool {
        // Custom logic - e.g., don't restart on auth errors
        !error.contains("Unauthorized")
    }

    // Called during graceful shutdown
    async fn on_shutdown(&self) {
        println!("Shutting down gracefully...");
    }
}
```

## Managing Multiple Tasks

Use `TaskRuntime` to supervise multiple tasks:

```rust
use foxtive_supervisor::TaskRuntime;

let mut runtime = TaskRuntime::new();

runtime
    .register(DatabaseWorker)
    .register(ApiServer)
    .register(BackgroundJob);

runtime.start_all().await?;

// Wait for any task to terminate
let result = runtime.wait_any().await;
println!("Task '{}' stopped: {:?}", result.task_name, result.final_status);

// Or wait for all tasks
let results = runtime.wait_all().await;
```

## Graceful Shutdown

Handle SIGTERM/SIGINT properly:

```rust
use tokio::signal;

let mut runtime = TaskRuntime::new();
runtime.register(MyTask);
runtime.start_all().await?;

tokio::select! {
    _ = signal::ctrl_c() => {
        println!("Shutting down...");
        runtime.shutdown().await;
    }
    result = runtime.wait_any() => {
        println!("Task terminated: {:?}", result);
    }
}
```

## Real-World Example

```rust
use foxtive_supervisor::*;
use std::time::Duration;

struct KafkaConsumer {
    topic: String,
}

#[async_trait::async_trait]
impl SupervisedTask for KafkaConsumer {
    fn name(&self) -> String {
        format!("kafka-consumer-{}", self.topic)
    }

    fn restart_policy(&self) -> RestartPolicy {
        RestartPolicy::Always // Keep consuming forever
    }

    fn backoff_strategy(&self) -> BackoffStrategy {
        BackoffStrategy::Exponential {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(30),
        }
    }

    async fn setup(&self) -> anyhow::Result<()> {
        // Connect to Kafka
        Ok(())
    }

    async fn run(&self) -> anyhow::Result<()> {
        loop {
            let message = fetch_message(&self.topic).await?;
            process(message).await?;
        }
    }

    async fn on_error(&self, error: &str, attempt: usize) {
        tracing::error!(
            topic = %self.topic,
            attempt = attempt,
            error = %error,
            "Consumer failed"
        );
    }

    async fn cleanup(&self) {
        // Close Kafka connection
    }
}
```

## Supervision Results

Every task returns a `SupervisionResult`:

```rust
pub struct SupervisionResult {
    pub task_name: String,
    pub total_attempts: usize,  // How many times it ran
    pub final_status: SupervisionStatus,
}

pub enum SupervisionStatus {
    CompletedNormally,      // Task finished successfully
    MaxAttemptsReached,     // Hit restart limit
    RestartPrevented,       // should_restart() returned false
    SetupFailed,            // setup() failed
    ManuallyStopped,        // Policy said stop, or task was aborted
}
```

## When to Use This

✅ **Great for:**
- Background workers that should keep running
- Message consumers (Kafka, RabbitMQ, etc.)
- Database connection pools
- Health check loops
- Webhook processors
- Any "run forever" service

❌ **Not for:**
- HTTP request handlers (use your web framework's middleware)
- One-off scripts
- Tasks that should fail fast

## Philosophy

We believe error handling should be:
- **Explicit**: You decide the restart policy
- **Observable**: Hooks show you what's happening
- **Flexible**: Customize behavior per task
- **Forgiving**: Panics don't crash your app

Tasks fail. That's okay. We've got your back.

## License

MIT

---

Built with 🦊 by the Foxtive team. Made for humans who write Rust.
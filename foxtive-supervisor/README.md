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
foxtive-supervisor = "0.2"
tokio = { version = "1", features = ["full"] }
anyhow = "1"
async-trait = "0.1"
```

## Quick Start

```rust
use foxtive_supervisor::{Supervisor, SupervisedTask};

struct MyWorker;

#[async_trait::async_trait]
impl SupervisedTask for MyWorker {
    fn id(&self) -> &'static str {
        "my-worker"
    }

    async fn run(&self) -> anyhow::Result<()> {
        process_messages().await?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    Supervisor::new()
        .add(MyWorker)
        .start_and_wait_any()
        .await?;

    Ok(())
}
```

That's it. Your task will automatically restart if it fails.

## Task Identity

Every task declares a unique `id`. This is used for logging, monitoring, and dependency resolution:

```rust
impl SupervisedTask for MyTask {
    fn id(&self) -> &'static str {
        "my-task"   // must be unique across registered tasks
    }

    // Optional: human-readable name (defaults to id)
    fn name(&self) -> String {
        "My Task (friendly name)".to_string()
    }
}
```

## Task Dependencies

Declare which tasks must complete their `setup()` phase before yours starts:

```rust
impl SupervisedTask for KafkaConsumer {
    fn id(&self) -> &'static str {
        "kafka-consumer"
    }

    fn dependencies(&self) -> &'static [&'static str] {
        &["database", "redis"]  // waits for these task IDs to finish setup
    }

    async fn run(&self) -> anyhow::Result<()> {
        // guaranteed: database and redis are ready
        Ok(())
    }
}
```

The supervisor validates the dependency graph at startup — unknown IDs and circular dependencies are caught immediately with a clear error before any task spawns.

## Prerequisites

Need to wait for something external before *any* task starts? Use prerequisites:

```rust
// Wait for your HTTP server to bind before starting consumers
let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();

// somewhere in your server startup code:
// ready_tx.send(()).unwrap();

Supervisor::new()
    .require("http-server-bound", async move {
        ready_rx.await.map_err(|_| anyhow::anyhow!("server never signalled ready"))
    })
    .add(MyConsumer)
    .start_and_wait_any()
    .await?;
```

Prerequisites run sequentially before any task is spawned. If one fails, startup is aborted immediately.

## Restart Policies

Control *when* tasks restart:

```rust
impl SupervisedTask for MyTask {
    fn restart_policy(&self) -> RestartPolicy {
        RestartPolicy::Never           // run once
        RestartPolicy::MaxAttempts(5)  // try up to 5 times, then give up
        RestartPolicy::Always          // keep trying forever (default)
    }
}
```

## Backoff Strategies

Control *how long* to wait between restarts:

```rust
use std::time::Duration;

impl SupervisedTask for MyTask {
    fn backoff_strategy(&self) -> BackoffStrategy {
        BackoffStrategy::fixed(Duration::from_secs(5))   // same wait each time
        BackoffStrategy::exponential()                    // 2s -> 4s -> 8s -> ... 60s (default)
        BackoffStrategy::linear()                         // 5s -> 10s -> 15s -> ...
        BackoffStrategy::fibonacci_with_default()         // 1s -> 1s -> 2s -> 3s -> 5s -> ...
        BackoffStrategy::custom(|attempt| Duration::from_secs(attempt as u64 * 3))
    }
}
```

## Lifecycle Hooks

Get notified about what's happening:

```rust
#[async_trait::async_trait]
impl SupervisedTask for MyTask {
    async fn setup(&self) -> anyhow::Result<()> {
        // run once before first attempt — connections, topology, validation
        Ok(())
    }

    async fn cleanup(&self) {
        // run after task stops (success, failure, or panic)
    }

    async fn on_restart(&self, attempt: usize) {
        // called before each restart (not on first attempt)
    }

    async fn on_error(&self, error: &str, attempt: usize) {
        // called when run() returns Err
    }

    async fn on_panic(&self, msg: &str, attempt: usize) {
        // called when run() panics
    }

    async fn should_restart(&self, attempt: usize, error: &str) -> bool {
        !error.contains("Unauthorized") // return false to prevent restart
    }

    async fn on_shutdown(&self) {
        // called during graceful shutdown
    }
}
```

## Managing Multiple Tasks

Use the `Supervisor` builder or `TaskRuntime` directly:

```rust
// Builder style
Supervisor::new()
    .add(DatabaseWorker)
    .add(ApiServer)
    .add(BackgroundJob)
    .start_and_wait_any()
    .await?;

// Mixed types (boxed or Arc)
Supervisor::new()
    .add_boxed(Box::new(TaskA::new()))
    .add_boxed(Box::new(TaskB::new()))
    .start_and_wait_all()
    .await?;

// Manual runtime control
let mut runtime = TaskRuntime::new();
runtime.register(DatabaseWorker);
runtime.register(ApiServer);
runtime.start_all().await?;

let result = runtime.wait_any().await;
println!("Task '{}' stopped: {:?}", result.task_name, result.final_status);
```

## Graceful Shutdown

Handle SIGTERM/SIGINT properly:

```rust
use tokio::signal;

let mut runtime = Supervisor::new()
    .add(MyTask)
    .start()
    .await?;

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

struct DatabasePool;
struct KafkaConsumer { topic: String }

#[async_trait::async_trait]
impl SupervisedTask for DatabasePool {
    fn id(&self) -> &'static str { "database" }

    async fn setup(&self) -> anyhow::Result<()> {
        // establish pool
        Ok(())
    }

    async fn run(&self) -> anyhow::Result<()> {
        // keep pool alive
        Ok(())
    }
}

#[async_trait::async_trait]
impl SupervisedTask for KafkaConsumer {
    fn id(&self) -> &'static str { "kafka-consumer" }

    // won't start until DatabasePool setup() completes
    fn dependencies(&self) -> &'static [&'static str] {
        &["database"]
    }

    fn restart_policy(&self) -> RestartPolicy { RestartPolicy::Always }

    fn backoff_strategy(&self) -> BackoffStrategy {
        BackoffStrategy::exponential_custom(Duration::from_secs(1), Duration::from_secs(30))
    }

    async fn run(&self) -> anyhow::Result<()> {
        loop {
            let message = fetch_message(&self.topic).await?;
            process(message).await?;
        }
    }

    async fn on_error(&self, error: &str, attempt: usize) {
        tracing::error!(topic = %self.topic, attempt, error, "Consumer failed");
    }

    async fn cleanup(&self) {
        // close Kafka connection
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    Supervisor::new()
        .add(DatabasePool)
        .add(KafkaConsumer { topic: "orders".into() })
        .start_and_wait_any()
        .await?;

    Ok(())
}
```

## Error Handling

The supervisor provides structured error handling through the `SupervisorError` enum:

```rust
use foxtive_supervisor::{Supervisor, SupervisorError};

async fn start_supervisor() -> Result<(), SupervisorError> {
    let mut supervisor = Supervisor::new();
    
    // This might fail with structured errors
    supervisor.start().await?;
    
    Ok(())
}

// Pattern matching for specific error handling
match start_supervisor().await {
    Err(SupervisorError::DependencyValidation { task_id, dependency_id, reason }) => {
        eprintln!("Task '{}' has invalid dependency '{}': {:?}", task_id, dependency_id, reason);
    }
    Err(SupervisorError::CircularDependency { task_id, dependency_id }) => {
        eprintln!("Circular dependency detected: '{}' -> '{}'", task_id, dependency_id);
    }
    Err(e) => {
        eprintln!("Supervisor failed: {}", e);
    }
    Ok(()) => println!("Supervisor started successfully"),
}
```

### Error Categories

- **Configuration Errors**: `DependencyValidation`, `CircularDependency`, `InvalidConfiguration`
- **Runtime Errors**: `PrerequisiteFailed`, `SetupFailed`, `DependencySetupFailed`
- **Execution Errors**: `TaskExecutionFailed`, `TaskPanicked`, `MaxAttemptsReached`, `RestartPrevented`
- **System Errors**: `RuntimeFailure`, `InternalError`

## Supervision Results

Every task returns a `SupervisionResult`:

```rust
pub struct SupervisionResult {
    pub task_name: String,
    pub id: String,
    pub total_attempts: usize,
    pub final_status: SupervisionStatus,
}

pub enum SupervisionStatus {
    CompletedNormally,    // task finished successfully
    MaxAttemptsReached,   // hit restart limit
    RestartPrevented,     // should_restart() returned false
    SetupFailed,          // setup() failed
    DependencyFailed,     // an upstream dependency's setup failed
    ManuallyStopped,      // policy said stop, or task was aborted
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
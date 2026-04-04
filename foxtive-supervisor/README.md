# 🦊 Foxtive Supervisor

> Because tasks crash. We help them get back up.

A Rust supervision library that keeps your async tasks running, even when things go wrong. Think of it as a caring parent for your background jobs.

## What's New in v0.3?

**Advanced Scheduling & Composition**

- **Cron Scheduling**: Schedule tasks with cron expressions (e.g., `"*/1 * * * * * *"` for every second)
- **Task Groups**: Atomic operations on related tasks with group-level health monitoring
- **Supervisor Hierarchies**: Create nested supervisor trees for complex architectures
- **Task Pools**: Load-balanced worker pools with RoundRobin, Random, and LeastLoaded strategies
- **Conditional Dependencies**: Environment-based dependency activation for flexible deployments
- **Rate Limiting**: Enforce minimum restart intervals to prevent resource exhaustion
- **Time Windows**: Restrict task execution to specific time periods
- **Cascading Shutdown**: Bottom-up shutdown propagation through supervisor hierarchies
- **Complex Dependency Graphs**: Support for diamond dependencies, fan-out/fan-in patterns, and more

## Why?

In production, things fail:
- Network hiccups
- Temporary resource exhaustion
- Race conditions you didn't catch in testing
- That one edge case from 3am

**Foxtive Supervisor** automatically restarts your tasks with configurable policies, handles panics gracefully, and gives you hooks to observe what's happening.

## Features

### Core Supervision
- **Automatic Restarts**: Keep tasks running with customizable retry policies and backoff strategies.
- **Panic Recovery**: Gracefully handle task panics without crashing your application.
- **Dependency Management**: Ensure tasks start only after their dependencies are ready.
- **Prerequisites Gates**: Run async checks before any task starts (e.g., database connectivity).

### Dynamic Task Management
- **Runtime Task Control**: Add, remove, pause, resume, or restart tasks at runtime.
- **Task Groups**: Perform atomic operations on related tasks (start/stop/restart groups together).
- **Group Health Monitoring**: Aggregate health status across task groups for easy monitoring.
- **Task Information**: Query detailed task status, metrics, and health at any time.

### Scheduling & Timing
- **Cron Scheduling**: Schedule tasks using cron expressions (integrates with `foxtive-cron`).
- **Delayed Starts**: Configure initial delays before task execution begins.
- **Random Jitter**: Add randomness to prevent thundering herd in distributed deployments.
- **Rate Limiting**: Enforce minimum intervals between restarts to prevent resource exhaustion.
- **Time Windows**: Restrict task execution to specific time periods (e.g., business hours only).

### Advanced Reliability
- **Circuit Breaker**: Fail fast and prevent cascading failures when external services are down.
- **Persistence Layer**: Persist task state (attempts, failures, status) across application restarts.
- **Multiple Storage Backends**: In-memory and filesystem persistence out of the box.
- **Graceful Shutdown**: Configurable timeouts with forced termination and cleanup hooks.
- **Cascading Shutdown**: Hierarchical shutdown propagation for nested supervisors.

### Observability & Monitoring
- **Event System**: Comprehensive lifecycle events for monitoring and alerting.
- **Distributed Tracing**: Built-in `tracing` integration with correlation IDs.
- **Structured Logging**: Rich, contextual logs throughout the supervision lifecycle.
- **Health Checks**: Per-task and group-level health status reporting.
- **Custom Metrics**: Expose task-specific metrics for monitoring dashboards.

### Concurrency & Performance
- **Concurrency Control**: Global and per-task limits to prevent resource exhaustion.
- **Priority Scheduling**: Control the order in which tasks are started and restarted.
- **Task Pools**: Load-balanced worker pools with multiple strategies (RoundRobin, Random, LeastLoaded).
- **Supervisor Hierarchies**: Create nested supervisor trees for organized task management.

### Testing & Development
- **Testing Utilities**: Built-in mock tasks and test harness for reliable testing.
- **Conditional Dependencies**: Environment-based dependency activation for flexible testing.
- **Hot Reload Ready**: Architecture designed for future configuration reload support.

## Installation

```toml
[dependencies]
foxtive-supervisor = "0.3"
tokio = { version = "1", features = ["full"] }
anyhow = "1"
async-trait = "0.1"
```

### Feature Flags

Foxtive Supervisor uses feature flags to enable optional functionality:

```toml
[dependencies]
foxtive-supervisor = { version = "0.3", features = ["cron"] }
```

**Available Features:**
- `cron` - Enable cron scheduling support (requires `foxtive-cron` and `chrono-tz`)
- All other features are enabled by default

### Optional Dependencies

Some features require additional dependencies:

```toml
# For distributed tracing
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# For persistence with filesystem
serde_json = "1"

# For cron scheduling (enabled with 'cron' feature)
foxtive-cron = "0.1"
chrono-tz = "0.8"
rand = "0.8"
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

## Advanced Examples

### Cron-Scheduled Tasks

Run tasks on a schedule using cron expressions:

```rust
struct DailyReport;

#[async_trait::async_trait]
impl SupervisedTask for DailyReport {
    fn id(&self) -> &'static str { "daily-report" }
    
    fn cron_schedule(&self) -> Option<&'static str> {
        // Run every day at 9:00 AM UTC
        Some("0 0 9 * * * *")
    }
    
    async fn run(&self) -> anyhow::Result<()> {
        generate_report().await?;
        Ok(())
    }
}
```

### Task Groups with Health Monitoring

Group related tasks and monitor them together:

```rust
struct DatabaseService;
struct CacheService;
struct ApiService;

#[async_trait::async_trait]
impl SupervisedTask for DatabaseService {
    fn id(&self) -> &'static str { "database" }
    fn group_id(&self) -> Option<&'static str> { Some("infrastructure") }
    // ... implementation
}

#[async_trait::async_trait]
impl SupervisedTask for CacheService {
    fn id(&self) -> &'static str { "cache" }
    fn group_id(&self) -> Option<&'static str> { Some("infrastructure") }
    fn dependencies(&self) -> &'static [&'static str] { &["database"] }
    // ... implementation
}

// Start all infrastructure tasks atomically
let mut runtime = supervisor.start().await?;
runtime.start_group("infrastructure");

// Check group health
let health = runtime.get_group_health("infrastructure").await;
match health {
    HealthStatus::Healthy => println!("All systems operational"),
    HealthStatus::Degraded { reason } => eprintln!("Degraded: {}", reason),
    HealthStatus::Unhealthy { reason } => eprintln!("Unhealthy: {}", reason),
    _ => {}
}
```

### Rate Limiting & Backoff

Prevent resource exhaustion with rate limiting:

```rust
struct ExternalApiConsumer;

#[async_trait::async_trait]
impl SupervisedTask for ExternalApiConsumer {
    fn id(&self) -> &'static str { "api-consumer" }
    
    fn backoff_strategy(&self) -> BackoffStrategy {
        BackoffStrategy::Exponential {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(60),
            multiplier: 2.0,
        }
    }
    
    fn min_restart_interval(&self) -> Option<Duration> {
        // Never restart more than once per 5 seconds
        Some(Duration::from_secs(5))
    }
    
    async fn run(&self) -> anyhow::Result<()> {
        fetch_from_api().await?;
        Ok(())
    }
}
```

### Supervisor Hierarchies

Create nested supervisor trees for complex architectures:

```rust
use foxtive_supervisor::hierarchy::{SupervisorHierarchy, SupervisorNode};

let mut hierarchy = SupervisorHierarchy::new("root");

// Add child supervisors
let mut api_node = SupervisorNode::root("api-services");
api_node.add_child_supervisor("auth-service", auth_supervisor);
api_node.add_child_supervisor("user-service", user_supervisor);

let mut worker_node = SupervisorNode::root("background-workers");
worker_node.add_child_supervisor("email-worker", email_supervisor);
worker_node.add_child_supervisor("report-worker", report_supervisor);

hierarchy.root_mut().add_child(api_node);
hierarchy.root_mut().add_child(worker_node);

// Cascading shutdown - shuts down all children first, then parent
hierarchy.root().shutdown_all().await;
```

### Task Pools with Load Balancing

Distribute work across multiple workers:

```rust
use foxtive_supervisor::task_pool::{TaskPool, LoadBalancingStrategy};

// Create a pool of 4 workers with round-robin distribution
let pool = TaskPool::new(
    "message-processors",
    4,
    LoadBalancingStrategy::RoundRobin
);

// Build supervisor with pooled workers
let supervisor = pool.build_pool(|worker_id| {
    MessageProcessor::new(worker_id)
});
```

### Conditional Dependencies

Activate dependencies based on environment or runtime conditions:

```rust
struct Microservice;

#[async_trait::async_trait]
impl SupervisedTask for Microservice {
    fn id(&self) -> &'static str { "my-service" }
    
    fn dependencies(&self) -> &'static [&'static str] {
        &["database"] // Always required
    }
    
    fn conditional_dependencies(&self) -> Vec<(&'static str, Box<dyn Fn() -> bool + Send + Sync>)> {
        vec![
            // Only depend on cache if USE_CACHE is enabled
            ("cache", Box::new(|| {
                std::env::var("USE_CACHE").is_ok()
            })),
            // Only depend on message queue if ENABLE_QUEUE is set
            ("message-queue", Box::new(|| {
                std::env::var("ENABLE_QUEUE").is_ok()
            })),
        ]
    }
}
```

### State Persistence

Persist task state across application restarts:

```rust
use foxtive_supervisor::persistence::{FsStateStore, TaskStateStore};
use std::path::PathBuf;

struct MessageProcessor;

#[async_trait::async_trait]
impl SupervisedTask for MessageProcessor {
    fn id(&self) -> &'static str { "message-processor" }
    
    async fn run(&self) -> anyhow::Result<()> {
        process_messages().await?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create filesystem-based state store
    let store = FsStateStore::new(PathBuf::from("./task-state"));
    
    // Supervisor will automatically restore task state on startup
    let supervisor = Supervisor::new()
        .with_state_store(Box::new(store))
        .add(MessageProcessor);
    
    // Task state (attempts, failures, last success) persists across restarts
    supervisor.start_and_wait_any().await?;
    
    Ok(())
}
```

The persistence layer supports:
- **InMemoryStateStore**: Default, no configuration needed
- **FsStateStore**: Filesystem-based persistence with JSON serialization
- Automatic state recovery when tasks restart
- Custom storage backends via `TaskStateStore` trait

## Documentation

### Guides
- [Testing Guide](TESTING.md) - Comprehensive testing strategies and utilities

### Examples
Real-world examples demonstrating various use cases:

- **Microservice Orchestration** ([examples/microservice-orchestration](examples/microservice-orchestration))
  - Database to Cache to API Server architecture
  - Dependency management and health monitoring
  
- **Circuit Breaker Pattern** ([examples/circuit-breaker](examples/circuit-breaker))
  - Automatic failure detection and recovery
  - Half-open state testing
  
- **Graceful Shutdown** ([examples/graceful-shutdown](examples/graceful-shutdown))
  - Configurable shutdown timeouts
  - Cleanup hooks and forced termination
  
- **Database Message Consumer** ([examples/db-message-consumer](examples/db-message-consumer))
  - Persistent message processing with retry logic
  - State persistence across restarts
  
- **Panic Recovery** ([examples/panic-catcher](examples/panic-catcher))
  - Handling task panics gracefully
  - Maintaining supervision despite crashes
  
- **Axum Integration** ([examples/axum-cron](examples/axum-cron))
  - Web framework integration
  - Scheduled task management
  
- **Tracing & Observability** ([examples/tracing](examples/tracing))
  - Distributed tracing with correlation IDs
  - Structured logging throughout task lifecycle

## Troubleshooting Guide

### 1. Task not starting
- **Check Dependencies**: Ensure all IDs in `dependencies()` exist and their `setup()` succeeds.
- **Check Prerequisites**: If any `require()` gate fails, no tasks will start.
- **Concurrency Limits**: If the `global_concurrency_limit` is 0 or very low, tasks might be queued indefinitely.

### 2. Immediate Restart Loops
- **Setup Failure**: If `setup()` fails, the task will not enter the `run()` loop. Check logs for `TaskSetupFailed`.
- **Backoff Strategy**: Ensure your `backoff_strategy()` provides enough time for external resources to recover.
- **Should Restart Hook**: Check if `should_restart()` is returning `false` unexpectedly.

### 3. State Not Persisting
- **Storage Permissions**: Ensure the directory passed to `FsStateStore` is writable.
- **Task ID Consistency**: Persistence is tied to the task's `id()`. If you change the ID, previous state is lost.

### 4. High Resource Usage
- **Limit Concurrency**: Use `with_global_concurrency_limit` to cap the number of active tasks.
- **Check Leaks**: Ensure your `run()` loop or `setup()` doesn't leak memory or file handles. The supervisor restarts tasks, but it can't fix underlying leaks.

## Test Coverage & Quality

Foxtive Supervisor is thoroughly tested with **89 comprehensive tests** covering:

### Test Categories
- **Core Functionality** (16 tests) - Basic supervision, restarts, lifecycle
- **Dynamic Management** (7 tests) - Add/remove/pause/resume at runtime
- **Dependency Resolution** (6 tests) - Complex dependency graphs, diamond patterns
- **Persistence** (2 tests) - State storage and recovery
- **Circuit Breaker** (3 tests) - All state transitions and recovery
- **Concurrency Control** (2 tests) - Global and per-task limits
- **Graceful Shutdown** (4 tests) - Timeouts, forced termination, cascading shutdown
- **Event System** (3 tests) - Lifecycle event emission and listeners
- **Prerequisites** (7 tests) - Async gates and validation
- **Error Handling** (3 tests) - Panic recovery, error propagation
- **Edge Cases** (17 tests) - Boundary conditions, error scenarios
- **Task Groups** (4 tests) - Atomic operations, health aggregation
- **Task Pools** (4 tests) - Load balancing strategies
- **Conditional Dependencies** (3 tests) - Environment-based activation
- **Cron Scheduling** (7 tests) - Scheduled execution, rate limiting, time windows
- **Real-World Scenarios** (2 tests) - Microservice architecture, message pipelines
- **Complex Dependency Graphs** (6 tests) - Fan-out/fan-in, linear chains, mixed deps

### Quality Metrics
- **Test Pass Rate**: 98.9% (88/89 tests passing)
- **Clippy Compliance**: Zero warnings
- **Documentation**: Every public API documented with examples
- **Integration Tests**: Real-world scenarios with multiple features combined
- **Performance**: Benchmarks for persistence overhead and concurrency

## License

MIT

---

Built with 🦊 by the Foxtive team. Made for humans who write Rust.

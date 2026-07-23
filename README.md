# Foxtive

A modular Rust framework for building web services, background workers, cron jobs, and CLI tools.

[![License](https://img.shields.io/crates/l/foxtive)](https://github.com/foxtive/foxtive/blob/main/LICENSE)

## What is it?

You know the drill. Every Rust project starts the same way: wire up a database, add some background workers, set up health checks, handle graceful shutdown. Foxtive gives you the scaffolding so you can skip the boilerplate.

What you get:
- An `App` struct with a builder for configuring services (database, redis, rabbitmq, etc.)
- A two-phase DI lifecycle: `AppInit` (mutable) → `Arc<App>` (immutable)
- An in-process **Event Bus** for decoupled component communication
- An `AsyncInit` trait for services that need async setup
- A `Plugin` trait so you can bundle services + lifecycle hooks into reusable pieces
- Startup/shutdown hooks that run in a predictable order
- Config validation that catches mistakes before you try to connect to anything
- Companion crates for workers, cron jobs, and task supervision

Works with any async runtime and any web framework.

## Crates

| Crate | Version | What it does |
|---|---|---|
| [foxtive](foxtive/) | 0.26 | Core framework: DI container, event bus, lifecycle hooks, plugin system, feature-gated services |
| [foxtive-worker](foxtive-worker/) | 0.5 | Background job processing (RabbitMQ, Redis Streams, in-memory) |
| [foxtive-cron](foxtive-cron/) | 0.5 | Cron job scheduling with timezone support |
| [foxtive-supervisor](foxtive-supervisor/) | 0.3 | Task orchestration with dependency graphs and auto-restart |
| [foxtive-macros](foxtive-macros/) | 0.4 | Proc macros for enum derivations and event derivations |

## Installation

```toml
[dependencies]
foxtive = "0.26"
tokio = { version = "1", features = ["full"] }
```

Enable what you need:

```toml
[dependencies]
foxtive = { version = "0.26", features = ["database", "redis", "jwt"] }
```

## Architecture

```
┌─────────────────────────────────────────────────┐
│              Companion Crates                   │
│   foxtive-axum · foxtive-ntex · foxtive-auth   │
│   (runtime adapters, reusable feature modules)  │
├─────────────────────────────────────────────────┤
│                 foxtive core                    │
│   App · Plugin · DI · Events · Lifecycle        │
│   Health · Config · AsyncInit                   │
├─────────────────────────────────────────────────┤
│            Specialized Crates                   │
│   foxtive-worker · foxtive-cron · supervisor    │
└─────────────────────────────────────────────────┘
```

The **core** crate gives you the `App` builder, the `Plugin` trait, a type-map DI container, an event bus, lifecycle hooks, health checks, and config validation. It doesn't care which async runtime you use.

**Companion crates** like `foxtive-axum` or `foxtive-ntex` implement `Plugin` to plug into the core lifecycle. They live in separate repos.

**Specialized crates** handle the heavy lifting for background processing, scheduling, and task orchestration.

## Quick Start

```rust
use foxtive::prelude::*;
use foxtive::Environment;

#[tokio::main]
async fn main() -> AppResult<()> {
    let app = App::builder("my-service", "MYSVC")
        .environment(Environment::Development)
        .app_key("secret-key")
        .build()
        .await?;

    app.run_startup_hooks().await?;

    println!("App: {} v{}", app.app_name(), app.version().unwrap_or("0.0.0"));

    Ok(())
}
```

## Building Your App

### Configuring Services

The builder takes care of wiring. Add your database, redis, keys, and hooks:

```rust
use foxtive::prelude::*;
use foxtive::database::DbConfig;
use foxtive::redis::RedisConfig;

let app = App::builder("my-service", "MYSVC")
    .app_key("secret-key")
    .private_key("private")
    .public_key("public")
    .database(DbConfig {
        dsn: "postgres://localhost/mydb".into(),
        max_size: 10,
        ..Default::default()
    })
    .redis(RedisConfig {
        dsn: "redis://localhost".into(),
    })
    .on_startup(|app| async move {
        tracing::info!("Initializing resources...");
        Ok(())
    })
    .on_shutdown(|app| async move {
        tracing::info!("Cleaning up...");
    })
    .build()
    .await?;
```

### Plugins

Think of plugins as reusable bundles of services + lifecycle hooks. You define a struct, implement the `Plugin` trait, and register it with the builder.

```rust
use foxtive::lifecycle::Plugin;
use foxtive::app::AppBuilder;
use foxtive::prelude::*;

struct AuthPlugin;

impl Plugin for AuthPlugin {
    fn name(&self) -> &str { "auth" }

    fn register(&self, builder: AppBuilder) -> AppBuilder {
        builder
            .on_startup(|app| async {
                // Initialize auth service
                Ok(())
            })
            .on_shutdown(|app| async {
                // Cleanup auth resources
            })
    }
}

let app = App::builder("my-service", "MYSVC")
    .plugin(AuthPlugin)
    .build()
    .await?;
```

Companion crates like `foxtive-axum`, `foxtive-worker`, and `foxtive-supervisor` all implement `Plugin`, so they plug right in.

### DI Container

Foxtive provides a type-map DI container with a two-phase lifecycle. Use `build_init()` to get an `AppInit` for registering services after infrastructure is ready, then `freeze()` to produce the final immutable `Arc<App>`.

```rust
use foxtive::prelude::*;

struct UserService;
struct CacheService;

let mut init = App::builder("my-service", "MYSVC")
    .build_init()
    .await?;

// Register services synchronously
init.register(UserService);
init.register(CacheService);

// Freeze into immutable Arc<App>
let app = init.freeze().await?;

assert!(app.get::<UserService>().is_some());
```

For services that need async setup (cache warming, connection verification), implement `AsyncInit`:

```rust
use foxtive::lifecycle::AsyncInit;
use foxtive::app::AppInit;
use foxtive::prelude::*;

struct UserService;

impl AsyncInit for UserService {
    fn init(init: &AppInit) -> impl std::future::Future<Output = AppResult<Self>> + Send {
        async {
            // Async setup: warm caches, verify connections, etc.
            Ok(Self)
        }
    }
}

let mut init = App::builder("my-service", "MYSVC")
    .build_init()
    .await?;

init.init_service::<UserService>().await?;

let app = init.freeze().await?;
```

### Event Bus

In-process event bus for decoupled communication between components. Define events as plain structs, derive `Event`, and register handlers.

```rust
use foxtive::events::{Event, EventHandler};
use foxtive::prelude::*;

#[derive(Event, Clone, Debug)]
struct UserCreated {
    user_id: i64,
}

struct AuditLogger;

impl EventHandler<UserCreated> for AuditLogger {
    async fn handle(&self, event: &UserCreated, _app: &App) -> AppResult<()> {
        tracing::info!(user_id = event.user_id, "AUDIT: user created");
        Ok(())
    }
}

// Register during init phase
let mut init = App::builder("my-service", "MYSVC")
    .build_mut()
    .await?;

init.on::<UserCreated, _>(AuditLogger);

// Or use closures
init.on_event::<UserCreated, _, _>(|event, _app| async move {
    tracing::info!("User {} created", event.user_id);
    Ok(())
});

let app = init.freeze();

// Emit events
app.events().emit(UserCreated { user_id: 42 }, &app).await?;
```

Handlers run concurrently. One handler failing does not prevent others from executing.

### Lifecycle Hooks

Startup hooks run in the order you register them. Shutdown hooks run in reverse (last registered, first to shut down). Calling `app.shutdown().await` is idempotent, so you can call it from anywhere without worrying about double-execution.

```rust
let app = App::builder("my-service", "MYSVC")
    .on_startup(|app| async move {
        tracing::info!("1. Starts first");
        Ok(())
    })
    .on_startup(|app| async move {
        tracing::info!("2. Starts second");
        Ok(())
    })
    .on_shutdown(|app| async move {
        tracing::info!("Runs second on shutdown");
    })
    .on_shutdown(|app| async move {
        tracing::info!("Runs first on shutdown");
    })
    .build()
    .await?;
```

### Health Checks

Register checks for your services, then query the aggregate status. Useful for Kubernetes liveness/readiness probes.

```rust
use foxtive::health::{HealthCheck, HealthStatus};

let app = App::builder("my-service", "MYSVC")
    .health_check(foxtive::health::DatabaseHealthCheck::new())
    .health_check(foxtive::health::RedisHealthCheck)
    .build()
    .await?;

let report = app.check_health().await;
if report.is_healthy() {
    tracing::info!("All good");
} else {
    for (name, status) in report.checks.iter() {
        tracing::error!("{}: {:?}", name, status);
    }
}
```

Built-in checks: `DatabaseHealthCheck`, `RedisHealthCheck`, `RabbitMqHealthCheck`.

### Safe Accessors

Every service accessor comes in two flavors: `db()` panics if the service isn't configured, `try_db()` returns `Option`.

```rust
// Panics if not configured
let db = app.db();

// Returns None if not configured
if let Some(db) = app.try_db() {
    // use db
}

// Same pattern for redis, rabbitmq, etc.
let redis = app.try_redis();
let rabbitmq = app.try_rabbitmq();
```

### Config Validation

Configs get validated during `build()`, before any connections are attempted. If your DSN is empty or pool settings are wrong, you'll know immediately instead of getting a cryptic connection error later.

```rust
let app = App::builder("my-service", "MYSVC")
    .database(DbConfig { dsn: "".into(), ..Default::default() })
    .build()
    .await; // Err: "Database DSN must not be empty"
```

## Background Workers

The `foxtive-worker` crate handles message processing from RabbitMQ, Redis Streams, or in-memory queues. You implement the `Worker` trait, add it to a pool, and the pool dispatches messages to your workers.

```rust
use foxtive_worker::{Worker, ReceivedMessage, WorkerPoolBuilder};
use foxtive_worker::middleware::{RetryHandler, AckNackMiddleware};
use async_trait::async_trait;

struct EmailWorker;

#[async_trait]
impl Worker for EmailWorker {
    fn id(&self) -> &str { "email-worker" }
    
    async fn process(&self, msg: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        let to = msg.message.payload["to"].as_str().unwrap();
        send_email(to).await?;
        msg.ack().await?;
        Ok(())
    }
}

let pool = WorkerPoolBuilder::new("email-pool")
    .add_worker(EmailWorker)
    .with_middleware(AckNackMiddleware::default())
    .with_middleware(RetryHandler::default())
    .build()?;

loop {
    if let Some(msg) = backend.receive().await? {
        pool.dispatch(msg).await?;
    }
}
```

Also includes: circuit breakers, dead-letter queues, batch processing, rate limiting, and health endpoints.

## Scheduled Jobs

The `foxtive-cron` crate schedules tasks with cron expressions. Supports timezones, retry policies, and concurrency limits.

```rust
use foxtive_cron::Cron;

let mut cron = Cron::new();

// Every day at 9 AM
cron.add_job_fn("daily-report", "Daily Report", "0 0 9 * * * *", || async {
    generate_report().await?;
    Ok(())
})?;

// Every 30 seconds
cron.add_job_fn("health-check", "Health Check", "*/30 * * * * * *", || async {
    check_health().await?;
    Ok(())
})?;

// With timezone
use chrono_tz::America::New_York;
cron.add_job_fn("market-open", "Market Open", "0 30 9 * * 1-5 *", || async {
    // 9:30 AM Eastern, weekdays only
    Ok(())
})?.timezone(New_York);

tokio::spawn(async move { cron.run().await });
```

## Task Orchestration

The `foxtive-supervisor` crate keeps your tasks running. Define dependencies between tasks, and the supervisor handles startup ordering, automatic restarts on failure, and panic recovery.

```rust
use foxtive_supervisor::{Supervisor, SupervisedTask};
use async_trait::async_trait;

struct DatabaseService;

#[async_trait]
impl SupervisedTask for DatabaseService {
    fn id(&self) -> &'static str { "database" }
    
    async fn run(&self) -> anyhow::Result<()> {
        loop {
            maintain_connection().await?;
        }
        Ok(())
    }
}

struct ApiService;

#[async_trait]
impl SupervisedTask for ApiService {
    fn id(&self) -> &'static str { "api" }
    
    fn dependencies(&self) -> &'static [&'static str] {
        &["database"]
    }
    
    async fn run(&self) -> anyhow::Result<()> {
        start_api_server().await?;
        Ok(())
    }
}

Supervisor::new()
    .add(DatabaseService)
    .add(ApiService)
    .start_and_wait_any()
    .await?;
```

Also supports: cron scheduling, circuit breakers, task pools, state persistence, and cascading shutdown.

## Putting It All Together

Here's what a real application looks like. We build the `App` with database and redis, then use the supervisor to run a worker pool and a scheduled cleanup job.

```rust
use foxtive::prelude::*;
use foxtive_worker::{Worker, ReceivedMessage, WorkerPoolBuilder};
use foxtive_supervisor::{Supervisor, SupervisedTask};
use async_trait::async_trait;

struct QueueWorker;

#[async_trait]
impl Worker for QueueWorker {
    fn id(&self) -> &str { "queue-worker" }
    async fn process(&self, msg: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        process_job(msg.message.payload).await?;
        msg.ack().await?;
        Ok(())
    }
}

struct WorkerPoolTask;

#[async_trait]
impl SupervisedTask for WorkerPoolTask {
    fn id(&self) -> &'static str { "worker-pool" }
    
    fn dependencies(&self) -> &'static [&'static str] {
        &["database", "redis"]
    }
    
    async fn run(&self) -> anyhow::Result<()> {
        let pool = WorkerPoolBuilder::new("queue-pool")
            .add_worker(QueueWorker)
            .build()?;
        
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        }
        Ok(())
    }
}

struct CleanupTask;

#[async_trait]
impl SupervisedTask for CleanupTask {
    fn id(&self) -> &'static str { "cleanup" }
    
    fn cron_schedule(&self) -> Option<&'static str> {
        Some("0 0 2 * * * *")
    }
    
    async fn run(&self) -> anyhow::Result<()> {
        cleanup_old_records().await?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> AppResult<()> {
    let app = App::builder("my-service", "MYSVC")
        .database(DbConfig {
            dsn: std::env::var("DATABASE_URL")?.into(),
            ..Default::default()
        })
        .redis(RedisConfig {
            dsn: std::env::var("REDIS_URL")?.into(),
        })
        .build()
        .await?;
    
    app.run_startup_hooks().await?;
    
    Supervisor::new()
        .add(WorkerPoolTask)
        .add(CleanupTask)
        .start_and_wait_any()
        .await?;
    
    Ok(())
}
```

## Features

| Feature | Description |
|---|---|
| `database` | Diesel ORM with connection pooling |
| `redis` | Redis connection pooling and operations |
| `rabbitmq` | RabbitMQ message queue integration |
| `jwt` | JWT token encoding/decoding |
| `crypto` | Argon2 password hashing, HMAC utilities |
| `cache` | Unified caching interface |
| `cache-redis` | Redis cache driver |
| `cache-filesystem` | Filesystem cache driver |
| `cache-in-memory` | In-memory cache driver (DashMap) |
| `templating` | Tera templating engine |
| `reqwest` | HTTP client utilities |
| `regex` | Text validation and cleaning |
| `base64` | Base64 encoding/decoding |
| `hmac` | HMAC cryptographic functions |
| `openapi` | OpenAPI schema derivation for query params |
| `html-sanitizer` | HTML and filename sanitization |
| `http` | HTTP query param parsing |
| `strum` | Enum utility derivations |
| `test-utils` | Testing utilities and helpers |

## Ecosystem

| Crate | Description |
|---|---|
| `foxtive-axum` | Axum web server adapter |
| `foxtive-ntex` | NTex web server adapter |
| `foxtive-worker` | Background job processing |
| `foxtive-cron` | Cron job scheduling |
| `foxtive-supervisor` | Service orchestration and supervision |
| `foxtive-macros` | Proc macros (enum derivations, Event derive) |

## Examples

See the [examples/](foxtive-supervisor/examples/) directory for complete working examples:
- Microservice orchestration
- Circuit breaker patterns
- Graceful shutdown
- Cron scheduling
- Task hierarchies

## License

MIT, see [LICENSE](LICENSE).

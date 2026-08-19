# Foxtive

A production-grade Rust backend infrastructure crate providing battle-tested primitives for HTTP services, async connection pooling, structured error handling, graceful shutdown, health checking, secret zeroization, and extensible plugin architecture.

Foxtive is the **foundation layer** of the Foxtive ecosystem - it provides the core primitives that web adapters (`foxtive-axum`, `foxtive-ntex`), background workers (`foxtive-worker`), and orchestration tools (`foxtive-supervisor`, `foxtive-cron`) build upon.

[![Crates.io](https://img.shields.io/crates/v/foxtive)](https://crates.io/crates/foxtive)
[![Documentation](https://docs.rs/foxtive/badge.svg)](https://docs.rs/foxtive)
[![License](https://img.shields.io/crates/l/foxtive)](https://github.com/foxtive/foxtive/blob/main/LICENSE)

**MSRV**: Rust 1.97 (edition 2024)

## Installation

```toml
[dependencies]
foxtive = "1.1"
```

With features:

```toml
[dependencies]
foxtive = { version = "1.1", features = ["database", "database-async", "redis", "jwt", "jwe", "cache-redis"] }
```

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

## Builder API

Configure services through the builder:

```rust
use foxtive::prelude::*;
use foxtive::database::DbConfig;

let app = App::builder("my-service", "MYSVC")
    .app_key("secret-key")
    .private_key("private")
    .public_key("public")
    .database(DbConfig {
        dsn: "postgres://localhost/mydb".into(),
        ..Default::default()
    })
    .on_startup(|app| async move {
        // Initialize resources
        Ok(())
    })
    .on_shutdown(|app| async move {
        // Clean up resources
    })
    .build()
    .await?;
```

## Plugins

Bundle services, lifecycle hooks, and health checks into reusable modules:

```rust
use foxtive::lifecycle::Plugin;
use foxtive::app::AppBuilder;
use foxtive::prelude::*;
use foxtive::App;

struct AuthPlugin;

impl Plugin for AuthPlugin {
    fn name(&self) -> &str { "auth" }

    fn register(&self, builder: AppBuilder) -> AppBuilder {
        builder
            .on_startup(|app| async { Ok(()) })
            .on_shutdown(|app| async {})
    }
}

let app = App::builder("my-service", "MYSVC")
    .plugin(AuthPlugin)
    .build()
    .await?;
```

Companion crates like `foxtive-axum`, `foxtive-worker`, and `foxtive-supervisor` implement `Plugin` for seamless integration.

## DI Container

Foxtive provides a type-map DI container with automatic dependency resolution. Services are registered and constructed during `build()` - the container handles topological ordering, lazy wiring, and lifecycle hooks.

```rust
use foxtive::prelude::*;
use foxtive::lifecycle::Service;
use std::sync::Arc;

#[derive(Service)]
#[service(all)]
struct UserService {
    cache: Arc<CacheService>,
}

#[derive(Service)]
struct CacheService;

let app = App::builder("my-service", "MYSVC")
    .register_service::<CacheService>()
    .register_service::<UserService>()
    .build()
    .await?;

let svc = app.get::<UserService>().unwrap();
```

### AsyncInit

For services that need async setup (cache warming, connection verification), implement `AsyncInit`:

```rust
use foxtive::lifecycle::AsyncInit;
use foxtive::app::AppInit;
use foxtive::prelude::*;

struct UserService {
    // fields...
}

impl AsyncInit for UserService {
    fn init(init: &AppInit) -> impl std::future::Future<Output = AppResult<Self>> + Send {
        async {
            // Async setup: warm caches, verify connections, etc.
            // Access app via Deref: init.app_name(), init.db(), etc.
            Ok(Self { })
        }
    }
}

let mut init = App::builder("my-service", "MYSVC")
    .build_init()
    .await?;

init.init_service::<UserService>().await?;

let app = init.freeze().await?;
assert!(app.get::<UserService>().is_some());
```

### Derive Macro

`#[derive(Service)]` eliminates boilerplate by auto-generating `ServiceInit` - dependency resolution, lazy wiring, and lifecycle hook delegation.

#### Opt-out mode: `#[service(all)]`

All fields are treated as dependencies. Opt out with `#[foxtive(default)]`:

```rust
use foxtive::lifecycle::Service;
use std::sync::Arc;

#[derive(Service)]
#[service(all)]
struct UserService {
    cache: Arc<CacheService>,          // resolved from container
    config: Arc<ConfigService>,        // resolved from container
    #[foxtive(default)]
    request_count: AtomicU64,          // AtomicU64::default()
}
```

#### Opt-in mode (default)

Only fields marked with `#[dependency]` are resolved:

```rust
#[derive(Service)]
struct UserService {
    #[dependency]
    cache: Arc<CacheService>,
    #[dependency]
    config: ConfigService,             // must implement Clone
    request_count: AtomicU64,          // Default::default()
}
```

#### Field resolution rules

| Field type | Resolution |
|---|---|
| `Arc<T>` | `Arc::new(app.require::<T>()?.clone())` |
| `Arc<InfraType>` | `Arc::new(app.accessor()?.clone())` (DB, Redis, Cache, etc.) |
| `Lazy<T>` | Deferred - wired after all services are constructed |
| `T` (plain) | `app.require::<T>()?.as_ref().clone()` (requires `Clone`) |
| `#[foxtive(default)]` | `Default::default()` |
| `#[foxtive(init = "expr")]` | Custom expression with `app` in scope |

#### Declarative field initialization: `#[foxtive(init = "expr")]`

For fields that need custom initialization from app config (not DI dependencies):

```rust
#[derive(Service)]
#[service(all)]
struct LoginService {
    cache: Arc<CacheService>,
    #[foxtive(init = "app.jwt_token_lifetime()")]
    jwt_token_lifetime: i64,
    #[foxtive(init = "app.app_name().to_string()")]
    app_name: String,
}
```

The expression has `app: &App` in scope. Cannot be combined with `#[dependency]` or `#[foxtive(default)]`.

### Lazy Dependencies

`Lazy<T>` breaks circular dependencies between services. The field is filled after all services are constructed - access panics if used before that point.

```rust
use foxtive::prelude::*;
use foxtive::lifecycle::Service;
use std::sync::Arc;

#[derive(Service)]
#[service(all)]
struct ServiceA {
    b: Lazy<ServiceB>,    // deferred - breaks the cycle
    name: String,
}

#[derive(Service)]
#[service(all)]
struct ServiceB {
    a: Arc<ServiceA>,     // eager - ServiceA is already constructed
}
```

`Lazy<T>` fields are wired automatically during `freeze()` Phase 3. No manual `wire_lazy` override needed.

For manual `ServiceInit` impls, use the `lazy!()` macro to initialize `Lazy` fields with debug metadata:

```rust
impl ServiceInit for MyService {
    async fn init(app: &App) -> AppResult<Self> {
        Ok(Self {
            dep: lazy!("MyService", "dep"),
        })
    }

    fn wire_lazy(app: &App) -> AppResult<()> {
        let svc = app.require::<Self>()?;
        app.require::<OtherService>()?;
        Ok(())
    }
}
```

### Lifecycle Hooks

Services have two post-construction hooks for setup logic:

| Hook | When | Signature | Use case |
|---|---|---|---|
| `after_init` | After `init()`, before boxing | `&mut self, &App` | Fill config values, computed fields |
| `on_ready` | After all `Lazy<T>` wired | `&App` | Validation, cache warming, logging |

**Execution order:** `init()` → `after_init()` → [boxing] → [wire_lazy] → `on_ready()`

#### Manual ServiceInit

```rust
impl ServiceInit for LoginService {
    async fn init(app: &App) -> AppResult<Self> {
        Ok(Self {
            jwt_token_lifetime: 0,
            cache: app.require::<CacheService>()?,
        })
    }

    fn after_init(&mut self, app: &App) -> AppResult<()> {
        self.jwt_token_lifetime = app.jwt_token_lifetime();
        Ok(())
    }

    fn on_ready(app: &App) -> AppResult<()> {
        tracing::info!("LoginService ready");
        Ok(())
    }
}
```

#### ServiceHooks with derive macro

When using `#[derive(Service)]`, add `skip_hooks` and implement `ServiceHooks` for programmable control with enforced signatures:

```rust
use foxtive::lifecycle::{Service, ServiceHooks};

#[derive(Service)]
#[service(all, skip_hooks)]
struct LoginService {
    cache: Arc<CacheService>,
    #[foxtive(default)]
    jwt_token_lifetime: i64,
}

impl ServiceHooks for LoginService {
    fn after_init(&mut self, app: &App) -> AppResult<()> {
        self.jwt_token_lifetime = app.jwt_token_lifetime();
        Ok(())
    }

    fn on_ready(app: &App) -> AppResult<()> {
        tracing::info!("LoginService ready");
        Ok(())
    }
}
```

Without `skip_hooks`, the derive generates a no-op `ServiceHooks` impl automatically.

### Mutable Services

`Mutable<T>` wraps a value in `parking_lot::RwLock` for shared interior mutability. Register with `register_mutable_service` or `register_mutable`, retrieve with `get_mutable`:

```rust
use foxtive::prelude::*;
use foxtive::container::Mutable;
use std::sync::Arc;

struct Counter {
    count: u64,
}

impl ServiceInit for Counter {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self { count: 0 })
    }
}

let app = App::builder("my-service", "MYSVC")
    .register_mutable_service::<Counter>()
    .build()
    .await?;

let counter: Arc<Mutable<Counter>> = app.get_mutable::<Counter>().unwrap();
counter.write().count += 1;
println!("{}", counter.read().count); // 1
```

Or via derive macro with `#[service(mutable)]`:

```rust
#[derive(Service)]
#[service(mutable)]
struct Counter {
    #[foxtive(default)]
    count: u64,
}
```

### Trait Binding

Register trait objects (`Arc<dyn Trait>`) and resolve them by trait type. Useful for swapping implementations (e.g., mock vs real):

```rust
use foxtive::prelude::*;
use std::sync::Arc;

trait Notifier: Send + Sync {
    fn notify(&self, msg: &str) -> String;
}

struct EmailNotifier;
impl Notifier for EmailNotifier {
    fn notify(&self, msg: &str) -> String { format!("[EMAIL] {msg}") }
}

let app = App::builder("my-service", "MYSVC")
    .register_trait::<dyn Notifier>(Arc::new(EmailNotifier))
    .build()
    .await?;

let notifier: Arc<dyn Notifier> = app.require_trait::<dyn Notifier>()?;
println!("{}", notifier.notify("hello"));
```

The derive macro auto-detects `Arc<dyn Trait>` fields and resolves them via `require_trait`:

```rust
#[derive(Service)]
struct UserService {
    #[dependency]
    notifier: Arc<dyn Notifier>,  // resolved via require_trait
}
```

### Factory Providers

Register types that don't implement `ServiceInit` via factory closures. The closure receives `&App` and returns `AppResult<T>`:

```rust
// Register a foreign type (e.g., from an external crate)
struct HttpClient { base_url: String }

let app = App::builder("my-service", "MYSVC")
    .register_with(|_app| async {
        Ok(HttpClient { base_url: "https://api.example.com".into() })
    })
    .build()
    .await?;

let client = app.get::<HttpClient>().unwrap();
```

Factory-registered services participate in topological ordering and Phase 2 retry, just like `ServiceInit` services.

### Conditional & Idempotent Registration

Control when and whether services are registered:

```rust
let enable_metrics = std::env::var("METRICS").is_ok();

let app = App::builder("my-service", "MYSVC")
    // Conditional: only register if a condition is true
    .register_service_if::<MetricsService>(enable_metrics)
    .register_if(enable_metrics, 42u32)

    // Idempotent: silently skips if type is already registered
    .register_service::<CacheService>()
    .try_register_service::<CacheService>()  // no-op

    // Replace: swap an existing registration with a new implementation
    .register_service::<V1Handler>()
    .replace_service::<V2Handler>()  // V1 removed, V2 registered
    .build()
    .await?;
```

All conditional/idempotent methods are also available on `AppInit` for use after `build_init()`.

### Optional Dependencies

Services can declare optional dependencies using `Option<Arc<T>>` or `Option<T>`. These resolve to `None` when the dependency is not registered, instead of failing:

```rust
use foxtive::lifecycle::Service;
use std::sync::Arc;

#[derive(Service)]
struct BusinessService {
    #[dependency]
    cache: Option<Arc<CacheService>>,   // None if not registered
    #[dependency]
    timeout: Option<u32>,               // None if not registered
}
```

Manual `ServiceInit` impls use `app.get::<T>()` (returns `Option<Arc<T>>`) instead of `app.require::<T>()`:

```rust
impl ServiceInit for BusinessService {
    async fn init(app: &App) -> AppResult<Self> {
        Ok(Self {
            cache: app.get::<CacheService>(),       // Option<Arc<CacheService>>
            timeout: app.get::<u32>().map(|v| *v),  // Option<u32>
        })
    }
}
```

Optional dependencies are excluded from topological ordering and never trigger Phase 2 retry.

## Event Bus

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
    .build_init()
    .await?;

init.on::<UserCreated, _>(AuditLogger);

// Or use closures
init.on_event::<UserCreated, _, _>(|event: Arc<UserCreated>, _app: Arc<App>| async move {
    tracing::info!("User {} created", event.user_id);
    Ok(())
});

let app = init.freeze().await?;

// Emit events
app.events().emit(UserCreated { user_id: 42 }, &app).await?;
```

Handlers run concurrently. One handler failing does not prevent others from executing.

## Lifecycle

Startup and shutdown hooks have deterministic behaviour:

- **Startup**: hooks run **concurrently** via `join_all` - no ordering guarantees. If hook B depends on hook A, combine them or use explicit synchronization.
- **Shutdown**: hooks run **sequentially in reverse order** (LIFO), idempotent via `app.shutdown().await`. Wrapped in a configurable timeout (default 30s).

```rust
let app = App::builder("my-service", "MYSVC")
    .on_startup(|app| async move {
        tracing::info!("Starting up");
        Ok(())
    })
    .on_shutdown(|app| async move {
        tracing::info!("Shutting down");
    })
    .shutdown_timeout(std::time::Duration::from_secs(60))
    .build()
    .await?;
```

## Health Checks

Register health checks and aggregate results:

```rust
use foxtive::health::DatabaseHealthCheck;

let app = App::builder("my-service", "MYSVC")
    .health_check(DatabaseHealthCheck::new())
    .build()
    .await?;

let report = app.check_health().await;
if report.is_healthy() {
    tracing::info!("All systems operational");
}
```

Built-in checks: `DatabaseHealthCheck` (blocking), `AsyncDatabaseHealthCheck` (async), `RedisHealthCheck`, `RabbitMqHealthCheck`.

```rust
use foxtive::health::{DatabaseHealthCheck, AsyncDatabaseHealthCheck};

// Register both blocking and async health checks
let app = App::builder("my-service", "MYSVC")
    .health_check(DatabaseHealthCheck::new())
    .health_check(AsyncDatabaseHealthCheck::new())
    .build()
    .await?;
```

Health checks support TTL caching to avoid running expensive checks on every request:

```rust
use foxtive::health::HealthCheckCache;
use std::time::Duration;

let cache = HealthCheckCache::new(Duration::from_secs(10));
let report = cache.get_or_run(&app).await;  // First call runs checks
let report = cache.get_or_run(&app).await;  // Returns cached result
```

## Database

Foxtive supports both **blocking** (diesel + r2d2) and **async** (diesel-async + deadpool) PostgreSQL connection pools. Both can be enabled simultaneously for mixed workloads.

### Blocking Pool (`database` feature)

```rust
use foxtive::database::DbConfig;
use foxtive::database::ext::DatabaseConnectionExt;
use diesel::prelude::*;

let app = App::builder("my-service", "MYSVC")
    .database(DbConfig {
        dsn: "postgres://localhost/mydb".into(),
        ..Default::default()
    })
    .build()
    .await?;

// Checkout a connection (blocking)
let mut conn = app.db()?.connection()?;
let users = users::table.load::<User>(&mut conn)?;
```

### Async Pool (`database-async` feature)

```rust
use foxtive::database::DbConfig;
use foxtive::database::async_ext::AsyncDatabaseConnectionExt;
use diesel_async::RunQueryDsl;

let app = App::builder("my-service", "MYSVC")
    .async_database(DbConfig {
        dsn: "postgres://localhost/mydb".into(),
        ..Default::default()
    })
    .build()
    .await?;

// Checkout a connection (natively async, no spawn_blocking)
let mut conn = app.async_db()?.connection().await?;
let users = users::table.load::<User>(&mut conn).await?;
```

### Mixed Workloads

Both pools can coexist. Use the blocking pool for simple synchronous queries and the async pool for high-concurrency async handlers:

```rust
let app = App::builder("my-service", "MYSVC")
    .database(DbConfig { dsn: "postgres://localhost/mydb".into(), ..Default::default() })
    .async_database(DbConfig { dsn: "postgres://localhost/mydb".into(), ..Default::default() })
    .build()
    .await?;

// Blocking pool
let mut sync_conn = app.db()?.connection()?;

// Async pool
let mut async_conn = app.async_db()?.connection().await?;
```

### Async Extension Traits

The `database::async_ext` module provides async counterparts to all blocking extension traits:

| Blocking (`database::ext`) | Async (`database::async_ext`) |
|---|---|
| `DatabaseConnectionExt` | `AsyncDatabaseConnectionExt` |
| `OptionalResultExt` | `AsyncOptionalResultExt` |
| `ShareableResultExt` | `AsyncShareableResultExt` |
| `ShareablePaginationResultExt` | `AsyncShareablePaginationResultExt` |
| `PaginationResultExt` | `AsyncPaginationResultExt` |

### Async Pagination

```rust
use foxtive::database::pagination::Paginate;

let page = users::table
    .paginate(1)
    .per_page(25)
    .load_and_count_pages_async::<User>(&mut async_conn)
    .await?;

println!("Page {} of {}", 1, page.total_pages);
for user in &page.records {
    println!("  {}", user.name);
}
```

## Tokio Runtime

Foxtive registers a `Tokio` service in the DI container that provides bounded blocking dispatch, async-bridging, and task scheduling utilities. Inject as `Arc<Tokio>` into services.

### Bounded Blocking Dispatch

`block()` runs closure on the Tokio blocking thread pool, bounded by a semaphore to prevent thread exhaustion:

```rust
use foxtive::tokio::Tokio;
use foxtive::lifecycle::Service;
use std::sync::Arc;

#[derive(Service)]
#[service(all)]
struct UserService {
    tokio: Arc<Tokio>,
}

impl UserService {
    async fn find_user(&self, id: i64) -> AppResult<User> {
        self.tokio.block(move || {
            let mut conn = pool.get()?;
            Ok(users::table.find(id).first(&mut conn)?)
        }).await
    }
}
```

### Async Bridging

`run_async()` bridges sync→async by running a future on the global fallback runtime:

```rust
// From sync context, bridge to async (run_async is synchronous, blocks until complete)
let result = tokio.run_async(async {
    // async work here
    Ok("done".to_string())
})?;
```

### Task Scheduling

```rust
use foxtive::tokio::{Tokio, CancellationToken};

let cancel = CancellationToken::new();

// Run once after a delay
Tokio::timeout(5000, || async { Ok(()) }, "delayed-task", cancel.clone());

// Run repeatedly on a fixed interval
Tokio::tick(1000, || async { Ok(()) }, "periodic-task", cancel.clone());

// Run repeatedly with exponential backoff on errors
Tokio::tick_with_backoff(
    1000,                        // base interval (ms)
    || async { Ok(()) },        // task function
    "resilient-task",
    Duration::from_millis(500),  // min backoff
    Duration::from_secs(30),     // max backoff
    cancel.clone(),
);

// Cancel all tasks
cancel.cancel();
```

## Infrastructure Accessors

Infrastructure services (database, Redis, cache, etc.) are accessed through typed methods on `App`. Each service has a fallible accessor (returns `AppResult`) and a `try_` variant (returns `Option`):

```rust
// Blocking database
let db = app.db()?;             // returns AppResult<&DBPool>
let db = app.try_db();          // returns Option<&DBPool>

// Async database
let adb = app.async_db()?;      // returns AppResult<&AsyncDBPool>
let adb = app.try_async_db();   // returns Option<&AsyncDBPool>

// Redis
let redis = app.redis()?;       // returns AppResult<&Redis>
let redis = app.try_redis();    // returns Option<&Redis>
```

## Builder Methods

| Method | Description |
|---|---|
| `.environment(env)` | Set the application environment |
| `.app_key(key)` | Set the application secret key |
| `.database(config)` | Configure the blocking database connection pool |
| `.async_database(config)` | Configure the async database connection pool |
| `.redis(config)` | Configure the Redis connection |
| `.rabbitmq(config)` | Configure the RabbitMQ connection |
| `.cache(setup)` | Configure the cache driver |
| `.jwt(config)` | Configure JWT (pass a `JwtConfig`) |
| `.template_directory(dir)` | Configure the template directory |
| `.register(service)` | Register a service instance in the DI container |
| `.register_service::<T>()` | Register a service for deferred construction via `ServiceInit` |
| `.register_mutable_service::<T>()` | Register a service wrapped in `Mutable<T>` for shared interior mutability |
| `.register_trait::<dyn T>(arc)` | Register a trait object binding (`Arc<dyn Trait>`) |
| `.register_with(closure)` | Register a service via factory closure (no `ServiceInit` needed) |
| `.register_service_if::<T>(cond)` | Register a service only if `cond` is true |
| `.register_if(cond, service)` | Register a service instance only if `cond` is true |
| `.try_register_service::<T>()` | Idempotent registration: silently skips if type already registered |
| `.replace_service::<T>()` | Replace a previously registered service with a new implementation |
| `.after_build(closure)` | Register a callback after infrastructure init (receives `&mut AppInit`) |
| `.on_startup(closure)` | Register a startup hook (runs concurrently with other hooks) |
| `.on_shutdown(closure)` | Register a shutdown hook (runs in LIFO order) |
| `.health_check(check)` | Register a health check |
| `.health_check_timeout(dur)` | Set per-check timeout (default 5s) |
| `.shutdown_timeout(dur)` | Set shutdown hook timeout (default 30s) |
| `.metrics(sink)` | Register a metrics sink for infrastructure events |
| `.plugin(plugin)` | Register a plugin |

## Config Validation

Configs are validated during `build()` before any connections are attempted:

```rust
// Fails fast with a descriptive error if DSN is empty or pool settings are invalid
let app = App::builder("my-service", "MYSVC")
    .database(DbConfig { dsn: "".into(), ..Default::default() })
    .build()
    .await; // Err: "Database DSN must not be empty"
```

## DI Error Reporting

Service construction failures produce structured `DiError` diagnostics with rustc-style formatting:

- **DI0001** (circular runtime dep): detected when a service can't be constructed due to undeclared dependencies forming a cycle at runtime
- **DI0002** (declared circular dep): detected during topological sort when declared dependencies form a cycle
- **DI0003** (construction failed): wraps the underlying error from `ServiceInit::init()`

Error output includes:
- Short type names (stripped module paths)
- Cycle detection with dependency chain visualization
- `Lazy<T>` fix suggestions when cycles can be broken with deferred wiring
- Blocked service analysis showing which services are blocked by missing dependencies

```text
error[DI0002]: circular dependency detected
  --> UserService -> CacheService -> UserService

  hint: break the cycle by wrapping one dependency in Lazy<T>
```

## JWT (JSON Web Token)

RFC 7519 compliant JWT helper with support for RSA (asymmetric) and HMAC (symmetric) algorithms. Keys are parsed once at construction and cached for reuse.

### HMAC (Symmetric)

```rust
use foxtive::helpers::jwt::{Jwt, JwtConfig, JwtAlgorithm, Validation};

// Create from shared secret (HS256 by default)
let jwt = Jwt::from_hmac(b"my-secret-key", 60);

// Generate token
let token = jwt.generate(claims)?;
println!("{}", token.access_token);

// Validate token
let mut validation = Validation::new(JwtAlgorithm::HS256);
validation.set_audience(&["my-audience"]);
let decoded = jwt.decode::<MyClaims>(&token.access_token, &validation)?;
```

### RSA (Asymmetric)

```rust
use foxtive::helpers::jwt::{Jwt, JwtConfig, JwtAlgorithm};

// Create from PEM-encoded keys
let config = JwtConfig::rsa_pem(public_pem, private_pem, 60)?;
let jwt = Jwt::new(config);

// Or with specific algorithm (RS256, RS384, RS512)
let config = JwtConfig::rsa_pem_with_algorithm(
    public_pem, private_pem,
    JwtAlgorithm::RS512, 60
)?;
```

### Microservice Key Distribution

In a multi-service architecture, only the **auth service** should hold the private key. All other services verify tokens using [`JwtVerifier`](#) - the private key never leaves the auth service.

```rust
use foxtive::helpers::jwt::{JwtVerifier, Validation, JwtAlgorithm};

// Non-auth service - only public key needed
let verifier = JwtVerifier::from_rsa_public_key(public_pem)?;

let mut validation = Validation::new(JwtAlgorithm::RS256);
validation.set_audience(&["my-service"]);
let decoded = verifier.decode::<MyClaims>(&token, &validation)?;
```

`JwtVerifier` provides compile-time safety - there is no `generate()` method, so services without the private key cannot accidentally sign tokens.

### Combined JWT + JWE

Sign-then-encrypt for token confidentiality:

```rust
use foxtive::helpers::jwt::Jwt;
use foxtive::helpers::jwe::Jwe;

let jwt = Jwt::from_hmac(b"jwt-secret", 60);
let jwe = Jwe::from_symmetric(b"0123456789abcdef0123456789abcdef")?;

// Sign + encrypt in one step
let jwe_token = jwt.generate_encrypted(claims, &jwe)?;

// Decrypt + verify in one step
let decoded = jwt.decode_decrypted::<MyClaims>(&jwe_token, &jwe, &validation)?;
```

## JWE (JSON Web Encryption)

RFC 7516 compliant JWE helper for encrypting arbitrary payloads. Supports symmetric (AES key wrap) and asymmetric (RSA-OAEP-256) key management.

### Symmetric Encryption

```rust
use foxtive::helpers::jwe::{Jwe, JweConfig, JweAlgorithm, JweEncryption};

// Create from symmetric key (A256KW + A256GCM by default)
let jwe = Jwe::from_symmetric(b"0123456789abcdef0123456789abcdef")?;

// Encrypt any serializable payload
let token = jwe.encrypt(&my_data)?;

// Decrypt and deserialize
let decrypted: MyData = jwe.decrypt(&token)?;

// Override algorithms per-call
let token = jwe.encrypt_with(
    &my_data,
    JweAlgorithm::A128KW,
    JweEncryption::A128GCM,
)?;
```

### Key Size Requirements

| Algorithm | Required Key Size |
|-----------|------------------|
| `A128KW`  | 16 bytes         |
| `A192KW`  | 24 bytes         |
| `A256KW`  | 32 bytes         |
| `Dir`     | depends on `enc` |

## Redis

High-level Redis client backed by a `deadpool-redis` connection pool with comprehensive operations.

### Basic Operations

```rust
use std::sync::Arc;
use foxtive::redis::Redis;

let redis = Arc::new(Redis::new(pool));

// Strings
redis.set("key", &"value").await?;
let val: String = redis.get("key").await?;

// Lists (queues)
redis.queue("my-queue", &"item1").await?;  // LPUSH
redis.rpush("my-queue", &"item2").await?;  // RPUSH
let item: String = redis.lpop("my-queue", None).await?;

// Hashes
redis.hset("user:1", "name", "Alice").await?;
let name: Option<String> = redis.hget("user:1", "name").await?;

// Sets
redis.sadd("tags", &"rust").await?;
let member: String = redis.spop("tags").await?;

// Sorted Sets
redis.zadd("leaderboard", 100.0, &"player1").await?;
let top: Option<(String, f64)> = redis.zpopmax("leaderboard", 1).await?;
```

### Queue Polling with Bounded Concurrency

```rust
use std::sync::Arc;
use std::num::NonZeroUsize;
use foxtive::redis::Redis;

// Poll queue with at-most-once delivery semantics
// Items are popped (rpop) then processed in spawned tasks
Redis::poll_queue(
    redis.clone(),
    "my-queue".to_string(),
    None,                    // interval (default: 500ms)
    None,                    // batch size (default: 1)
    Some(NonZeroUsize::new(50).unwrap()),  // max concurrent tasks
    |item| async move {
        println!("Processing: {}", item);
        Ok(())
    },
).await;
```

### Pub/Sub with Bounded Concurrency

```rust
use std::num::NonZeroUsize;
use foxtive::redis::Redis;

Redis::subscribe(
    "my-channel".to_string(),
    "redis://localhost:6379".to_string(),
    Some(NonZeroUsize::new(100).unwrap()),  // max concurrent handlers
    |msg| async move {
        let payload: String = msg?;
        println!("Received: {}", payload);
        Ok(())
    },
).await?;
```

### Safety Features

- **Flush Protection**: `flush_all()` and `flush_db()` require `FOXTIVE_ALLOW_FLUSH=true` env var
- **SCAN-based iteration**: `keys()` and `keys_by_pattern()` use SCAN to avoid blocking

## RabbitMQ

High-level RabbitMQ client with automatic reconnection, push-based consumers, and pull-based consumer streams.

### Publishing

```rust
use foxtive::prelude::RabbitMQ;

let rmq = RabbitMQ::new(pool).await?;

// Simple publish
rmq.publish("events", "user.created", b"{\"id\": 1}").await?;

// Publisher builder for advanced use
rmq.publisher()
    .exchange("events")
    .routing_key("user.created")
    .payload(b"{\"user_id\": 123}")
    .delay(Duration::from_secs(300))
    .send().await?;
```

### Push-Based Consumption

```rust
// Consume with automatic retry on failure
rmq.consume("user_queue", "worker-1", |msg| async move {
    println!("Received: {:?}", msg.data());
    msg.ack().await?;
    Ok(())
}).await?;

// Consume forever with exponential backoff
rmq.consume_forever("tasks", "worker-2", |msg| async move {
    // Process message
    msg.ack().await?;
    Ok(())
}).await?;

// Detached (background) consumption
let handle = rmq.consume_detached("queue", "tag", handler).await;
```

### Pull-Based Consumption

```rust
use futures_util::StreamExt;

// Create consumer stream
let (mut stream, _guard) = rmq.create_consumer("my_queue", "consumer-1").await?;

while let Some(result) = stream.next().await {
    let message = result?;
    println!("Got: {:?}", message.data());
    message.ack().await?;
}

// Single message with timeout
if let Some(msg) = rmq.receive_message("queue", "tag", Some(Duration::from_secs(5))).await? {
    msg.ack().await?;
}
```

### Configuration

```rust
let mut rmq = RabbitMQ::new(pool).await?;

rmq.nack_on_failure(true)           // Nack on handler error
   .requeue_on_failure(true)        // Requeue failed messages
   .execute_handler_asynchronously(true)  // Spawn handlers
   .prefetch_count(10)              // QoS prefetch
   .health_check_interval(100)      // Check connection every 100 msgs
   .operation_timeout(Duration::from_secs(30));
```

## Password Hashing

Argon2-based password hashing with pepper support and legacy hash compatibility.

```rust
use foxtive::helpers::password::{Password, VerifyResult};

let password = Password::new("server-pepper".to_string());

// Hash a password (random salt generated per-hash)
let hash = password.hash("my-secret-password")?;

// Verify with detailed result
let result = password.verify_ex(&hash, "my-secret-password")?;
match result {
    VerifyResult::Matched => println!("Valid password"),
    VerifyResult::MatchedLegacy => {
        println!("Valid but needs rehash");
        let new_hash = password.hash("my-secret-password")?;
        // Persist new_hash
    }
    VerifyResult::Mismatch => println!("Invalid password"),
}

// Production configuration (higher security)
let config = argon2::Config {
    mem_cost: 65536,  // 64 MB
    time_cost: 3,
    parallelism: 4,
    ..Default::default()
};
let password = Password::with_config("pepper".to_string(), config);
```

## Features

| Feature | Description |
|---|---|
| `database` | Blocking Diesel + r2d2 connection pool |
| `database-async` | Async Diesel + deadpool connection pool (diesel-async) |
| `redis` | Redis connection pooling and operations |
| `rabbitmq` | RabbitMQ message queue integration |
| `jwt` | JWT token encoding/decoding (RSA, HMAC) |
| `jwe` | JWE encryption/decryption (AES-KW, RSA-OAEP) |
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
| `tracing-setup` | Tracing subscriber configuration |


## Ecosystem

Foxtive is designed to be composed with companion crates for specific use cases:

| Crate | Description |
|---|---|
| [`foxtive-axum`](https://crates.io/crates/foxtive-axum) | Axum web server adapter (CORS, extractors, shutdown) |
| [`foxtive-ntex`](https://crates.io/crates/foxtive-ntex) | NTex web server adapter (CORS, extractors, shutdown) |
| [`foxtive-worker`](https://crates.io/crates/foxtive-worker) | Background job processing (RabbitMQ, Redis Streams, retry, DLQ) |
| [`foxtive-cron`](https://crates.io/crates/foxtive-cron) | Cron job scheduling (timezone support, persistence) |
| [`foxtive-supervisor`](https://crates.io/crates/foxtive-supervisor) | Task supervision (circuit breaker, distributed coordination) |
| [`foxtive-macros`](https://crates.io/crates/foxtive-macros) | Proc macros (`#[derive(Service)]`, enum derivations, Diesel helpers, Event derive) |

## License

MIT - see [LICENSE](LICENSE).

//! The core application container - replaces the global `FOXTIVE: OnceLock` pattern.
//!
//! `App` is the central DI container that holds all application state, services,
//! lifecycle hooks, and health checks. It is created via [`AppBuilder`] and
//! passed explicitly (typically as `Arc<App>`) rather than accessed globally.

use std::borrow::Cow;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use zeroize::Zeroizing;

use crate::Environment;
use crate::container::{Lazy, Mutable, TypeMap};
use crate::enums::AppMessage;
use crate::events::EventBus;
use crate::health::{HealthCheck, HealthReport, aggregate_status};
use crate::lifecycle::{ServiceFactory, ShutdownHook, StartupHook};
use crate::metrics::{InfraEvent, MetricsSink};
use crate::results::AppResult;
use crate::tokio::Tokio;

#[cfg(feature = "cache")]
use crate::cache::Cache;
#[cfg(feature = "database-async")]
use crate::database::AsyncDBPool;
#[cfg(feature = "database")]
use crate::database::DBPool;
#[cfg(feature = "jwe")]
use crate::helpers::jwe::Jwe;
#[cfg(feature = "jwt")]
use crate::helpers::jwt::Jwt;
#[cfg(feature = "crypto")]
use crate::helpers::password::Password;
#[cfg(feature = "rabbitmq")]
use crate::rabbitmq::RabbitMQ;
#[cfg(feature = "redis")]
use crate::redis::Redis;
#[cfg(feature = "templating")]
use tera::{Context, Tera};

// Re-export builder and init
pub use builder::AppBuilder;
pub use init::AppInit;

mod builder;
pub(crate) mod deps;
pub(crate) mod di_error;
mod init;

pub use di_error::DiError;

/// The core application container.
///
/// Replaces the global `FOXTIVE: OnceLock<FoxtiveState>` with an explicit,
/// injectable container. Create instances via [`App::builder()`] or
/// [`AppBuilder::new()`].
///
/// # Synchronization Model
///
/// Services are stored in `TypeMap` inside `App`. After `freeze()` or `build()`,
/// the `App` is wrapped in `Arc<App>` - `get()` performs an `Arc::clone` on the
/// inner `TypeMap` entry (a cheap atomic ref-count increment) with zero lock
/// overhead. The `shutdown_initiated` flag uses `AtomicBool` for lock-free reads.
///
/// Services receive `&App` during construction (not `Arc<App>`), which eliminates
/// the need for dual-store architecture or consolidation logic.
///
/// # Example
///
/// ```no_run
/// use foxtive::App;
/// use foxtive::Environment;
///
/// # async fn run() -> foxtive::results::AppResult<()> {
/// let app = App::builder("My Service", "MYAPP")
///     .environment(Environment::Production)
///     .build()
///     .await?;
///
/// assert_eq!(app.app_name(), "My Service");
/// # Ok(())
/// # }
/// ```
pub struct App {
    pub(crate) env: Environment,
    pub(crate) app_name: String,
    pub(crate) app_code: String,
    /// Application secret key - zeroized on drop.
    pub(crate) app_key: Zeroizing<String>,
    pub(crate) app_env_prefix: String,

    /// Zeroized on drop.
    pub(crate) app_public_key: Zeroizing<String>,
    /// Zeroized on drop.
    pub(crate) app_private_key: Zeroizing<String>,

    pub(crate) app_version: Option<String>,
    pub(crate) started_at: Instant,
    pub(crate) shutdown_initiated: AtomicBool,

    pub(crate) services: TypeMap,
    pub(crate) service_factories: Vec<Box<dyn ServiceFactory>>,

    #[cfg(feature = "database")]
    pub(crate) db: Option<DBPool>,

    #[cfg(feature = "database-async")]
    pub(crate) async_db: Option<AsyncDBPool>,

    #[cfg(feature = "redis")]
    pub(crate) redis: Option<Redis>,
    #[cfg(feature = "redis")]
    pub(crate) redis_pool: Option<deadpool_redis::Pool>,

    #[cfg(feature = "rabbitmq")]
    pub(crate) rabbitmq: Option<RabbitMQ>,
    #[cfg(feature = "rabbitmq")]
    pub(crate) rabbitmq_pool: Option<deadpool_lapin::Pool>,

    #[cfg(feature = "cache")]
    pub(crate) cache: Option<Cache>,

    #[cfg(feature = "templating")]
    pub(crate) tera: Option<Arc<Tera>>,

    #[cfg(feature = "jwt")]
    pub(crate) jwt: Option<Jwt>,
    #[cfg(feature = "jwe")]
    pub(crate) jwe: Option<Jwe>,
    #[cfg(feature = "crypto")]
    pub(crate) password: Option<Password>,

    pub(crate) startup_hooks: Vec<StartupHook>,
    pub(crate) shutdown_hooks: Vec<ShutdownHook>,

    pub(crate) health_checks: Vec<Box<dyn HealthCheck>>,
    pub(crate) health_check_timeout: std::time::Duration,
    pub(crate) shutdown_timeout: std::time::Duration,
    pub(crate) metrics: Option<Arc<dyn MetricsSink>>,
    pub(crate) event_bus: EventBus,
    pub(crate) tokio: Tokio,
}

impl App {
    /// Create a new [`AppBuilder`] with the given app name and code.
    pub fn builder(app_name: impl Into<String>, app_code: impl Into<String>) -> AppBuilder {
        AppBuilder::new(app_name, app_code)
    }

    /// Resolve a registered service by type.
    ///
    /// Returns an `Arc<T>` - a cheap atomic ref-count increment.
    /// The returned handle can be cloned and retained freely.
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.services.get::<T>()
    }

    /// Resolve a registered service, returning an error if missing.
    pub fn require<T: Send + Sync + 'static>(&self) -> AppResult<Arc<T>> {
        self.get::<T>().ok_or_else(|| {
            AppMessage::not_found(format!(
                "Service of type {} not registered",
                std::any::type_name::<T>()
            ))
        })
    }

    /// Resolve a mutable service by type.
    ///
    /// Returns `Arc<Mutable<T>>` - use `.read()` and `.write()` to access
    /// the inner value. Returns `None` if not registered.
    pub fn get_mutable<T: Send + Sync + 'static>(&self) -> Option<Arc<Mutable<T>>> {
        self.get::<Mutable<T>>()
    }

    /// Resolve a mutable service, returning an error if missing.
    pub fn require_mutable<T: Send + Sync + 'static>(&self) -> AppResult<Arc<Mutable<T>>> {
        self.require::<Mutable<T>>()
    }

    /// Resolve a trait binding. Returns `Arc<dyn Trait>`.
    pub fn get_trait<T: ?Sized + Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.services.get_trait::<T>()
    }

    /// Resolve a trait binding, returning an error if missing.
    pub fn require_trait<T: ?Sized + Send + Sync + 'static>(&self) -> AppResult<Arc<T>> {
        self.get_trait::<T>().ok_or_else(|| {
            AppMessage::not_found(format!(
                "Trait binding for {} not registered",
                std::any::type_name::<T>()
            ))
        })
    }

    /// Returns `true` if a service of type `T` is registered in the container.
    ///
    /// Shorthand for `app.get::<T>().is_some()` — more self-documenting
    /// for existence checks.
    pub fn has_service<T: Send + Sync + 'static>(&self) -> bool {
        self.services.contains::<T>()
    }

    /// Returns the `TypeId` keys of all registered services (for debugging).
    ///
    /// Combine with `std::any::type_name::<KnownType>()` to correlate.
    /// `TypeId` does not carry the original type name at runtime.
    pub fn service_type_ids(&self) -> Vec<std::any::TypeId> {
        self.services.type_ids()
    }

    /// Resolve a service and clone the inner value.
    ///
    /// Shorthand for `app.get::<T>().map(|arc| arc.as_ref().clone())`.
    /// Requires `T: Clone`. Returns `None` if not registered.
    ///
    /// Use when you need an owned `T` and `T` is cheap to clone
    /// (e.g., config structs, small value types). For expensive-to-clone
    /// types, prefer `get()` and work with `Arc<T>`.
    pub fn get_cloned<T: Clone + Send + Sync + 'static>(&self) -> Option<T> {
        self.get::<T>().map(|arc| arc.as_ref().clone())
    }

    /// Resolve a service and clone the inner value, returning an error if missing.
    pub fn require_cloned<T: Clone + Send + Sync + 'static>(&self) -> AppResult<T> {
        self.get_cloned::<T>().ok_or_else(|| {
            AppMessage::not_found(format!(
                "Service of type {} not registered",
                std::any::type_name::<T>()
            ))
        })
    }

    /// Resolve a trait binding and clone the inner value.
    ///
    /// Shorthand for `app.get_trait::<T>().map(|arc| arc.as_ref().clone())`.
    pub fn get_trait_cloned<T: Clone + Send + Sync + 'static>(&self) -> Option<T> {
        self.get_trait::<T>().map(|arc| arc.as_ref().clone())
    }

    /// Resolve a trait binding and clone the inner value, returning an error if missing.
    pub fn require_trait_cloned<T: Clone + Send + Sync + 'static>(&self) -> AppResult<T> {
        self.get_trait_cloned::<T>().ok_or_else(|| {
            AppMessage::not_found(format!(
                "Trait binding for {} not registered",
                std::any::type_name::<T>()
            ))
        })
    }

    /// Resolve `T` from the container and fill the `Lazy<T>` field.
    /// Returns error if `T` is not registered.
    pub fn require_lazy<T: Send + Sync + 'static>(&self, lazy: &Lazy<T>) -> AppResult<()> {
        lazy.fill(self.require::<T>()?)
    }

    /// Resolve `T` from the container and fill the `Lazy<T>` field.
    ///
    /// Returns `Ok(())` on success. Returns an error if:
    /// - `T` is not registered in the container (dependency missing)
    /// - The `Lazy<T>` field was already filled (duplicate wiring)
    ///
    /// This is the fallible counterpart to [`require_lazy()`](Self::require_lazy).
    /// Unlike the previous version, errors are propagated to the caller
    /// instead of being silently logged.
    pub fn get_lazy<T: Send + Sync + 'static>(&self, lazy: &Lazy<T>) -> AppResult<()> {
        let arc = self.get::<T>().ok_or_else(|| {
            AppMessage::not_found(format!(
                "Service of type {} not registered",
                std::any::type_name::<T>()
            ))
        })?;
        lazy.fill(arc)
    }

    /// Returns the application environment.
    pub fn env(&self) -> Environment {
        self.env
    }

    /// Returns the application name.
    pub fn app_name(&self) -> &str {
        &self.app_name
    }

    /// Returns the application code.
    pub fn app_code(&self) -> &str {
        &self.app_code
    }

    /// Returns the application key.
    ///
    /// # Security
    /// Returns a redacted placeholder to prevent accidental secret leakage
    /// in logs or error messages. Use [`app_key_raw()`](Self::app_key_raw) for
    /// cryptographic operations that need the actual key material.
    pub fn app_key(&self) -> &str {
        "[REDACTED]"
    }

    /// Returns the raw application key for cryptographic operations.
    ///
    /// # Security
    /// Prefer [`app_key()`](Self::app_key) for logging/display. This method
    /// exposes the actual secret key material. A tracing warning is emitted
    /// on each call to aid audit logging.
    pub fn app_key_raw(&self) -> &str {
        tracing::warn!(
            app = self.app_name(),
            "app_key_raw() accessed - secret material exposed"
        );
        &self.app_key
    }

    /// Returns the environment variable prefix.
    pub fn app_env_prefix(&self) -> &str {
        &self.app_env_prefix
    }

    /// Returns the application public key.
    pub fn app_public_key(&self) -> &str {
        &self.app_public_key
    }

    /// Returns the application private key.
    ///
    /// # Security
    /// Returns a redacted placeholder to prevent accidental secret leakage.
    /// Use [`app_private_key_raw()`](Self::app_private_key_raw) for
    /// cryptographic operations.
    pub fn app_private_key(&self) -> &str {
        "[REDACTED]"
    }

    /// Returns the raw application private key for cryptographic operations.
    ///
    /// # Security
    /// Prefer [`app_private_key()`](Self::app_private_key) for logging/display.
    /// A tracing warning is emitted on each call to aid audit logging.
    pub fn app_private_key_raw(&self) -> &str {
        tracing::warn!(
            app = self.app_name(),
            "app_private_key_raw() accessed - secret material exposed"
        );
        &self.app_private_key
    }

    /// Returns the application version, if set.
    pub fn version(&self) -> Option<&str> {
        self.app_version.as_deref()
    }

    /// Returns the instant when the app was built.
    pub fn started_at(&self) -> Instant {
        self.started_at
    }

    /// Returns the duration since the app was built.
    pub fn uptime(&self) -> std::time::Duration {
        self.started_at.elapsed()
    }

    /// Returns `true` if `shutdown()` has been called.
    pub fn is_shutting_down(&self) -> bool {
        self.shutdown_initiated.load(Ordering::SeqCst)
    }

    /// Returns a reference to the configured metrics sink, if any.
    pub fn metrics(&self) -> Option<&Arc<dyn MetricsSink>> {
        self.metrics.as_ref()
    }

    /// Returns a reference to the event bus.
    pub fn events(&self) -> &EventBus {
        &self.event_bus
    }

    /// Returns the number of registered services (for debugging).
    pub fn service_count(&self) -> usize {
        self.services.len()
    }

    /// Returns the configured per-health-check timeout.
    pub fn health_check_timeout(&self) -> std::time::Duration {
        self.health_check_timeout
    }

    /// Format a page title with the app name suffix.
    pub fn title(&self, text: &str) -> Cow<'_, str> {
        Cow::Owned(format!("{} - {}", text, self.app_name))
    }

    /// Returns a reference to the database connection pool.
    ///
    /// # Errors
    /// Returns an error if the database feature is enabled but no pool was configured.
    #[cfg(feature = "database")]
    pub fn db(&self) -> AppResult<&DBPool> {
        self.db.as_ref().ok_or_else(|| AppMessage::Infrastructure {
            message: "Database pool not configured".to_string(),
            source: None,
        })
    }

    /// Returns a reference to the database pool, or `None` if not configured.
    #[cfg(feature = "database")]
    pub fn try_db(&self) -> Option<&DBPool> {
        self.db.as_ref()
    }

    /// Returns a reference to the async database connection pool.
    ///
    /// # Errors
    /// Returns an error if the database-async feature is enabled but no pool was configured.
    #[cfg(feature = "database-async")]
    pub fn async_db(&self) -> AppResult<&AsyncDBPool> {
        self.async_db
            .as_ref()
            .ok_or_else(|| AppMessage::Infrastructure {
                message: "Async database pool not configured".to_string(),
                source: None,
            })
    }

    /// Returns a reference to the async database pool, or `None` if not configured.
    #[cfg(feature = "database-async")]
    pub fn try_async_db(&self) -> Option<&AsyncDBPool> {
        self.async_db.as_ref()
    }

    /// Returns a reference to the Redis client.
    ///
    /// # Errors
    /// Returns an error if the redis feature is enabled but no redis was configured.
    #[cfg(feature = "redis")]
    pub fn redis(&self) -> AppResult<&Redis> {
        self.redis
            .as_ref()
            .ok_or_else(|| AppMessage::Infrastructure {
                message: "Redis not configured".to_string(),
                source: None,
            })
    }

    /// Returns a reference to the Redis client, or `None` if not configured.
    #[cfg(feature = "redis")]
    pub fn try_redis(&self) -> Option<&Redis> {
        self.redis.as_ref()
    }

    /// Returns a clone of the Redis pool.
    ///
    /// # Errors
    /// Returns an error if the redis feature is enabled but no pool was configured.
    #[cfg(feature = "redis")]
    pub fn redis_pool(&self) -> AppResult<deadpool_redis::Pool> {
        self.redis_pool
            .clone()
            .ok_or_else(|| AppMessage::Infrastructure {
                message: "Redis pool not configured".to_string(),
                source: None,
            })
    }

    /// Returns a clone of the Redis pool, or `None` if not configured.
    #[cfg(feature = "redis")]
    pub fn try_redis_pool(&self) -> Option<deadpool_redis::Pool> {
        self.redis_pool.clone()
    }

    /// Returns an Arc to the RabbitMQ client.
    ///
    /// # Errors
    /// Returns an error if RabbitMQ is enabled but was not configured.
    #[cfg(feature = "rabbitmq")]
    pub fn rabbitmq(&self) -> AppResult<&RabbitMQ> {
        self.rabbitmq
            .as_ref()
            .ok_or_else(|| AppMessage::Infrastructure {
                message: "RabbitMQ not configured".to_string(),
                source: None,
            })
    }

    /// Returns a reference to the RabbitMQ client, or `None` if not configured.
    #[cfg(feature = "rabbitmq")]
    pub fn try_rabbitmq(&self) -> Option<&RabbitMQ> {
        self.rabbitmq.as_ref()
    }

    /// Returns a clone of the RabbitMQ pool.
    ///
    /// # Errors
    /// Returns an error if RabbitMQ is enabled but no pool was configured.
    #[cfg(feature = "rabbitmq")]
    pub fn rabbitmq_pool(&self) -> AppResult<deadpool_lapin::Pool> {
        self.rabbitmq_pool
            .clone()
            .ok_or_else(|| AppMessage::Infrastructure {
                message: "RabbitMQ pool not configured".to_string(),
                source: None,
            })
    }

    /// Returns a clone of the RabbitMQ pool, or `None` if not configured.
    #[cfg(feature = "rabbitmq")]
    pub fn try_rabbitmq_pool(&self) -> Option<deadpool_lapin::Pool> {
        self.rabbitmq_pool.clone()
    }

    /// Returns a clone of the Cache instance.
    ///
    /// # Errors
    /// Returns an error if cache is enabled but was not configured.
    #[cfg(feature = "cache")]
    pub fn cache(&self) -> AppResult<&Cache> {
        self.cache
            .as_ref()
            .ok_or_else(|| AppMessage::Infrastructure {
                message: "Cache not configured".to_string(),
                source: None,
            })
    }

    /// Returns a reference to the Cache instance, or `None` if not configured.
    #[cfg(feature = "cache")]
    pub fn try_cache(&self) -> Option<&Cache> {
        self.cache.as_ref()
    }

    /// Returns a reference to the Tera template engine.
    ///
    /// Tera is stored as a dedicated `Arc<Tera>` field on `App`, initialized
    /// during build. This accessor provides a safe borrowed reference.
    ///
    /// # Errors
    /// Returns an error if templating is enabled but was not configured.
    #[cfg(feature = "templating")]
    pub fn tera(&self) -> AppResult<&Tera> {
        self.tera
            .as_deref()
            .ok_or_else(|| AppMessage::Infrastructure {
                message: "Templating not configured".to_string(),
                source: None,
            })
    }

    /// Returns a reference to the Tera template engine, or `None` if not configured.
    #[cfg(feature = "templating")]
    pub fn try_tera(&self) -> Option<&Tera> {
        self.tera.as_deref()
    }

    /// Render a Tera template by name.
    #[cfg(feature = "templating")]
    pub fn render(&self, file: impl Into<String>, context: &Context) -> AppResult<String> {
        let mut file = file.into();
        if !file.ends_with(".tera.html") {
            file.push_str(".tera.html");
        }
        self.tera()?
            .render(&file, context)
            .map_err(|e| AppMessage::Infrastructure {
                message: format!("Template render error: {e}"),
                source: Some(Box::new(e)),
            })
    }

    /// Returns a reference to the JWT helper.
    ///
    /// # Errors
    /// Returns an error if JWT is enabled but was not configured.
    #[cfg(feature = "jwt")]
    pub fn jwt(&self) -> AppResult<&Jwt> {
        self.jwt.as_ref().ok_or_else(|| AppMessage::Infrastructure {
            message: "JWT not configured".to_string(),
            source: None,
        })
    }

    /// Returns a reference to the JWT helper, or `None` if not configured.
    #[cfg(feature = "jwt")]
    pub fn try_jwt(&self) -> Option<&Jwt> {
        self.jwt.as_ref()
    }

    /// Returns a reference to the JWE helper.
    ///
    /// # Errors
    /// Returns an error if JWE is enabled but was not configured.
    #[cfg(feature = "jwe")]
    pub fn jwe(&self) -> AppResult<&Jwe> {
        self.jwe.as_ref().ok_or_else(|| AppMessage::Infrastructure {
            message: "JWE not configured".to_string(),
            source: None,
        })
    }

    /// Returns a reference to the JWE helper, or `None` if not configured.
    #[cfg(feature = "jwe")]
    pub fn try_jwe(&self) -> Option<&Jwe> {
        self.jwe.as_ref()
    }

    /// Returns a reference to the password helper.
    ///
    /// # Errors
    /// Returns an error if crypto is enabled but was not configured.
    #[cfg(feature = "crypto")]
    pub fn password(&self) -> AppResult<&Password> {
        self.password
            .as_ref()
            .ok_or_else(|| AppMessage::Infrastructure {
                message: "Password helper not configured".to_string(),
                source: None,
            })
    }

    /// Returns a reference to the password helper, or `None` if not configured.
    #[cfg(feature = "crypto")]
    pub fn try_password(&self) -> Option<&Password> {
        self.password.as_ref()
    }

    /// Run all registered startup hooks concurrently.
    ///
    /// This is called automatically by [`AppBuilder::build()`] after services
    /// are initialized. You typically don't need to call this manually.
    ///
    /// # Concurrency
    ///
    /// **All hooks are dispatched concurrently via `join_all`** - they may
    /// execute in any order and overlap in time. This means:
    ///
    /// - **No ordering guarantees**: If hook B depends on work done by hook A,
    ///   they must be combined into a single hook or use explicit synchronization
    ///   (e.g., `Arc<Mutex<T>>`, channels, or `tokio::sync::Barrier`).
    /// - **Shared state must be thread-safe**: Multiple hooks may access shared
    ///   resources concurrently; use `Arc`, `Mutex`, `RwLock`, or atomic types.
    /// - **Fail-fast behavior**: If any hook fails, the error is logged and
    ///   returned, but other hooks continue to run concurrently.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use foxtive::App;
    ///
    /// # async fn run() -> foxtive::results::AppResult<()> {
    /// let app = App::builder("my-app", "MYAPP")
    ///     .on_startup(|app| Box::pin(async move {
    ///         println!("Hook 1: initializing cache");
    ///         Ok(())
    ///     }))
    ///     .on_startup(|app| Box::pin(async move {
    ///         println!("Hook 2: warming up indexes");
    ///         Ok(())
    ///     }))
    ///     .build()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn run_startup_hooks(self: &Arc<Self>) -> AppResult<()> {
        if self.startup_hooks.len() > 1 {
            tracing::warn!(
                hook_count = self.startup_hooks.len(),
                "Multiple startup hooks dispatched concurrently with no ordering guarantees. \
                 Use run_startup_hooks_sequential() if hooks have dependencies."
            );
        }

        let futures: Vec<_> = self
            .startup_hooks
            .iter()
            .map(|hook| hook(self.clone()))
            .collect();
        let results = futures_util::future::join_all(futures).await;
        for result in &results {
            if let Err(e) = result {
                tracing::error!(error = %e, "Startup hook failed");
            }
        }
        results.into_iter().find(|r| r.is_err()).unwrap_or(Ok(()))
    }

    /// Run all registered startup hooks sequentially, in registration order.
    ///
    /// Use this when hooks have implicit dependencies (e.g., hook B requires
    /// state initialized by hook A). For independent hooks, prefer
    /// [`run_startup_hooks()`](Self::run_startup_hooks) for parallel execution.
    ///
    /// All hooks run even if some fail; the first error is returned.
    pub async fn run_startup_hooks_sequential(self: &Arc<Self>) -> AppResult<()> {
        let mut first_error = None;
        for hook in &self.startup_hooks {
            if let Err(e) = hook(self.clone()).await {
                tracing::error!(error = %e, "Startup hook failed");
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Run all registered shutdown hooks in reverse registration order.
    pub async fn run_shutdown_hooks(self: &Arc<Self>) {
        for hook in self.shutdown_hooks.iter().rev() {
            hook(self.clone()).await;
        }
    }

    /// Initiate a graceful shutdown of the application.
    ///
    /// Idempotent - only the first call executes shutdown hooks.
    /// Hooks run in LIFO order (last registered, first shut down).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use foxtive::App;
    ///
    /// # async fn run() -> foxtive::results::AppResult<()> {
    /// let app: Arc<App> = App::builder("my-app", "MYAPP")
    ///     .build()
    ///     .await?;
    ///
    /// // ... application running ...
    ///
    /// // Graceful shutdown (safe to call multiple times)
    /// app.shutdown().await;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn shutdown(self: &Arc<Self>) {
        if self.shutdown_initiated.swap(true, Ordering::SeqCst) {
            tracing::debug!(
                app = self.app_name(),
                "Shutdown already in progress, skipping"
            );
            return;
        }

        tracing::info!(
            app = self.app_name(),
            hooks = self.shutdown_hooks.len(),
            timeout = ?self.shutdown_timeout,
            "Initiating graceful shutdown"
        );

        let start = Instant::now();

        // Run shutdown hooks with a timeout
        match tokio::time::timeout(self.shutdown_timeout, self.run_shutdown_hooks()).await {
            Ok(()) => {}
            Err(_) => {
                tracing::error!(
                    app = self.app_name(),
                    timeout = ?self.shutdown_timeout,
                    "Shutdown hooks timed out, proceeding with forced shutdown"
                );
            }
        }

        // Drain connection pools
        #[cfg(feature = "redis")]
        if let Some(pool) = &self.redis_pool {
            pool.close();
            tracing::debug!("Redis connection pool drained");
        }

        #[cfg(feature = "rabbitmq")]
        if let Some(pool) = &self.rabbitmq_pool {
            pool.close();
            tracing::debug!("RabbitMQ connection pool drained");
        }

        #[cfg(feature = "database")]
        if let Some(_pool) = &self.db {
            // r2d2 pools don't have an explicit drain; drop handles cleanup
            tracing::debug!("Database connection pool released");
        }

        #[cfg(feature = "database-async")]
        if let Some(_pool) = &self.async_db {
            // deadpool pools don't have an explicit close(); drop handles cleanup
            tracing::debug!("Async database connection pool released");
        }

        tracing::info!(
            app = self.app_name(),
            duration_ms = start.elapsed().as_millis() as u64,
            "Graceful shutdown complete"
        );
    }

    /// Returns a reference to the Tokio helper.
    ///
    /// Always available - created during `AppBuilder::build()` with configured
    /// concurrency limits. Use `block()` for spawning blocking work and
    /// `run_async()` for sync→async bridging.
    pub fn tokio(&self) -> &Tokio {
        &self.tokio
    }

    /// Run all registered health checks concurrently (each with a per-check timeout)
    /// and return an aggregated report.
    pub async fn check_health(&self) -> HealthReport {
        let (checks, duration) = crate::health::run_health_checks(
            &self.health_checks,
            self,
            self.health_check_timeout,
            self.metrics.as_ref(),
        )
        .await;

        let status = aggregate_status(&checks);

        if let Some(sink) = &self.metrics {
            sink.record(&InfraEvent::HealthReportGenerated {
                duration,
                healthy: status.is_healthy(),
                check_count: checks.len(),
            });
        }

        HealthReport {
            status,
            checks: Arc::new(checks),
            duration,
        }
    }
}

impl Debug for App {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("App");
        debug
            .field("env", &self.env)
            .field("app_name", &self.app_name)
            .field("app_code", &self.app_code)
            .field("app_version", &self.app_version)
            .field("app_key", &"[REDACTED]")
            .field("service_count", &self.services.len())
            .field("startup_hooks", &self.startup_hooks.len())
            .field("shutdown_hooks", &self.shutdown_hooks.len())
            .field("health_checks", &self.health_checks.len())
            .field(
                "is_shutting_down",
                &self.shutdown_initiated.load(Ordering::SeqCst),
            );

        // Add pool status for debugging
        #[cfg(feature = "database")]
        if let Some(pool) = &self.db {
            let state = pool.state();
            debug.field(
                "db_pool",
                &format!(
                    "{}/{} connections",
                    state.idle_connections, state.connections
                ),
            );
        }

        #[cfg(feature = "database-async")]
        if let Some(pool) = &self.async_db {
            let status = pool.status();
            debug.field(
                "async_db_pool",
                &format!("{}/{} connections", status.available, status.size),
            );
        }

        #[cfg(feature = "redis")]
        if let Some(pool) = &self.redis_pool {
            let status = pool.status();
            debug.field(
                "redis_pool",
                &format!("{}/{} connections", status.available, status.size),
            );
        }

        #[cfg(feature = "rabbitmq")]
        if let Some(pool) = &self.rabbitmq_pool {
            let status = pool.status();
            debug.field(
                "rabbitmq_pool",
                &format!("{}/{} connections", status.available, status.size),
            );
        }

        debug.finish_non_exhaustive()
    }
}

//! Builder for constructing [`App`] instances.

use std::collections::HashSet;
use std::future::Future;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

use zeroize::Zeroizing;

use crate::app::init::AppInit;
use crate::app::App;
use crate::container::TypeMap;
use crate::events::EventBus;
use crate::health::HealthCheck;
use crate::lifecycle::{ClosureFactory, Plugin, ServiceFactoryImpl, ServiceInit, ShutdownFuture, ShutdownHook, StartupFuture, StartupHook};
use crate::metrics::MetricsSink;
use crate::app::deps::ServiceResolutionError;
use crate::results::AppResult;
use crate::Environment;

/// Type-erased callback that runs after infrastructure is initialized,
/// receiving `&mut AppInit` so it can register infrastructure-dependent services.
pub(crate) type AfterBuildHook =
    Box<dyn FnMut(&mut AppInit) -> AppResult<()> + Send + Sync + 'static>;

#[cfg(any(feature = "templating", feature = "cache-redis", feature = "rabbitmq"))]
use crate::enums::AppMessage;

use tracing::debug;

#[cfg(feature = "cache")]
use crate::cache::Cache;
#[cfg(feature = "database")]
use crate::database::{create_db_pool, DbConfig};
#[cfg(feature = "database-async")]
use crate::database::{create_async_db_pool, AsyncDBPool};
#[cfg(feature = "jwt")]
use crate::helpers::jwt::{Jwt, JwtConfig};
#[cfg(feature = "jwe")]
use crate::helpers::jwe::{Jwe, JweConfig};
use crate::helpers::{RuntimeConfig, set_runtime_config};
#[cfg(feature = "crypto")]
use crate::helpers::password::Password;
#[cfg(feature = "rabbitmq")]
use crate::prelude::RabbitMQ;
#[cfg(feature = "redis")]
use crate::prelude::Redis;
#[cfg(feature = "rabbitmq")]
use crate::rabbitmq::conn::create_rmq_conn_pool;
#[cfg(feature = "redis")]
use crate::redis::conn::create_redis_conn_pool;
#[cfg(feature = "cache")]
use crate::setup::CacheDriverSetup;
#[cfg(feature = "templating")]
use tera::Tera;
use crate::tokio::Tokio;

/// A builder for constructing [`App`] instances.
///
/// Provides a fluent API for configuring all application services, lifecycle hooks,
/// and health checks before building the final [`App`] container.
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
///     .app_key("my-secret-key")
///     .build()
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct AppBuilder {
    app_name: String,
    app_code: String,

    env: Environment,
    app_version: Option<String>,
    app_key: Zeroizing<String>,
    app_env_prefix: String,
    app_public_key: Zeroizing<String>,
    app_private_key: Zeroizing<String>,

    services: TypeMap,
    service_factories: Vec<Box<dyn crate::lifecycle::ServiceFactory>>,
    /// Tracks registered service type names to detect duplicates.
    registered_service_types: HashSet<&'static str>,

    #[cfg(feature = "database")]
    db_config: Option<DbConfig>,

    #[cfg(feature = "database-async")]
    async_db_config: Option<DbConfig>,

    #[cfg(feature = "redis")]
    redis_config: Option<crate::redis::config::RedisConfig>,

    #[cfg(feature = "rabbitmq")]
    rmq_config: Option<crate::rabbitmq::config::RabbitmqConfig>,

    #[cfg(feature = "cache")]
    cache_driver_setup: Option<CacheDriverSetup>,

    #[cfg(feature = "jwt")]
    jwt_config: Option<JwtConfig>,

    #[cfg(feature = "jwe")]
    jwe_config: Option<JweConfig>,

    #[cfg(feature = "templating")]
    template_directory: Option<String>,

    startup_hooks: Vec<StartupHook>,
    shutdown_hooks: Vec<ShutdownHook>,
    after_build_hooks: Vec<AfterBuildHook>,

    health_checks: Vec<Box<dyn HealthCheck>>,
    health_check_timeout: std::time::Duration,
    shutdown_timeout: std::time::Duration,
    metrics: Option<Arc<dyn MetricsSink>>,
    runtime_config: Option<RuntimeConfig>,
}

impl AppBuilder {
    /// Create a new builder with the given app name and code.
    pub fn new(app_name: impl Into<String>, app_code: impl Into<String>) -> Self {
        Self {
            app_name: app_name.into(),
            app_code: app_code.into(),
            env: Environment::default(),
            app_version: None,
            app_key: Zeroizing::new(String::new()),
            app_env_prefix: String::new(),
            app_public_key: Zeroizing::new(String::new()),
            app_private_key: Zeroizing::new(String::new()),
            services: TypeMap::new(),
            service_factories: Vec::new(),
            registered_service_types: HashSet::new(),

            #[cfg(feature = "database")]
            db_config: None,
            #[cfg(feature = "database-async")]
            async_db_config: None,
            #[cfg(feature = "redis")]
            redis_config: None,
            #[cfg(feature = "rabbitmq")]
            rmq_config: None,
            #[cfg(feature = "cache")]
            cache_driver_setup: None,
            #[cfg(feature = "jwt")]
            jwt_config: None,
            #[cfg(feature = "jwe")]
            jwe_config: None,
            #[cfg(feature = "templating")]
            template_directory: None,

            startup_hooks: Vec::new(),
            shutdown_hooks: Vec::new(),
            after_build_hooks: Vec::new(),

            health_checks: Vec::new(),
            health_check_timeout: std::time::Duration::from_secs(5),
            shutdown_timeout: std::time::Duration::from_secs(30),
            metrics: None,
            runtime_config: None,
        }
    }

    /// Set the application environment.
    pub fn environment(mut self, env: Environment) -> Self {
        self.env = env;
        self
    }

    /// Set the application version.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.app_version = Some(version.into());
        self
    }

    /// Set the application secret key.
    pub fn app_key(mut self, key: impl Into<String>) -> Self {
        self.app_key = Zeroizing::new(key.into());
        self
    }

    /// Set the environment variable prefix.
    pub fn env_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.app_env_prefix = prefix.into();
        self
    }

    /// Set the application public key.
    pub fn public_key(mut self, key: impl Into<String>) -> Self {
        self.app_public_key = Zeroizing::new(key.into());
        self
    }

    /// Set the application private key.
    pub fn private_key(mut self, key: impl Into<String>) -> Self {
        self.app_private_key = Zeroizing::new(key.into());
        self
    }

    /// Configure the global fallback Tokio runtime.
    ///
    /// Controls worker threads, blocking threads, and thread naming for the
    /// dedicated runtime used by `run_async()` and as a fallback for `block()`.
    /// Must be called before `build()` - once the runtime is created, changes
    /// have no effect.
    pub fn runtime_config(mut self, config: RuntimeConfig) -> Self {
        self.runtime_config = Some(config);
        self
    }

    /// Configure the database connection pool.
    #[cfg(feature = "database")]
    pub fn database(mut self, config: DbConfig) -> Self {
        self.db_config = Some(config);
        self
    }

    /// Configure the async database connection pool.
    #[cfg(feature = "database-async")]
    pub fn async_database(mut self, config: DbConfig) -> Self {
        self.async_db_config = Some(config);
        self
    }

    /// Configure the Redis connection.
    #[cfg(feature = "redis")]
    pub fn redis(mut self, config: crate::redis::config::RedisConfig) -> Self {
        self.redis_config = Some(config);
        self
    }

    /// Configure the RabbitMQ connection.
    #[cfg(feature = "rabbitmq")]
    pub fn rabbitmq(mut self, config: crate::rabbitmq::config::RabbitmqConfig) -> Self {
        self.rmq_config = Some(config);
        self
    }

    /// Configure the cache driver.
    #[cfg(feature = "cache")]
    pub fn cache(mut self, setup: CacheDriverSetup) -> Self {
        self.cache_driver_setup = Some(setup);
        self
    }

    /// Configure JWT token generation and validation.
    ///
    /// Accepts a `JwtConfig` which can be created for RSA or HMAC:
    /// - `JwtConfig::rsa_pem(public_pem, private_pem, lifetime)` for asymmetric keys
    /// - `JwtConfig::hmac(secret, lifetime)` for symmetric keys
    ///
    /// # Example
    ///
    /// ```ignore
    /// use foxtive::helpers::jwt::JwtConfig;
    ///
    /// // HMAC-based JWT
    /// builder.jwt(JwtConfig::hmac(b"my-secret", 60));
    ///
    /// // RSA-based JWT
    /// builder.jwt(JwtConfig::rsa_pem(public_pem, private_pem, 60)?);
    /// ```
    #[cfg(feature = "jwt")]
    pub fn jwt(mut self, config: JwtConfig) -> Self {
        self.jwt_config = Some(config);
        self
    }

    /// Configure JWE encryption.
    ///
    /// Accepts a `JweConfig` which holds the key material and optional default algorithms.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use foxtive::helpers::jwe::{JweConfig, JweAlgorithm, JweEncryption};
    ///
    /// let config = JweConfig::symmetric(b"0123456789abcdef")?
    ///     .with_defaults(JweAlgorithm::A256KW, JweEncryption::A256GCM);
    /// builder.jwe(config);
    /// ```
    #[cfg(feature = "jwe")]
    pub fn jwe(mut self, config: JweConfig) -> Self {
        self.jwe_config = Some(config);
        self
    }

    /// Configure the template directory for Tera.
    #[cfg(feature = "templating")]
    pub fn template_directory(mut self, dir: impl Into<String>) -> Self {
        self.template_directory = Some(dir.into());
        self
    }

    /// Register a callback that runs after infrastructure is initialized
    /// but before the `App` is frozen.
    ///
    /// The callback receives `&mut AppInit` with full access to typed accessors
    /// (`init.db()`, `init.redis()`, etc.) and `register()`. Use it to register
    /// services that depend on infrastructure. Callbacks run in registration order.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use foxtive::App;
    /// use foxtive::prelude::AppResult;
    ///
    /// struct UserService;
    ///
    /// # async fn run() -> AppResult<()> {
    /// let app = App::builder("my-app", "MYAPP")
    ///     // .database(config).redis(config)
    ///     .after_build(|init| {
    ///         init.register(UserService);
    ///         Ok(())
    ///     })
    ///     .build()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn after_build<F>(mut self, hook: F) -> Self
    where
        F: FnMut(&mut AppInit) -> AppResult<()> + Send + Sync + 'static,
    {
        self.after_build_hooks.push(Box::new(hook));
        self
    }

    /// Register a custom service in the DI container.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use foxtive::App;
    ///
    /// # async fn run() -> foxtive::results::AppResult<()> {
    /// struct UserService { db_url: String }
    ///
    /// let app = App::builder("my-app", "MYAPP")
    ///     .register(UserService { db_url: "postgres://...".into() })
    ///     .build()
    ///     .await?;
    ///
    /// let svc = app.get::<UserService>().unwrap();
    /// # Ok(())
    /// # }
    /// ```
    pub fn register<T: Send + Sync + 'static>(mut self, service: T) -> Self {
        self.services.insert(service);
        self
    }

    /// Register a service type for deferred construction.
    ///
    /// Services implementing [`ServiceInit`](ServiceInit) are constructed
    /// during `build()`, receiving `&App` to access dependencies.
    ///
    /// Dependencies declared via `ServiceInit::dependencies()` are resolved automatically
    /// using topological sort.
    ///
    /// If the same type is registered twice, a warning is logged and the duplicate is skipped.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use foxtive::App;
    /// use foxtive::lifecycle::ServiceInit;
    /// use foxtive::prelude::AppResult;
    ///
    /// struct UserService;
    ///
    /// impl ServiceInit for UserService {
    ///     async fn init(app: &App) -> AppResult<Self> {
    ///         Ok(Self)
    ///     }
    /// }
    ///
    /// # async fn run() -> AppResult<()> {
    /// let app = App::builder("my-app", "MYAPP")
    ///     .register_service::<UserService>()
    ///     .build()
    ///     .await?;
    ///
    /// let svc = app.get::<UserService>().unwrap();
    /// # Ok(())
    /// # }
    /// ```
    pub fn register_service<T: ServiceInit>(mut self) -> Self {
        let type_name = std::any::type_name::<T>();
        if !self.registered_service_types.insert(type_name) {
            tracing::warn!(
                service = type_name,
                "Duplicate register_service call - skipping"
            );
            return self;
        }
        let factory = ServiceFactoryImpl::<T>::new().with_dependencies(T::dependencies());
        self.service_factories.push(Box::new(factory));
        self
    }

    /// Register a service for deferred construction, stored as [`Mutable<T>`](crate::container::Mutable).
    ///
    /// The service `T` must implement `ServiceInit`. It will be constructed
    /// during `build()` and wrapped in `Mutable<T>` for shared interior mutability.
    /// Retrieve with `app.get_mutable::<T>()` or `app.require_mutable::<T>()`.
    ///
    /// This is the builder-form equivalent of [`AppInit::register_mutable_service`].
    pub fn register_mutable_service<T: ServiceInit>(mut self) -> Self {
        let type_name = std::any::type_name::<T>();
        if !self.registered_service_types.insert(type_name) {
            tracing::warn!(
                service = type_name,
                "Duplicate register_mutable_service call - skipping"
            );
            return self;
        }
        let factory = ServiceFactoryImpl::<T>::new()
            .with_dependencies(T::dependencies())
            .with_mutable(true);
        self.service_factories.push(Box::new(factory));
        self
    }

    /// Register a trait binding (eager).
    ///
    /// The implementation is boxed as `Arc<dyn Trait>` and keyed by
    /// `TypeId::of::<dyn Trait>()`. Retrieve with `app.require_trait::<dyn Trait>()`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use foxtive::App;
    /// use std::sync::Arc;
    ///
    /// trait Notifier: Send + Sync {
    ///     fn notify(&self, msg: &str);
    /// }
    ///
    /// struct EmailNotifier;
    /// impl Notifier for EmailNotifier {
    ///     fn notify(&self, _msg: &str) {}
    /// }
    ///
    /// # async fn run() -> foxtive::results::AppResult<()> {
    /// let app = App::builder("my-app", "MYAPP")
    ///     .register_trait::<dyn Notifier>(Arc::new(EmailNotifier))
    ///     .build()
    ///     .await?;
    ///
    /// let notifier = app.require_trait::<dyn Notifier>()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn register_trait<Trait>(mut self, impl_: Arc<Trait>) -> Self
    where
        Trait: ?Sized + Send + Sync + 'static,
    {
        self.services.insert_trait::<Trait>(impl_);
        self
    }

    /// Register a service via a factory closure.
    ///
    /// The closure receives `&App` and returns an `AppResult<T>`.
    /// The service participates in topological ordering and Phase 2 retry.
    ///
    /// # Example
    /// ```no_run
    /// # use foxtive::App;
    /// # use foxtive::prelude::AppResult;
    /// struct MyClient { url: String }
    ///
    /// # async fn run() -> AppResult<()> {
    /// let app = App::builder("app", "APP")
    ///     .register_with(|_app| async {
    ///         Ok(MyClient { url: "http://...".into() })
    ///     })
    ///     .build()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn register_with<T, F, Fut>(mut self, f: F) -> Self
    where
        T: Send + Sync + 'static,
        F: Fn(&App) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = AppResult<T>> + Send + 'static,
    {
        let type_name_str = std::any::type_name::<T>();
        let wrapper = move |app: &App| -> crate::lifecycle::ServiceFactoryFuture<'static> {
            let fut = f(app);
            Box::pin(async move {
                let service = fut.await.map_err(|e| {
                    ServiceResolutionError::Terminal(
                        crate::app::DiError::ServiceConstructionFailed {
                            service: type_name_str.to_string(),
                            source: Box::new(e),
                        }.into(),
                    )
                })?;
                Ok(Box::new(service) as Box<dyn std::any::Any + Send + Sync>)
            })
        };
        self.service_factories.push(Box::new(ClosureFactory::new(
            Box::new(wrapper), type_name_str, vec![],
        )));
        self
    }

    /// Register a service for deferred construction only if `condition` is true.
    pub fn register_service_if<T: ServiceInit>(self, condition: bool) -> Self {
        if condition {
            self.register_service::<T>()
        } else {
            self
        }
    }

    /// Register a service for deferred construction.
    /// Silently no-ops if the same type is already registered (idempotent).
    pub fn try_register_service<T: ServiceInit>(mut self) -> Self {
        let type_name = std::any::type_name::<T>();
        if self.registered_service_types.contains(type_name) {
            return self;
        }
        self.registered_service_types.insert(type_name);
        let factory = ServiceFactoryImpl::<T>::new().with_dependencies(T::dependencies());
        self.service_factories.push(Box::new(factory));
        self
    }

    /// Replace a previously registered service with a new registration.
    /// Logs the replacement. If no prior registration exists, acts as `register_service`.
    pub fn replace_service<T: ServiceInit>(mut self) -> Self {
        let type_name = std::any::type_name::<T>();
        if let Some(pos) = self.service_factories.iter().position(|f| f.type_name() == type_name) {
            tracing::info!(service = type_name, "Replacing previously registered service");
            self.service_factories.remove(pos);
        }
        self.registered_service_types.insert(type_name);
        let factory = ServiceFactoryImpl::<T>::new().with_dependencies(T::dependencies());
        self.service_factories.push(Box::new(factory));
        self
    }

    /// Register a service instance only if `condition` is true.
    pub fn register_if<T: Send + Sync + 'static>(self, condition: bool, service: T) -> Self {
        if condition {
            self.register(service)
        } else {
            self
        }
    }

    /// Register a [`Plugin`] - a self-contained module that bundles services,
    /// lifecycle hooks, and health checks.
    ///
    /// Plugins are the recommended way to attach companion crates (foxtive-axum,
    /// foxtive-worker, etc.) or reusable feature modules to the application.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use foxtive::App;
    /// use foxtive::lifecycle::Plugin;
    /// use foxtive::app::AppBuilder;
    /// use foxtive::prelude::AppResult;
    ///
    /// struct AuthPlugin;
    ///
    /// impl Plugin for AuthPlugin {
    ///     fn name(&self) -> &str { "auth" }
    ///
    ///     fn register(&self, builder: AppBuilder) -> AppBuilder {
    ///         builder // register services here
    ///     }
    /// }
    ///
    /// # async fn run() -> foxtive::results::AppResult<()> {
    /// let app = App::builder("my-app", "MYAPP")
    ///     .plugin(AuthPlugin)
    ///     .build()
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn plugin<P: Plugin>(self, plugin: P) -> Self {
        debug!("Registering plugin: {}", plugin.name());

        let mut builder = plugin.register(self);

        for check in plugin.health_checks() {
            builder.health_checks.push(check);
        }

        let name = plugin.name().to_string();
        let plugin = Arc::new(plugin);

        // Wire up after_build hook
        let p = Arc::clone(&plugin);
        let ab_name = name.clone();
        builder.after_build_hooks.push(Box::new(move |init| {
            debug!(plugin = %ab_name, "Running plugin after_build");
            p.after_build(init)
        }));

        let p = Arc::clone(&plugin);
        let startup_name = name.clone();
        builder.startup_hooks.push(Box::new(move |app| {
            let p = Arc::clone(&p);
            let startup_name = startup_name.clone();
            Box::pin(async move {
                debug!(plugin = %startup_name, "Running plugin startup");
                p.on_startup(&app).await
            }) as StartupFuture
        }));

        let p = Arc::clone(&plugin);
        let shutdown_name = name;
        builder.shutdown_hooks.push(Box::new(move |app| {
            let p = Arc::clone(&p);
            let shutdown_name = shutdown_name.clone();
            Box::pin(async move {
                debug!(plugin = %shutdown_name, "Running plugin shutdown");
                p.on_shutdown(&app).await
            }) as ShutdownFuture
        }));

        builder
    }

    /// Register a startup hook that runs during `App::run_startup_hooks()`.
    pub fn on_startup<F, Fut>(mut self, hook: F) -> Self
    where
        F: Fn(Arc<App>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = AppResult<()>> + Send + 'static,
    {
        self.startup_hooks.push(Box::new(move |app| {
            let fut = hook(app);
            Box::pin(fut) as StartupFuture
        }));
        self
    }

    /// Register a shutdown hook that runs during `App::run_shutdown_hooks()`.
    pub fn on_shutdown<F, Fut>(mut self, hook: F) -> Self
    where
        F: Fn(Arc<App>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.shutdown_hooks.push(Box::new(move |app| {
            let fut = hook(app);
            Box::pin(fut) as ShutdownFuture
        }));
        self
    }

    /// Register a health check.
    pub fn health_check(mut self, check: impl HealthCheck + 'static) -> Self {
        self.health_checks.push(Box::new(check));
        self
    }

    /// Set the per-check timeout for health checks.
    ///
    /// Each individual health check will be cancelled if it does not complete
    /// within this duration. Defaults to 5 seconds.
    pub fn health_check_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.health_check_timeout = timeout;
        self
    }

    /// Set the graceful shutdown timeout.
    ///
    /// Shutdown hooks will be cancelled if they do not complete within this
    /// duration. Defaults to 30 seconds.
    pub fn shutdown_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.shutdown_timeout = timeout;
        self
    }

    /// Register a [`MetricsSink`] for infrastructure metrics events.
    ///
    /// The sink receives events such as health-check durations, pool statistics,
    /// and operation latencies.
    pub fn metrics(mut self, sink: Arc<impl MetricsSink>) -> Self {
        self.metrics = Some(sink as Arc<dyn MetricsSink>);
        self
    }

    /// Build the [`App`], run `after_build` callbacks, construct all registered
    /// services, and return `Arc<App>`.
    ///
    /// 1. Initialize feature-gated services (database, redis, rabbitmq, cache, templating)
    /// 2. Create helper instances (jwt, password)
    /// 3. Run `after_build` callbacks (may register more services via `init.register()`)
    /// 4. Construct all registered services in dependency order
    /// 5. Wrap in `Arc<App>` and return
    ///
    /// Startup hooks are NOT called here - call `app.run_startup_hooks()` after build.
    pub async fn build(self) -> AppResult<Arc<App>> {
        let (mut init, after_build_hooks) = self.build_inner().await?;

        // Run after_build hooks (plugins register services here)
        for mut hook in after_build_hooks {
            hook(&mut init)?;
        }

        // freeze() runs service factories and wraps in Arc<App>
        init.freeze().await
    }

    /// Build the [`App`] and return an [`AppInit`] for manual service registration.
    ///
    /// Unlike [`build()`](Self::build), this returns `AppInit` before freezing,
    /// allowing manual service registration via `init.register()`. The `after_build`
    /// hooks (used by plugins) are still run - they execute before this method returns.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use foxtive::App;
    /// use foxtive::prelude::AppResult;
    ///
    /// struct UserService;
    ///
    /// # async fn run() -> AppResult<()> {
    /// let mut init = App::builder("my-app", "MYAPP")
    ///     // .database(config)
    ///     .build_init()
    ///     .await?;
    ///
    /// init.register(UserService);
    ///
    /// let app = init.freeze().await?; // -> Arc<App>
    /// assert!(app.get::<UserService>().is_some());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn build_init(self) -> AppResult<AppInit> {
        let (mut init, after_build_hooks) = self.build_inner().await?;

        // Run after_build hooks - plugins register their services here
        for mut hook in after_build_hooks {
            hook(&mut init)?;
        }

        Ok(init)
    }

    #[allow(unused_mut)]
    async fn build_inner(mut self) -> AppResult<(AppInit, Vec<AfterBuildHook>)> {
        debug!("Building App for: {} ({})", self.app_name, self.app_code);

        // Wire runtime config before anything can trigger runtime creation
        if let Some(config) = self.runtime_config.take() {
            set_runtime_config(config);
        }

        if self.env.is_dev_like() {
            tracing::warn!(
                app = self.app_name,
                env = self.env.as_str(),
                "Running in dev-like environment - ensure this is intentional for production deployments"
            );
        }

        // Validate all feature configurations
        self.validate_configs()?;

        // Initialize feature-gated services
        #[cfg(feature = "database")]
        let db = self.init_database()?;

        #[cfg(feature = "database-async")]
        let async_db = self.init_async_database()?;

        #[cfg(feature = "redis")]
        let (redis, redis_pool) = self.init_redis()?;

        #[cfg(feature = "rabbitmq")]
        let (rabbitmq, rabbitmq_pool) = self.init_rabbitmq().await?;

        #[cfg(all(feature = "cache", feature = "redis"))]
        let cache = self.init_cache(redis.as_ref().map(|r| Arc::new(r.clone())))?;
        #[cfg(all(feature = "cache", not(feature = "redis")))]
        let cache = self.init_cache()?;

        #[cfg(feature = "templating")]
        let tera = self.init_templating()?;

        #[cfg(feature = "jwt")]
        let jwt = self.init_jwt()?;

        #[cfg(feature = "jwe")]
        let jwe = self.init_jwe()?;

        #[cfg(feature = "crypto")]
        let password = self.init_password()?;

        // Auto-register helper structs and infrastructure in DI container
        let mut services = self.services;
        services.insert(crate::helpers::StringHelper);
        services.insert(crate::helpers::InputSanitizer);
        #[cfg(feature = "base64")]
        services.insert(crate::helpers::Base64);
        #[cfg(feature = "hmac")]
        services.insert(crate::helpers::hmac::Hmac::new(
            (*self.app_key).as_ref(),
            crate::helpers::hmac::HashFunc::default(),
        ));

        // Register Tokio struct with separate limits for each resource pool
        let config = self.runtime_config.as_ref();
        let max_blocking = config
            .and_then(|c| c.max_concurrent_blocking_tasks)
            .unwrap_or(512);
        let max_async_bridges = config
            .and_then(|c| c.max_concurrent_async_bridges)
            .unwrap_or(128);
        let tokio = Tokio::new(max_blocking, max_async_bridges);
        services.insert(tokio.clone());

        // Register infrastructure in DI container so services can resolve
        // them via app.get::<T>() / app.require::<T>().
        #[cfg(feature = "database")]
        if let Some(db) = &db {
            services.insert(db.clone());
        }
        #[cfg(feature = "database-async")]
        if let Some(ref pool) = async_db {
            services.insert(pool.clone());
        }
        #[cfg(feature = "redis")]
        if let Some(ref r) = redis {
            services.insert(r.clone());
        }
        #[cfg(feature = "redis")]
        if let Some(ref pool) = redis_pool {
            services.insert(pool.clone());
        }
        #[cfg(feature = "rabbitmq")]
        if let Some(ref rmq) = rabbitmq {
            services.insert(rmq.clone());
        }
        #[cfg(feature = "rabbitmq")]
        if let Some(ref pool) = rabbitmq_pool {
            services.insert(pool.clone());
        }
        #[cfg(feature = "cache")]
        if let Some(ref c) = cache {
            services.insert(c.clone());
        }
        // Note: Tera is stored both as a dedicated App field (for safe &Tera access)
        // and in the DI container (for app.get::<Tera>() resolution).
        // We dereference the Arc so the TypeMap key is TypeId::of::<Tera>(),
        // not TypeId::of::<Arc<Tera>>() - services resolve via app.require::<Tera>()
        // which returns Arc<Tera>.
        #[cfg(feature = "templating")]
        if let Some(ref t) = tera {
            services.insert((**t).clone());
        }
        #[cfg(feature = "jwt")]
        if let Some(ref j) = jwt {
            services.insert(j.clone());
        }
        #[cfg(feature = "jwe")]
        if let Some(ref j) = jwe {
            services.insert(j.clone());
        }
        #[cfg(feature = "crypto")]
        if let Some(ref p) = password {
            services.insert(p.clone());
        }

        debug!("All components initialized, constructing App");

        let app = App {
            env: self.env,
            app_name: self.app_name,
            app_code: self.app_code,
            app_version: self.app_version,
            app_key: self.app_key,
            app_env_prefix: self.app_env_prefix,
            app_public_key: self.app_public_key,
            app_private_key: self.app_private_key,
            started_at: Instant::now(),
            shutdown_initiated: AtomicBool::new(false),
            services,
            service_factories: self.service_factories,

            #[cfg(feature = "database")]
            db,
            #[cfg(feature = "database-async")]
            async_db,
            #[cfg(feature = "redis")]
            redis,
            #[cfg(feature = "redis")]
            redis_pool,
            #[cfg(feature = "rabbitmq")]
            rabbitmq,
            #[cfg(feature = "rabbitmq")]
            rabbitmq_pool,
            #[cfg(feature = "cache")]
            cache,
            #[cfg(feature = "templating")]
            tera,
            #[cfg(feature = "jwt")]
            jwt,
            #[cfg(feature = "jwe")]
            jwe,
            #[cfg(feature = "crypto")]
            password,

            startup_hooks: self.startup_hooks,
            shutdown_hooks: self.shutdown_hooks,
            health_checks: self.health_checks,
            health_check_timeout: self.health_check_timeout,
            shutdown_timeout: self.shutdown_timeout,
            metrics: self.metrics,
            event_bus: EventBus::new(),
            tokio,
        };

        debug!("App built successfully: {}", app.app_name());

        Ok((AppInit::new(app), self.after_build_hooks))
    }

    /// Validate all feature-gated configurations
    fn validate_configs(&self) -> AppResult<()> {
        #[cfg(feature = "database")]
        if let Some(ref config) = self.db_config {
            config.validate()?;
        }
        #[cfg(feature = "database-async")]
        if let Some(ref config) = self.async_db_config {
            config.validate()?;
        }
        #[cfg(feature = "redis")]
        if let Some(ref config) = self.redis_config {
            config.validate()?;
        }
        #[cfg(feature = "rabbitmq")]
        if let Some(ref config) = self.rmq_config {
            config.validate()?;
        }
        #[cfg(feature = "cache-redis")]
        if let Some(CacheDriverSetup::Redis(_)) = &self.cache_driver_setup
            && self.redis_config.is_none()
        {
            return Err(AppMessage::Infrastructure {
                    message: "cache-redis driver requires redis to be configured. Call .redis(config) before .cache()".to_string(),
                    source: None,
                });
        }
        Ok(())
    }

    /// Initialize database connection pool
    #[cfg(feature = "database")]
    fn init_database(&mut self) -> AppResult<Option<crate::database::DBPool>> {
        if let Some(config) = self.db_config.take() {
            debug!("Initializing database pool");
            Ok(Some(create_db_pool(config)?))
        } else {
            Ok(None)
        }
    }

    /// Initialize async database connection pool
    #[cfg(feature = "database-async")]
    fn init_async_database(&mut self) -> AppResult<Option<AsyncDBPool>> {
        if let Some(config) = self.async_db_config.take() {
            debug!("Initializing async database pool");
            Ok(Some(create_async_db_pool(config)?))
        } else {
            Ok(None)
        }
    }

    /// Initialize Redis connection pool
    #[cfg(feature = "redis")]
    fn init_redis(&mut self) -> AppResult<(Option<Redis>, Option<deadpool_redis::Pool>)> {
        if let Some(config) = self.redis_config.take() {
            debug!("Initializing Redis connection pool");
            let pool = create_redis_conn_pool(config)?;
            let redis = Redis::new(pool.clone());
            Ok((Some(redis), Some(pool)))
        } else {
            Ok((None, None))
        }
    }

    /// Initialize RabbitMQ connection pool
    #[cfg(feature = "rabbitmq")]
    async fn init_rabbitmq(
        &mut self,
    ) -> AppResult<(Option<RabbitMQ>, Option<deadpool_lapin::Pool>)> {
        if let Some(config) = self.rmq_config.take() {
            debug!("Initializing RabbitMQ connection pool");
            let pool = create_rmq_conn_pool(config.clone()).await?;
            let mut rmq = RabbitMQ::new(pool.clone()).await?;
            if let Some(setup_fn) = config.setup_fn {
                rmq.setup_fn_raw(setup_fn);
                rmq.setup().await.map_err(|e| AppMessage::Infrastructure {
                    message: format!("RabbitMQ setup function failed: {e}"),
                    source: None,
                })?;
            }
            Ok((Some(rmq), Some(pool)))
        } else {
            Ok((None, None))
        }
    }

    /// Initialize cache driver
    #[cfg(feature = "cache")]
    fn init_cache(
        &self,
        #[cfg(feature = "redis")] redis: Option<Arc<Redis>>,
    ) -> AppResult<Option<Cache>> {
        if let Some(setup) = &self.cache_driver_setup {
            debug!("Setting up cache driver");
            let driver = match setup {
                #[cfg(feature = "cache-redis")]
                CacheDriverSetup::Redis(setup_fn) => {
                    let redis_ref = redis
                        .ok_or_else(|| AppMessage::Infrastructure {
                            message: "cache-redis requires redis to be configured".to_string(),
                            source: None,
                        })?
                        .clone();
                    setup_fn(redis_ref)
                }
                #[cfg(feature = "cache-filesystem")]
                CacheDriverSetup::Filesystem(setup_fn) => setup_fn(),
                #[cfg(feature = "cache-in-memory")]
                CacheDriverSetup::InMemory(setup_fn) => setup_fn(),
            };
            Ok(Some(Cache::new(driver)))
        } else {
            Ok(None)
        }
    }

    /// Initialize Tera templating
    #[cfg(feature = "templating")]
    fn init_templating(&self) -> AppResult<Option<Arc<Tera>>> {
        if let Some(dir) = &self.template_directory {
            debug!("Initializing Tera templating from: {}", dir);
            let mut tera = Tera::default();
            tera.load_from_glob(dir)
                .map_err(|e| AppMessage::Infrastructure {
                    message: format!("Template loading error: {e}"),
                    source: Some(Box::new(e)),
                })?;
            Ok(Some(Arc::new(tera)))
        } else {
            Ok(None)
        }
    }

    /// Initialize JWT helper
    #[cfg(feature = "jwt")]
    fn init_jwt(&self) -> AppResult<Option<Jwt>> {
        Ok(self.jwt_config.clone().map(Jwt::new))
    }

    /// Initialize JWE helper
    #[cfg(feature = "jwe")]
    fn init_jwe(&self) -> AppResult<Option<Jwe>> {
        Ok(self.jwe_config.clone().map(Jwe::new))
    }

    /// Initialize Password helper
    #[cfg(feature = "crypto")]
    fn init_password(&self) -> AppResult<Option<Password>> {
        Ok(Some(Password::new((*self.app_key).clone())))
    }
}

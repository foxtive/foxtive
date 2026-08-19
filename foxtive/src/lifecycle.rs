//! Lifecycle traits for application startup and shutdown hooks.

use crate::app::deps::ServiceResolutionError;
use crate::app::di_error::{DiError, short_type_name};
use crate::prelude::AppResult;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::app::AppInit;
use crate::health::HealthCheck;
use crate::App;
use crate::app::AppBuilder;

// Re-export the derive macros
pub use foxtive_macros::Service;
pub use foxtive_macros::FromApp;

/// A future produced by a startup hook.
pub type StartupFuture = Pin<Box<dyn Future<Output = AppResult<()>> + Send + 'static>>;

/// A future produced by a shutdown hook.
pub type ShutdownFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// A future produced by a service factory (type-erased service construction).
///
/// Returns `ServiceResolutionError` to distinguish retryable dependency
/// failures (`DependencyMissing`) from terminal failures (`Terminal`).
pub(crate) type ServiceFactoryFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Box<dyn std::any::Any + Send + Sync>, ServiceResolutionError>> + Send + 'a>>;

/// Trait for services that need async initialization after construction.
///
/// Some services can't be fully initialized in a synchronous constructor -
/// they need to warm caches, verify connections, or load initial state.
/// Implement this trait to keep async initialization co-located with the service.
///
/// # Example
///
/// ```no_run
/// use foxtive::lifecycle::AsyncInit;
/// use foxtive::app::AppInit;
/// use foxtive::prelude::AppResult;
///
/// struct UserService {
///     // fields...
/// }
///
/// impl AsyncInit for UserService {
///     async fn init(init: &AppInit) -> AppResult<Self> {
///         // Async setup: warm caches, verify connections, etc.
///         // Access app via Deref: init.app_name(), init.db(), etc.
///         Ok(Self { })
///     }
/// }
/// ```
pub trait AsyncInit: Sized + Send + Sync + 'static {
    /// Perform async initialization. Receives a reference to `AppInit` with
    /// all infrastructure available. Called during `AppInit::init_service()`.
    fn init(init: &AppInit) -> impl Future<Output = AppResult<Self>> + Send;
}

/// Optional lifecycle hooks for post-construction and readiness logic.
///
/// When using `#[derive(Service)]`, a no-op impl is generated automatically.
/// To provide custom hooks, add `#[service(skip_hooks)]` and implement this
/// trait manually - the derive will delegate `after_init` / `on_ready` to it.
///
/// # Example
///
/// ```no_run
/// use foxtive::lifecycle::{Service, ServiceHooks};
/// use foxtive::prelude::AppResult;
/// use foxtive::App;
///
/// #[derive(Service)]
/// #[service(all, skip_hooks)]
/// struct LoginService {
///     #[foxtive(default)]
///     jwt_token_lifetime: i64,
/// }
///
/// impl ServiceHooks for LoginService {
///     fn after_init(&mut self, _app: &App) -> AppResult<()> {
///         // Post-construction setup
///         self.jwt_token_lifetime = 3600;
///         Ok(())
///     }
/// }
/// ```
pub trait ServiceHooks {
    /// Post-construction hook called before the service is boxed.
    fn after_init(&mut self, _app: &App) -> AppResult<()> {
        Ok(())
    }

    /// Readiness hook called after all `Lazy<T>` fields are wired.
    fn on_ready(_app: &App) -> AppResult<()> {
        Ok(())
    }
}

/// Trait for services that are constructed after the app is frozen.
///
/// Services implementing `ServiceInit` receive `&App` - a borrowed reference
/// to the app during construction. They can access other services, infrastructure,
/// and configuration, but cannot retain the app reference. This is intentional:
/// it eliminates the dual-store architecture and RwLock overhead that would be
/// needed if services could retain `Arc<App>`.
///
/// Extract the dependencies you need during `init()` and store them directly.
///
/// # Example
///
/// ```no_run
/// use foxtive::lifecycle::ServiceInit;
/// use foxtive::prelude::AppResult;
/// use foxtive::App;
/// use std::sync::Arc;
///
/// struct UserService {
///     cache: Arc<CacheService>,
/// }
///
/// # struct CacheService;
///
/// impl ServiceInit for UserService {
///     async fn init(app: &App) -> AppResult<Self> {
///         Ok(Self {
///             cache: app.require::<CacheService>()?,
///         })
///     }
/// }
/// ```
pub trait ServiceInit: Sized + Send + Sync + 'static {
    /// Construct the service with borrowed access to the app.
    ///
    /// Use `app.get::<T>()` or `app.require::<T>()` to access dependencies.
    /// Do NOT attempt to retain the app reference - extract what you need.
    ///
    /// # Side-Effect Constraint
    ///
    /// Call `app.require::<T>()` for all dependencies BEFORE performing any
    /// side-effecting operations (DB writes, HTTP calls, file I/O). Services
    /// with undeclared dependencies may be retried during construction, and
    /// side effects would be repeated on each retry.
    fn init(app: &App) -> impl Future<Output = AppResult<Self>> + Send;

    /// Declare dependencies for topological sort.
    ///
    /// Override to declare type-name dependencies used for graph-based
    /// initialization ordering. Default: no dependencies.
    ///
    /// # Semantics
    ///
    /// - Macro-generated impls (`#[derive(Service)]`) auto-declare deps.
    /// - Manual impls SHOULD override `dependencies()` for efficiency
    ///   (avoids Phase 2 retry overhead during `freeze()`).
    /// - Undeclared deps are handled via a single retry pass - slower, but correct.
    fn dependencies() -> Vec<&'static str> {
        vec![]
    }

    /// Whether this service should be wrapped in [`Mutable`](crate::container::Mutable)
    /// when registered in the DI container.
    ///
    /// When `true`, the service is stored as `Mutable<T>` and retrieved via
    /// `app.get_mutable::<T>()` / `app.require_mutable::<T>()`.
    ///
    /// Default: `false`.
    fn is_mutable() -> bool {
        false
    }

    /// Wire `Lazy<T>` fields after all services are constructed.
    ///
    /// Called during `freeze()` Phase 3, after ALL services are already
    /// constructed in Phases 1–2. This fills `Lazy<T>` fields by calling
    /// `app.require_lazy::<T>(&self.field)`.
    ///
    /// **Important:** `Lazy<T>` defers *wiring*, not *construction*.
    /// The target service is constructed eagerly during Phase 1b/2.
    /// `Lazy<T>`'s purpose is cycle-breaking, not lazy initialization.
    ///
    /// Default: no-op (services without Lazy fields need not override).
    fn wire_lazy(_app: &App) -> AppResult<()> {
        Ok(())
    }

    /// Post-construction hook called before the service is boxed.
    ///
    /// Runs after `init()` returns, while the service is still mutable.
    /// Use this to fill config values, computed fields, or any setup
    /// that needs `&App` access but isn't a DI dependency.
    ///
    /// Default: no-op.
    fn after_init(&mut self, _app: &App) -> AppResult<()> {
        Ok(())
    }

    /// Readiness hook called after all `Lazy<T>` fields are wired.
    ///
    /// Runs during `freeze()` Phase 3, after `wire_lazy()`. At this point
    /// all services are constructed and all lazy dependencies are filled.
    /// Use this for validation, cache warming, or logging.
    ///
    /// Default: no-op.
    fn on_ready(_app: &App) -> AppResult<()> {
        Ok(())
    }
}

/// Type-erased service factory for deferred construction.
pub(crate) trait ServiceFactory: Send + Sync {
    /// Returns the type name for debugging.
    fn type_name(&self) -> &'static str;
    /// Returns the list of dependency type names for graph resolution.
    fn dependencies(&self) -> &[&'static str] {
        &[]
    }
    /// Construct the service and return it as Box<dyn Any>.
    fn create<'a>(
        &'a self,
        app: &'a App,
    ) -> ServiceFactoryFuture<'a>;

    /// Fill all `Lazy<T>` fields. Default: no-op.
    fn wire_lazy(&self, _app: &App) -> AppResult<()> {
        Ok(())
    }

    /// Run `on_ready` hook. Default: no-op.
    fn on_ready(&self, _app: &App) -> AppResult<()> {
        Ok(())
    }
}

/// Wrapper that implements ServiceFactory for a concrete ServiceInit type.
pub(crate) struct ServiceFactoryImpl<T: ServiceInit> {
    _marker: std::marker::PhantomData<T>,
    dependencies: Vec<&'static str>,
    force_mutable: bool,
}

impl<T: ServiceInit> ServiceFactoryImpl<T> {
    pub(crate) fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
            dependencies: vec![],
            force_mutable: false,
        }
    }

    pub(crate) fn with_dependencies(mut self, deps: Vec<&'static str>) -> Self {
        self.dependencies = deps;
        self
    }

    pub(crate) fn with_mutable(mut self, mutable: bool) -> Self {
        self.force_mutable = mutable;
        self
    }
}

impl<T: ServiceInit> ServiceFactory for ServiceFactoryImpl<T> {
    fn type_name(&self) -> &'static str {
        std::any::type_name::<T>()
    }

    fn dependencies(&self) -> &[&'static str] {
        &self.dependencies
    }

    fn create<'a>(
        &'a self,
        app: &'a App,
    ) -> ServiceFactoryFuture<'a>
    {
        let type_name = self.type_name();
        let short_name = short_type_name(type_name).to_string();
        Box::pin(async move {
            let mut service = T::init(app).await.map_err(|e| match e {
                crate::enums::AppMessage::NotFound(msg) => ServiceResolutionError::DependencyMissing {
                    service: type_name,
                    missing_type: msg,
                },
                other => ServiceResolutionError::Terminal(
                    DiError::ServiceConstructionFailed {
                        service: short_name.clone(),
                        source: Box::new(other),
                    }.into(),
                ),
            })?;
            T::after_init(&mut service, app).map_err(|e| {
                ServiceResolutionError::Terminal(
                    DiError::ServiceConstructionFailed {
                        service: short_name.clone(),
                        source: Box::new(e),
                    }.into(),
                )
            })?;
            if self.force_mutable || T::is_mutable() {
                Ok(Box::new(crate::container::Mutable::new(service)) as Box<dyn std::any::Any + Send + Sync>)
            } else {
                Ok(Box::new(service) as Box<dyn std::any::Any + Send + Sync>)
            }
        })
    }

    fn wire_lazy(&self, app: &App) -> AppResult<()> {
        T::wire_lazy(app)
    }

    fn on_ready(&self, app: &App) -> AppResult<()> {
        T::on_ready(app)
    }
}

/// Type-erased factory wrapping a user-provided closure.
pub(crate) struct ClosureFactory {
    closure: Box<dyn Fn(&App) -> ServiceFactoryFuture<'static> + Send + Sync>,
    type_name_str: &'static str,
    deps: Vec<&'static str>,
}

impl ClosureFactory {
    pub(crate) fn new(
        closure: Box<dyn Fn(&App) -> ServiceFactoryFuture<'static> + Send + Sync>,
        type_name_str: &'static str,
        deps: Vec<&'static str>,
    ) -> Self {
        Self { closure, type_name_str, deps }
    }
}

impl ServiceFactory for ClosureFactory {
    fn type_name(&self) -> &'static str { self.type_name_str }
    fn dependencies(&self) -> &[&'static str] { &self.deps }
    fn create<'a>(&'a self, app: &'a App) -> ServiceFactoryFuture<'a> {
        // 'static future can be coerced to any lifetime 'a
        (self.closure)(app)
    }
}

/// Trait for extracting dependencies from the app container.
///
/// Types implementing `FromApp` can be automatically injected into
/// services that derive `ServiceInit`. This is the foundation for
/// the extractor pattern (like Axum/Actix).
///
/// # Example
///
/// ```no_run
/// use foxtive::lifecycle::FromApp;
/// use foxtive::prelude::AppResult;
/// use foxtive::App;
///
/// #[derive(Clone)]
/// struct MyService;
///
/// impl FromApp for MyService {
///     fn from_app(app: &App) -> AppResult<Self> {
///         // require() returns Arc<T> - deref and clone to get T
///         Ok(app.require::<MyService>()?.as_ref().clone())
///     }
/// }
/// ```
pub trait FromApp: Sized {
    /// Extract this type from the app container.
    fn from_app(app: &App) -> AppResult<Self>;
}

/// Trait for components that need startup initialization.
///
/// Implement this trait on your services to hook into the application
/// lifecycle. All registered `Startup` implementations are called
/// during `App::startup()`.
///
/// # Example
///
/// ```no_run
/// use foxtive::lifecycle::Startup;
/// use foxtive::prelude::AppResult;
/// use foxtive::App;
///
/// struct CacheWarmer;
///
/// impl Startup for CacheWarmer {
///     fn on_startup(&self, app: &App) -> impl std::future::Future<Output = AppResult<()>> + Send {
///         async move {
///             // Warm up caches, load initial data, etc.
///             tracing::info!("Warming up caches...");
///             Ok(())
///         }
///     }
/// }
///
/// # async fn register() -> AppResult<()> {
/// // Register a closure-based startup hook
/// let app = App::builder("my-app", "MYAPP")
///     .on_startup(|_app| async move {
///         tracing::info!("Warming up caches...");
///         Ok(())
///     })
///     .build()
///     .await?;
/// # Ok(())
/// # }
/// ```
///
/// # When to Use
///
/// - Warming caches after database connection is established
/// - Loading initial configuration or feature flags
/// - Starting background tasks that need app infrastructure
/// - Verifying external service connectivity
pub trait Startup: Send + Sync {
    /// Called during application startup.
    fn on_startup(&self, app: &App) -> impl Future<Output = AppResult<()>> + Send;
}

/// Trait for components that need cleanup on shutdown.
///
/// Implement this trait on your services to hook into the application
/// shutdown lifecycle. All registered `Shutdown` implementations are
/// called (in reverse registration order) during `App::shutdown()`.
///
/// # Example
///
/// ```no_run
/// use foxtive::lifecycle::Shutdown;
/// use foxtive::App;
///
/// struct DatabaseCleaner;
///
/// impl Shutdown for DatabaseCleaner {
///     fn on_shutdown(&self, app: &App) -> impl std::future::Future<Output = ()> + Send {
///         async move {
///             // Flush pending writes, close connections, etc.
///             tracing::info!("Cleaning up database connections...");
///         }
///     }
/// }
///
/// # async fn register() {
/// // Register a closure-based shutdown hook
/// let app = App::builder("my-app", "MYAPP")
///     .on_shutdown(|_app| async move {
///         tracing::info!("Cleaning up database connections...");
///     })
///     .build()
///     .await
///     .unwrap();
///
/// // Shutdown runs all registered hooks in reverse order
/// app.shutdown().await;
/// # }
/// ```
///
/// # When to Use
///
/// - Flushing pending writes to database or message queues
/// - Closing network connections gracefully
/// - Releasing file locks or temporary resources
/// - Notifying external services of shutdown
///
/// # Execution Order
///
/// Shutdown hooks run in **reverse registration order** (LIFO). If you
/// register hooks A, B, C, they execute as C, B, A. This ensures that
/// dependencies are shut down in the correct order.
pub trait Shutdown: Send + Sync {
    /// Called during application shutdown.
    fn on_shutdown(&self, app: &App) -> impl Future<Output = ()> + Send;
}

/// Type-erased startup hook (closure form).
pub(crate) type StartupHook =
    Box<dyn Fn(Arc<App>) -> StartupFuture + Send + Sync + 'static>;

/// Type-erased shutdown hook (closure form).
pub(crate) type ShutdownHook =
    Box<dyn Fn(Arc<App>) -> ShutdownFuture + Send + Sync + 'static>;

/// A self-contained module that bundles services, lifecycle hooks, and health checks.
///
/// Plugins let you package a reusable feature (auth, metrics, etc.) into a single
/// type that registers everything it needs in one step. Companion crates like
/// `foxtive-axum` or `foxtive-worker` implement this to attach themselves to the app.
///
/// # Example
///
/// ```no_run
/// use foxtive::lifecycle::Plugin;
/// use foxtive::app::AppBuilder;
/// use foxtive::prelude::AppResult;
/// use foxtive::App;
///
/// struct AuthPlugin {
///     token_ttl_secs: u64,
/// }
///
/// impl Plugin for AuthPlugin {
///     fn name(&self) -> &str {
///         "auth"
///     }
///
///     fn register(&self, builder: AppBuilder) -> AppBuilder {
///         builder
///             .on_shutdown(|app| async move {
///                 // Clean up auth resources
///             })
///     }
///
///     fn on_startup(&self, app: &App) -> impl std::future::Future<Output = AppResult<()>> + Send {
///         async { Ok(()) }
///     }
/// }
/// ```
pub trait Plugin: Send + Sync + 'static {
    /// Human-readable name for logging.
    fn name(&self) -> &str;

    /// Register services, hooks, and health checks into the builder.
    fn register(&self, builder: AppBuilder) -> AppBuilder {
        let _ = builder;
        builder
    }

    /// Called after infrastructure is initialized, before the app is frozen.
    ///
    /// Use this to register services that depend on DB, Redis, RabbitMQ, etc.
    fn after_build(&self, init: &mut AppInit) -> AppResult<()> {
        let _ = init;
        Ok(())
    }

    /// Called during startup (after all plugins are registered).
    fn on_startup(&self, app: &App) -> impl Future<Output = AppResult<()>> + Send {
        let _ = app;
        async { Ok(()) }
    }

    /// Called during shutdown.
    fn on_shutdown(&self, app: &App) -> impl Future<Output = ()> + Send {
        let _ = app;
        async {}
    }

    /// Health checks contributed by this plugin.
    fn health_checks(&self) -> Vec<Box<dyn HealthCheck>> {
        vec![]
    }
}

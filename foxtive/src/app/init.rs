//! Mutable initialization phase of [`App`].
//!
//! `AppInit` is returned by [`AppBuilder::build_init()`] and provides
//! `register()` for adding services after infrastructure is initialized.
//! Call [`freeze()`](AppInit::freeze) to produce the final `Arc<App>`.

use std::collections::HashMap;
use std::collections::HashSet;
use std::future::Future;
use std::ops::Deref;
use std::sync::Arc;

use crate::app::App;
use crate::app::deps::ServiceResolutionError;
use crate::container::{Mutable, TypeMap};
use crate::events::{Event, EventHandler};
use crate::lifecycle::{AsyncInit, ClosureFactory, ServiceFactoryImpl, ServiceInit};
use crate::results::AppResult;

/// The mutable initialization phase of the application.
///
/// Returned by [`AppBuilder::build_init()`](super::AppBuilder::build_init).
/// Exposes all read-only `App` accessors via `Deref` plus:
/// - `register()` for adding service instances
/// - `register_service::<T>()` for deferred `ServiceInit` construction
/// - `init_service::<T>()` for async `AsyncInit` construction
///
/// Call `freeze()` when done to produce the final `Arc<App>`.
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
///     .build_init()
///     .await?;
///
/// init.register(UserService);
///
/// let app = init.freeze().await?;
/// assert!(app.get::<UserService>().is_some());
/// # Ok(())
/// # }
/// ```
pub struct AppInit {
    pub(crate) inner: App,
}

impl AppInit {
    /// Create an `AppInit` wrapping an already-built `App`.
    pub(crate) fn new(app: App) -> Self {
        Self { inner: app }
    }

    /// Register a service in the DI container.
    ///
    /// If a service of the same type was already registered, it is replaced
    /// and the old value is returned. A warning is logged when replacement
    /// occurs to aid debugging.
    pub fn register<T: Send + Sync + 'static>(&mut self, service: T) -> Option<T> {
        let old = self.inner.services.insert(service);
        if old.is_some() {
            tracing::warn!(
                service_type = std::any::type_name::<T>(),
                "Service registration replaced an existing entry"
            );
        }
        old
    }

    /// Register a mutable service in the DI container.
    ///
    /// Wraps the value in [`Mutable<T>`] for shared interior mutability.
    /// Retrieve with `app.get_mutable::<T>()` or `app.require_mutable::<T>()`.
    pub fn register_mutable<T: Send + Sync + 'static>(&mut self, value: T) {
        self.inner.services.insert(Mutable::new(value));
    }

    /// Initialize and register a service that implements [`AsyncInit`].
    ///
    /// Calls `T::init(&init)` to perform async setup, then registers the
    /// resulting instance in the DI container. Use this for services that
    /// need async initialization (cache warming, connection verification, etc.).
    ///
    /// # Example
    ///
    /// ```no_run
    /// use foxtive::App;
    /// use foxtive::app::AppInit;
    /// use foxtive::lifecycle::AsyncInit;
    /// use foxtive::prelude::AppResult;
    ///
    /// struct UserService;
    ///
    /// impl AsyncInit for UserService {
    ///     async fn init(init: &AppInit) -> AppResult<Self> {
    ///         // async setup...
    ///         Ok(Self)
    ///     }
    /// }
    ///
    /// # async fn run() -> AppResult<()> {
    /// let mut init = App::builder("my-app", "MYAPP")
    ///     .build_init()
    ///     .await?;
    ///
    /// init.init_service::<UserService>().await?;
    ///
    /// let app = init.freeze().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn init_service<T: AsyncInit>(&mut self) -> AppResult<()> {
        let service = T::init(self).await?;
        self.inner.services.insert(service);
        Ok(())
    }

    /// Register a deferred service factory for construction during `freeze()`.
    ///
    /// The service `T` must implement `ServiceInit`. It will be constructed
    /// during `freeze()` in topological order alongside services registered
    /// on the builder. Use this when the service needs access to infrastructure
    /// that is only available after `build_init()`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use foxtive::App;
    /// use foxtive::lifecycle::Service;
    /// use foxtive::prelude::AppResult;
    /// use std::sync::Arc;
    ///
    /// #[derive(Service)]
    /// struct WorkshopService {
    ///     #[dependency]
    ///     repository: Arc<WorkshopRepository>,
    /// }
    ///
    /// # struct WorkshopRepository;
    ///
    /// # async fn run() -> AppResult<()> {
    /// let mut init = App::builder("my-app", "MYAPP")
    ///     .build_init()
    ///     .await?;
    ///
    /// init.register(WorkshopRepository);
    /// init.register_service::<WorkshopService>();
    ///
    /// let app = init.freeze().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn register_service<T: ServiceInit>(&mut self) {
        let factory = ServiceFactoryImpl::<T>::new().with_dependencies(T::dependencies());
        self.inner.service_factories.push(Box::new(factory));
    }

    /// Register a deferred service factory for construction during `freeze()`,
    /// stored as [`Mutable<T>`](crate::container::Mutable).
    ///
    /// The service `T` must implement `ServiceInit`. It will be constructed
    /// during `freeze()` and wrapped in `Mutable<T>` for shared interior
    /// mutability. Retrieve with `app.get_mutable::<T>()` or
    /// `app.require_mutable::<T>()`.
    pub fn register_mutable_service<T: ServiceInit>(&mut self) {
        let factory = ServiceFactoryImpl::<T>::new()
            .with_dependencies(T::dependencies())
            .with_mutable(true);
        self.inner.service_factories.push(Box::new(factory));
    }

    /// Register a trait binding (eager, post-build_init).
    pub fn register_trait<Trait>(&mut self, impl_: Arc<Trait>) -> &mut Self
    where
        Trait: ?Sized + Send + Sync + 'static,
    {
        self.inner.services.insert_trait::<Trait>(impl_);
        self
    }

    /// Register a service via a factory closure (post-build_init).
    ///
    /// The closure receives `&App` and returns an `AppResult<T>`.
    pub fn register_with<T, F, Fut>(&mut self, f: F) -> &mut Self
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
                        }
                        .into(),
                    )
                })?;
                Ok(Box::new(service) as Box<dyn std::any::Any + Send + Sync>)
            })
        };
        self.inner
            .service_factories
            .push(Box::new(ClosureFactory::new(
                Box::new(wrapper),
                type_name_str,
                vec![],
            )));
        self
    }

    /// Register a service for deferred construction only if `condition` is true.
    pub fn register_service_if<T: ServiceInit>(&mut self, condition: bool) -> &mut Self {
        if condition {
            self.register_service::<T>();
        }
        self
    }

    /// Register a service for deferred construction.
    /// Silently no-ops if the same type is already registered.
    pub fn try_register_service<T: ServiceInit>(&mut self) -> &mut Self {
        let type_name = std::any::type_name::<T>();
        // Check if already registered in service_factories
        if self
            .inner
            .service_factories
            .iter()
            .any(|f| f.type_name() == type_name)
        {
            return self;
        }
        let factory = ServiceFactoryImpl::<T>::new().with_dependencies(T::dependencies());
        self.inner.service_factories.push(Box::new(factory));
        self
    }

    /// Replace a previously registered service.
    pub fn replace_service<T: ServiceInit>(&mut self) -> &mut Self {
        let type_name = std::any::type_name::<T>();
        if let Some(pos) = self
            .inner
            .service_factories
            .iter()
            .position(|f| f.type_name() == type_name)
        {
            tracing::info!(
                service = type_name,
                "Replacing previously registered service"
            );
            self.inner.service_factories.remove(pos);
        }
        let factory = ServiceFactoryImpl::<T>::new().with_dependencies(T::dependencies());
        self.inner.service_factories.push(Box::new(factory));
        self
    }

    /// Register a typed [`EventHandler`] for event type `T`.
    pub fn on<T: Event, H: EventHandler<T>>(&mut self, handler: H) {
        self.inner.event_bus.on::<T, H>(handler);
    }

    /// Register a closure as an event handler for event type `T`.
    pub fn on_event<T, F, Fut>(&mut self, handler: F)
    where
        T: Event,
        F: Fn(Arc<T>, Arc<App>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = AppResult<()>> + Send + 'static,
    {
        self.inner.event_bus.on_event::<T, F, Fut>(handler);
    }

    /// Consume this `AppInit`, construct all registered services, and return
    /// a frozen `Arc<App>`.
    ///
    /// Runs service factories (registered via `register_service::<T>()`) in
    /// three phases:
    ///
    /// 1. **Phase 1a** - DFS-based order resolution from declared deps (sync graph walk).
    /// 2. **Phase 1b** - Sequential construction in resolved order. Factories that
    ///    fail with `DependencyMissing` (undeclared runtime dep) are deferred.
    /// 3. **Phase 2** - Single retry pass for deferred factories. Each iteration
    ///    must make progress or the loop terminates with a deadlock error.
    /// 4. **Phase 3** - Wire all `Lazy<T>` fields (sequential, deterministic).
    ///
    /// After freezing, no more services can be registered.
    pub async fn freeze(mut self) -> AppResult<Arc<App>> {
        let factories = std::mem::take(&mut self.inner.service_factories);

        if !factories.is_empty() {
            // Phase 1a: DFS-based order resolution
            let order = crate::app::deps::resolve_construction_order(&factories)?;

            // Phase 1b: Construct in resolved order
            let mut constructed_set: HashSet<usize> = HashSet::new();
            let mut phase2_candidates: Vec<usize> = Vec::new();
            let mut phase2_errors: HashMap<usize, ServiceResolutionError> = HashMap::new();

            for &idx in &order {
                match factories[idx].create(&self.inner).await {
                    Ok(service) => {
                        self.inner.services.insert_boxed(service);
                        constructed_set.insert(idx);
                    }
                    Err(e @ ServiceResolutionError::DependencyMissing { .. }) => {
                        // Undeclared dep missing - defer to Phase 2
                        phase2_candidates.push(idx);
                        phase2_errors.insert(idx, e);
                    }
                    Err(ServiceResolutionError::Terminal(e)) => {
                        // Non-retryable error - fail immediately
                        return Err(e);
                    }
                }
            }

            // Phase 2: Retry pass for undeclared deps
            // Each successful iteration constructs ≥1 service (or terminates).
            // At most N iterations for N initial candidates.
            let max_iterations = phase2_candidates.len();
            let mut iteration = 0;

            while !phase2_candidates.is_empty() && iteration < max_iterations {
                iteration += 1;
                let mut progress = false;
                let mut next_candidates = Vec::new();

                for &idx in &phase2_candidates {
                    match factories[idx].create(&self.inner).await {
                        Ok(service) => {
                            self.inner.services.insert_boxed(service);
                            constructed_set.insert(idx);
                            progress = true;
                        }
                        Err(e @ ServiceResolutionError::DependencyMissing { .. }) => {
                            phase2_errors.insert(idx, e);
                            next_candidates.push(idx);
                        }
                        Err(ServiceResolutionError::Terminal(e)) => return Err(e),
                    }
                }

                if !progress {
                    return Err(crate::app::deps::format_deadlock_error(
                        &phase2_errors,
                        &factories,
                        &constructed_set,
                    )
                    .into());
                }

                phase2_candidates = next_candidates;
            }

            if !phase2_candidates.is_empty() {
                return Err(crate::app::deps::format_deadlock_error(
                    &phase2_errors,
                    &factories,
                    &constructed_set,
                )
                .into());
            }
        }

        // Wrap in Arc
        let app = Arc::new(self.inner);

        // Phase 3: wire all Lazy<T> fields, then run on_ready hooks.
        // CRITICAL: This loop MUST NOT be parallelized (invariant D8).
        // OnceLock::set() is thread-safe, but parallelizing would violate
        // the deterministic startup guarantee. Keep this sequential.
        for factory in &factories {
            let start = std::time::Instant::now();
            factory.wire_lazy(&app)?;
            factory.on_ready(&app)?;
            let elapsed = start.elapsed();
            if elapsed > std::time::Duration::from_millis(1) {
                tracing::debug!(
                    service = factory.type_name(),
                    elapsed_ms = ?elapsed,
                    "Slow lazy wiring detected"
                );
            }
        }

        Ok(app)
    }

    /// Returns a mutable reference to the inner service container.
    ///
    /// Useful for advanced scenarios where direct `TypeMap` access is needed.
    pub fn services_mut(&mut self) -> &mut TypeMap {
        &mut self.inner.services
    }
}

impl Deref for AppInit {
    type Target = App;

    fn deref(&self) -> &App {
        &self.inner
    }
}

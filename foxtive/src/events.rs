//! In-process event bus for decoupled, typed communication between components.
//!
//! Events are plain Rust structs that implement [`Event`]. Handlers implement
//! [`EventHandler<T>`] for a specific event type. The [`EventBus`] dispatches
//! events to all registered handlers concurrently.
//!
//! # Example
//!
//! ```
//! use foxtive::events::{Event, EventHandler, EventBus};
//! use foxtive::prelude::AppResult;
//! use foxtive::App;
//!
//! #[derive(Event, Clone, Debug)]
//! struct UserCreated {
//!     user_id: i64,
//! }
//!
//! struct AuditLogger;
//!
//! impl EventHandler<UserCreated> for AuditLogger {
//!     async fn handle(&self, event: &UserCreated, _app: &App) -> AppResult<()> {
//!         tracing::info!(user_id = event.user_id, "AUDIT: user created");
//!         Ok(())
//!     }
//! }
//! ```

/// Derive macro for the `Event` trait.
///
/// This generates an empty `impl Event for T {}`. The trait bounds
/// (`Clone + Send + Sync + 'static`) are enforced by the struct's own derives.
pub use foxtive_macros::Event;

use std::any::TypeId;
use std::collections::HashMap;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;

use crate::app::App;
use crate::enums::AppMessage;
use crate::results::AppResult;

/// Marker trait for event types.
///
/// Events must be `Clone` (each handler gets its own copy), `Send + Sync`
/// (handlers run concurrently), and `'static` (type-erased internally).
///
/// Derive this with `#[derive(Event)]` or implement manually.
pub trait Event: Clone + Send + Sync + 'static {}

/// Handler for a specific event type.
///
/// Implement this trait on your types to react to events dispatched
/// through the [`EventBus`].
pub trait EventHandler<T: Event>: Send + Sync + 'static {
    /// Handle the event. Receives a reference to the app for accessing services.
    fn handle(&self, event: &T, app: &App) -> impl Future<Output = AppResult<()>> + Send;
}

type BoxFuture<'a> = Pin<Box<dyn Future<Output = AppResult<()>> + Send + 'a>>;

/// Type-erased handler collection for a single event type.
/// Avoids per-emit downcast by dispatching directly through a trait object.
trait ErasedHandlerVec: Send + Sync {
    fn dispatch(
        &self,
        event: Arc<dyn std::any::Any + Send + Sync>,
        app: Arc<App>,
    ) -> Option<Vec<BoxFuture<'_>>>;
    fn len(&self) -> usize;
}

struct ErasedTypedHandler<T: Event, H: EventHandler<T>> {
    inner: Arc<H>,
    _marker: PhantomData<fn() -> T>,
}

impl<T: Event, H: EventHandler<T>> ErasedHandlerVec for ErasedTypedHandler<T, H> {
    fn dispatch(
        &self,
        event: Arc<dyn std::any::Any + Send + Sync>,
        app: Arc<App>,
    ) -> Option<Vec<BoxFuture<'_>>> {
        let event = event.downcast::<T>().ok()?;
        let inner = Arc::clone(&self.inner);
        Some(vec![Box::pin(
            async move { inner.handle(&event, &app).await },
        )])
    }
    fn len(&self) -> usize {
        1
    }
}

struct ErasedClosureHandler<T: Event, F, Fut>
where
    F: Fn(Arc<T>, Arc<App>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = AppResult<()>> + Send + 'static,
{
    inner: Arc<F>,
    _marker: PhantomData<fn() -> (T, Fut)>,
}

impl<T: Event, F, Fut> ErasedHandlerVec for ErasedClosureHandler<T, F, Fut>
where
    F: Fn(Arc<T>, Arc<App>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = AppResult<()>> + Send + 'static,
{
    fn dispatch(
        &self,
        event: Arc<dyn std::any::Any + Send + Sync>,
        app: Arc<App>,
    ) -> Option<Vec<BoxFuture<'_>>> {
        let event = event.downcast::<T>().ok()?;
        let f = Arc::clone(&self.inner);
        Some(vec![Box::pin(async move { f(event, app).await })])
    }
    fn len(&self) -> usize {
        1
    }
}

/// In-process event bus that dispatches events to registered handlers.
///
/// Stored inside [`App`] - access via [`App::events()`].
///
/// Handlers run concurrently. One handler failing does not prevent other
/// handlers from executing. Errors are logged; the first error is returned.
///
/// # Handler Ordering
///
/// Handlers are dispatched in registration order (the order they were added
/// via `on()` or `on_event()`). When using plugins, this depends on the order
/// in which plugins were registered with the builder. All handlers start
/// concurrently via `join_all`, but their initiation follows registration order.
pub struct EventBus {
    handlers: HashMap<TypeId, Vec<Box<dyn ErasedHandlerVec>>>,
}

impl EventBus {
    pub(crate) fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    /// Register a typed [`EventHandler`] for event type `T`.
    pub fn on<T: Event, H: EventHandler<T>>(&mut self, handler: H) {
        let erased = Box::new(ErasedTypedHandler {
            inner: Arc::new(handler),
            _marker: PhantomData,
        });
        self.handlers
            .entry(TypeId::of::<T>())
            .or_default()
            .push(erased);
    }

    /// Register a closure as an event handler for event type `T`.
    pub fn on_event<T, F, Fut>(&mut self, handler: F)
    where
        T: Event,
        F: Fn(Arc<T>, Arc<App>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = AppResult<()>> + Send + 'static,
    {
        let erased = Box::new(ErasedClosureHandler {
            inner: Arc::new(handler),
            _marker: PhantomData,
        });
        self.handlers
            .entry(TypeId::of::<T>())
            .or_default()
            .push(erased);
    }

    /// Emit an event, dispatching to all registered handlers concurrently.
    ///
    /// Returns `Ok(())` if all handlers succeeded. If any handler failed,
    /// errors are logged and an appropriate error is returned:
    /// - Single failure: returns that error
    /// - Multiple failures: returns a summary error with count
    pub async fn emit<T: Event>(&self, event: T, app: &Arc<App>) -> AppResult<()> {
        let type_id = TypeId::of::<T>();
        let Some(handlers) = self.handlers.get(&type_id) else {
            return Ok(());
        };

        let event_arc: Arc<dyn std::any::Any + Send + Sync> = Arc::new(event);

        let mut futures = Vec::with_capacity(handlers.len());
        for handler in handlers {
            if let Some(futs) = handler.dispatch(Arc::clone(&event_arc), Arc::clone(app)) {
                futures.extend(futs);
            }
        }

        let results = futures_util::future::join_all(futures).await;

        let errors: Vec<_> = results.into_iter().filter_map(|r| r.err()).collect();
        match errors.len() {
            0 => Ok(()),
            1 => {
                let e = errors.into_iter().next().unwrap();
                tracing::error!(
                    event = std::any::type_name::<T>(),
                    error = %e,
                    "Event handler failed"
                );
                Err(e)
            }
            n => {
                for e in &errors {
                    tracing::error!(
                        event = std::any::type_name::<T>(),
                        error = %e,
                        "Event handler failed"
                    );
                }
                Err(AppMessage::Infrastructure {
                    message: format!(
                        "{n} event handlers failed for {}",
                        std::any::type_name::<T>()
                    ),
                    source: None,
                })
            }
        }
    }

    /// Returns the number of registered handlers for event type `T`.
    pub fn handler_count<T: Event>(&self) -> usize {
        self.handlers
            .get(&TypeId::of::<T>())
            .map(|v| v.iter().map(|h| h.len()).sum())
            .unwrap_or(0)
    }

    /// Returns the total number of registered handlers across all event types.
    pub fn total_handler_count(&self) -> usize {
        self.handlers
            .values()
            .map(|v| v.iter().map(|h| h.len()).sum::<usize>())
            .sum()
    }
}

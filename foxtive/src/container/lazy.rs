//! Lazy dependency wrapper for circular dependency resolution.
//!
//! `Lazy<T>` defers dependency resolution until after all services are
//! constructed (post-`freeze()`). The topological sort skips `Lazy` edges,
//! allowing cycles through lazy boundaries.
//!
//! # Example
//!
//! ```ignore
//! use foxtive::container::Lazy;
//! use foxtive::lifecycle::Service;
//!
//! #[derive(Service, Default)]
//! struct ServiceA {
//!     #[dependency]
//!     b: Lazy<ServiceB>,  // deferred - breaks cycle
//! }
//! ```
//!
//! # Clone Semantics
//!
//! Each clone gets its own independent `OnceLock` cell. `Lazy<T>` does NOT
//! share state across clones. For shared mutable lazy dependencies, compose:
//! `Lazy<Mutable<T>>`.
//!
//! # Limitations
//!
//! - Only supported on factory-registered services (`register_service::<T>()`).
//! - `Lazy<InfraType>` is rejected by the derive macro.
//! - `Lazy<Arc<T>>` is rejected - `Lazy<T>` already stores `Arc<T>` internally.
//! - Must use the canonical name `Lazy` (aliases break macro detection).
//!
//! # What `Lazy<T>` Does NOT Do
//!
//! `Lazy<T>` defers **wiring** (the `require_lazy` call that fills the
//! `OnceLock`), not **construction**. The service `T` is constructed
//! eagerly during `freeze()` Phase 1b/2, just like any other service.
//! The `Lazy<T>` field is filled afterward in Phase 3.
//!
//! If you need to defer construction itself (e.g., to avoid startup cost
//! for a service that may not be used), use conditional registration
//! (`register_service_if`) instead; `Lazy<T>` will not help here.

use std::fmt;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::OnceLock;

use crate::enums::AppMessage;
use crate::results::AppResult;

/// Creates an unfilled `Lazy<T>` with compile-time source location metadata.
///
/// Use this in manual `ServiceInit::init()` instead of `Lazy::default()`.
/// This macro captures `file!()` and `line!()` at the **call site**, so
/// premature-access panics point to the exact source location.
///
/// # Example
///
/// ```ignore
/// use foxtive::prelude::*;
///
/// struct InventoryService { warehouse: Lazy<()> }
///
/// impl ServiceInit for InventoryService {
///     async fn init(_app: &App) -> AppResult<Self> {
///         Ok(Self {
///             warehouse: lazy!(),  // panic → "src/services/inventory.rs:42"
///         })
///     }
/// }
/// ```
#[macro_export]
macro_rules! lazy {
    () => {
        $crate::container::Lazy::new(
            concat!(file!(), ":", line!()),
            "<manual>",
        )
    };
}

/// A lazy dependency wrapper that defers resolution until after `freeze()`.
///
/// Stores `Arc<T>` internally (via `OnceLock<Arc<T>>`) to match `TypeMap`
/// storage and `app.require::<T>()`'s return type. Access via `Deref`
/// returns `&T` directly.
///
/// # Access Patterns
///
/// - `Deref` (`&lazy`) - convenient, but performs atomic read on every access.
/// - `resolve()` - returns `Arc<T>` directly. Cache in hot paths to avoid
///   repeated atomic reads.
/// - `try_get()` - non-panicking access. Returns `None` if unfilled.
///
/// # Performance
///
/// In tight loops, cache the `Arc<T>` via `resolve()`:
///
/// ```ignore
/// // BAD: atomic read on every iteration
/// for item in items {
///     self.lazy_dep.process(item);
/// }
///
/// // GOOD: cache the Arc, single atomic read
/// let dep = self.lazy_dep.resolve();
/// for item in items {
///     dep.process(item);
/// }
/// ```
pub struct Lazy<T> {
    inner: OnceLock<Arc<T>>,
    owner_type: &'static str,
    field_name: &'static str,
}

impl<T> Lazy<T> {
    /// Create a new unfilled `Lazy<T>` with metadata for panic messages.
    pub fn new(owner_type: &'static str, field_name: &'static str) -> Self {
        Self {
            inner: OnceLock::new(),
            owner_type,
            field_name,
        }
    }

    /// Fill the lazy with a value. Returns an error if already filled.
    ///
    /// Called by the framework during `freeze()` phase 2. The value is
    /// typically obtained via `app.require::<T>()`.
    pub fn fill(&self, value: Arc<T>) -> AppResult<()> {
        self.inner.set(value).map_err(|_| AppMessage::Infrastructure {
            message: format!(
                "Lazy<{}> on {}.{} already filled",
                std::any::type_name::<T>(),
                self.owner_type,
                self.field_name,
            ),
            source: None,
        })
    }

    /// Returns `&T` via double-deref through `Arc<T>`.
    ///
    /// Panics with owner/field metadata if unfilled.
    pub fn get(&self) -> &T {
        self.inner.get().unwrap_or_else(|| {
            panic!(
                "Lazy<{}> on {}.{} not filled - was freeze() completed?",
                std::any::type_name::<T>(),
                self.owner_type,
                self.field_name,
            )
        }).as_ref()
    }

    /// Non-panicking access. Returns `None` if unfilled.
    ///
    /// Useful for health checks and graceful degradation paths.
    pub fn try_get(&self) -> Option<&T> {
        self.inner.get().map(|arc| arc.as_ref())
    }

    /// Returns the inner `Arc<T>` directly.
    ///
    /// Use this in hot paths to cache the `Arc` and avoid repeated atomic
    /// reads from `OnceLock::get()`. Panics if unfilled.
    pub fn resolve(&self) -> Arc<T> {
        self.inner.get().unwrap_or_else(|| {
            panic!(
                "Lazy<{}> on {}.{} not filled - was freeze() completed?",
                std::any::type_name::<T>(),
                self.owner_type,
                self.field_name,
            )
        }).clone()
    }

    /// Returns `true` if the lazy has been filled.
    pub fn is_filled(&self) -> bool {
        self.inner.get().is_some()
    }

    /// Returns the owner type metadata for panic messages.
    pub fn owner_type(&self) -> &str {
        self.owner_type
    }

    /// Returns the field name metadata for panic messages.
    pub fn field_name(&self) -> &str {
        self.field_name
    }
}

impl<T> Deref for Lazy<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.get()
    }
}

impl<T> Default for Lazy<T> {
    /// Creates an unfilled `Lazy<T>` with empty metadata.
    ///
    /// The derive macro overwrites this with `Lazy::new(owner, field)`
    /// during `init()`. Direct use of `Default` is not recommended -
    /// use `Lazy::new()` instead for meaningful panic messages.
    fn default() -> Self {
        Self {
            inner: OnceLock::new(),
            owner_type: "<unknown>",
            field_name: "<unknown>",
        }
    }
}

impl<T> Clone for Lazy<T> {
    fn clone(&self) -> Self {
        let new_lock = OnceLock::new();
        if let Some(val) = self.inner.get() {
            let _ = new_lock.set(Arc::clone(val));
        }
        Self {
            inner: new_lock,
            owner_type: self.owner_type,
            field_name: self.field_name,
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for Lazy<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("Lazy");
        s.field("owner", &self.owner_type);
        s.field("field", &self.field_name);
        if let Some(val) = self.inner.get() {
            s.field("value", &**val);
        } else {
            s.field("value", &"<unfilled>");
        }
        s.finish()
    }
}

impl<T> AsRef<T> for Lazy<T> {
    fn as_ref(&self) -> &T {
        self.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_unfilled_lazy() {
        let lazy = Lazy::<u32>::new("Owner", "field");
        assert!(!lazy.is_filled());
    }

    #[test]
    fn fill_then_deref_returns_value() {
        let lazy = Lazy::<u32>::new("Owner", "field");
        lazy.fill(Arc::new(42)).unwrap();
        assert_eq!(*lazy, 42);
    }

    #[test]
    fn double_fill_returnss_error() {
        let lazy = Lazy::<u32>::new("Owner", "field");
        lazy.fill(Arc::new(42)).unwrap();
        let result = lazy.fill(Arc::new(99));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("already filled"));
    }

    #[test]
    fn clone_produces_independent_state_unfilled() {
        let lazy = Lazy::<u32>::new("Owner", "field");
        let cloned = lazy.clone();
        lazy.fill(Arc::new(42)).unwrap();
        assert!(lazy.is_filled());
        assert!(!cloned.is_filled());
    }

    #[test]
    fn clone_after_fill_copies_value() {
        let lazy = Lazy::<u32>::new("Owner", "field");
        lazy.fill(Arc::new(42)).unwrap();
        let cloned = lazy.clone();
        assert!(cloned.is_filled());
        assert_eq!(*cloned, 42);
    }

    #[test]
    fn is_filled_reflects_state() {
        let lazy = Lazy::<String>::new("Owner", "field");
        assert!(!lazy.is_filled());
        lazy.fill(Arc::new("hello".to_string())).unwrap();
        assert!(lazy.is_filled());
    }

    #[test]
    fn send_sync_bound_compiles() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Lazy<u32>>();
        assert_send_sync::<Lazy<String>>();
    }

    #[test]
    fn try_get_returns_none_when_unfilled() {
        let lazy = Lazy::<u32>::new("Owner", "field");
        assert!(lazy.try_get().is_none());
    }

    #[test]
    fn try_get_returns_some_when_filled() {
        let lazy = Lazy::<u32>::new("Owner", "field");
        lazy.fill(Arc::new(42)).unwrap();
        assert_eq!(lazy.try_get(), Some(&42));
    }

    #[test]
    fn resolve_returns_arc_when_filled() {
        let lazy = Lazy::<u32>::new("Owner", "field");
        lazy.fill(Arc::new(42)).unwrap();
        let arc: Arc<u32> = lazy.resolve();
        assert_eq!(*arc, 42);
    }

    #[test]
    #[should_panic(expected = "not filled")]
    fn resolve_panics_when_unfilled() {
        let lazy = Lazy::<u32>::new("Owner", "field");
        lazy.resolve();
    }

    #[test]
    fn as_ref_returns_ref_when_filled() {
        let lazy = Lazy::<u32>::new("Owner", "field");
        lazy.fill(Arc::new(42)).unwrap();
        let r: &u32 = lazy.as_ref();
        assert_eq!(*r, 42);
    }

    #[test]
    fn debug_shows_unfilled_when_unfilled() {
        let lazy = Lazy::<u32>::new("Owner", "field");
        let debug = format!("{lazy:?}");
        assert!(debug.contains("<unfilled>"));
        assert!(debug.contains("Owner"));
        assert!(debug.contains("field"));
    }

    #[test]
    fn debug_shows_value_when_filled() {
        let lazy = Lazy::<u32>::new("Owner", "field");
        lazy.fill(Arc::new(42)).unwrap();
        let debug = format!("{lazy:?}");
        assert!(debug.contains("42"));
        assert!(!debug.contains("<unfilled>"));
    }

    #[test]
    #[should_panic(expected = "not filled")]
    fn get_panics_when_unfilled() {
        let lazy = Lazy::<u32>::new("Owner", "field");
        lazy.get();
    }

    #[test]
    fn panic_message_includes_metadata() {
        let lazy = Lazy::<u32>::new("ServiceA", "b");
        lazy.fill(Arc::new(1)).unwrap();
        // Verify metadata is stored correctly
        assert_eq!(lazy.owner_type, "ServiceA");
        assert_eq!(lazy.field_name, "b");
    }
}

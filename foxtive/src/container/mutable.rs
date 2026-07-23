//! Shared mutable wrapper for services in the DI container.
//!
//! Services stored as `Arc<T>` only expose `&self`. [`Mutable<T>`] wraps a
//! value in a `parking_lot::RwLock`, enabling `&mut self` access through the
//! container without manual `Arc<RwLock<T>>` boilerplate.
//!
//! # Example
//!
//! ```
//! use foxtive::container::Mutable;
//!
//! struct Counter { count: u64 }
//!
//! let counter = Mutable::new(Counter { count: 0 });
//! counter.write().count += 1;
//! assert_eq!(counter.read().count, 1);
//! ```

use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::fmt;

/// A shared mutable wrapper for use in the DI container.
///
/// Wraps a value in a `parking_lot::RwLock`, allowing multiple concurrent
/// readers or a single writer. Uses `parking_lot` for better performance
/// (no syscalls on Linux, no lock poisoning).
///
/// Register via `init.register(Mutable::new(svc))` or the convenience
/// method `init.register_mutable(svc)`.
///
/// Retrieve with `app.get_mutable::<T>()` or `app.require_mutable::<T>()`.
pub struct Mutable<T> {
    inner: RwLock<T>,
}

impl<T> Mutable<T> {
    /// Create a new `Mutable<T>` wrapping the given value.
    pub fn new(value: T) -> Self {
        Self {
            inner: RwLock::new(value),
        }
    }

    /// Acquire a read lock. Multiple readers can hold the lock simultaneously.
    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        self.inner.read()
    }

    /// Acquire a write lock. Only one writer is allowed at a time.
    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        self.inner.write()
    }

    /// Consume the wrapper and return the inner value.
    pub fn into_inner(self) -> T {
        self.inner.into_inner()
    }
}

impl<T: fmt::Debug> fmt::Debug for Mutable<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mutable")
            .field("inner", &*self.inner.read())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn new_and_read_returns_correct_value() {
        let m = Mutable::new(42u32);
        assert_eq!(*m.read(), 42);
    }

    #[test]
    fn write_mutates_subsequent_read_sees_change() {
        let m = Mutable::new(0u32);
        *m.write() = 99;
        assert_eq!(*m.read(), 99);
    }

    #[test]
    fn concurrent_reads_succeed() {
        let m = Arc::new(Mutable::new(vec![1, 2, 3]));
        let r1 = m.read();
        let r2 = m.read();
        assert_eq!(*r1, vec![1, 2, 3]);
        assert_eq!(*r2, vec![1, 2, 3]);
    }

    #[test]
    fn into_inner_recovers_value() {
        let m = Mutable::new("hello".to_string());
        let v = m.into_inner();
        assert_eq!(v, "hello");
    }

    #[test]
    fn mutable_is_send_sync_when_t_is() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Mutable<u32>>();
        assert_send_sync::<Mutable<String>>();
    }

    #[test]
    fn debug_impl_works() {
        let m = Mutable::new(42u32);
        let debug = format!("{m:?}");
        assert!(debug.contains("42"));
    }

    #[test]
    fn write_lock_is_exclusive() {
        let m = Arc::new(Mutable::new(0u32));
        let m2 = Arc::clone(&m);

        let handle = std::thread::spawn(move || {
            let mut guard = m2.write();
            *guard = 42;
        });
        handle.join().unwrap();

        assert_eq!(*m.read(), 42);
    }
}

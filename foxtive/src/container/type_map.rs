use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

/// A type-safe map for storing heterogeneous service instances.
///
/// Inspired by `anymap` / `typemap`. Each type can have at most one entry.
/// Used as the DI container's service registry.
///
/// Values are stored as `Arc<T>` - retrieval returns a cloned `Arc` (a cheap
/// atomic ref-count increment) rather than a borrowed reference. This
/// eliminates lifetime issues and makes the map safe to read concurrently.
///
/// # Example
///
/// ```
/// use foxtive::container::TypeMap;
///
/// let mut map = TypeMap::new();
/// map.insert(42u32);
/// map.insert("hello".to_string());
///
/// assert_eq!(*map.get::<u32>().unwrap(), 42);
/// assert_eq!(*map.get::<String>().unwrap(), "hello".to_string());
/// assert!(map.get::<bool>().is_none());
/// assert!(map.contains::<u32>());
/// ```
pub struct TypeMap {
    inner: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl TypeMap {
    /// Creates an empty `TypeMap`.
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    /// Insert a value into the map. If a value of this type already existed,
    /// it is returned.
    ///
    /// # Note
    /// If the old value is still referenced elsewhere (multiple `Arc` clones exist),
    /// the old value cannot be extracted and is silently dropped. A warning is logged
    /// in this case to aid debugging.
    pub fn insert<T: Send + Sync + 'static>(&mut self, value: T) -> Option<T> {
        let old = self.inner.insert(TypeId::of::<T>(), Arc::new(value))?;
        // Downcast to concrete type, then try to extract the value
        let result = old
            .downcast::<T>()
            .ok()
            .and_then(|arc| Arc::try_unwrap(arc).ok());

        if result.is_none() {
            tracing::warn!(
                type_name = std::any::type_name::<T>(),
                "TypeMap::insert: old value had multiple Arc references and could not be returned"
            );
        }

        result
    }

    /// Insert a boxed `Any` value into the map.
    ///
    /// This is used internally for type-erased service construction.
    /// The `TypeId` is determined from the boxed value's concrete type.
    pub(crate) fn insert_boxed(&mut self, value: Box<dyn Any + Send + Sync>) {
        let type_id = (*value).type_id();
        self.inner.insert(type_id, Arc::from(value));
    }

    /// Get an `Arc` to a value of the given type.
    ///
    /// Returns `None` if no value of this type is registered.
    /// The returned `Arc` is a cheap clone - callers can retain it.
    ///
    /// # Performance
    ///
    /// Each call performs a `HashMap` lookup by `TypeId` followed by an `Arc::clone`
    /// (an atomic reference count increment) and a downcast. For frequently-accessed
    /// services, cache the returned `Arc<T>` in your service struct rather than
    /// calling `get()` repeatedly. This avoids the lookup overhead on every access.
    ///
    /// # Example
    ///
    /// ```
    /// use foxtive::container::TypeMap;
    /// use std::sync::Arc;
    ///
    /// let mut map = TypeMap::new();
    /// map.insert(42u32);
    ///
    /// // Cache the Arc for repeated access
    /// let cached_value: Arc<u32> = map.get::<u32>().unwrap();
    /// assert_eq!(*cached_value, 42);
    /// ```
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.inner
            .get(&TypeId::of::<T>())
            .and_then(|arc| Arc::clone(arc).downcast().ok())
    }

    /// Get a mutable reference to a value of the given type.
    ///
    /// Only succeeds if the `Arc` has a single owner (no other clones exist).
    pub fn get_mut<T: Send + Sync + 'static>(&mut self) -> Option<&mut T> {
        self.inner
            .get_mut(&TypeId::of::<T>())
            .and_then(|arc| Arc::get_mut(arc))
            .and_then(|val| val.downcast_mut())
    }

    /// Returns `true` if the map contains a value of the given type.
    pub fn contains<T: Send + Sync + 'static>(&self) -> bool {
        self.inner.contains_key(&TypeId::of::<T>())
    }

    /// Remove a value of the given type from the map.
    pub fn remove<T: Send + Sync + 'static>(&mut self) -> Option<T> {
        let arc = self.inner.remove(&TypeId::of::<T>())?;
        // Downcast to concrete type, then try to extract the value
        arc.downcast::<T>()
            .ok()
            .and_then(|arc| Arc::try_unwrap(arc).ok())
    }

    /// Returns the number of entries in the map.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if the map is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Insert a trait object binding. Keys on `TypeId::of::<T>()`
    /// where T may be `?Sized` (e.g. `dyn Notifier`).
    pub fn insert_trait<T: ?Sized + Send + Sync + 'static>(&mut self, arc: Arc<T>) {
        let boxed: Box<dyn Any + Send + Sync> = Box::new(arc);
        self.inner.insert(TypeId::of::<T>(), Arc::from(boxed));
    }

    /// Get a trait object by its `?Sized` type key.
    /// Returns a cloned `Arc<T>` (no double-Arc).
    pub fn get_trait<T: ?Sized + Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.inner.get(&TypeId::of::<T>()).and_then(|arc| {
            let typed: Arc<Arc<T>> = Arc::clone(arc).downcast().ok()?;
            Some(Arc::clone(&*typed))
        })
    }

    /// Check if a trait binding exists.
    pub fn contains_trait<T: ?Sized + Send + Sync + 'static>(&self) -> bool {
        self.inner.contains_key(&TypeId::of::<T>())
    }
}

impl Default for TypeMap {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get() {
        let mut map = TypeMap::new();
        map.insert(42u32);
        map.insert("hello".to_string());

        assert_eq!(*map.get::<u32>().unwrap(), 42);
        assert_eq!(*map.get::<String>().unwrap(), "hello".to_string());
        assert!(map.get::<bool>().is_none());
    }

    #[test]
    fn test_insert_replaces() {
        let mut map = TypeMap::new();
        assert!(map.insert(1u32).is_none());
        assert_eq!(map.insert(2u32), Some(1));
        assert_eq!(*map.get::<u32>().unwrap(), 2);
    }

    #[test]
    fn test_contains() {
        let mut map = TypeMap::new();
        assert!(!map.contains::<u32>());
        map.insert(42u32);
        assert!(map.contains::<u32>());
    }

    #[test]
    fn test_remove() {
        let mut map = TypeMap::new();
        map.insert(42u32);
        assert_eq!(map.remove::<u32>(), Some(42));
        assert!(!map.contains::<u32>());
        assert_eq!(map.remove::<u32>(), None);
    }

    #[test]
    fn test_get_mut() {
        let mut map = TypeMap::new();
        map.insert(vec![1, 2, 3]);
        if let Some(v) = map.get_mut::<Vec<i32>>() {
            v.push(4);
        }
        assert_eq!(*map.get::<Vec<i32>>().unwrap(), vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_len_and_is_empty() {
        let mut map = TypeMap::new();
        assert!(map.is_empty());
        assert_eq!(map.len(), 0);

        map.insert(1u32);
        assert!(!map.is_empty());
        assert_eq!(map.len(), 1);

        map.insert("hi".to_string());
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn test_arc_clone_shares_value() {
        let mut map = TypeMap::new();
        map.insert(42u32);
        let a = map.get::<u32>().unwrap();
        let b = map.get::<u32>().unwrap();
        assert!(Arc::ptr_eq(&a, &b));
    }
}

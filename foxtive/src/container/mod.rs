//! Dependency injection container.
//!
//! Provides [`TypeMap`] - a type-safe heterogeneous map for storing and
//! retrieving service instances by their concrete type. Used as the
//! backing store for [`App`](crate::App)'s service registry.

mod lazy;
mod mutable;
pub mod type_map;

pub use lazy::Lazy;
pub use mutable::Mutable;
pub use type_map::TypeMap;

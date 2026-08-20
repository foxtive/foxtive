mod common;

use foxtive::container::{Lazy, Mutable};
use foxtive::lifecycle::{Service, ServiceInit};
use foxtive::prelude::*;
use std::sync::Arc;

/// Strip ANSI escape sequences so assertions work regardless of TTY.
fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip ESC [ ... final byte (0x40-0x7E)
            if chars.next() == Some('[') {
                while let Some(p) = chars.next() {
                    if (0x40..=0x7E).contains(&(p as u32)) {
                        break;
                    }
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

// Circular dependency via Lazy

#[derive(Service, Default)]
struct ServiceA {
    #[dependency]
    b: Lazy<ServiceB>,
}

#[derive(Service, Default)]
struct ServiceB {
    #[dependency]
    a: Lazy<ServiceA>,
}

#[tokio::test]
async fn circular_dependency_via_lazy_builds_successfully() {
    let app = App::builder("lazy-test", "LAZY")
        .register_service::<ServiceA>()
        .register_service::<ServiceB>()
        .build()
        .await
        .unwrap();

    let a = app.require::<ServiceA>().unwrap();
    let b = app.require::<ServiceB>().unwrap();

    // Both should be filled after freeze
    assert!(a.b.is_filled());
    assert!(b.a.is_filled());
}

#[tokio::test]
async fn lazy_resolution_correctness() {
    let app = App::builder("lazy-test", "LAZY")
        .register_service::<ServiceA>()
        .register_service::<ServiceB>()
        .build()
        .await
        .unwrap();

    let a = app.require::<ServiceA>().unwrap();
    // Access through Lazy should give us the real ServiceB
    let b_via_lazy = &*a.b;
    let b_direct = app.require::<ServiceB>().unwrap();

    // They should point to the same instance
    assert!(Arc::ptr_eq(&a.b.resolve(), &b_direct));
    let _ = b_via_lazy; // just verify we can deref
}

// Mixed eager + lazy

#[derive(Service, Default)]
struct ServiceC {
    #[dependency]
    a: Arc<ServiceA>,
    #[dependency]
    b: Lazy<ServiceB>,
}

#[tokio::test]
async fn mixed_eager_and_lazy() {
    let app = App::builder("lazy-test", "LAZY")
        .register_service::<ServiceA>()
        .register_service::<ServiceB>()
        .register_service::<ServiceC>()
        .build()
        .await
        .unwrap();

    let c = app.require::<ServiceC>().unwrap();
    // Eager dep should be available
    let _a = &*c.a;
    // Lazy dep should also be filled
    assert!(c.b.is_filled());
    let _b = &*c.b;
}

// Non-lazy cycle still errors

struct CycleX;
impl ServiceInit for CycleX {
    async fn init(app: &App) -> AppResult<Self> {
        let _y = app.require::<CycleY>()?;
        Ok(Self)
    }
    fn dependencies() -> Vec<&'static str> {
        vec![std::any::type_name::<CycleY>()]
    }
}

struct CycleY;
impl ServiceInit for CycleY {
    async fn init(app: &App) -> AppResult<Self> {
        let _x = app.require::<CycleX>()?;
        Ok(Self)
    }
    fn dependencies() -> Vec<&'static str> {
        vec![std::any::type_name::<CycleX>()]
    }
}

#[tokio::test]
async fn non_lazy_cycle_still_errors() {
    let result = App::builder("lazy-test", "LAZY")
        .register_service::<CycleX>()
        .register_service::<CycleY>()
        .build()
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("DI0002") || err.to_string().contains("circular dependency"));
}

// Premature access panics

#[test]
fn premature_access_panics() {
    let lazy = Lazy::<ServiceA>::new("TestOwner", "test_field");
    let result = std::panic::catch_unwind(|| {
        let _ = *lazy;
    });
    assert!(result.is_err());
    let msg = result.unwrap_err().downcast::<String>().unwrap();
    assert!(msg.contains("TestOwner"));
    assert!(msg.contains("test_field"));
}

// Concurrent access after freeze

#[tokio::test]
async fn concurrent_access_after_freeze() {
    let app = App::builder("lazy-test", "LAZY")
        .register_service::<ServiceA>()
        .register_service::<ServiceB>()
        .build()
        .await
        .unwrap();

    let a = app.require::<ServiceA>().unwrap();

    let mut handles = vec![];
    for _ in 0..10 {
        let a_clone = Arc::clone(&a);
        handles.push(tokio::spawn(async move {
            // Access through Lazy from multiple tasks
            let _b = &*a_clone.b;
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }
}

// Lazy chain: A → Lazy(B) → Lazy(C) → A

#[derive(Service, Default)]
struct ChainA {
    #[dependency]
    b: Lazy<ChainB>,
}

#[derive(Service, Default)]
struct ChainB {
    #[dependency]
    c: Lazy<ChainC>,
}

#[derive(Service, Default)]
struct ChainC {
    #[dependency]
    a: Lazy<ChainA>,
}

#[tokio::test]
async fn lazy_chain_resolves() {
    let app = App::builder("lazy-test", "LAZY")
        .register_service::<ChainA>()
        .register_service::<ChainB>()
        .register_service::<ChainC>()
        .build()
        .await
        .unwrap();

    let a = app.require::<ChainA>().unwrap();
    assert!(a.b.is_filled());
    assert!(a.b.c.is_filled());
    assert!(a.b.c.a.is_filled());
}

// Lazy<Mutable<T>>

#[derive(Service, Default)]
struct MutableOwner {
    #[dependency]
    counter: Lazy<Mutable<CounterService>>,
}

#[derive(Default)]
struct CounterService {
    count: u64,
}

impl ServiceInit for CounterService {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self::default())
    }
}

#[tokio::test]
async fn lazy_mutable_works() {
    let mut init = App::builder("lazy-test", "LAZY")
        .register_service::<CounterService>()
        .register_service::<MutableOwner>()
        .build_init()
        .await
        .unwrap();

    // Register the Mutable<CounterService> manually
    init.register(Mutable::new(CounterService { count: 0 }));

    let app = init.freeze().await.unwrap();
    let owner = app.require::<MutableOwner>().unwrap();
    assert!(owner.counter.is_filled());

    // Access through Lazy<Mutable<T>>
    let mutable = owner.counter.resolve();
    assert_eq!(mutable.read().count, 0);
    mutable.write().count = 42;
    assert_eq!(mutable.read().count, 42);
}

// Clone independence

#[test]
fn clone_independence_before_fill() {
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

    // But they have independent OnceLock cells
    // (filling cloned again should error since it's already filled)
    assert!(cloned.fill(Arc::new(99)).is_err());
}

// Cycle error message includes edge details

#[tokio::test]
async fn cycle_error_includes_edge_details() {
    let result = App::builder("lazy-test", "LAZY")
        .register_service::<CycleX>()
        .register_service::<CycleY>()
        .build()
        .await;

    let err = strip_ansi(&result.unwrap_err().to_string());
    assert!(
        err.contains("DI0002") || err.contains("circular dependency"),
        "should mention circular: {err}"
    );
    assert!(
        err.contains("\u{2192}") || err.contains("Dependency chain"),
        "should show dep chain: {err}"
    );
    assert!(
        err.contains("Lazy<CycleY>"),
        "should suggest concrete Lazy type: {err}"
    );
}

// Unfilled Lazy caught at startup

struct BrokenLazyService {
    dep: Lazy<NonExistentService>,
}

struct NonExistentService;

impl ServiceInit for BrokenLazyService {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self { dep: lazy!() })
    }

    fn dependencies() -> Vec<&'static str> {
        vec![]
    }

    fn wire_lazy(app: &App) -> AppResult<()> {
        let svc = app.require::<Self>()?;
        app.require_lazy::<NonExistentService>(&svc.dep)?;
        Ok(())
    }
}

impl ServiceInit for NonExistentService {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self)
    }
}

#[tokio::test]
async fn unfilled_lazy_caught_at_startup() {
    let result = App::builder("lazy-test", "LAZY")
        .register_service::<BrokenLazyService>()
        .build()
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    // Should fail during fill phase because NonExistentService isn't registered
    assert!(err.contains("not registered") || err.contains("not filled"));
}

// lazy!() macro captures call-site location

#[test]
fn lazy_macro_captures_call_site_location() {
    let lazy = lazy!();
    let lazy: Lazy<u32> = lazy;
    // owner_type should contain this file's path and line number
    assert!(
        lazy.owner_type().contains("lazy_dep_tests.rs"),
        "expected file path in owner_type, got: {}",
        lazy.owner_type()
    );
    assert_eq!(lazy.field_name(), "<manual>");
}

#[test]
fn lazy_macro_panic_includes_call_site() {
    let lazy: Lazy<u32> = lazy!();
    let result = std::panic::catch_unwind(|| {
        let _ = *lazy;
    });
    assert!(result.is_err());
    let msg = result.unwrap_err().downcast::<String>().unwrap();
    // Should contain this file's path, not lazy.rs
    assert!(
        msg.contains("lazy_dep_tests.rs"),
        "panic message should reference call-site file, got: {msg}"
    );
}

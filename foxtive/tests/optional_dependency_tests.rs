mod common;

use foxtive::lifecycle::{Service, ServiceInit};
use foxtive::prelude::*;
use std::sync::Arc;

struct CacheService {
    #[allow(dead_code)]
    driver: String,
}

struct RequiredService {
    #[allow(dead_code)]
    name: String,
}

struct ManualOptionalService {
    cache: Option<Arc<CacheService>>,
    timeout: Option<u32>,
}

impl ServiceInit for ManualOptionalService {
    async fn init(app: &App) -> AppResult<Self> {
        Ok(Self {
            cache: app.get::<CacheService>(),
            timeout: app.get::<u32>().map(|v| *v),
        })
    }
}

#[derive(Service)]
struct DerivedOptionalService {
    #[dependency]
    cache: Option<Arc<CacheService>>,
}

#[derive(Service)]
struct MixedService {
    #[dependency]
    required: Arc<RequiredService>,
    #[dependency]
    cache: Option<Arc<CacheService>>,
}

#[tokio::test]
async fn optional_arc_dep_present() {
    let app = App::builder("test", "TST")
        .register(CacheService {
            driver: "redis".into(),
        })
        .register_service::<ManualOptionalService>()
        .build()
        .await
        .unwrap();

    let svc = app.get::<ManualOptionalService>().unwrap();
    assert!(svc.cache.is_some());
    assert_eq!(svc.cache.as_ref().unwrap().driver, "redis");
}

#[tokio::test]
async fn optional_arc_dep_absent() {
    let app = App::builder("test", "TST")
        .register_service::<ManualOptionalService>()
        .build()
        .await
        .unwrap();

    let svc = app.get::<ManualOptionalService>().unwrap();
    assert!(svc.cache.is_none());
}

#[tokio::test]
async fn optional_dep_present() {
    let app = App::builder("test", "TST")
        .register(42u32)
        .register_service::<ManualOptionalService>()
        .build()
        .await
        .unwrap();

    let svc = app.get::<ManualOptionalService>().unwrap();
    assert_eq!(svc.timeout, Some(42));
}

#[tokio::test]
async fn optional_dep_absent() {
    let app = App::builder("test", "TST")
        .register_service::<ManualOptionalService>()
        .build()
        .await
        .unwrap();

    let svc = app.get::<ManualOptionalService>().unwrap();
    assert_eq!(svc.timeout, None);
}

#[tokio::test]
async fn optional_dep_not_in_topo_sort() {
    // OptionalService constructs successfully even though
    // CacheService is not registered (optional dep is None).
    let app = App::builder("test", "TST")
        .register_service::<DerivedOptionalService>()
        .build()
        .await
        .unwrap();

    let svc = app.get::<DerivedOptionalService>().unwrap();
    assert!(svc.cache.is_none());
}

#[tokio::test]
async fn mixed_optional_and_required() {
    // Required is present, optional is absent
    let app = App::builder("test", "TST")
        .register(RequiredService { name: "req".into() })
        .register_service::<MixedService>()
        .build()
        .await
        .unwrap();

    let svc = app.get::<MixedService>().unwrap();
    assert_eq!(svc.required.name, "req");
    assert!(svc.cache.is_none());
}

#[tokio::test]
async fn mixed_optional_and_required_both_present() {
    let app = App::builder("test", "TST")
        .register(RequiredService { name: "req".into() })
        .register(CacheService {
            driver: "memory".into(),
        })
        .register_service::<MixedService>()
        .build()
        .await
        .unwrap();

    let svc = app.get::<MixedService>().unwrap();
    assert_eq!(svc.required.name, "req");
    assert!(svc.cache.is_some());
    assert_eq!(svc.cache.as_ref().unwrap().driver, "memory");
}

#[tokio::test]
async fn derived_optional_dep_present() {
    let app = App::builder("test", "TST")
        .register(CacheService {
            driver: "filesystem".into(),
        })
        .register_service::<DerivedOptionalService>()
        .build()
        .await
        .unwrap();

    let svc = app.get::<DerivedOptionalService>().unwrap();
    assert!(svc.cache.is_some());
    assert_eq!(svc.cache.as_ref().unwrap().driver, "filesystem");
}

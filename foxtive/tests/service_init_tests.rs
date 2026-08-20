mod common;

use foxtive::App;
use foxtive::container::Lazy;
use foxtive::enums::AppMessage;
use foxtive::lifecycle::{Service, ServiceInit};
use foxtive::prelude::AppResult;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

// Simple service that doesn't depend on anything
struct SimpleService {
    id: usize,
}

impl ServiceInit for SimpleService {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self { id: 1 })
    }
}

// Service that accesses app during init
struct ServiceWithAppAccess {
    app_name: String,
}

impl ServiceInit for ServiceWithAppAccess {
    async fn init(app: &App) -> AppResult<Self> {
        Ok(Self {
            app_name: app.app_name().to_string(),
        })
    }
}

// Service that depends on another service
struct DependentService {
    simple_id: usize,
}

impl ServiceInit for DependentService {
    async fn init(app: &App) -> AppResult<Self> {
        let simple = app.require::<SimpleService>()?;
        Ok(Self {
            simple_id: simple.id,
        })
    }
}

// Service that fails during init
struct FailingService;

impl ServiceInit for FailingService {
    async fn init(_app: &App) -> AppResult<Self> {
        Err(AppMessage::Infrastructure {
            message: "init failed".into(),
            source: None,
        })
    }
}

#[tokio::test]
async fn service_init_registers_simple_service() {
    let app = App::builder("svc", "SVC")
        .register_service::<SimpleService>()
        .build()
        .await
        .unwrap();

    let svc = app.get::<SimpleService>().unwrap();
    assert_eq!(svc.id, 1);
}

#[tokio::test]
async fn service_init_can_access_app_during_init() {
    let app = App::builder("My App", "APP")
        .register_service::<ServiceWithAppAccess>()
        .build()
        .await
        .unwrap();

    let svc = app.get::<ServiceWithAppAccess>().unwrap();
    assert_eq!(svc.app_name, "My App");
}

#[tokio::test]
async fn service_init_can_depend_on_other_services() {
    let app = App::builder("svc", "SVC")
        .register_service::<SimpleService>()
        .register_service::<DependentService>()
        .build()
        .await
        .unwrap();

    let simple = app.get::<SimpleService>().unwrap();
    assert_eq!(simple.id, 1);

    let dependent = app.get::<DependentService>().unwrap();
    assert_eq!(dependent.simple_id, 1);
}

#[tokio::test]
async fn service_init_failure_propagates_error() {
    let result = App::builder("svc", "SVC")
        .register_service::<FailingService>()
        .build()
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn service_init_multiple_services() {
    struct ServiceA;
    impl ServiceInit for ServiceA {
        async fn init(_app: &App) -> AppResult<Self> {
            Ok(Self)
        }
    }

    struct ServiceB;
    impl ServiceInit for ServiceB {
        async fn init(_app: &App) -> AppResult<Self> {
            Ok(Self)
        }
    }

    let app = App::builder("svc", "SVC")
        .register_service::<ServiceA>()
        .register_service::<ServiceB>()
        .build()
        .await
        .unwrap();

    assert!(app.get::<ServiceA>().is_some());
    assert!(app.get::<ServiceB>().is_some());
}

#[tokio::test]
async fn service_init_with_mixed_registration() {
    struct SyncService {
        value: i32,
    }

    struct AsyncService {
        value: i32,
    }

    impl ServiceInit for AsyncService {
        async fn init(_app: &App) -> AppResult<Self> {
            Ok(Self { value: 42 })
        }
    }

    let mut init = App::builder("svc", "SVC")
        .register(SyncService { value: 10 })
        .register_service::<AsyncService>()
        .build_init()
        .await
        .unwrap();

    // Can still register sync services after build_init
    init.register(100i32);

    let app = init.freeze().await.unwrap();

    // Sync service is available
    let sync_svc = app.get::<SyncService>().unwrap();
    assert_eq!(sync_svc.value, 10);

    // Async service is available
    let async_svc = app.get::<AsyncService>().unwrap();
    assert_eq!(async_svc.value, 42);

    // Other sync registration is available
    assert_eq!(*app.get::<i32>().unwrap(), 100);
}

#[tokio::test]
async fn service_init_services_constructed_in_order() {
    static CONSTRUCTION_ORDER: AtomicUsize = AtomicUsize::new(0);

    struct FirstService {
        order: usize,
    }

    impl ServiceInit for FirstService {
        async fn init(_app: &App) -> AppResult<Self> {
            let order = CONSTRUCTION_ORDER.fetch_add(1, Ordering::SeqCst);
            Ok(Self { order })
        }
    }

    struct SecondService {
        order: usize,
        first_order: usize,
    }

    impl ServiceInit for SecondService {
        async fn init(app: &App) -> AppResult<Self> {
            let order = CONSTRUCTION_ORDER.fetch_add(1, Ordering::SeqCst);
            let first = app.require::<FirstService>()?;
            Ok(Self {
                order,
                first_order: first.order,
            })
        }
    }

    let app = App::builder("svc", "SVC")
        .register_service::<FirstService>()
        .register_service::<SecondService>()
        .build()
        .await
        .unwrap();

    let first = app.get::<FirstService>().unwrap();
    let second = app.get::<SecondService>().unwrap();

    // First service was constructed before second
    assert_eq!(first.order, 0);
    assert_eq!(second.order, 1);
    // Second service can access first service
    assert_eq!(second.first_order, 0);
}

// ─── AppInit::register_service ───────────────────────────────────────

#[derive(Service)]
struct InitSvcA {
    #[allow(dead_code)]
    #[dependency]
    b: Arc<InitSvcB>,
}

#[derive(Service)]
struct InitSvcB;

#[derive(Service)]
struct BuilderSvc;

#[derive(Service)]
struct InitSvc;

#[derive(Service)]
struct LazyTarget;

#[derive(Service)]
struct LazyHolder {
    #[dependency]
    dep: Lazy<LazyTarget>,
}

#[tokio::test]
async fn register_service_on_app_init() {
    let mut init = App::builder("init-reg", "INITREG")
        .build_init()
        .await
        .unwrap();

    init.register_service::<InitSvcB>();
    init.register_service::<InitSvcA>();

    let app = init.freeze().await.unwrap();
    assert!(app.get::<InitSvcA>().is_some());
    assert!(app.get::<InitSvcB>().is_some());
}

#[tokio::test]
async fn mixed_builder_and_init_service_registration() {
    let mut init = App::builder("mixed-reg", "MIXED")
        .register_service::<BuilderSvc>()
        .build_init()
        .await
        .unwrap();

    init.register_service::<InitSvc>();

    let app = init.freeze().await.unwrap();
    assert!(app.get::<BuilderSvc>().is_some());
    assert!(app.get::<InitSvc>().is_some());
}

#[tokio::test]
async fn app_init_register_service_with_lazy_deps() {
    let mut init = App::builder("lazy-init", "LINIT")
        .register_service::<LazyTarget>()
        .build_init()
        .await
        .unwrap();

    init.register_service::<LazyHolder>();

    let app = init.freeze().await.unwrap();
    let holder = app.require::<LazyHolder>().unwrap();
    assert!(holder.dep.is_filled());
}

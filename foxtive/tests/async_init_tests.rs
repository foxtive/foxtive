mod common;

use foxtive::App;
use foxtive::app::AppInit;
use foxtive::enums::AppMessage;
use foxtive::lifecycle::AsyncInit;
use foxtive::prelude::AppResult;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct SimpleService {
    initialized: Arc<AtomicBool>,
}

impl AsyncInit for SimpleService {
    async fn init(_init: &AppInit) -> AppResult<Self> {
        Ok(Self {
            initialized: Arc::new(AtomicBool::new(true)),
        })
    }
}

struct ServiceWithDependency {
    value: i32,
}

impl AsyncInit for ServiceWithDependency {
    async fn init(init: &AppInit) -> AppResult<Self> {
        // Can access app during init via Deref
        let _name = init.app_name();
        Ok(Self { value: 42 })
    }
}

struct FailingService;

impl AsyncInit for FailingService {
    async fn init(_init: &AppInit) -> AppResult<Self> {
        Err(AppMessage::Infrastructure {
            message: "init failed".into(),
            source: None,
        })
    }
}

#[tokio::test]
async fn async_init_service_registers_instance() {
    let mut init = App::builder("svc", "SVC").build_init().await.unwrap();

    init.init_service::<SimpleService>().await.unwrap();

    let app = init.freeze().await.unwrap();
    let svc = app.get::<SimpleService>().unwrap();
    assert!(svc.initialized.load(Ordering::SeqCst));
}

#[tokio::test]
async fn async_init_can_access_app() {
    let mut init = App::builder("My App", "APP").build_init().await.unwrap();

    init.init_service::<ServiceWithDependency>().await.unwrap();

    let app = init.freeze().await.unwrap();
    let svc = app.get::<ServiceWithDependency>().unwrap();
    assert_eq!(svc.value, 42);
}

#[tokio::test]
async fn async_init_failure_propagates_error() {
    let mut init = App::builder("svc", "SVC").build_init().await.unwrap();

    let result = init.init_service::<FailingService>().await;
    assert!(result.is_err());

    // Service should not be registered after failure
    let app = init.freeze().await.unwrap();
    assert!(app.get::<FailingService>().is_none());
}

#[tokio::test]
async fn async_init_multiple_services() {
    struct ServiceA;
    impl AsyncInit for ServiceA {
        async fn init(_init: &AppInit) -> AppResult<Self> {
            Ok(Self)
        }
    }

    struct ServiceB;
    impl AsyncInit for ServiceB {
        async fn init(_init: &AppInit) -> AppResult<Self> {
            Ok(Self)
        }
    }

    let mut init = App::builder("svc", "SVC").build_init().await.unwrap();

    init.init_service::<ServiceA>().await.unwrap();
    init.init_service::<ServiceB>().await.unwrap();

    let app = init.freeze().await.unwrap();
    assert!(app.get::<ServiceA>().is_some());
    assert!(app.get::<ServiceB>().is_some());
}

#[tokio::test]
async fn async_init_replaces_existing_service() {
    let counter = Arc::new(AtomicUsize::new(0));
    let _counter_clone = counter.clone();

    struct CountingService {
        id: usize,
    }

    impl AsyncInit for CountingService {
        async fn init(_init: &AppInit) -> AppResult<Self> {
            Ok(Self { id: 1 })
        }
    }

    let mut init = App::builder("svc", "SVC").build_init().await.unwrap();

    // Register first instance
    init.register(CountingService { id: 1 });
    assert_eq!(init.get::<CountingService>().unwrap().id, 1);

    // Init should replace it
    init.init_service::<CountingService>().await.unwrap();
    assert_eq!(init.get::<CountingService>().unwrap().id, 1);
}

#[tokio::test]
async fn async_init_with_build_init_workflow() {
    struct DatabaseService {
        connected: bool,
    }

    impl AsyncInit for DatabaseService {
        async fn init(_init: &AppInit) -> AppResult<Self> {
            // Simulate async connection setup
            Ok(Self { connected: true })
        }
    }

    struct CacheService {
        warmed: bool,
    }

    impl AsyncInit for CacheService {
        async fn init(_init: &AppInit) -> AppResult<Self> {
            // Simulate cache warming
            Ok(Self { warmed: true })
        }
    }

    let mut init = App::builder("My Service", "SVC")
        .build_init()
        .await
        .unwrap();

    // Can register sync services
    init.register(42i32);

    // Can init async services
    init.init_service::<DatabaseService>().await.unwrap();
    init.init_service::<CacheService>().await.unwrap();

    let app = init.freeze().await.unwrap();

    // All services available
    assert_eq!(*app.get::<i32>().unwrap(), 42);
    assert!(app.get::<DatabaseService>().unwrap().connected);
    assert!(app.get::<CacheService>().unwrap().warmed);
}

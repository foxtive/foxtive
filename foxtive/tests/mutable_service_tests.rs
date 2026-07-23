mod common;

use foxtive::container::Mutable;
use foxtive::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

struct Counter {
    count: u64,
}

impl Counter {
    fn new() -> Self {
        Self { count: 0 }
    }

    fn increment(&mut self) {
        self.count += 1;
    }

    fn value(&self) -> u64 {
        self.count
    }
}

#[tokio::test]
async fn register_mutable_via_init_and_retrieve_via_app() {
    let mut init = App::builder("svc", "SVC").build_init().await.unwrap();
    init.register(Mutable::new(Counter::new()));

    let app = init.freeze().await.unwrap();
    let counter = app.get::<Mutable<Counter>>().unwrap();
    assert_eq!(counter.read().value(), 0);
}

#[tokio::test]
async fn register_mutable_convenience_method() {
    let mut init = App::builder("svc", "SVC").build_init().await.unwrap();
    init.register_mutable(Counter::new());

    let app = init.freeze().await.unwrap();
    let counter = app.require_mutable::<Counter>().unwrap();
    assert_eq!(counter.read().value(), 0);
}

#[tokio::test]
async fn mutate_through_arc_mutable_visible_across_clones() {
    let mut init = App::builder("svc", "SVC").build_init().await.unwrap();
    init.register_mutable(Counter::new());

    let app = init.freeze().await.unwrap();
    let c1 = app.require_mutable::<Counter>().unwrap();
    let c2 = app.require_mutable::<Counter>().unwrap();

    // Mutate through c1
    c1.write().increment();
    c1.write().increment();

    // Visible through c2 (same underlying data)
    assert_eq!(c2.read().value(), 2);
    assert!(Arc::ptr_eq(&c1, &c2));
}

#[tokio::test]
async fn require_mutable_returns_error_for_unregistered_type() {
    let app = App::builder("svc", "SVC").build().await.unwrap();
    let result = app.require_mutable::<Counter>();
    assert!(result.is_err());
}

#[tokio::test]
async fn get_mutable_returns_none_for_unregistered_type() {
    let app = App::builder("svc", "SVC").build().await.unwrap();
    assert!(app.get_mutable::<Counter>().is_none());
}

#[tokio::test]
async fn mutable_works_with_service_init() {
    use foxtive::lifecycle::ServiceInit;

    struct MutableConsumer;

    impl ServiceInit for MutableConsumer {
        async fn init(app: &App) -> AppResult<Self> {
            let counter = app.require_mutable::<Counter>()?;
            counter.write().increment();
            Ok(MutableConsumer)
        }
    }

    let mut init = App::builder("svc", "SVC")
        .register_service::<MutableConsumer>()
        .build_init()
        .await
        .unwrap();

    init.register_mutable(Counter::new());

    let app = init.freeze().await.unwrap();

    // The MutableConsumer incremented the counter during init
    let counter = app.require_mutable::<Counter>().unwrap();
    assert_eq!(counter.read().value(), 1);
}

#[tokio::test]
async fn mutable_works_with_after_build_callback() {
    let app = App::builder("svc", "SVC")
        .after_build(|init: &mut AppInit| {
            init.register_mutable(Counter::new());
            Ok(())
        })
        .build()
        .await
        .unwrap();

    let counter = app.require_mutable::<Counter>().unwrap();
    counter.write().increment();
    assert_eq!(counter.read().value(), 1);
}

#[tokio::test]
async fn concurrent_writes_from_multiple_tasks() {
    let mut init = App::builder("svc", "SVC").build_init().await.unwrap();
    init.register_mutable(Counter::new());

    let app = init.freeze().await.unwrap();
    let counter = app.require_mutable::<Counter>().unwrap();

    let mut handles = Vec::new();
    for _ in 0..10 {
        let c = Arc::clone(&counter);
        handles.push(tokio::spawn(async move {
            c.write().increment();
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert_eq!(counter.read().value(), 10);
}

#[tokio::test]
async fn mutable_with_complex_state() {
    let mut init = App::builder("svc", "SVC").build_init().await.unwrap();

    let cache = Mutable::new(HashMap::<String, u64>::new());
    init.register(cache);

    let app = init.freeze().await.unwrap();
    let cache = app.require::<Mutable<HashMap<String, u64>>>().unwrap();

    cache.write().insert("hits".into(), 42);
    cache.write().insert("misses".into(), 7);

    assert_eq!(cache.read().get("hits"), Some(&42));
    assert_eq!(cache.read().get("misses"), Some(&7));
}

#[tokio::test]
async fn multiple_mutable_services_of_different_types() {
    let mut init = App::builder("svc", "SVC").build_init().await.unwrap();
    init.register_mutable(Counter::new());
    init.register_mutable(vec![1, 2, 3]);

    let app = init.freeze().await.unwrap();

    let counter = app.require_mutable::<Counter>().unwrap();
    let items = app.require_mutable::<Vec<i32>>().unwrap();

    counter.write().increment();
    items.write().push(4);

    assert_eq!(counter.read().value(), 1);
    assert_eq!(*items.read(), vec![1, 2, 3, 4]);
}

#[tokio::test]
async fn mutable_with_async_init_service() {
    use foxtive::lifecycle::AsyncInit;

    struct AsyncCounter {
        count: u64,
    }

    impl AsyncInit for AsyncCounter {
        async fn init(_init: &AppInit) -> AppResult<Self> {
            Ok(Self { count: 100 })
        }
    }

    let mut init = App::builder("svc", "SVC").build_init().await.unwrap();
    let counter = AsyncCounter::init(&init).await.unwrap();
    init.register(Mutable::new(counter));

    let app = init.freeze().await.unwrap();
    let c = app.require_mutable::<AsyncCounter>().unwrap();
    assert_eq!(c.read().count, 100);
    c.write().count += 1;
    assert_eq!(c.read().count, 101);
}

#[test]
fn into_inner_recovers_value() {
    let m = Mutable::new(vec![1, 2, 3]);
    let v = m.into_inner();
    assert_eq!(v, vec![1, 2, 3]);
}

#[test]
fn debug_format_includes_value() {
    let m = Mutable::new(42u32);
    let debug = format!("{m:?}");
    assert!(debug.contains("Mutable"));
    assert!(debug.contains("42"));
}

#[tokio::test]
async fn builder_register_mutable_service_deferred_construction() {
    use foxtive::lifecycle::Service;

    #[derive(Service)]
    struct MutableCounter {
        count: u64,
    }

    let app = App::builder("svc", "SVC")
        .register_mutable_service::<MutableCounter>()
        .build()
        .await
        .unwrap();

    let counter = app.require_mutable::<MutableCounter>().unwrap();
    assert_eq!(counter.read().count, 0);
    counter.write().count = 42;
    assert_eq!(counter.read().count, 42);
}

#[tokio::test]
async fn init_register_mutable_service_deferred_construction() {
    use foxtive::lifecycle::Service;

    #[derive(Service)]
    struct AppState {
        value: String,
    }

    let mut init = App::builder("svc", "SVC").build_init().await.unwrap();
    init.register_mutable_service::<AppState>();

    let app = init.freeze().await.unwrap();
    let state = app.require_mutable::<AppState>().unwrap();
    assert_eq!(state.read().value, "");
    state.write().value = "hello".into();
    assert_eq!(state.read().value, "hello");
}

#[tokio::test]
async fn service_mutable_attribute_wraps_in_mutable() {
    use foxtive::lifecycle::Service;

    #[derive(Service)]
    #[service(mutable)]
    struct DeriveMutableCounter {
        count: u64,
    }

    let app = App::builder("svc", "SVC")
        .register_service::<DeriveMutableCounter>()
        .build()
        .await
        .unwrap();

    // Stored as Mutable<DeriveMutableCounter>, not DeriveMutableCounter
    let counter = app.require_mutable::<DeriveMutableCounter>().unwrap();
    assert_eq!(counter.read().count, 0);
    counter.write().count += 10;
    assert_eq!(counter.read().count, 10);
}

#[tokio::test]
async fn service_mutable_attribute_with_dependencies() {
    use foxtive::lifecycle::Service;

    #[derive(Service)]
    struct CacheService {
        name: String,
    }

    #[derive(Service)]
    #[service(mutable)]
    struct MutableWorker {
        #[dependency]
        cache: Arc<CacheService>,
        processed: u64,
    }

    let app = App::builder("svc", "SVC")
        .register_service::<CacheService>()
        .register_service::<MutableWorker>()
        .build()
        .await
        .unwrap();

    let worker = app.require_mutable::<MutableWorker>().unwrap();
    assert_eq!(worker.read().cache.name, "");
    assert_eq!(worker.read().processed, 0);
    worker.write().processed = 5;
    assert_eq!(worker.read().processed, 5);
}

#[tokio::test]
async fn builder_register_mutable_service_dedup() {
    use foxtive::lifecycle::Service;

    #[derive(Service)]
    struct DedupService;

    // Registering the same type twice should not panic - just logs a warning
    let app = App::builder("svc", "SVC")
        .register_mutable_service::<DedupService>()
        .register_mutable_service::<DedupService>()
        .build()
        .await
        .unwrap();

    let svc = app.require_mutable::<DedupService>();
    assert!(svc.is_ok());
}

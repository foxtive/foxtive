mod common;

use foxtive::lifecycle::ServiceInit;
use foxtive::prelude::*;

struct MetricsService {
    #[allow(dead_code)]
    enabled: bool,
}

impl ServiceInit for MetricsService {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self { enabled: true })
    }
}

struct LoggingService {
    #[allow(dead_code)]
    level: String,
}

impl ServiceInit for LoggingService {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self {
            level: "info".to_string(),
        })
    }
}

struct OverrideV1 {
    #[allow(dead_code)]
    version: u32,
}

impl ServiceInit for OverrideV1 {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self { version: 1 })
    }
}

struct OverrideV2 {
    #[allow(dead_code)]
    version: u32,
}

impl ServiceInit for OverrideV2 {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self { version: 2 })
    }
}

#[tokio::test]
async fn register_service_if_false_skips() {
    let app = App::builder("test", "TST")
        .register_service_if::<MetricsService>(false)
        .build()
        .await
        .unwrap();

    assert!(app.get::<MetricsService>().is_none());
}

#[tokio::test]
async fn register_service_if_true_registers() {
    let app = App::builder("test", "TST")
        .register_service_if::<MetricsService>(true)
        .build()
        .await
        .unwrap();

    assert!(app.get::<MetricsService>().is_some());
}

#[tokio::test]
async fn try_register_service_first_succeeds() {
    let app = App::builder("test", "TST")
        .try_register_service::<MetricsService>()
        .build()
        .await
        .unwrap();

    assert!(app.get::<MetricsService>().is_some());
}

#[tokio::test]
async fn try_register_service_duplicate_noop() {
    let app = App::builder("test", "TST")
        .register_service::<MetricsService>()
        .try_register_service::<MetricsService>() // silently skipped
        .build()
        .await
        .unwrap();

    // Still resolvable, no error
    assert!(app.get::<MetricsService>().is_some());
}

#[tokio::test]
async fn replace_service_replaces() {
    let app = App::builder("test", "TST")
        .register_service::<OverrideV1>()
        .replace_service::<OverrideV2>()
        .build()
        .await
        .unwrap();

    assert!(app.get::<OverrideV2>().is_some());
}

#[tokio::test]
async fn register_if_false_skips() {
    let app = App::builder("test", "TST")
        .register_if(false, 42u32)
        .build()
        .await
        .unwrap();

    assert!(app.get::<u32>().is_none());
}

#[tokio::test]
async fn register_if_true_registers() {
    let app = App::builder("test", "TST")
        .register_if(true, 42u32)
        .build()
        .await
        .unwrap();

    assert_eq!(*app.get::<u32>().unwrap(), 42);
}

#[tokio::test]
async fn app_init_try_register_service() {
    let mut init = App::builder("test", "TST")
        .build_init()
        .await
        .unwrap();

    init.try_register_service::<MetricsService>();
    init.try_register_service::<MetricsService>(); // duplicate - silently skipped

    let app = init.freeze().await.unwrap();
    assert!(app.get::<MetricsService>().is_some());
}

#[tokio::test]
async fn app_init_replace_service() {
    let mut init = App::builder("test", "TST")
        .build_init()
        .await
        .unwrap();

    init.register_service::<OverrideV1>();
    init.replace_service::<OverrideV2>();

    let app = init.freeze().await.unwrap();
    assert!(app.get::<OverrideV2>().is_some());
}

#[tokio::test]
async fn app_init_register_service_if() {
    let mut init = App::builder("test", "TST")
        .build_init()
        .await
        .unwrap();

    init.register_service_if::<MetricsService>(true);
    init.register_service_if::<LoggingService>(false);

    let app = init.freeze().await.unwrap();
    assert!(app.get::<MetricsService>().is_some());
    assert!(app.get::<LoggingService>().is_none());
}

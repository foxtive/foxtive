mod common;
#[cfg(test)]
mod serde_json_tests;

use common::{Duration, Environment};
use foxtive::App;
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test]
async fn minimal_build_succeeds() {
    let app = App::builder("Test App", "TEST")
        .environment(Environment::Production)
        .app_key("secret")
        .build()
        .await
        .unwrap();

    assert_eq!(app.app_name(), "Test App");
    assert_eq!(app.app_code(), "TEST");
    assert_eq!(app.env(), Environment::Production);
    assert_eq!(app.app_key_raw(), "secret");
}

#[tokio::test]
async fn builder_stores_all_fields() {
    let app = App::builder("My Service", "MYSVC")
        .environment(Environment::Staging)
        .app_key("my-key")
        .env_prefix("MYAPP_")
        .public_key("pub-key")
        .private_key("priv-key")
        .version("1.2.3")
        .build()
        .await
        .unwrap();

    assert_eq!(app.app_name(), "My Service");
    assert_eq!(app.app_code(), "MYSVC");
    assert_eq!(app.env(), Environment::Staging);
    assert_eq!(app.app_key_raw(), "my-key");
    assert_eq!(app.app_env_prefix(), "MYAPP_");
    assert_eq!(app.app_public_key(), "pub-key");
    assert_eq!(app.app_private_key_raw(), "priv-key");
    assert_eq!(app.version(), Some("1.2.3"));
}

#[tokio::test]
async fn version_is_none_by_default() {
    let app = App::builder("svc", "SVC").build().await.unwrap();
    assert_eq!(app.version(), None);
}

#[tokio::test]
async fn register_and_retrieve_service() {
    struct MyService {
        value: i32,
    }

    let app = App::builder("svc", "SVC")
        .register(MyService { value: 42 })
        .build()
        .await
        .unwrap();

    let svc = app.get::<MyService>().unwrap();
    assert_eq!(svc.value, 42);
}

#[tokio::test]
async fn get_returns_none_for_unregistered_service() {
    let app = App::builder("svc", "SVC").build().await.unwrap();
    assert!(app.get::<String>().is_none());
}

#[tokio::test]
async fn require_errors_for_missing_service() {
    #[derive(Debug)]
    struct Missing;

    let app = App::builder("svc", "SVC").build().await.unwrap();
    let err = app.require::<Missing>().unwrap_err();
    assert!(err.message().contains("not registered"));
}

#[tokio::test]
async fn require_succeeds_for_registered_service() {
    struct Present {
        data: String,
    }

    let app = App::builder("svc", "SVC")
        .register(Present {
            data: "hello".into(),
        })
        .build()
        .await
        .unwrap();

    let svc = app.require::<Present>().unwrap();
    assert_eq!(svc.data, "hello");
}

#[tokio::test]
async fn register_multiple_services() {
    struct ServiceA;
    struct ServiceB;
    struct ServiceC;

    let app = App::builder("svc", "SVC")
        .register(ServiceA)
        .register(ServiceB)
        .register(ServiceC)
        .build()
        .await
        .unwrap();

    assert!(app.get::<ServiceA>().is_some());
    assert!(app.get::<ServiceB>().is_some());
    assert!(app.get::<ServiceC>().is_some());
}

#[tokio::test]
async fn shutdown_is_idempotent() {
    let counter = Arc::new(AtomicUsize::new(0));
    let c = counter.clone();

    let app = App::builder("svc", "SVC")
        .on_shutdown(move |_| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
            }
        })
        .build()
        .await
        .unwrap();

    app.shutdown().await;
    app.shutdown().await;
    app.shutdown().await;

    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn is_shutting_down_reflects_state() {
    let app = App::builder("svc", "SVC").build().await.unwrap();
    assert!(!app.is_shutting_down());

    app.shutdown().await;
    assert!(app.is_shutting_down());
}

#[tokio::test]
async fn health_check_timeout_defaults_to_five_seconds() {
    let app = App::builder("svc", "SVC").build().await.unwrap();
    assert_eq!(app.health_check_timeout(), Duration::from_secs(5));
}

#[tokio::test]
async fn health_check_timeout_is_configurable() {
    let app = App::builder("svc", "SVC")
        .health_check_timeout(Duration::from_secs(30))
        .build()
        .await
        .unwrap();

    assert_eq!(app.health_check_timeout(), Duration::from_secs(30));
}

#[tokio::test]
async fn title_formats_with_app_name() {
    let app = App::builder("My App", "SVC").build().await.unwrap();
    assert_eq!(app.title("Dashboard"), "Dashboard - My App");
}

#[tokio::test]
async fn uptime_is_nonzero_after_build() {
    let app = App::builder("svc", "SVC").build().await.unwrap();
    std::thread::sleep(std::time::Duration::from_millis(10));
    assert!(app.uptime().as_millis() > 0);
}

#[tokio::test]
async fn metrics_sink_is_none_by_default() {
    let app = App::builder("svc", "SVC").build().await.unwrap();
    assert!(app.metrics().is_none());
}

#[tokio::test]
async fn debug_impl_does_not_panic() {
    let app = App::builder("svc", "SVC").build().await.unwrap();
    let debug = format!("{app:?}");
    assert!(debug.contains("App"));
}

use std::sync::Arc;

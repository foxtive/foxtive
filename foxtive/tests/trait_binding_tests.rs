mod common;

use foxtive::lifecycle::Service;
use foxtive::prelude::*;
use std::sync::Arc;

trait Notifier: Send + Sync + 'static {
    fn notify(&self, msg: &str) -> String;
}

trait Logger: Send + Sync + 'static {
    fn log(&self, msg: &str) -> String;
}

struct EmailNotifier;
impl Notifier for EmailNotifier {
    fn notify(&self, msg: &str) -> String {
        format!("[EMAIL] {msg}")
    }
}

struct SmsNotifier;
impl Notifier for SmsNotifier {
    fn notify(&self, msg: &str) -> String {
        format!("[SMS] {msg}")
    }
}

struct ConsoleLogger;
impl Logger for ConsoleLogger {
    fn log(&self, msg: &str) -> String {
        format!("[LOG] {msg}")
    }
}

#[tokio::test]
async fn register_trait_eager_resolves() {
    let app = App::builder("test", "TST")
        .register_trait::<dyn Notifier>(Arc::new(EmailNotifier))
        .build()
        .await
        .unwrap();

    let notifier = app.require_trait::<dyn Notifier>().unwrap();
    assert_eq!(notifier.notify("hello"), "[EMAIL] hello");
}

#[tokio::test]
async fn register_trait_missing_returns_error() {
    let app = App::builder("test", "TST")
        .build()
        .await
        .unwrap();

    let result = app.require_trait::<dyn Notifier>();
    assert!(result.is_err());
}

#[tokio::test]
async fn register_trait_on_app_init() {
    let mut init = App::builder("test", "TST")
        .build_init()
        .await
        .unwrap();

    init.register_trait::<dyn Notifier>(Arc::new(SmsNotifier));
    let app = init.freeze().await.unwrap();

    let notifier = app.require_trait::<dyn Notifier>().unwrap();
    assert_eq!(notifier.notify("test"), "[SMS] test");
}

#[tokio::test]
async fn register_trait_replaces() {
    let mut init = App::builder("test", "TST")
        .register_trait::<dyn Notifier>(Arc::new(EmailNotifier))
        .build_init()
        .await
        .unwrap();

    // Replace with SMS notifier
    init.register_trait::<dyn Notifier>(Arc::new(SmsNotifier));
    let app = init.freeze().await.unwrap();

    let notifier = app.require_trait::<dyn Notifier>().unwrap();
    assert_eq!(notifier.notify("test"), "[SMS] test");
}

#[tokio::test]
async fn trait_and_concrete_coexist() {
    // Register both concrete type and trait binding
    struct ConcreteType {
        value: String,
    }

    let app = App::builder("test", "TST")
        .register(ConcreteType {
            value: "concrete".into(),
        })
        .register_trait::<dyn Notifier>(Arc::new(EmailNotifier))
        .build()
        .await
        .unwrap();

    // Both resolvable independently
    let concrete = app.get::<ConcreteType>().unwrap();
    assert_eq!(concrete.value, "concrete");

    let notifier = app.require_trait::<dyn Notifier>().unwrap();
    assert_eq!(notifier.notify("x"), "[EMAIL] x");
}

#[tokio::test]
async fn get_trait_returns_none_when_missing() {
    let app = App::builder("test", "TST")
        .build()
        .await
        .unwrap();

    assert!(app.get_trait::<dyn Notifier>().is_none());
}

#[tokio::test]
async fn get_trait_returns_some_when_present() {
    let app = App::builder("test", "TST")
        .register_trait::<dyn Logger>(Arc::new(ConsoleLogger))
        .build()
        .await
        .unwrap();

    assert!(app.get_trait::<dyn Logger>().is_some());
}

#[tokio::test]
async fn multiple_trait_bindings_coexist() {
    let app = App::builder("test", "TST")
        .register_trait::<dyn Notifier>(Arc::new(EmailNotifier))
        .register_trait::<dyn Logger>(Arc::new(ConsoleLogger))
        .build()
        .await
        .unwrap();

    let notifier = app.require_trait::<dyn Notifier>().unwrap();
    let logger = app.require_trait::<dyn Logger>().unwrap();

    assert_eq!(notifier.notify("hi"), "[EMAIL] hi");
    assert_eq!(logger.log("hi"), "[LOG] hi");
}

#[tokio::test]
async fn derive_service_with_dyn_trait_field() {
    #[derive(Service)]
    struct NotificationService {
        #[dependency]
        notifier: Arc<dyn Notifier>,
    }

    let app = App::builder("test", "TST")
        .register_trait::<dyn Notifier>(Arc::new(EmailNotifier))
        .register_service::<NotificationService>()
        .build()
        .await
        .unwrap();

    let svc = app.get::<NotificationService>().unwrap();
    assert_eq!(svc.notifier.notify("test"), "[EMAIL] test");
}

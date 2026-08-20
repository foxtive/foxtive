//! Tests for after_init and on_ready lifecycle hooks,
//! #[foxtive(init = "expr")] field initialization,
//! and ServiceHooks trait with skip_hooks.

mod common;

use foxtive::App;
use foxtive::lifecycle::{Service, ServiceHooks, ServiceInit};
use foxtive::prelude::AppResult;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

// after_init via manual ServiceInit (no derive)

static AFTER_INIT_CALLED: AtomicBool = AtomicBool::new(false);

struct AfterInitService {
    #[allow(dead_code)]
    value: Arc<LeafService>,
    config_value: i64,
}

impl AfterInitService {
    fn config_value(&self) -> i64 {
        self.config_value
    }
}

impl ServiceInit for AfterInitService {
    async fn init(app: &App) -> AppResult<Self> {
        Ok(Self {
            value: app.require::<LeafService>()?,
            config_value: 0,
        })
    }

    fn dependencies() -> Vec<&'static str> {
        vec![std::any::type_name::<LeafService>()]
    }

    fn after_init(&mut self, _app: &App) -> AppResult<()> {
        self.config_value = 42;
        AFTER_INIT_CALLED.store(true, Ordering::SeqCst);
        Ok(())
    }
}

struct LeafService {
    name: String,
}

impl ServiceInit for LeafService {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self {
            name: "leaf".to_string(),
        })
    }
}

#[tokio::test]
async fn after_init_is_called() {
    AFTER_INIT_CALLED.store(false, Ordering::SeqCst);

    let app = App::builder("lifecycle-test", "LCTEST")
        .register_service::<AfterInitService>()
        .register_service::<LeafService>()
        .build()
        .await
        .unwrap();

    assert!(AFTER_INIT_CALLED.load(Ordering::SeqCst));
    let svc = app.get::<AfterInitService>().unwrap();
    assert_eq!(svc.config_value(), 42);
}

// on_ready via manual ServiceInit (no derive)

static ON_READY_CALLED: AtomicBool = AtomicBool::new(false);

struct OnReadyService {
    #[allow(dead_code)]
    ready_value: AtomicUsize,
}

impl ServiceInit for OnReadyService {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self {
            ready_value: AtomicUsize::new(0),
        })
    }

    fn on_ready(_app: &App) -> AppResult<()> {
        ON_READY_CALLED.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn on_ready_is_called() {
    ON_READY_CALLED.store(false, Ordering::SeqCst);

    let app = App::builder("lifecycle-test", "LCTEST")
        .register_service::<OnReadyService>()
        .build()
        .await
        .unwrap();

    assert!(ON_READY_CALLED.load(Ordering::SeqCst));
    let _ = app.get::<OnReadyService>().unwrap();
}

// #[foxtive(init = "expr")] field initialization

#[derive(Service)]
#[service(all)]
struct InitExprService {
    leaf: Arc<LeafService>,
    #[foxtive(init = "99")]
    config: i64,
    #[foxtive(init = "app.app_name().to_string()")]
    app_name: String,
}

#[tokio::test]
async fn init_expr_sets_field_values() {
    let app = App::builder("init-expr-test", "IEXPR")
        .register_service::<InitExprService>()
        .register_service::<LeafService>()
        .build()
        .await
        .unwrap();

    let svc = app.get::<InitExprService>().unwrap();
    assert_eq!(svc.config, 99);
    assert_eq!(svc.app_name, "init-expr-test");
    assert_eq!(svc.leaf.name, "leaf");
}

// after_init + on_ready ordering (manual ServiceInit)

static ORDER_LOG: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());

struct OrderingService;

impl ServiceInit for OrderingService {
    async fn init(_app: &App) -> AppResult<Self> {
        ORDER_LOG.lock().unwrap().push("init");
        Ok(Self)
    }

    fn after_init(&mut self, _app: &App) -> AppResult<()> {
        ORDER_LOG.lock().unwrap().push("after_init");
        Ok(())
    }

    fn on_ready(_app: &App) -> AppResult<()> {
        ORDER_LOG.lock().unwrap().push("on_ready");
        Ok(())
    }
}

#[tokio::test]
async fn lifecycle_hooks_run_in_correct_order() {
    ORDER_LOG.lock().unwrap().clear();

    let _app = App::builder("order-test", "ORD")
        .register_service::<OrderingService>()
        .build()
        .await
        .unwrap();

    let log = ORDER_LOG.lock().unwrap();
    assert_eq!(&*log, &["init", "after_init", "on_ready"]);
}

// ServiceHooks via derive + skip_hooks

static HOOKS_AFTER_INIT_CALLED: AtomicBool = AtomicBool::new(false);
static HOOKS_ON_READY_CALLED: AtomicBool = AtomicBool::new(false);

#[derive(Service)]
#[service(all, skip_hooks)]
struct HooksService {
    #[allow(dead_code)]
    leaf: Arc<LeafService>,
    #[foxtive(default)]
    config_value: i64,
}

impl ServiceHooks for HooksService {
    fn after_init(&mut self, _app: &App) -> AppResult<()> {
        self.config_value = 77;
        HOOKS_AFTER_INIT_CALLED.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn on_ready(_app: &App) -> AppResult<()> {
        HOOKS_ON_READY_CALLED.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn service_hooks_after_init_is_called() {
    HOOKS_AFTER_INIT_CALLED.store(false, Ordering::SeqCst);

    let app = App::builder("hooks-test", "HKTEST")
        .register_service::<HooksService>()
        .register_service::<LeafService>()
        .build()
        .await
        .unwrap();

    assert!(HOOKS_AFTER_INIT_CALLED.load(Ordering::SeqCst));
    let svc = app.get::<HooksService>().unwrap();
    assert_eq!(svc.config_value, 77);
}

#[tokio::test]
async fn service_hooks_on_ready_is_called() {
    HOOKS_ON_READY_CALLED.store(false, Ordering::SeqCst);

    let _app = App::builder("hooks-test", "HKTEST")
        .register_service::<HooksService>()
        .register_service::<LeafService>()
        .build()
        .await
        .unwrap();

    assert!(HOOKS_ON_READY_CALLED.load(Ordering::SeqCst));
}

// derive without skip_hooks gets no-op ServiceHooks

#[derive(Service)]
#[service(all)]
struct NoHooksService {
    #[allow(dead_code)]
    leaf: Arc<LeafService>,
    #[foxtive(init = "55")]
    config_value: i64,
}

#[tokio::test]
async fn derive_without_skip_hooks_gets_noop_hooks() {
    let app = App::builder("nohooks-test", "NHK")
        .register_service::<NoHooksService>()
        .register_service::<LeafService>()
        .build()
        .await
        .unwrap();

    let svc = app.get::<NoHooksService>().unwrap();
    assert_eq!(svc.config_value, 55);
}

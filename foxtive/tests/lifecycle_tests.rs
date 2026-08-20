mod common;

use foxtive::App;
use foxtive::app::AppBuilder;
use foxtive::lifecycle::Plugin;
use foxtive::results::AppResult;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[tokio::test]
async fn plugin_startup_hook_runs() {
    static STARTUP_CALLED: AtomicBool = AtomicBool::new(false);
    STARTUP_CALLED.store(false, Ordering::SeqCst);

    struct TestPlugin;

    impl Plugin for TestPlugin {
        fn name(&self) -> &str {
            "test-plugin"
        }

        async fn on_startup(&self, _app: &App) -> AppResult<()> {
            STARTUP_CALLED.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    let app = App::builder("svc", "SVC")
        .plugin(TestPlugin)
        .build()
        .await
        .unwrap();

    app.run_startup_hooks().await.unwrap();
    assert!(STARTUP_CALLED.load(Ordering::SeqCst));
}

#[tokio::test]
async fn plugin_shutdown_hook_runs() {
    static SHUTDOWN_CALLED: AtomicBool = AtomicBool::new(false);
    SHUTDOWN_CALLED.store(false, Ordering::SeqCst);

    struct TestPlugin;

    impl Plugin for TestPlugin {
        fn name(&self) -> &str {
            "shutdown-test"
        }

        async fn on_shutdown(&self, _app: &App) {
            SHUTDOWN_CALLED.store(true, Ordering::SeqCst);
        }
    }

    let app = App::builder("svc", "SVC")
        .plugin(TestPlugin)
        .build()
        .await
        .unwrap();

    app.shutdown().await;
    assert!(SHUTDOWN_CALLED.load(Ordering::SeqCst));
}

#[tokio::test]
async fn plugin_registers_services() {
    struct MyService {
        value: i32,
    }

    struct ServicePlugin;

    impl Plugin for ServicePlugin {
        fn name(&self) -> &str {
            "service-plugin"
        }

        fn register(&self, builder: AppBuilder) -> AppBuilder {
            builder.register(MyService { value: 99 })
        }
    }

    let app = App::builder("svc", "SVC")
        .plugin(ServicePlugin)
        .build()
        .await
        .unwrap();

    let svc = app.get::<MyService>().unwrap();
    assert_eq!(svc.value, 99);
}

#[tokio::test]
async fn multiple_plugins_all_run() {
    static PLUGIN_A_RAN: AtomicBool = AtomicBool::new(false);
    static PLUGIN_B_RAN: AtomicBool = AtomicBool::new(false);
    PLUGIN_A_RAN.store(false, Ordering::SeqCst);
    PLUGIN_B_RAN.store(false, Ordering::SeqCst);

    struct PluginA;
    struct PluginB;

    impl Plugin for PluginA {
        fn name(&self) -> &str {
            "plugin-a"
        }

        async fn on_startup(&self, _app: &App) -> AppResult<()> {
            PLUGIN_A_RAN.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    impl Plugin for PluginB {
        fn name(&self) -> &str {
            "plugin-b"
        }

        async fn on_startup(&self, _app: &App) -> AppResult<()> {
            PLUGIN_B_RAN.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    let app = App::builder("svc", "SVC")
        .plugin(PluginA)
        .plugin(PluginB)
        .build()
        .await
        .unwrap();

    app.run_startup_hooks().await.unwrap();
    assert!(PLUGIN_A_RAN.load(Ordering::SeqCst));
    assert!(PLUGIN_B_RAN.load(Ordering::SeqCst));
}

#[tokio::test]
async fn shutdown_hooks_run_in_reverse_order() {
    let order = Arc::new(std::sync::Mutex::new(Vec::new()));

    let o1 = order.clone();
    let o2 = order.clone();

    let app = App::builder("svc", "SVC")
        .on_shutdown(move |_| {
            let o1 = o1.clone();
            async move {
                o1.lock().unwrap().push(1);
            }
        })
        .on_shutdown(move |_| {
            let o2 = o2.clone();
            async move {
                o2.lock().unwrap().push(2);
            }
        })
        .build()
        .await
        .unwrap();

    app.shutdown().await;

    let final_order = order.lock().unwrap();
    assert_eq!(*final_order, vec![2, 1]);
}

#[tokio::test]
async fn startup_hook_receives_app_reference() {
    static APP_NAME_MATCHES: AtomicBool = AtomicBool::new(false);
    APP_NAME_MATCHES.store(false, Ordering::SeqCst);

    let app = App::builder("My Service", "SVC")
        .on_startup(|app| async move {
            if app.app_name() == "My Service" {
                APP_NAME_MATCHES.store(true, Ordering::SeqCst);
            }
            Ok(())
        })
        .build()
        .await
        .unwrap();

    app.run_startup_hooks().await.unwrap();
    assert!(APP_NAME_MATCHES.load(Ordering::SeqCst));
}

#[tokio::test]
async fn no_hooks_builds_and_runs_cleanly() {
    let app = App::builder("svc", "SVC").build().await.unwrap();
    app.run_startup_hooks().await.unwrap();
    app.shutdown().await;
}

#[tokio::test]
async fn multiple_startup_hooks_all_run() {
    let counter = Arc::new(AtomicUsize::new(0));

    let c1 = counter.clone();
    let c2 = counter.clone();
    let c3 = counter.clone();

    let app = App::builder("svc", "SVC")
        .on_startup(move |_| {
            let c1 = c1.clone();
            async move {
                c1.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .on_startup(move |_| {
            let c2 = c2.clone();
            async move {
                c2.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .on_startup(move |_| {
            let c3 = c3.clone();
            async move {
                c3.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .build()
        .await
        .unwrap();

    app.run_startup_hooks().await.unwrap();
    assert_eq!(counter.load(Ordering::SeqCst), 3);
}
